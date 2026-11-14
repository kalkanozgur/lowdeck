//! LowDeck PC sunucusu (ADR-0002). Kullanıcı oturumunda çalışır; servis değil.
//!
//! Thread modeli (async yok, polling yok):
//! - main: tray mesaj döngüsü (`tray.rs`), olay yoksa uyur.
//! - accept: yeni bağlantı eskisinin yerine geçer (telefon tek istemci).
//! - client: telefondan okur, komutları audio/media thread'lerine kanalla iletir.
//! - audio / media (`win.rs`): COM/WinRT nesnelerinin sahibi, `recv`'de bloklu;
//!   Windows olayları (volume callback, SMTC event) kanala mesaj bırakır.
//! - `Hub`: son durumu tutar, yalnız gerçekten değişince istemciye push eder.

// Windows dışında yalnız testler derlenir (CI Linux); gerisi kullanılmaz.
#![cfg_attr(not(windows), allow(dead_code, unused_imports))]
// Konsol penceresi yok; arayüz tray ikonu (tray.rs).
#![cfg_attr(all(windows, not(test)), windows_subsystem = "windows")]

use lowdeck_proto::*;
use std::io::{BufReader, BufWriter};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[cfg(windows)]
mod tray;
#[cfg(windows)]
mod win;

#[cfg(not(test))]
const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
#[cfg(test)]
const HELLO_TIMEOUT: Duration = Duration::from_millis(300);
/// İstemci 25 s'de bir ping atar; bu sürede hiçbir şey gelmezse bağlantı ölü.
const READ_TIMEOUT: Duration = Duration::from_secs(90);
/// Yavaş/kaybolmuş istemci Hub kilidini bu süreden uzun tutamaz.
const WRITE_TIMEOUT: Duration = Duration::from_secs(2);

// ---------------- Hub ----------------

#[derive(Default)]
pub struct Hub {
    inner: Mutex<HubInner>,
    /// Bağlı telefon değişince çağrılır (tray ikonu). Kilit dışında çağrılır.
    on_client: std::sync::OnceLock<Box<dyn Fn() + Send + Sync>>,
}

struct Client {
    id: u64,
    name: String,
    w: BufWriter<TcpStream>,
}

#[derive(Default)]
struct HubInner {
    audio: AudioState,
    media: MediaState,
    next_id: u64,
    client: Option<Client>,
}

impl HubInner {
    /// false: yazma hatası, istemci düşürüldü.
    fn send(&mut self, msg: &S2C) -> bool {
        if let Some(c) = &mut self.client {
            if write_frame(&mut c.w, msg).is_err() {
                let _ = c.w.get_ref().shutdown(Shutdown::Both);
                self.client = None;
                return false;
            }
        }
        true
    }
}

impl Hub {
    fn lock(&self) -> std::sync::MutexGuard<'_, HubInner> {
        // Zehirlenmiş kilit: içerik yalnız önbellek + soket, kullanmaya devam etmek güvenli.
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn changed(&self) {
        if let Some(f) = self.on_client.get() {
            f();
        }
    }

    pub fn client_name(&self) -> Option<String> {
        self.lock().client.as_ref().map(|c| c.name.clone())
    }

    fn push(&self, msg: S2C) {
        let ok = self.lock().send(&msg);
        if !ok {
            self.changed();
        }
    }

    pub fn set_audio(&self, a: AudioState) {
        let mut g = self.lock();
        if g.audio != a {
            g.audio = a.clone();
            drop(g);
            self.push(S2C::Audio(a));
        }
    }

    /// Volume callback cihaz adını vermez; ad önbellekten korunur.
    pub fn set_audio_level(&self, volume: u8, muted: bool) {
        let device = self.lock().audio.device.clone();
        self.set_audio(AudioState { volume, muted, device });
    }

    pub fn set_media(&self, m: MediaState) {
        let mut g = self.lock();
        if g.media != m {
            g.media = m.clone();
            drop(g);
            self.push(S2C::Media(m));
        }
    }

    /// Welcome + tam durumu kilit altında yazar: arada push kaçmaz/araya girmez.
    /// Önceki istemci kapatılır. Dönen id ile `reply`/`detach` yapılır.
    fn attach(&self, name: String, mut w: BufWriter<TcpStream>) -> Option<u64> {
        let mut g = self.lock();
        let hello = [
            S2C::Welcome { proto: PROTO_VERSION, server: server_name() },
            S2C::Audio(g.audio.clone()),
            S2C::Media(g.media.clone()),
        ];
        for m in &hello {
            write_frame(&mut w, m).ok()?;
        }
        if let Some(old) = g.client.take() {
            let _ = old.w.get_ref().shutdown(Shutdown::Both);
        }
        g.next_id += 1;
        let id = g.next_id;
        g.client = Some(Client { id, name, w });
        drop(g);
        self.changed();
        Some(id)
    }

    fn reply(&self, id: u64, msg: &S2C) {
        let mut g = self.lock();
        if g.client.as_ref().is_some_and(|c| c.id == id) && !g.send(msg) {
            drop(g);
            self.changed();
        }
    }

    /// Bağlı istemciyi düşürür (ayar yeniden yüklendi).
    fn kick(&self) {
        let mut g = self.lock();
        if let Some(c) = g.client.take() {
            let _ = c.w.get_ref().shutdown(Shutdown::Both);
            drop(g);
            self.changed();
        }
    }

    fn detach(&self, id: u64) {
        let mut g = self.lock();
        if g.client.as_ref().is_some_and(|c| c.id == id) {
            g.client = None;
            drop(g);
            self.changed();
        }
    }
}

fn server_name() -> String {
    std::env::var("COMPUTERNAME").unwrap_or_else(|_| "pc".into())
}

/// Komutların gideceği arka uç (Windows'ta audio/media thread kanalları).
pub trait Backend: Send + Sync {
    fn audio(&self, cmd: AudioCmd);
    fn media(&self, cmd: MediaCmd);
}

// ---------------- Bağlantı ----------------

fn handle(stream: TcpStream, token: &str, hub: &Hub, backend: &dyn Backend) -> Result<(), FrameError> {
    let peer = stream.peer_addr().ok();
    stream.set_nodelay(true).ok();
    stream.set_write_timeout(Some(WRITE_TIMEOUT)).ok();
    // Timeout'lar okuyan handle'a: Windows'ta try_clone'lu handle SO_RCVTIMEO'yu paylaşmıyor.
    let mut rd = BufReader::new(stream.try_clone()?);
    rd.get_ref().set_read_timeout(Some(HELLO_TIMEOUT)).ok();
    let mut buf = [0u8; MAX_FRAME];

    match read_frame::<_, C2S>(&mut rd, &mut buf)? {
        C2S::Hello { proto, token: t, name } => {
            let err = if proto != PROTO_VERSION {
                Some(ErrorCode::BadVersion)
            } else if t != token {
                // shortcut: sabit zamanlı karşılaştırma ve hatalı deneme cezası yok; LAN + eşleştirme gelince (PROTOCOL §5).
                Some(ErrorCode::BadToken)
            } else {
                None
            };
            if let Some(e) = err {
                println!("- reddedildi: {peer:?} ({e:?})");
                write_frame(&mut BufWriter::new(stream), &S2C::Error(e))?;
                return Ok(());
            }
            println!("+ bağlandı: {name} {peer:?}");
            rd.get_ref().set_read_timeout(Some(READ_TIMEOUT)).ok();
            let Some(id) = hub.attach(name, BufWriter::new(stream)) else { return Ok(()) };
            session(&mut rd, &mut buf, id, hub, backend)
        }
        _ => Ok(()),
    }
}

fn session(
    rd: &mut BufReader<TcpStream>,
    buf: &mut [u8; MAX_FRAME],
    id: u64,
    hub: &Hub,
    backend: &dyn Backend,
) -> Result<(), FrameError> {
    let r = (|| loop {
        match read_frame::<_, C2S>(rd, buf)? {
            C2S::Ping(n) => hub.reply(id, &S2C::Pong(n)),
            // Sonuç Windows olayı ile (gerçekten değişirse) push edilir.
            C2S::Audio(cmd) => backend.audio(cmd),
            C2S::Media(cmd) => backend.media(cmd),
            C2S::Hello { .. } => hub.reply(id, &S2C::Error(ErrorCode::Unsupported)),
        }
    })();
    hub.detach(id);
    r
}

// ---------------- Ayar ----------------

#[derive(Clone)]
struct AgentConfig {
    bind: String,
    port: u16,
    token: String,
}

fn config_path() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| ".".into());
    base.join("LowDeck").join("agent.cfg")
}

/// Yoksa LAN IP'si + rastgele token ile oluşturur. Biçim telefondakiyle aynı: `key=value`.
fn load_or_create(path: &std::path::Path) -> std::io::Result<AgentConfig> {
    if let Ok(text) = std::fs::read_to_string(path) {
        let mut c = AgentConfig { bind: String::new(), port: DEFAULT_PORT, token: String::new() };
        for (k, v) in text.lines().filter_map(|l| l.split_once('=')) {
            let v = v.trim().to_string();
            match k.trim() {
                "bind" => c.bind = v,
                "port" => c.port = v.parse().unwrap_or(DEFAULT_PORT),
                "token" => c.token = v,
                _ => {}
            }
        }
        if c.bind.is_empty() || c.token.is_empty() {
            return Err(std::io::Error::other(format!("{}: bind ve token zorunlu", path.display())));
        }
        return Ok(c);
    }
    let c = AgentConfig { bind: lan_ip(), port: DEFAULT_PORT, token: random_token() };
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(
        path,
        format!(
            "# bind: BU PC'nin LAN IP'si (telefonunki değil). Telefon ⚙ bu adres + port + token.\n\
             # Değişiklikten sonra tray → Ayarı yeniden yükle.\n\
             bind={}\nport={}\ntoken={}\n",
            c.bind, c.port, c.token
        ),
    )?;
    println!("ayar oluşturuldu: {}", path.display());
    Ok(c)
}

/// Varsayılan rotanın yerel adresi. UDP `connect` paket göndermez; 192.0.2.1 TEST-NET (RFC 5737).
fn lan_ip() -> String {
    std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| s.connect("192.0.2.1:9").map(|_| s))
        .and_then(|s| s.local_addr())
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| "127.0.0.1".into())
}

/// shortcut: std RandomState (işletim sistemi rastgelesiyle tohumlanır) → 64 bit; eşleştirme gelince 256-bit CSPRNG.
fn random_token() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
    format!("{:016x}", h.finish())
}

// ---------------- main ----------------

/// Çalışan dinleyici. `stop` ile kapatılır (yeniden yükleme).
struct Running {
    cfg: AgentConfig,
    addr: std::net::SocketAddr,
    stop: Arc<std::sync::atomic::AtomicBool>,
    thread: std::thread::JoinHandle<()>,
}

impl Running {
    fn stop(self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        // accept() bloklu: kendine bağlanarak uyandır, sonra thread (ve dinleyici) bitsin.
        let _ = TcpStream::connect_timeout(&self.addr, Duration::from_secs(1));
        let _ = self.thread.join();
    }
}

/// 0.0.0.0 değil: yalnız seçilen arayüz (ADR-0002).
fn listen(cfg: AgentConfig, hub: &Arc<Hub>, backend: &Arc<dyn Backend>) -> std::io::Result<Running> {
    let listener = TcpListener::bind((cfg.bind.as_str(), cfg.port))?;
    let addr = listener.local_addr()?;
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (token, hub, backend, s) = (Arc::<str>::from(cfg.token.as_str()), hub.clone(), backend.clone(), stop.clone());
    let thread = std::thread::Builder::new()
        .name("lowdeck-accept".into())
        .spawn(move || accept_loop(listener, token, hub, backend, s))?;
    Ok(Running { cfg, addr, stop, thread })
}

/// Kullanıcıya gösterilecek bind hatası; en sık hata: bind'a telefonun IP'sini yazmak.
fn bind_error(cfg: &AgentConfig, e: &std::io::Error) -> String {
    match e.raw_os_error() {
        Some(10049) => format!(
            "{} bu PC'nin adresi değil (bu PC: {}).\nbind = PC'nin LAN IP'si, telefonunki değil.",
            cfg.bind,
            lan_ip()
        ),
        Some(10048) => format!("Port {} kullanımda (başka bir program ya da ikinci agent).", cfg.port),
        _ => format!("{}:{} dinlenemedi: {e}", cfg.bind, cfg.port),
    }
}

#[cfg(windows)]
fn main() {
    if !tray::single_instance() {
        return;
    }
    let path = config_path();
    let hub = Arc::new(Hub::default());
    let backend: Arc<dyn Backend> = Arc::new(win::start(hub.clone()));
    let first = match load_or_create(&path) {
        Ok(c) => listen(c.clone(), &hub, &backend).map_err(|e| bind_error(&c, &e)),
        Err(e) => Err(e.to_string()),
    };
    // Konsol yok: başlangıç hatası kullanıcıya kutuyla gösterilir.
    let mut running = match first {
        Ok(r) => Some(r),
        Err(e) => return tray::error_box(&format!("Başlatılamadı: {e}\n\nAyar: {}", path.display())),
    };
    let info_of = {
        let path = path.clone();
        move |c: &AgentConfig| tray::Info { bind: c.bind.clone(), port: c.port, token: c.token.clone(), cfg_path: path.clone() }
    };
    let info = info_of(&running.as_ref().expect("başladı").cfg);

    // Tray → "Ayarı yeniden yükle": dosyayı oku, eskiyi durdur, yeniyi aç; olmazsa eskiye dön.
    let reload = {
        let hub = hub.clone();
        move || -> Result<tray::Info, String> {
            let new = load_or_create(&path).map_err(|e| e.to_string())?;
            let old = running.take().map(|r| {
                let cfg = r.cfg.clone();
                r.stop();
                cfg
            });
            hub.kick(); // telefon yeni adres/token ile yeniden bağlansın
            match listen(new.clone(), &hub, &backend) {
                Ok(r) => {
                    let info = info_of(&r.cfg);
                    running = Some(r);
                    Ok(info)
                }
                Err(e) => {
                    running = old.and_then(|c| listen(c, &hub, &backend).ok());
                    let state = if running.is_some() { "Eski ayarla devam ediliyor." } else { "Dinleme durdu." };
                    Err(format!("{}\n\n{state}", bind_error(&new, &e)))
                }
            }
        }
    };
    tray::run(info, hub, Box::new(reload)); // mesaj döngüsü; Çıkış ile döner
}

fn accept_loop(
    listener: TcpListener,
    token: Arc<str>,
    hub: Arc<Hub>,
    backend: Arc<dyn Backend>,
    stop: Arc<std::sync::atomic::AtomicBool>,
) {
    for conn in listener.incoming() {
        if stop.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        let Ok(s) = conn else { continue };
        let (hub, backend, token) = (hub.clone(), backend.clone(), token.clone());
        let _ = std::thread::Builder::new()
            .name("lowdeck-client".into())
            .stack_size(64 * 1024)
            .spawn(move || {
                if let Err(e) = handle(s, &token, &hub, &*backend) {
                    println!("- koptu ({e})");
                }
            });
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("lowdeck-agent yalnız Windows'ta çalışır (Core Audio + SMTC). Geliştirme için mock-server.");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    struct Null;
    impl Backend for Null {
        fn audio(&self, _: AudioCmd) {}
        fn media(&self, _: MediaCmd) {}
    }

    /// Hello → tam durum; değişmeyen durum push edilmez; yeni bağlantı eskisini kapatır.
    #[test]
    fn hub_flow() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let hub = Arc::new(Hub::default());
        hub.set_audio(AudioState { volume: 40, muted: false, device: "H".into() });
        {
            let hub = hub.clone();
            std::thread::spawn(move || {
                for s in l.incoming() {
                    let hub = hub.clone();
                    std::thread::spawn(move || handle(s.unwrap(), "t", &hub, &Null));
                }
            });
        }
        let connect = || {
            let s = TcpStream::connect(addr).unwrap();
            let mut w = BufWriter::new(s.try_clone().unwrap());
            write_frame(&mut w, &C2S::Hello { proto: PROTO_VERSION, token: "t".into(), name: "x".into() }).unwrap();
            BufReader::new(s)
        };
        let mut buf = [0u8; MAX_FRAME];
        let mut a = connect();
        assert!(matches!(read_frame(&mut a, &mut buf).unwrap(), S2C::Welcome { .. }));
        assert_eq!(read_frame::<_, S2C>(&mut a, &mut buf).unwrap(), S2C::Audio(hub.lock().audio.clone()));
        assert!(matches!(read_frame(&mut a, &mut buf).unwrap(), S2C::Media(_)));

        hub.set_audio_level(40, false); // değişmedi → push yok
        hub.set_audio_level(55, true);
        let got: S2C = read_frame(&mut a, &mut buf).unwrap();
        assert_eq!(got, S2C::Audio(AudioState { volume: 55, muted: true, device: "H".into() }));

        let mut b = connect();
        assert!(matches!(read_frame(&mut b, &mut buf).unwrap(), S2C::Welcome { .. }));
        assert!(matches!(read_frame::<_, S2C>(&mut a, &mut buf), Err(FrameError::Closed | FrameError::Io(_))));
    }

    /// Hello zaman aşımı oturuma taşmamalı (Windows'ta try_clone'lu handle timeout'u paylaşmıyor).
    #[test]
    fn idle_session_survives_hello_timeout() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        std::thread::spawn(move || {
            let hub = Hub::default();
            let _ = handle(l.accept().unwrap().0, "t", &hub, &Null);
        });
        let s = TcpStream::connect(addr).unwrap();
        let mut w = BufWriter::new(s.try_clone().unwrap());
        write_frame(&mut w, &C2S::Hello { proto: PROTO_VERSION, token: "t".into(), name: "x".into() }).unwrap();
        let mut rd = BufReader::new(s);
        let mut buf = [0u8; MAX_FRAME];
        for _ in 0..3 {
            read_frame::<_, S2C>(&mut rd, &mut buf).unwrap(); // Welcome, Audio, Media
        }
        std::thread::sleep(HELLO_TIMEOUT * 2);
        write_frame(&mut w, &C2S::Ping(7)).unwrap();
        assert_eq!(read_frame::<_, S2C>(&mut rd, &mut buf).unwrap(), S2C::Pong(7));
    }

    /// Yeniden yükleme: stop() dinleyiciyi gerçekten kapatır, aynı port hemen tekrar açılır.
    #[test]
    fn reload_rebinds_same_port() {
        let hub = Arc::new(Hub::default());
        let backend: Arc<dyn Backend> = Arc::new(Null);
        let cfg = AgentConfig { bind: "127.0.0.1".into(), port: 0, token: "t".into() };
        let r = listen(cfg, &hub, &backend).unwrap();
        let port = r.addr.port();
        r.stop();
        let again = AgentConfig { bind: "127.0.0.1".into(), port, token: "t".into() };
        listen(again, &hub, &backend).expect("aynı port yeniden açılmalı").stop();
    }

    #[cfg(windows)]
    #[test]
    fn bind_error_explains_wrong_ip() {
        let hub = Arc::new(Hub::default());
        let backend: Arc<dyn Backend> = Arc::new(Null);
        // TEST-NET adresi bu PC'de yok → WSAEADDRNOTAVAIL
        let cfg = AgentConfig { bind: "192.0.2.1".into(), port: 47810, token: "t".into() };
        let e = listen(cfg.clone(), &hub, &backend).err().expect("bind başarısız olmalı");
        assert!(bind_error(&cfg, &e).contains("bu PC'nin adresi değil"), "{}", bind_error(&cfg, &e));
    }

    #[test]
    fn bad_token() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        std::thread::spawn(move || {
            let hub = Hub::default();
            let _ = handle(l.accept().unwrap().0, "t", &hub, &Null);
        });
        let s = TcpStream::connect(addr).unwrap();
        write_frame(&mut BufWriter::new(s.try_clone().unwrap()), &C2S::Hello { proto: PROTO_VERSION, token: "x".into(), name: "x".into() }).unwrap();
        let mut buf = [0u8; MAX_FRAME];
        assert_eq!(read_frame::<_, S2C>(&mut BufReader::new(s), &mut buf).unwrap(), S2C::Error(ErrorCode::BadToken));
    }
}

//! LowDeck PC sunucusu (ADR-0002). Kullanıcı oturumunda çalışır; servis değil.
//!
//! Thread modeli (async yok, polling yok):
//! - main: `accept`. Yeni bağlantı eskisinin yerine geçer (telefon tek istemci).
//! - client: telefondan okur, komutları audio/media thread'lerine kanalla iletir.
//! - audio / media (`win.rs`): COM/WinRT nesnelerinin sahibi, `recv`'de bloklu;
//!   Windows olayları (volume callback, SMTC event) kanala mesaj bırakır.
//! - `Hub`: son durumu tutar, yalnız gerçekten değişince istemciye push eder.

// Windows dışında yalnız testler derlenir (CI Linux); gerisi kullanılmaz.
#![cfg_attr(not(windows), allow(dead_code, unused_imports))]

use lowdeck_proto::*;
use std::io::{BufReader, BufWriter};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[cfg(windows)]
mod win;

const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
/// İstemci 25 s'de bir ping atar; bu sürede hiçbir şey gelmezse bağlantı ölü.
const READ_TIMEOUT: Duration = Duration::from_secs(90);
/// Yavaş/kaybolmuş istemci Hub kilidini bu süreden uzun tutamaz.
const WRITE_TIMEOUT: Duration = Duration::from_secs(2);

// ---------------- Hub ----------------

#[derive(Default)]
pub struct Hub(Mutex<HubInner>);

#[derive(Default)]
struct HubInner {
    audio: AudioState,
    media: MediaState,
    next_id: u64,
    client: Option<(u64, BufWriter<TcpStream>)>,
}

impl HubInner {
    fn send(&mut self, msg: &S2C) {
        if let Some((_, w)) = &mut self.client {
            if write_frame(w, msg).is_err() {
                let _ = w.get_ref().shutdown(Shutdown::Both);
                self.client = None;
            }
        }
    }
}

impl Hub {
    fn lock(&self) -> std::sync::MutexGuard<'_, HubInner> {
        // Zehirlenmiş kilit: içerik yalnız önbellek + soket, kullanmaya devam etmek güvenli.
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn set_audio(&self, a: AudioState) {
        let mut g = self.lock();
        if g.audio != a {
            g.audio = a.clone();
            g.send(&S2C::Audio(a));
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
            g.send(&S2C::Media(m));
        }
    }

    /// Welcome + tam durumu kilit altında yazar: arada push kaçmaz/araya girmez.
    /// Önceki istemci kapatılır. Dönen id ile `reply`/`detach` yapılır.
    fn attach(&self, mut w: BufWriter<TcpStream>) -> Option<u64> {
        let mut g = self.lock();
        let hello = [
            S2C::Welcome { proto: PROTO_VERSION, server: server_name() },
            S2C::Audio(g.audio.clone()),
            S2C::Media(g.media.clone()),
        ];
        for m in &hello {
            write_frame(&mut w, m).ok()?;
        }
        if let Some((_, old)) = g.client.take() {
            let _ = old.get_ref().shutdown(Shutdown::Both);
        }
        g.next_id += 1;
        let id = g.next_id;
        g.client = Some((id, w));
        Some(id)
    }

    fn reply(&self, id: u64, msg: &S2C) {
        let mut g = self.lock();
        if g.client.as_ref().is_some_and(|(cid, _)| *cid == id) {
            g.send(msg);
        }
    }

    fn detach(&self, id: u64) {
        let mut g = self.lock();
        if g.client.as_ref().is_some_and(|(cid, _)| *cid == id) {
            g.client = None;
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
    stream.set_read_timeout(Some(HELLO_TIMEOUT)).ok();
    let mut rd = BufReader::new(stream.try_clone()?);
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
        }
        _ => return Ok(()),
    }

    stream.set_read_timeout(Some(READ_TIMEOUT)).ok();
    let Some(id) = hub.attach(BufWriter::new(stream)) else { return Ok(()) };
    let r = (|| loop {
        match read_frame::<_, C2S>(&mut rd, &mut buf)? {
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
    std::fs::write(path, format!("bind={}\nport={}\ntoken={}\n", c.bind, c.port, c.token))?;
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

#[cfg(windows)]
fn main() -> std::io::Result<()> {
    let path = config_path();
    let cfg = load_or_create(&path)?;
    let hub = Arc::new(Hub::default());
    let backend: Arc<dyn Backend> = Arc::new(win::start(hub.clone()));

    // 0.0.0.0 değil: yalnız seçilen arayüz (ADR-0002).
    let listener = TcpListener::bind((cfg.bind.as_str(), cfg.port))?;
    println!("lowdeck-agent: {}:{}  ayar: {}", cfg.bind, cfg.port, path.display());
    println!("telefon ⚙ → host={} port={} token={}", cfg.bind, cfg.port, cfg.token);

    let token: Arc<str> = cfg.token.into();
    for conn in listener.incoming() {
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
    Ok(())
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

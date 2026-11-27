//! UI'dan bağımsız istemci. Async runtime YOK (tokio vb.): 2 hafif thread.
//!
//! - `net` thread: bağlan -> Hello -> bloklayan okuma döngüsü. Kopunca üstel
//!   geri çekilmeyle (1 s .. 30 s) yeniden bağlanır.
//! - `ping` thread: bağlıyken `PING_EVERY` aralıkla Ping (tek periyodik uyanma).
//!
//! Olaylar `on_event` callback'i ile verilir; callback net thread'inde çalışır,
//! UI tarafı kendi event loop'una aktarmalıdır (Slint: `invoke_from_event_loop`).

use lowdeck_proto::*;
use std::io::{BufReader, BufWriter};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const PING_EVERY: Duration = Duration::from_secs(25);
/// Bu süre boyunca hiçbir şey (Pong dahil) gelmezse bağlantı ölü sayılır.
const READ_TIMEOUT: Duration = Duration::from_secs(60);
const BACKOFF_MAX: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub token: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub enum Event {
    Connecting,
    Connected { server: String },
    Disconnected { reason: String },
    Msg(S2C),
}

pub struct Client {
    writer: Arc<Mutex<Option<BufWriter<TcpStream>>>>,
    stop: Arc<AtomicBool>,
    /// İstenen abonelik; her Welcome'dan sonra yeniden gönderilir (sunucuda bağlantı başına).
    topics: Arc<AtomicU8>,
}

impl Client {
    pub fn start(cfg: Config, on_event: impl Fn(Event) + Send + Sync + 'static) -> Self {
        let writer: Arc<Mutex<Option<BufWriter<TcpStream>>>> = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let topics = Arc::new(AtomicU8::new(0));
        let on_event = Arc::new(on_event);

        // net thread
        {
            let writer = writer.clone();
            let stop = stop.clone();
            let on_event = on_event.clone();
            let topics = topics.clone();
            thread::Builder::new()
                .name("lowdeck-net".into())
                .spawn(move || net_loop(cfg, writer, stop, &topics, &*on_event))
                .expect("spawn net");
        }

        // ping thread
        {
            let writer = writer.clone();
            let stop = stop.clone();
            thread::Builder::new()
                .name("lowdeck-ping".into())
                .spawn(move || {
                    let seq = AtomicU32::new(0);
                    while !stop.load(Ordering::Relaxed) {
                        thread::sleep(PING_EVERY);
                        let n = seq.fetch_add(1, Ordering::Relaxed);
                        let _ = send_on(&writer, &C2S::Ping(n));
                    }
                })
                .expect("spawn ping");
        }

        Client { writer, stop, topics }
    }

    /// Değişmediyse bir şey göndermez. Bağlı değilse bağlanınca gönderilir.
    pub fn subscribe(&self, t: Topics) {
        if self.topics.swap(t, Ordering::Relaxed) != t {
            self.send(&C2S::Subscribe(t));
        }
    }

    /// Bağlı değilse komut sessizce düşer (UI bağlantı durumunu zaten gösteriyor).
    pub fn send(&self, msg: &C2S) -> bool {
        send_on(&self.writer, msg)
    }

    /// Bağlantıyı hemen kapatır; net thread geri çekilmeyle yeniden bağlanır. Yanıt beklenen komut
    /// gelmediyse (ölü bağlantı, ör. PC uykudan döndü) `READ_TIMEOUT`'u beklememek için.
    pub fn reconnect(&self) {
        if let Ok(mut g) = self.writer.lock() {
            if let Some(w) = g.take() {
                let _ = w.get_ref().shutdown(std::net::Shutdown::Both);
            }
        }
    }

    pub fn audio(&self, cmd: AudioCmd) -> bool {
        self.send(&C2S::Audio(cmd))
    }

    pub fn media(&self, cmd: MediaCmd) -> bool {
        self.send(&C2S::Media(cmd))
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Ok(mut g) = self.writer.lock() {
            if let Some(w) = g.take() {
                let _ = w.get_ref().shutdown(std::net::Shutdown::Both);
            }
        }
    }
}

fn send_on(writer: &Mutex<Option<BufWriter<TcpStream>>>, msg: &C2S) -> bool {
    let mut g = match writer.lock() {
        Ok(g) => g,
        Err(_) => return false,
    };
    match g.as_mut() {
        Some(w) => {
            if write_frame(w, msg).is_err() {
                // Yazma hatası: soketi kapat, net thread okuma hatasıyla yeniden bağlanır.
                let _ = w.get_ref().shutdown(std::net::Shutdown::Both);
                *g = None;
                false
            } else {
                true
            }
        }
        None => false,
    }
}

fn resolve(host: &str, port: u16) -> Option<SocketAddr> {
    (host, port).to_socket_addrs().ok()?.next()
}

fn net_loop(
    cfg: Config,
    writer: Arc<Mutex<Option<BufWriter<TcpStream>>>>,
    stop: Arc<AtomicBool>,
    topics: &AtomicU8,
    on_event: &(dyn Fn(Event) + Send + Sync),
) {
    let mut backoff = Duration::from_secs(1);
    let mut buf = [0u8; MAX_FRAME];

    while !stop.load(Ordering::Relaxed) {
        on_event(Event::Connecting);
        let (reason, fatal) = match session(&cfg, &writer, &stop, topics, &mut buf, on_event, &mut backoff) {
            Ok(()) => ("closed".to_string(), false),
            Err(Fail::Retry(e)) => (e, false),
            Err(Fail::Fatal(e)) => (e, true),
        };
        if let Ok(mut g) = writer.lock() {
            *g = None;
        }
        on_event(Event::Disconnected { reason });
        // Yanlış token / sürüm: tekrar denemek anlamsız, ayar değişene kadar dur.
        if fatal || stop.load(Ordering::Relaxed) {
            break;
        }
        thread::sleep(backoff);
        backoff = (backoff * 2).min(BACKOFF_MAX);
    }
}

enum Fail {
    Retry(String),
    Fatal(String),
}

impl<E: std::fmt::Display> From<E> for Fail {
    fn from(e: E) -> Self {
        Fail::Retry(e.to_string())
    }
}

fn session(
    cfg: &Config,
    writer: &Mutex<Option<BufWriter<TcpStream>>>,
    stop: &AtomicBool,
    topics: &AtomicU8,
    buf: &mut [u8; MAX_FRAME],
    on_event: &(dyn Fn(Event) + Send + Sync),
    backoff: &mut Duration,
) -> Result<(), Fail> {
    let addr = resolve(&cfg.host, cfg.port).ok_or_else(|| Fail::Retry("adres çözülemedi".into()))?;
    let stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)?;
    stream.set_nodelay(true).ok(); // komutlar küçük ve gecikmeye duyarlı
    // Timeout okuyan handle'a: Windows'ta try_clone'lu handle SO_RCVTIMEO'yu paylaşmıyor.
    let mut rd = BufReader::new(stream.try_clone()?);
    rd.get_ref().set_read_timeout(Some(READ_TIMEOUT)).ok();
    {
        let mut w = BufWriter::new(stream);
        write_frame(
            &mut w,
            &C2S::Hello { proto: PROTO_VERSION, token: cfg.token.clone(), name: cfg.name.clone() },
        )?;
        *writer.lock().map_err(|_| Fail::Fatal("lock poisoned".into()))? = Some(w);
    }
    // Drop, connect sürerken geldiyse writer'ı göremedi: soketi burada kapat
    // (yoksa zombi bağlantı READ_TIMEOUT'a kadar açık kalır). Drop önce `stop`'u
    // yazıp sonra kilidi aldığı için iki taraftan biri mutlaka kapatır.
    if stop.load(Ordering::Relaxed) {
        return Ok(());
    }

    loop {
        let msg: S2C = read_frame(&mut rd, buf)?;
        match msg {
            S2C::Welcome { proto, server } => {
                if proto != PROTO_VERSION {
                    return Err(Fail::Fatal(format!(
                        "protokol uyumsuz: sunucu v{proto}, istemci v{PROTO_VERSION}"
                    )));
                }
                *backoff = Duration::from_secs(1);
                on_event(Event::Connected { server });
                let t = topics.load(Ordering::Relaxed);
                if t != 0 {
                    send_on(writer, &C2S::Subscribe(t));
                }
            }
            S2C::Pong(_) => {} // sadece read timeout'u sıfırlar
            S2C::Error(code @ (ErrorCode::BadToken | ErrorCode::BadVersion)) => {
                return Err(Fail::Fatal(format!("sunucu reddetti: {code:?}")))
            }
            S2C::Error(code) => return Err(Fail::Retry(format!("sunucu hatası: {code:?}"))),
            other => on_event(Event::Msg(other)),
        }
    }
}

//! LowDeck kablo protokolü.
//!
//! - Taşıma: TCP, tek kalıcı bağlantı (istemci = telefon, sunucu = lowdeck-agent / mock-server).
//! - Çerçeve: `[u16 LE uzunluk][postcard payload]`, payload <= `MAX_FRAME`.
//! - Telefon, agent ve mock aynı crate'i kullanır; tipler tek kaynaktan gelir.
//!
//! Uyumluluk kuralı: enum varyantları yalnızca SONA eklenir (postcard varyant
//! indeksini kodlar). Kırıcı değişiklikte `PROTO_VERSION` artırılır.

use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};

/// v2: `Subscribe` / `Metrics` / `Top`. v3: olaylar (`TOPIC_EVENTS`, `Alarms`, `Event`, `QueryEvents`; ADR-0003).
/// v4: metrik geçmişi (`QueryHistory`, `History`; ADR-0005).
pub const PROTO_VERSION: u16 = 4;
pub const DEFAULT_PORT: u16 = 47810;
pub const MAX_FRAME: usize = 4096;

// ---------------- İstemci -> Sunucu ----------------

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum C2S {
    /// Bağlantıdan sonraki ilk mesaj. Token LAN içi basit paylaşılan anahtar.
    Hello { proto: u16, token: String, name: String },
    /// Ölü bağlantı tespiti; sunucu aynı sayıyla Pong döner.
    Ping(u32),
    Audio(AudioCmd),
    Media(MediaCmd),
    /// Abone olunan konular (`TOPIC_*` bitleri); 0 = hepsinden çık. Bağlantı başına durum.
    Subscribe(Topics),
    /// LowMonitor olay geçmişi `[from_ms, to_ms)`: bir ya da birkaç `S2C::Events`, boş `Events` = bitti.
    QueryEvents { from_ms: u64, to_ms: u64 },
    /// LowMonitor metrik geçmişi `[from_ms, to_ms)`, `points` kovaya bölünmüş (≤ `HIST_POINTS_MAX`, sunucu kırpar).
    /// Yanıt: bir ya da birkaç `S2C::History`, boş `points` = bitti.
    QueryHistory { from_ms: u64, to_ms: u64, points: u16 },
    // İleride (SONA eklenecek): Lock(LockCmd) ...
}

pub type Topics = u8;
/// LowMonitor metrikleri (`S2C::Metrics` + `S2C::Top`).
pub const TOPIC_METRICS: Topics = 1;
/// LowMonitor alarm/sistem olayları: önce `S2C::Alarms` (aktif liste), sonra her olay `S2C::Event`.
pub const TOPIC_EVENTS: Topics = 2;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum AudioCmd {
    /// Mutlak seviye 0..=100 (sunucu clamp eder).
    SetVolume(u8),
    /// Göreli adım, ör. +2 / -2.
    Step(i8),
    ToggleMute,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum MediaCmd {
    PlayPause,
    Next,
    Prev,
    Stop,
}

// ---------------- Sunucu -> İstemci ----------------

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum S2C {
    Welcome { proto: u16, server: String },
    Pong(u32),
    /// Durum değiştiğinde PUSH edilir (istemci polling yapmaz).
    Audio(AudioState),
    Media(MediaState),
    Error(ErrorCode),
    /// Abonelikte her örnekte (≈2 s). `None` = LowMonitor servisi yok / bağlanılamadı.
    Metrics(Option<Metrics>),
    /// Abonelikte Top-N üretildikçe (LowMonitor `topn_every`), CPU'ya göre azalan.
    Top(Vec<TopItem>),
    /// `TOPIC_EVENTS`: şu an Raised olan LowMonitor kural id'leri.
    Alarms(Vec<String>),
    /// `TOPIC_EVENTS`: yeni olay.
    Event(EventItem),
    /// `QueryEvents` yanıtı (≤ `EVENTS_PER_FRAME` öğe); boş = bitti.
    Events(Vec<EventItem>),
    /// `QueryHistory` yanıt parçası (≤ `HIST_PER_FRAME` kova), sırayla; boş `points` = bitti.
    /// Kova i: `[from_ms + i·step_ms, +step_ms)` (i, baştan itibaren tüm parçalarda sürer). `None` = veri yok.
    History { from_ms: u64, step_ms: u64, points: Vec<Option<HistPoint>> },
}

/// Bir sorgudaki en fazla kova (masaüstü grafiği genişliği).
pub const HIST_POINTS_MAX: u16 = 600;
/// Bir çerçevedeki en fazla kova; en kötü ~32 B/kova → `MAX_FRAME` aşılmaz (test).
pub const HIST_PER_FRAME: usize = 100;

/// Geçmiş kovası: `Metrics` birimleri, değerler kova ortalaması (`cpu_max` hariç).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Default)]
pub struct HistPoint {
    /// Toplam CPU, 0.01 %.
    pub cpu_avg: u16,
    /// Kovadaki en yüksek toplam CPU (dakika özetinde `Max`), 0.01 %.
    pub cpu_max: u16,
    pub ram_mb: u32,
    pub disk_r_kbps: u32,
    pub disk_w_kbps: u32,
    pub net_rx_kbps: u32,
    pub net_tx_kbps: u32,
}

/// Bir çerçevedeki en fazla olay; ad `EVENT_NAME_MAX`'ta kırpılır → `MAX_FRAME` aşılmaz.
pub const EVENTS_PER_FRAME: usize = 50;
pub const EVENT_NAME_MAX: usize = 32;

/// LowMonitor olayı (LM ADR-0028).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct EventItem {
    pub ts_ms: u64,
    /// 0 = AlarmRaise, 1 = AlarmClear, 2 = System.
    pub kind: u8,
    /// Kural id'si ya da sistem olayı adı.
    pub name: String,
    /// Alarmda değer × 100; sistem olayında koda göre.
    pub value: i64,
}

/// Kompakt temsil: postcard varint kodlar, küçük birim = az bayt.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Metrics {
    /// Toplam CPU, 0.01 % (10000 = %100).
    pub cpu: u16,
    /// Çekirdek başına, 0.5 % (200 = %100).
    pub cores: Vec<u8>,
    pub ram_used_mb: u32,
    pub ram_total_mb: u32,
    /// Tüm fiziksel diskler toplamı, KB/s (1 KB = 1024 B).
    pub disk_r_kbps: u32,
    pub disk_w_kbps: u32,
    /// Tüm arayüzler toplamı, KB/s.
    pub net_rx_kbps: u32,
    pub net_tx_kbps: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct TopItem {
    pub name: String,
    /// Sistem kapasitesinin 0.01 %'u.
    pub cpu: u16,
    /// Working set, MB.
    pub ws_mb: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct AudioState {
    pub volume: u8,
    pub muted: bool,
    pub device: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct MediaState {
    pub has_session: bool,
    pub playing: bool,
    pub app: String,
    pub title: String,
    pub artist: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum ErrorCode {
    BadVersion,
    BadToken,
    Unsupported,
    Internal,
}

// ---------------- Keşif (UDP, ADR-0007) ----------------

/// İstemci bu porta yayın (broadcast) sorgusu atar; sunucu tek UDP datagram ile yanıtlar.
/// TCP protokolünden bağımsız: `PROTO_VERSION` yanıtın içinde bilgi olarak gelir.
pub const DISCOVERY_PORT: u16 = 47811;
/// Sorgu: sabit 4 bayt (sürümlü sihirli sayı); başka içerik yanıtlanmaz.
pub const DISCOVER_REQ: [u8; 4] = *b"LDQ1";
const DISCOVER_RESP: [u8; 4] = *b"LDA1";
/// Yanıttaki metin alanları kırpılır: datagram küçük kalır (yansıtma kazancı ≤ ~25×, yalnız LAN).
pub const BEACON_TEXT_MAX: usize = 63;

/// Keşif yanıtı. **Token içermez**: bulunan PC'ye bağlanmak için token yine elle girilir.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Beacon {
    pub proto: u16,
    /// İstemcinin bağlanacağı adres (agent'ın `bind` IP'si).
    pub host: String,
    pub port: u16,
    /// PC adı (gösterim).
    pub name: String,
}

pub fn encode_beacon(b: &Beacon) -> Vec<u8> {
    let cut = |s: &str| s.chars().take(BEACON_TEXT_MAX).collect::<String>();
    let b = Beacon { host: cut(&b.host), name: cut(&b.name), ..b.clone() };
    let mut v = DISCOVER_RESP.to_vec();
    v.extend(postcard::to_allocvec(&b).unwrap_or_default());
    v
}

pub fn decode_beacon(datagram: &[u8]) -> Option<Beacon> {
    postcard::from_bytes(datagram.strip_prefix(&DISCOVER_RESP)?).ok()
}

// ---------------- Çerçeveleme ----------------

#[derive(Debug)]
pub enum FrameError {
    Io(io::Error),
    TooLarge(usize),
    Decode(postcard::Error),
    /// Karşı taraf çerçeve sınırında bağlantıyı kapattı (temiz kapanış).
    Closed,
}

impl From<io::Error> for FrameError {
    fn from(e: io::Error) -> Self {
        FrameError::Io(e)
    }
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FrameError::Io(e) => write!(f, "io: {e}"),
            FrameError::TooLarge(n) => write!(f, "frame too large: {n}"),
            FrameError::Decode(e) => write!(f, "decode: {e}"),
            FrameError::Closed => write!(f, "bağlantı kapandı"),
        }
    }
}

impl std::error::Error for FrameError {}

/// Tek çağrıda tek `write_all`: Nagle ile birlikte küçük paket sayısını azaltır.
pub fn write_frame<W: Write, T: Serialize>(w: &mut W, msg: &T) -> Result<(), FrameError> {
    let mut buf = [0u8; MAX_FRAME + 2];
    let n = postcard::to_slice(msg, &mut buf[2..])
        .map_err(FrameError::Decode)?
        .len();
    buf[..2].copy_from_slice(&(n as u16).to_le_bytes());
    w.write_all(&buf[..n + 2])?;
    w.flush()?;
    Ok(())
}

/// `buf` çağıran tarafından tekrar kullanılır: okuma döngüsünde alloc yok
/// (String alanlar hariç).
pub fn read_frame<R: Read, T: for<'de> Deserialize<'de>>(
    r: &mut R,
    buf: &mut [u8; MAX_FRAME],
) -> Result<T, FrameError> {
    let mut len = [0u8; 2];
    r.read_exact(&mut len).map_err(|e| match e.kind() {
        io::ErrorKind::UnexpectedEof => FrameError::Closed,
        _ => FrameError::Io(e),
    })?;
    let n = u16::from_le_bytes(len) as usize;
    if n > MAX_FRAME {
        return Err(FrameError::TooLarge(n));
    }
    r.read_exact(&mut buf[..n])?;
    postcard::from_bytes(&buf[..n]).map_err(FrameError::Decode)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let msgs = [
            C2S::Hello { proto: PROTO_VERSION, token: "t".into(), name: "toco".into() },
            C2S::Audio(AudioCmd::SetVolume(42)),
            C2S::Audio(AudioCmd::Step(-2)),
            C2S::Media(MediaCmd::Next),
            C2S::Ping(7),
            C2S::Subscribe(TOPIC_METRICS | TOPIC_EVENTS),
            C2S::QueryEvents { from_ms: 1, to_ms: u64::MAX },
        ];
        let mut wire = Vec::new();
        for m in &msgs {
            write_frame(&mut wire, m).unwrap();
        }
        let mut rd = &wire[..];
        let mut buf = [0u8; MAX_FRAME];
        for m in &msgs {
            let got: C2S = read_frame(&mut rd, &mut buf).unwrap();
            assert_eq!(&got, m);
        }
    }

    #[test]
    fn eof_at_boundary_is_closed() {
        let mut buf = [0u8; MAX_FRAME];
        let r: Result<C2S, _> = read_frame(&mut &[][..], &mut buf);
        assert!(matches!(r, Err(FrameError::Closed)));
        // Gövde ortasında kesilme gerçek hatadır.
        let r: Result<C2S, _> = read_frame(&mut &[5u8, 0, 1][..], &mut buf);
        assert!(matches!(r, Err(FrameError::Io(_))));
    }

    /// 16 çekirdek + Top-5 tek çerçeveye rahat sığar.
    #[test]
    fn metrics_fit_frame() {
        let m = S2C::Metrics(Some(Metrics {
            cpu: 10_000,
            cores: vec![200; 16],
            ram_used_mb: 32_000,
            ram_total_mb: 65_000,
            disk_r_kbps: u32::MAX,
            disk_w_kbps: u32::MAX,
            net_rx_kbps: u32::MAX,
            net_tx_kbps: u32::MAX,
        }));
        let t = S2C::Top(vec![TopItem { name: "x".repeat(32), cpu: 10_000, ws_mb: 100_000 }; 5]);
        let mut wire = Vec::new();
        write_frame(&mut wire, &m).unwrap();
        assert!(wire.len() < 64, "metrics {} bayt", wire.len());
        let mut rd = &wire[..];
        let mut buf = [0u8; MAX_FRAME];
        assert_eq!(read_frame::<_, S2C>(&mut rd, &mut buf).unwrap(), m);
        wire.clear();
        write_frame(&mut wire, &t).unwrap();
        assert!(wire.len() < 256, "top {} bayt", wire.len());
    }

    /// En kötü durum olay çerçevesi sınıra sığar.
    #[test]
    fn events_fit_frame() {
        let e = EventItem { ts_ms: u64::MAX, kind: 2, name: "x".repeat(EVENT_NAME_MAX), value: i64::MIN };
        let mut wire = Vec::new();
        write_frame(&mut wire, &S2C::Events(vec![e; EVENTS_PER_FRAME])).unwrap();
        assert!(wire.len() <= MAX_FRAME, "events {} bayt", wire.len());
    }

    /// En kötü durum geçmiş çerçevesi sınıra sığar.
    #[test]
    fn history_fits_frame() {
        let p = HistPoint {
            cpu_avg: u16::MAX,
            cpu_max: u16::MAX,
            ram_mb: u32::MAX,
            disk_r_kbps: u32::MAX,
            disk_w_kbps: u32::MAX,
            net_rx_kbps: u32::MAX,
            net_tx_kbps: u32::MAX,
        };
        let msg = S2C::History { from_ms: u64::MAX, step_ms: u64::MAX, points: vec![Some(p); HIST_PER_FRAME] };
        let mut wire = Vec::new();
        write_frame(&mut wire, &msg).unwrap();
        assert!(wire.len() <= MAX_FRAME, "history {} bayt", wire.len());
    }

    #[test]
    fn beacon_roundtrip_and_limits() {
        let b = Beacon { proto: PROTO_VERSION, host: "192.168.1.20".into(), port: DEFAULT_PORT, name: "x".repeat(500) };
        let wire = encode_beacon(&b);
        assert!(wire.len() <= 4 + 2 + 1 + 15 + 3 + 1 + BEACON_TEXT_MAX, "beacon {} bayt", wire.len());
        let d = decode_beacon(&wire).unwrap();
        assert_eq!((d.host.as_str(), d.port, d.name.len()), ("192.168.1.20", DEFAULT_PORT, BEACON_TEXT_MAX));
        assert_eq!(decode_beacon(&DISCOVER_REQ), None);
        assert_eq!(decode_beacon(b"LDA1"), None);
    }

    #[test]
    fn small_on_wire() {
        let mut wire = Vec::new();
        write_frame(&mut wire, &C2S::Media(MediaCmd::PlayPause)).unwrap();
        assert!(wire.len() <= 4, "komut çerçevesi {} bayt", wire.len());
    }
}

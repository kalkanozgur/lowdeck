//! LowMonitor pipe istemcisi (ADR-0002 "Metrikler", seçenek a).
//!
//! Telefon `Subscribe(TOPIC_METRICS)` dediğinde `\\.\pipe\LowMonitor`'a Viewer olarak
//! bağlanır, `Sample` + `Top`'u kompakt `lowdeck_proto` tiplerine çevirip push eder.
//! Abone yoksa pipe kapalı, thread `Want` koşul değişkeninde uyur (sıfır uyanma).
//! Servis yoksa abonelik sürdükçe 10 s'de bir yeniden dener (LowMonitor tray gibi).

use crate::Hub;
use lowdeck_proto::{EventItem, Metrics, TopItem, EVENT_NAME_MAX, S2C};
use lowmon_proto::metrics::EventKind;
use lowmon_proto::metrics::{Sample, TopEntry};
use std::sync::Arc;
use std::time::Duration;

/// LowMonitor örnek aralığı 1 s; telefona 2 s yeterli (yarı trafik).
const RATE_MS: u32 = 2000;
const RETRY: Duration = Duration::from_secs(10);
const TOP_N: usize = 5;
/// Ad kırpılır: çerçeve sınırı (4 KB) asla aşılmaz, telefonda zaten elide.
const NAME_MAX: usize = 32;

fn kbps(bytes_per_s: u64) -> u32 {
    (bytes_per_s / 1024).min(u32::MAX as u64) as u32
}

pub fn metrics_of(s: &Sample, ram_total_kb: u64) -> Metrics {
    Metrics {
        cpu: s.cpu_total,
        cores: s.cpu_core.clone(),
        ram_used_mb: (s.ram_used_kb / 1024) as u32,
        ram_total_mb: (ram_total_kb / 1024) as u32,
        disk_r_kbps: kbps(s.disk.iter().map(|d| d.r_bps).sum()),
        disk_w_kbps: kbps(s.disk.iter().map(|d| d.w_bps).sum()),
        net_rx_kbps: kbps(s.net.iter().map(|n| n.rx_bps).sum()),
        net_tx_kbps: kbps(s.net.iter().map(|n| n.tx_bps).sum()),
    }
}

/// LowMonitor olayı → kompakt öğe; ad kırpılır (çerçeve sınırı).
pub fn event_of(ts_ms: u64, kind: EventKind, name: &str, value: i64) -> EventItem {
    let kind = match kind {
        EventKind::AlarmRaise => 0,
        EventKind::AlarmClear => 1,
        EventKind::System => 2,
    };
    EventItem { ts_ms, kind, name: name.chars().take(EVENT_NAME_MAX).collect(), value }
}

/// CPU'ya göre azalan ilk `TOP_N`.
pub fn top_of(t: &[TopEntry]) -> Vec<TopItem> {
    let mut v: Vec<&TopEntry> = t.iter().collect();
    v.sort_by_key(|e| std::cmp::Reverse(e.cpu));
    v.iter()
        .take(TOP_N)
        .map(|e| TopItem {
            name: e.name.chars().take(NAME_MAX).collect(),
            cpu: e.cpu,
            ws_mb: (e.ws_kb / 1024) as u32,
        })
        .collect()
}

#[cfg(windows)]
pub fn start(hub: Arc<Hub>) {
    let _ = std::thread::Builder::new()
        .name("lowdeck-lowmon".into())
        .stack_size(64 * 1024)
        .spawn(move || run(&hub));
}

#[cfg(not(windows))]
pub fn start(_: Arc<Hub>) {}

#[cfg(windows)]
fn run(hub: &Hub) {
    loop {
        hub.want.wait_on();
        let r = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(lowmon_proto::proto::PIPE_NAME)
            .and_then(|f| session(f, hub));
        // Abonelik bittiyse sessizce bekle; sürüyorsa servis yok/koptu.
        if hub.want.get() {
            if let Err(e) = r {
                println!("lowmon: {e}");
            }
            hub.push_metrics(S2C::Metrics(None));
            hub.set_alarms(Vec::new()); // bilinmiyor; yeniden bağlanınca pipe `Alarms` gönderir
            hub.want.sleep_while_on(RETRY);
        }
    }
}

/// Abonelik bitince bir sonraki çerçevede (≤ RATE_MS) döner; `File` düşünce pipe kapanır.
#[cfg(windows)]
fn session(mut f: std::fs::File, hub: &Hub) -> std::io::Result<()> {
    use lowmon_proto::proto::{self, ClientMsg, Role, ServerMsg, CH_EVENT, CH_SAMPLE, CH_TOP, PROTO};
    proto::write(&mut f, &ClientMsg::Hello { proto: PROTO, role: Role::Viewer })?;
    proto::write(&mut f, &ClientMsg::Subscribe { channels: CH_SAMPLE | CH_TOP | CH_EVENT, rate_ms: RATE_MS })?;
    let total = ram_total_kb();
    while hub.want.get() {
        match proto::read::<ServerMsg>(&mut f)? {
            ServerMsg::Sample(s) => hub.push_metrics(S2C::Metrics(Some(metrics_of(&s, total)))),
            ServerMsg::Top(t) => hub.push_metrics(S2C::Top(top_of(&t))),
            ServerMsg::Alarms(a) => hub.set_alarms(a),
            ServerMsg::Event { ts_ms, kind, name, value } => hub.push_event(event_of(ts_ms, kind, &name, value)),
            ServerMsg::Error { code } => return Err(std::io::Error::other(format!("servis: {code:?}"))),
            _ => {} // Ack
        }
    }
    Ok(())
}

/// Pipe `QueryEvents` (LM `3241c3a`): ayrı kısa bağlantı, yalnız olaylar. Servis yok / hata → `Err`.
// shortcut: üst sınır yok; aylık olay sayısı küçük (alarm geçişi + dakikada ≤ 1 sistem olayı), gerekirse sınır eklenir
#[cfg(windows)]
pub fn query_events(from_ms: u64, to_ms: u64) -> std::io::Result<Vec<EventItem>> {
    use lowmon_proto::proto::{self, ClientMsg, RangeItem, Role, ServerMsg, PROTO};
    let mut f = std::fs::OpenOptions::new().read(true).write(true).open(lowmon_proto::proto::PIPE_NAME)?;
    proto::write(&mut f, &ClientMsg::Hello { proto: PROTO, role: Role::Viewer })?;
    proto::write(&mut f, &ClientMsg::QueryEvents { from_ms, to_ms })?;
    let (mut out, mut acks) = (Vec::new(), 0);
    loop {
        match proto::read::<ServerMsg>(&mut f)? {
            ServerMsg::Range(items) => out.extend(items.into_iter().filter_map(|it| match it {
                RangeItem::Event { ts_ms, kind, name, value } => Some(event_of(ts_ms, kind, &name, value)),
                _ => None,
            })),
            // İlk Ack = Hello yanıtı, ikincisi sorgu sonu
            ServerMsg::Ack => {
                acks += 1;
                if acks == 2 {
                    return Ok(out);
                }
            }
            ServerMsg::Error { code } => return Err(std::io::Error::other(format!("servis: {code:?}"))),
            _ => {}
        }
    }
}

/// Sample'da toplam yok; fiziksel RAM oturumda değişmez, bağlantı başına bir kez.
#[cfg(windows)]
fn ram_total_kb() -> u64 {
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut m = MEMORYSTATUSEX { dwLength: size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    // SAFETY: m yerel ve dwLength doğru; fonksiyon yalnız m'ye yazar.
    unsafe { GlobalMemoryStatusEx(&mut m) }.map_or(0, |_| m.ullTotalPhys / 1024)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lowmon_proto::metrics::{DiskIo, NetIo};

    #[test]
    fn convert() {
        let s = Sample {
            cpu_total: 1234,
            cpu_core: vec![10, 200],
            ram_used_kb: 8 * 1024 * 1024,
            disk: vec![DiskIo { drive: 0, r_bps: 1024, w_bps: 0 }, DiskIo { drive: 1, r_bps: 1024, w_bps: 4096 }],
            net: vec![NetIo { if_index: 3, rx_bps: 10 * 1024, tx_bps: 1023 }],
            ..Default::default()
        };
        let m = metrics_of(&s, 16 * 1024 * 1024);
        assert_eq!(
            m,
            Metrics {
                cpu: 1234,
                cores: vec![10, 200],
                ram_used_mb: 8192,
                ram_total_mb: 16384,
                disk_r_kbps: 2,
                disk_w_kbps: 4,
                net_rx_kbps: 10,
                net_tx_kbps: 0,
            }
        );
        let e = |name: &str, cpu| TopEntry { pid: 1, cpu, ws_kb: 2048, name: name.into() };
        let long = "x".repeat(100);
        let t = top_of(&[e("a", 1), e(&long, 9), e("c", 5), e("d", 0), e("e", 3), e("f", 2)]);
        assert_eq!(t.len(), TOP_N);
        assert_eq!((t[0].name.len(), t[0].cpu, t[0].ws_mb), (NAME_MAX, 9, 2));
        assert_eq!(t.iter().map(|i| i.cpu).collect::<Vec<_>>(), [9, 5, 3, 2, 1]);
    }

    #[test]
    fn event_kinds_and_name_limit() {
        let e = event_of(7, EventKind::AlarmClear, &"y".repeat(100), -3);
        assert_eq!((e.ts_ms, e.kind, e.name.len(), e.value), (7, 1, EVENT_NAME_MAX, -3));
        assert_eq!(event_of(0, EventKind::AlarmRaise, "a", 0).kind, 0);
        assert_eq!(event_of(0, EventKind::System, "a", 0).kind, 2);
    }
}

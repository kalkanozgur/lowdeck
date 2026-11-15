//! LowMonitor PC arayüzü (LM ADR-0029, LowDeck ADR-0003): Canlı + Olaylar.
//! Veri yalnız `lowdeck-agent` loopback'ten (127.0.0.1); token aynı kullanıcının `agent.cfg`'sinden.
//! Kullanım: `lowdeck-desktop [port token]` (argümanlar agent.cfg'yi ezer; mock-server için `47810 dev`).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

slint::include_modules!();

mod history;
#[cfg(windows)]
mod win;

use history::{nice_scale, series_path, History, Sample};
use lowdeck_client::{Client, Config, Event};
use lowdeck_proto::{
    EventItem, HistPoint, Metrics, TopItem, C2S, DEFAULT_PORT, HIST_POINTS_MAX, S2C, TOPIC_EVENTS, TOPIC_METRICS,
};
use slint::{Model, ModelRc, VecModel};
use std::rc::Rc;

const HISTORY_MS: u64 = 24 * 3_600_000;
/// Liste üst sınırı: eski olaylar sondan atılır (bellek sabit).
const EVENTS_MAX: usize = 1000;

fn main() -> Result<(), slint::PlatformError> {
    #[cfg(windows)]
    if !win::single_instance() {
        return Ok(());
    }
    #[cfg(windows)]
    win::register_ui_exe();

    let ui = MainWindow::new()?;
    let events = Rc::new(VecModel::<EventRow>::default());
    ui.set_events(ModelRc::from(events.clone()));
    let offset = local_offset_s();

    match target() {
        Some((port, token)) => start_client(&ui, events, port, token, offset),
        // İlk kurulum: cfg'yi agent oluşturur (ADR-0004 madde 4).
        None if start_agent() => {
            AGENT_TRIED.with(|t| t.set(true));
            ui.set_status("Agent başlatılıyor…".into());
            retry_cfg(ui.as_weak(), events, offset, 1);
        }
        None => ui.set_status("agent.cfg yok: LowDeck agent'ı bir kez çalıştırın".into()),
    }
    ui.run()
}

/// `agent.cfg` yeniden okuma denemesi sayısı (1 s aralık); sonra durur, periyodik iş kalmaz.
const CFG_RETRIES: u32 = 5;

/// Tek-atışlık timer zinciri: cfg gelince istemci başlar, `CFG_RETRIES` sonra vazgeçer.
fn retry_cfg(weak: slint::Weak<MainWindow>, events: Rc<VecModel<EventRow>>, offset: i64, n: u32) {
    slint::Timer::single_shot(std::time::Duration::from_secs(1), move || {
        let Some(ui) = weak.upgrade() else { return };
        match target() {
            Some((port, token)) => start_client(&ui, events, port, token, offset),
            None if n < CFG_RETRIES => retry_cfg(weak, events, offset, n + 1),
            None => ui.set_status("agent.cfg oluşmadı: %APPDATA%\\LowDeck\\agent.cfg".into()),
        }
    });
}

/// İstemciyi başlatır; `Rc<Client>` kancaların (küçültme, Yenile, bağlanınca geçmiş) closure'larında yaşar.
fn start_client(ui: &MainWindow, events: Rc<VecModel<EventRow>>, port: u16, token: String, offset: i64) {
    let weak = ui.as_weak();
    let client = Rc::new(Client::start(
        Config { host: "127.0.0.1".into(), port, token, name: "LowMonitor arayüzü".into() },
        move |ev| {
            let _ = weak.upgrade_in_event_loop(move |ui| apply(&ui, ev, offset));
        },
    ));
    client.subscribe(TOPIC_METRICS | TOPIC_EVENTS);

    // Küçültülünce metrik düşer (agent abone kalmazsa pipe'ı kapatır); olaylar sürer: alarm bandı + liste güncel.
    {
        use slint::winit_030::{winit::event::WindowEvent, EventResult, WinitWindowAccessor};
        let client = client.clone();
        ui.window().on_winit_window_event(move |_, ev| {
            // Windows'ta küçültme = 0×0 Resized
            if let WindowEvent::Resized(s) = ev {
                let minimized = s.width == 0 && s.height == 0;
                client.subscribe(if minimized { TOPIC_EVENTS } else { TOPIC_METRICS | TOPIC_EVENTS });
            }
            EventResult::Propagate
        });
    }

    // Geçmiş sekmesi: düğme / sekmeye geçiş → tek sorgu (ADR-0005); otomatik yenileme yok.
    {
        let (client, weak) = (client.clone(), ui.as_weak());
        ui.on_query_history(move |span| {
            let Some(ui) = weak.upgrade() else { return };
            let now = now_ms();
            let from_ms = now.saturating_sub(HIST_SPANS_MS[span.clamp(0, 3) as usize]);
            if client.send(&C2S::QueryHistory { from_ms, to_ms: now, points: HIST_POINTS_MAX }) {
                HQ.with(|h| {
                    let mut h = h.borrow_mut();
                    h.pending += 1;
                    h.pts.clear();
                });
                ui.set_hist_status("Yükleniyor…".into());
            } else {
                ui.set_hist_status("Agent'a bağlı değil".into());
            }
        });
    }

    let query = move || {
        events.set_vec(Vec::new());
        let now = now_ms();
        client.send(&C2S::QueryEvents { from_ms: now.saturating_sub(HISTORY_MS), to_ms: u64::MAX });
    };
    ui.on_refresh_events(query.clone());
    // Her bağlanışta geçmiş yeniden istenir: `apply` Connected'ı görünce bu kancayı çağırır.
    QUERY.with(|q| *q.borrow_mut() = Some(Box::new(query)));
}

/// Agent'ı başlatmayı oturum başına bir kez dene (ADR-0004 madde 3); Windows dışında yok.
fn start_agent() -> bool {
    #[cfg(windows)]
    return win::start_agent();
    #[cfg(not(windows))]
    false
}

/// Geçmiş aralıkları (UI düğme sırası): 1 sa, 6 sa, 24 sa, 7 g.
const HIST_SPANS_MS: [u64; 4] = [3_600_000, 6 * 3_600_000, 24 * 3_600_000, 7 * 24 * 3_600_000];

/// Geçmiş yanıt tamponu. `pending` = yanıtı bitmemiş sorgu sayısı: yalnız sonuncusunun parçaları tutulur
/// (öncekilerin parçaları `pending > 1` iken gelir, atılır).
#[derive(Default)]
struct HistRx {
    pending: u32,
    from_ms: u64,
    step_ms: u64,
    pts: Vec<Option<HistPoint>>,
    /// Son `Metrics`'ten; RAM grafiği ölçeği (yoksa geçmişteki max).
    ram_total_mb: u32,
}

thread_local! {
    static HQ: std::cell::RefCell<HistRx> = std::cell::RefCell::new(HistRx::default());
    /// Oturumda agent başlatma denendi mi (ilk kopmada bir kez).
    static AGENT_TRIED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static QUERY: std::cell::RefCell<Option<Box<dyn Fn()>>> = const { std::cell::RefCell::new(None) };
    /// Canlı grafik geçmişi + path buffer'ı (yalnız event loop thread'i).
    static HIST: std::cell::RefCell<(History, String)> = std::cell::RefCell::new((History::default(), String::new()));
}

/// (port, token): argümanlar, yoksa `%APPDATA%\LowDeck\agent.cfg`.
fn target() -> Option<(u16, String)> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    if let [port, token] = a.as_slice() {
        return Some((port.parse().ok()?, token.clone()));
    }
    let p = std::path::Path::new(&std::env::var_os("APPDATA")?).join("LowDeck").join("agent.cfg");
    parse_cfg(&std::fs::read_to_string(p).ok()?)
}

/// Agent'ın `key=value` dosyası; yalnız `port` ve `token` gerekir (`bind` LAN adresi, burada loopback).
fn parse_cfg(text: &str) -> Option<(u16, String)> {
    let (mut port, mut token) = (DEFAULT_PORT, None);
    for (k, v) in text.lines().filter_map(|l| l.split_once('=')) {
        match k.trim() {
            "port" => port = v.trim().parse().unwrap_or(DEFAULT_PORT),
            "token" => token = Some(v.trim().to_string()),
            _ => {}
        }
    }
    Some((port, token.filter(|t| !t.is_empty())?))
}

fn apply(ui: &MainWindow, ev: Event, offset: i64) {
    match ev {
        Event::Connecting => {
            ui.set_connected(false);
            ui.set_status("Bağlanıyor…".into());
        }
        Event::Connected { server } => {
            ui.set_connected(true);
            ui.set_status(format!("Bağlı: {server}").into());
            QUERY.with(|q| q.borrow().as_ref().map(|f| f()));
        }
        Event::Disconnected { reason } => {
            ui.set_connected(false);
            let starting = !AGENT_TRIED.with(|t| t.replace(true)) && start_agent();
            ui.set_status(if starting { "Agent başlatılıyor…".into() } else { format!("Agent yok: {reason}").into() });
            ui.set_lm_state(0);
            ui.set_alarms("".into());
            clear_graphs(ui);
            HQ.with(|h| h.borrow_mut().pending = 0); // yanıtlar kayboldu
            ui.set_hist_status("".into());
        }
        Event::Msg(S2C::Metrics(None)) => {
            ui.set_lm_state(2);
            clear_graphs(ui);
            set_vec(ui.get_lm_cores(), Vec::new(), |v| ui.set_lm_cores(v));
            set_vec(ui.get_lm_top(), Vec::new(), |v| ui.set_lm_top(v));
        }
        Event::Msg(S2C::Metrics(Some(m))) => show_metrics(ui, &m),
        Event::Msg(S2C::Top(t)) => set_vec(ui.get_lm_top(), t.iter().map(top_row).collect(), |v| ui.set_lm_top(v)),
        Event::Msg(S2C::Alarms(a)) => ui.set_alarms(a.join(", ").into()),
        Event::Msg(S2C::Event(e)) => {
            // Aktif liste agent'ta da tutulur; burada yalnız gösterim için güncellenir.
            let mut a: Vec<String> = ui.get_alarms().split(", ").filter(|s| !s.is_empty()).map(String::from).collect();
            match e.kind {
                0 if !a.contains(&e.name) => a.push(e.name.clone()),
                1 => a.retain(|x| *x != e.name),
                _ => {}
            }
            ui.set_alarms(a.join(", ").into());
            prepend(ui, &[e], offset);
        }
        // Parçalar eskiden yeniye gelir; her biri başa eklenince liste yeniden eskiye sıralı kalır.
        Event::Msg(S2C::Events(v)) => prepend(ui, &v, offset),
        Event::Msg(S2C::History { from_ms, step_ms, points }) => {
            let done = HQ.with(|h| {
                let mut h = h.borrow_mut();
                if h.pending == 0 {
                    return false;
                }
                if points.is_empty() {
                    h.pending -= 1;
                    return h.pending == 0;
                }
                if h.pending == 1 {
                    (h.from_ms, h.step_ms) = (from_ms, step_ms);
                    h.pts.extend(points);
                }
                false
            });
            if done {
                draw_history(ui, offset);
            }
        }
        Event::Msg(_) => {}
    }
}

/// Geçmiş sorgusu sürerken gelen canlı olay geçmişte de olabilir: baştaki satırlarda aynısı varsa eklenmez.
fn prepend(ui: &MainWindow, v: &[EventItem], offset: i64) {
    let model = ui.get_events();
    let Some(m) = model.as_any().downcast_ref::<VecModel<EventRow>>() else { return };
    for e in v {
        add(m, event_row(e, offset));
    }
    while m.row_count() > EVENTS_MAX {
        m.remove(m.row_count() - 1);
    }
}

/// Çift kontrolü için bakılan baş satır sayısı (sorgu süresince gelebilecek canlı olaylardan fazla).
const DEDUP_ROWS: usize = 50;

/// Başa ekler; aynı saniye + tür + ad baştaki satırlarda varsa (aynı olay) eklemez.
fn add(m: &VecModel<EventRow>, row: EventRow) {
    if !m.iter().take(DEDUP_ROWS).any(|r| r.time == row.time && r.kind == row.kind && r.name == row.name) {
        m.insert(0, row);
    }
}

fn event_row(e: &EventItem, offset: i64) -> EventRow {
    EventRow {
        time: fmt_time(e.ts_ms, offset).into(),
        kind: e.kind as i32,
        name: e.name.as_str().into(),
        value: e.value.to_string().into(),
    }
}

/// Tamamlanan geçmiş yanıtından 7 çizgi + ölçek etiketleri.
fn draw_history(ui: &MainWindow, offset: i64) {
    HQ.with(|h| {
        let h = h.borrow();
        let pts = &h.pts;
        let mut buf = String::new();
        let mut line = |f: &dyn Fn(&HistPoint) -> f32, set: &dyn Fn(&MainWindow, slint::SharedString)| {
            series_path(pts.iter().map(|p| p.as_ref().map(f)), &mut buf);
            set(ui, buf.as_str().into());
        };
        let max = |f: &dyn Fn(&HistPoint) -> u32| pts.iter().flatten().map(f).max().unwrap_or(0);
        line(&|p| p.cpu_avg as f32 / 10_000.0, &MainWindow::set_hist_cpu_avg);
        line(&|p| p.cpu_max as f32 / 10_000.0, &MainWindow::set_hist_cpu_max);
        let ram = if h.ram_total_mb > 0 { h.ram_total_mb } else { max(&|p| p.ram_mb).max(1) } as f32;
        line(&|p| p.ram_mb as f32 / ram, &MainWindow::set_hist_ram);
        let disk = nice_scale(max(&|p| p.disk_r_kbps.max(p.disk_w_kbps))) as f32;
        line(&|p| p.disk_r_kbps as f32 / disk, &MainWindow::set_hist_disk_r);
        line(&|p| p.disk_w_kbps as f32 / disk, &MainWindow::set_hist_disk_w);
        let net = nice_scale(max(&|p| p.net_rx_kbps.max(p.net_tx_kbps))) as f32;
        line(&|p| p.net_rx_kbps as f32 / net, &MainWindow::set_hist_net_rx);
        line(&|p| p.net_tx_kbps as f32 / net, &MainWindow::set_hist_net_tx);
        ui.set_hist_ram_scale(format!("{:.0} GB", ram / 1024.0).into());
        ui.set_hist_disk_scale(rate(disk as u32).into());
        ui.set_hist_net_scale(rate(net as u32).into());
        ui.set_hist_n(pts.len().max(1) as f32);
        let to_ms = h.from_ms + h.step_ms * pts.len() as u64;
        ui.set_hist_range(format!("{} – {}", fmt_time(h.from_ms, offset), fmt_time(to_ms, offset)).into());
        let empty = pts.iter().all(Option::is_none);
        ui.set_hist_status(if empty { "Veri yok (LowMonitor servisi?)" } else { "" }.into());
    });
}

fn show_metrics(ui: &MainWindow, m: &Metrics) {
    ui.set_lm_state(1);
    HQ.with(|h| h.borrow_mut().ram_total_mb = m.ram_total_mb);
    ui.set_lm_cpu(format!("{:.0}%", m.cpu as f32 / 100.0).into());
    ui.set_lm_ram(format!("{:.1} / {:.0} GB", m.ram_used_mb as f32 / 1024.0, m.ram_total_mb as f32 / 1024.0).into());
    ui.set_lm_disk(format!("R {}  W {}", rate(m.disk_r_kbps), rate(m.disk_w_kbps)).into());
    ui.set_lm_net(format!("↓ {}  ↑ {}", rate(m.net_rx_kbps), rate(m.net_tx_kbps)).into());
    let cores = m.cores.iter().map(|&c| c as f32 / 200.0).collect();
    set_vec(ui.get_lm_cores(), cores, |v| ui.set_lm_cores(v));
    graphs(ui, m, now_ms() / 1000);
}

/// Örneği geçmişe ekler, 6 çizgi + 2 ölçek etiketini günceller (Metrics başına bir kez, ≈ 1 Hz).
fn graphs(ui: &MainWindow, m: &Metrics, now_s: u64) {
    HIST.with(|h| {
        let (h, buf) = &mut *h.borrow_mut();
        h.push(Sample {
            ts_s: now_s,
            cpu: m.cpu,
            ram: m.ram_used_mb as f32 / m.ram_total_mb.max(1) as f32,
            dr: m.disk_r_kbps,
            dw: m.disk_w_kbps,
            rx: m.net_rx_kbps,
            tx: m.net_tx_kbps,
        });
        let mut line = |f: &dyn Fn(&Sample) -> f32, set: &dyn Fn(slint::SharedString)| {
            h.path(now_s, f, buf);
            set(buf.as_str().into());
        };
        line(&|s| s.cpu as f32 / 10_000.0, &|v| ui.set_lm_cpu_path(v));
        line(&|s| s.ram, &|v| ui.set_lm_ram_path(v));
        // Disk/ağ: iki çizgi ortak ölçekte (penceredeki max, 1-2-5 yuvarlama).
        let disk = nice_scale(h.max(now_s, |s| s.dr.max(s.dw))) as f32;
        line(&|s| s.dr as f32 / disk, &|v| ui.set_lm_disk_r_path(v));
        line(&|s| s.dw as f32 / disk, &|v| ui.set_lm_disk_w_path(v));
        let net = nice_scale(h.max(now_s, |s| s.rx.max(s.tx))) as f32;
        line(&|s| s.rx as f32 / net, &|v| ui.set_lm_net_rx_path(v));
        line(&|s| s.tx as f32 / net, &|v| ui.set_lm_net_tx_path(v));
        ui.set_lm_disk_scale(rate(disk as u32).into());
        ui.set_lm_net_scale(rate(net as u32).into());
    });
}

fn clear_graphs(ui: &MainWindow) {
    HIST.with(|h| h.borrow_mut().0.clear());
    for set in [
        MainWindow::set_lm_cpu_path,
        MainWindow::set_lm_ram_path,
        MainWindow::set_lm_disk_r_path,
        MainWindow::set_lm_disk_w_path,
        MainWindow::set_lm_net_rx_path,
        MainWindow::set_lm_net_tx_path,
    ] {
        set(ui, "".into());
    }
}

fn top_row(t: &TopItem) -> TopRow {
    TopRow {
        name: t.name.as_str().into(),
        cpu: format!("{:.1}%", t.cpu as f32 / 100.0).into(),
        ws: format!("{} MB", t.ws_mb).into(),
    }
}

fn rate(kbps: u32) -> String {
    match kbps {
        0..1024 => format!("{kbps}K"),
        1024..1_048_576 => format!("{:.1}M", kbps as f32 / 1024.0),
        _ => format!("{:.1}G", kbps as f32 / 1_048_576.0),
    }
}

fn set_vec<T: Clone + 'static>(cur: ModelRc<T>, v: Vec<T>, set: impl FnOnce(ModelRc<T>)) {
    match cur.as_any().downcast_ref::<VecModel<T>>() {
        Some(m) => m.set_vec(v),
        None => set(ModelRc::new(VecModel::from(v))),
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

// shortcut: yerel saat farkı açılışta bir kez alınır; yaz saatine geçişte uygulama yeniden açılana kadar 1 saat kayar
fn local_offset_s() -> i64 {
    #[cfg(windows)]
    return win::utc_offset_s();
    #[cfg(not(windows))]
    0
}

/// `YYYY-MM-DD HH:MM:SS` (yerel). Gün → takvim: H. Hinnant `civil_from_days`.
fn fmt_time(ts_ms: u64, offset_s: i64) -> String {
    let s = ts_ms as i64 / 1000 + offset_s;
    let (days, sod) = (s.div_euclid(86_400), s.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}", sod / 3600, sod % 3600 / 60, sod % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_format() {
        assert_eq!(fmt_time(0, 0), "1970-01-01 00:00:00");
        // 2025-11-15 13:40:05 UTC, TSİ +3
        assert_eq!(fmt_time(1_763_214_005_000, 3 * 3600), "2025-11-15 16:40:05");
        assert_eq!(fmt_time(951_782_400_000, 0), "2000-02-29 00:00:00");
    }

    #[test]
    fn dedup() {
        let m = VecModel::default();
        let e = |ts, kind| event_row(&EventItem { ts_ms: ts, kind, name: "cpu".into(), value: 1 }, 0);
        add(&m, e(1_000, 0)); // canlı
        add(&m, e(500, 2));
        add(&m, e(1_400, 0)); // geçmişten aynı olay (aynı saniye)
        add(&m, e(1_000, 1)); // farklı tür
        assert_eq!(m.row_count(), 3);
    }

    #[test]
    fn cfg() {
        assert_eq!(parse_cfg("bind=192.168.1.2\nport=5000\ntoken=abc\n"), Some((5000, "abc".into())));
        assert_eq!(parse_cfg("token= t \n"), Some((DEFAULT_PORT, "t".into())));
        assert_eq!(parse_cfg("port=1\ntoken=\n"), None);
        assert_eq!(parse_cfg(""), None);
    }
}

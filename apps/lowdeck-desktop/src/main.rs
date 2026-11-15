//! LowMonitor PC arayüzü (LM ADR-0029, LowDeck ADR-0003): Canlı + Olaylar.
//! Veri yalnız `lowdeck-agent` loopback'ten (127.0.0.1); token aynı kullanıcının `agent.cfg`'sinden.
//! Kullanım: `lowdeck-desktop [port token]` (argümanlar agent.cfg'yi ezer; mock-server için `47810 dev`).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

slint::include_modules!();

#[cfg(windows)]
mod win;

use lowdeck_client::{Client, Config, Event};
use lowdeck_proto::{EventItem, Metrics, TopItem, C2S, DEFAULT_PORT, S2C, TOPIC_EVENTS, TOPIC_METRICS};
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

    let Some((port, token)) = target() else {
        ui.set_status("agent.cfg yok: LowDeck agent'ı bir kez çalıştırın".into());
        return ui.run();
    };
    let offset = local_offset_s();
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

    let query = {
        let (client, events) = (client.clone(), events.clone());
        move || {
            events.set_vec(Vec::new());
            let now = now_ms();
            client.send(&C2S::QueryEvents { from_ms: now.saturating_sub(HISTORY_MS), to_ms: u64::MAX });
        }
    };
    ui.on_refresh_events(query.clone());
    // Her bağlanışta geçmiş yeniden istenir: `apply` Connected'ı görünce bu kancayı çağırır.
    QUERY.with(|q| *q.borrow_mut() = Some(Box::new(query)));
    ui.run()
}

thread_local! {
    static QUERY: std::cell::RefCell<Option<Box<dyn Fn()>>> = const { std::cell::RefCell::new(None) };
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
            ui.set_status(format!("Agent yok: {reason}").into());
            ui.set_lm_state(0);
            ui.set_alarms("".into());
        }
        Event::Msg(S2C::Metrics(None)) => {
            ui.set_lm_state(2);
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

fn show_metrics(ui: &MainWindow, m: &Metrics) {
    ui.set_lm_state(1);
    ui.set_lm_cpu(format!("{:.0}%", m.cpu as f32 / 100.0).into());
    ui.set_lm_ram(format!("{:.1} / {:.0} GB", m.ram_used_mb as f32 / 1024.0, m.ram_total_mb as f32 / 1024.0).into());
    ui.set_lm_disk(format!("R {}  W {}", rate(m.disk_r_kbps), rate(m.disk_w_kbps)).into());
    ui.set_lm_net(format!("↓ {}  ↑ {}", rate(m.net_rx_kbps), rate(m.net_tx_kbps)).into());
    let cores = m.cores.iter().map(|&c| c as f32 / 200.0).collect();
    set_vec(ui.get_lm_cores(), cores, |v| ui.set_lm_cores(v));
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

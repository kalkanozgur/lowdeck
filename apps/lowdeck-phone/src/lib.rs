//! LowDeck telefon uygulaması. Giriş noktaları:
//! - Android: `android_main` (cdylib)
//! - Masaüstü: `src/main.rs` -> `run()`

slint::include_modules!();

mod config;
mod discover;

use config::AppConfig;
use lowdeck_client::{Client, Config, Event};
use lowdeck_proto::{
    AiUsage, AiWindow, AudioCmd, MediaCmd, Metrics, Topics, TopItem, PROTO_VERSION, S2C, TOPIC_AI, TOPIC_METRICS,
};
use slint::{Model, ModelRc, VecModel};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Slider sürüklenirken en fazla bu sıklıkta SetVolume gönderilir (~20 Hz).
const VOLUME_THROTTLE: Duration = Duration::from_millis(50);
/// Yerel değişiklikten sonra sunucudan gelen ses değeri bu süre yok sayılır
/// (slider'ın geri zıplamasını önler).
const LOCAL_ECHO_GUARD: Duration = Duration::from_millis(400);

/// Bu süre dokunulmazsa siyah perde iner.
const BLACKOUT_AFTER: Duration = Duration::from_secs(120);

/// Ekran açıkken abone olunan konular: PC metrikleri + AI kullanımı (LM ADR-0032).
const TOPICS: Topics = TOPIC_METRICS | TOPIC_AI;

/// Yaşam döngüsü kancası (true = öne geldi). `run` kurar, Android olay dinleyicisi çağırır.
type ActiveHook = Box<dyn Fn(bool)>;

thread_local! {
    static ON_ACTIVE: RefCell<Option<ActiveHook>> = const { RefCell::new(None) };
    /// Son AI durumu: geri sayım her metrik push'unda (≈2 s) telefonun saatiyle yenilenir.
    static LAST_AI: RefCell<Option<AiUsage>> = const { RefCell::new(None) };
}

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: slint::android::AndroidApp) {
    use slint::android::android_activity::{MainEvent, PollEvent, WindowManagerFlags};
    let data_dir = app.internal_data_path();
    // Önde iken ekran açık; boşta siyah perde UI'da. JNI gerekmez.
    app.set_window_flags(WindowManagerFlags::KEEP_SCREEN_ON, WindowManagerFlags::empty());
    slint::android::init_with_event_listener(app, |ev| {
        let active = match ev {
            PollEvent::Main(MainEvent::Pause) => false,
            PollEvent::Main(MainEvent::Resume { .. }) => true,
            _ => return,
        };
        ON_ACTIVE.with(|h| {
            if let Some(f) = h.borrow().as_ref() {
                f(active)
            }
        });
    })
    .expect("slint android init");
    run(data_dir, "toco").expect("run");
}

pub fn run(data_dir: Option<PathBuf>, device_name: &str) -> Result<(), slint::PlatformError> {
    let ui = MainWindow::new()?;
    let cfg_path = data_dir.map(|d| d.join("lowdeck.cfg"));
    let cfg = config::load(cfg_path.as_deref());

    ui.set_cfg_host(cfg.host.clone().into());
    ui.set_cfg_port(cfg.port.to_string().into());
    ui.set_cfg_token(cfg.token.clone().into());

    let client: Rc<RefCell<Option<Client>>> = Rc::new(RefCell::new(None));
    let current = Rc::new(RefCell::new(cfg));
    // Her (yeniden) bağlanma/duraklatmada artar; eski client'ın kuyruktaki olayları atılır.
    let generation = Arc::new(AtomicU32::new(0));
    let last_local_edit = Arc::new(Mutex::new(Instant::now() - LOCAL_ECHO_GUARD));
    let name = device_name.to_string();

    // (Yeniden) bağlan: eski Client drop edilince thread'leri durur.
    let connect = {
        let client = client.clone();
        let weak = ui.as_weak();
        let last_local_edit = last_local_edit.clone();
        let generation = generation.clone();
        move |c: &AppConfig| {
            client.borrow_mut().take();
            let my_gen = generation.fetch_add(1, Ordering::Relaxed) + 1;
            let Some(ui) = weak.upgrade() else { return };
            if c.host.is_empty() {
                ui.set_status("PC adresi ayarlanmadı".into());
                ui.set_settings_open(true);
                return;
            }
            let weak = weak.clone();
            let guard = last_local_edit.clone();
            let generation = generation.clone();
            let c = Client::start(
                Config { host: c.host.clone(), port: c.port, token: c.token.clone(), name: name.clone() },
                move |ev| {
                    let guard = guard.clone();
                    let generation = generation.clone();
                    let _ = weak.upgrade_in_event_loop(move |ui| {
                        if generation.load(Ordering::Relaxed) == my_gen {
                            apply(&ui, ev, &guard);
                        }
                    });
                },
            );
            // Perde inikken metrik yok: ekranda görünmeyen veri için PC de telefon da uyanmasın.
            if !ui.get_blackout() {
                c.subscribe(TOPICS);
            }
            *client.borrow_mut() = Some(c);
        }
    };
    let connect = Rc::new(connect);
    connect(&current.borrow());

    // --- Boşta siyah perde: tek-atışlık timer, her dokunuşta yeniden kurulur ---
    let idle = Rc::new(slint::Timer::default());
    {
        let weak = ui.as_weak();
        let client = client.clone();
        idle.start(slint::TimerMode::SingleShot, BLACKOUT_AFTER, move || {
            if let Some(ui) = weak.upgrade() {
                ui.set_blackout(true);
                ui.set_lm_state(0);
                ui.set_ai_state(0);
            }
            if let Some(c) = client.borrow().as_ref() {
                c.subscribe(0);
            }
        });
    }
    let poke = {
        let idle = idle.clone();
        let weak = ui.as_weak();
        let client = client.clone();
        move || {
            idle.restart();
            if let Some(ui) = weak.upgrade() {
                ui.set_blackout(false);
            }
            if let Some(c) = client.borrow().as_ref() {
                c.subscribe(TOPICS); // değişmediyse gönderilmez
            }
        }
    };
    ui.global::<Activity>().on_poke(poke.clone());

    // --- Yaşam döngüsü: arka planda bağlantı yok (sıfır ağ trafiği) ---
    {
        let client = client.clone();
        let connect = connect.clone();
        let current = current.clone();
        let generation = generation.clone();
        let weak = ui.as_weak();
        ON_ACTIVE.with(|h| {
            *h.borrow_mut() = Some(Box::new(move |active| {
                if active {
                    poke();
                    if client.borrow().is_none() {
                        connect(&current.borrow());
                    }
                } else {
                    client.borrow_mut().take();
                    generation.fetch_add(1, Ordering::Relaxed);
                    if let Some(ui) = weak.upgrade() {
                        ui.set_connected(false);
                        ui.set_status("Duraklatıldı".into());
                    }
                }
            }))
        });
    }

    // --- Ses: throttle'lı mutlak seviye ---
    {
        let client = client.clone();
        let pending: Rc<Cell<Option<u8>>> = Rc::new(Cell::new(None));
        let last_sent = Rc::new(Cell::new(Instant::now() - VOLUME_THROTTLE));
        let flush_scheduled = Rc::new(Cell::new(false));
        let last_local_edit = last_local_edit.clone();

        ui.on_volume_changed(move |v| {
            *last_local_edit.lock().unwrap() = Instant::now();
            pending.set(Some(v.round().clamp(0.0, 100.0) as u8));

            let flush = {
                let client = client.clone();
                let pending = pending.clone();
                let last_sent = last_sent.clone();
                move || {
                    if let (Some(level), Some(c)) = (pending.take(), client.borrow().as_ref()) {
                        c.audio(AudioCmd::SetVolume(level));
                        last_sent.set(Instant::now());
                    }
                }
            };

            let since = last_sent.get().elapsed();
            if since >= VOLUME_THROTTLE {
                flush();
            } else if !flush_scheduled.get() {
                // Son değer kaybolmasın: kalan süre sonunda tek seferlik gönder.
                flush_scheduled.set(true);
                let flag = flush_scheduled.clone();
                slint::Timer::single_shot(VOLUME_THROTTLE - since, move || {
                    flag.set(false);
                    flush();
                });
            }
        });
    }
    {
        let client = client.clone();
        ui.on_volume_step(move |d| {
            if let Some(c) = client.borrow().as_ref() {
                c.audio(AudioCmd::Step(d.clamp(-100, 100) as i8));
            }
        });
    }
    {
        let client = client.clone();
        ui.on_toggle_mute(move || {
            if let Some(c) = client.borrow().as_ref() {
                c.audio(AudioCmd::ToggleMute);
            }
        });
    }

    // --- Medya ---
    {
        let client = client.clone();
        ui.on_media(move |i| {
            let cmd = match i {
                0 => MediaCmd::Prev,
                1 => MediaCmd::PlayPause,
                2 => MediaCmd::Next,
                _ => MediaCmd::Stop,
            };
            if let Some(c) = client.borrow().as_ref() {
                c.media(cmd);
            }
        });
    }

    // --- Ayarlar ---
    {
        let weak = ui.as_weak();
        let connect = connect.clone();
        let current = current.clone();
        ui.on_save_settings(move || {
            let Some(ui) = weak.upgrade() else { return false };
            match config::parse(&ui.get_cfg_host(), &ui.get_cfg_port(), &ui.get_cfg_token()) {
                Ok(c) => {
                    ui.set_cfg_error("".into());
                    config::save(cfg_path.as_deref(), &c);
                    connect(&c);
                    *current.borrow_mut() = c;
                    true
                }
                Err(e) => {
                    ui.set_cfg_error(e.into());
                    false
                }
            }
        });
    }

    // PC keşfi: ayrı thread'de ~1,5 s; sonuç listesine dokununca host/port dolar (token elle).
    {
        let weak = ui.as_weak();
        ui.on_discover(move || {
            let Some(ui) = weak.upgrade() else { return };
            ui.set_discover_status("Aranıyor…".into());
            set_vec(ui.get_found(), Vec::new(), |v| ui.set_found(v));
            let weak = weak.clone();
            std::thread::spawn(move || {
                let found = discover::discover(discover::WAIT);
                let _ = weak.upgrade_in_event_loop(move |ui| {
                    let rows: Vec<FoundPc> = found
                        .iter()
                        .map(|b| FoundPc {
                            label: format!(
                                "{}  ·  {}:{}{}",
                                b.name,
                                b.host,
                                b.port,
                                if b.proto != PROTO_VERSION { "  (sürüm farklı)" } else { "" }
                            )
                            .into(),
                            host: b.host.as_str().into(),
                            port: b.port.to_string().into(),
                        })
                        .collect();
                    ui.set_discover_status(match rows.len() {
                        0 => "Bulunamadı: aynı Wi-Fi'da mı, agent açık mı, firewall kuralı var mı?".into(),
                        n => format!("{n} PC bulundu, seç; token agent tray menüsünde").into(),
                    });
                    set_vec(ui.get_found(), rows, |v| ui.set_found(v));
                });
            });
        });
    }

    let r = ui.run();
    // Kanca ui/client'ı tutuyor; event loop bitince bırak (Android'de android_main tekrar çağrılabilir).
    ON_ACTIVE.with(|h| h.borrow_mut().take());
    r
}

/// Ağ olayını UI'a uygular (event loop thread'inde çalışır).
fn apply(ui: &MainWindow, ev: Event, last_local_edit: &Mutex<Instant>) {
    match ev {
        Event::Connecting => {
            ui.set_connected(false);
            ui.set_status("Bağlanıyor…".into());
        }
        Event::Connected { server } => {
            ui.set_connected(true);
            ui.set_status(format!("Bağlı: {server}").into());
        }
        Event::Disconnected { reason } => {
            ui.set_connected(false);
            ui.set_status(format!("Koptu: {reason}").into());
            ui.set_lm_state(0);
            ui.set_ai_state(0);
        }
        Event::Msg(S2C::Audio(a)) => {
            if last_local_edit.lock().unwrap().elapsed() >= LOCAL_ECHO_GUARD {
                ui.set_volume(a.volume as f32);
            }
            ui.set_muted(a.muted);
            ui.set_device(a.device.into());
        }
        Event::Msg(S2C::Media(m)) => {
            ui.set_has_session(m.has_session);
            ui.set_playing(m.playing);
            ui.set_media_app(m.app.into());
            ui.set_track_title(m.title.into());
            ui.set_track_artist(m.artist.into());
        }
        Event::Msg(S2C::Metrics(None)) => {
            ui.set_lm_state(2);
            set_vec(ui.get_lm_cores(), Vec::new(), |v| ui.set_lm_cores(v));
            set_vec(ui.get_lm_top(), Vec::new(), |v| ui.set_lm_top(v));
        }
        Event::Msg(S2C::Metrics(Some(m))) => {
            show_metrics(ui, &m);
            LAST_AI.with_borrow(|a| show_ai(ui, a.as_ref(), unix_now()));
        }
        Event::Msg(S2C::Ai(a)) => {
            show_ai(ui, a.as_ref(), unix_now());
            LAST_AI.set(a);
        }
        Event::Msg(S2C::Top(t)) => {
            let rows = t.iter().map(top_row).collect();
            set_vec(ui.get_lm_top(), rows, |v| ui.set_lm_top(v));
        }
        Event::Msg(_) => {}
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

fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// AI bölümü; `None` = PC'de veri yok.
fn show_ai(ui: &MainWindow, a: Option<&AiUsage>, now: u64) {
    let Some(a) = a else {
        ui.set_ai_state(2);
        return;
    };
    ui.set_ai_state(1);
    // 3 = soluk (eski veri), aksi halde PC'nin seviyesi
    let level = |w: &Option<AiWindow>| if a.stale { 3 } else { w.map_or(0, |w| w.level.min(2) as i32) };
    ui.set_ai_session(ai_line("Oturum", &a.session, now).into());
    ui.set_ai_week(ai_line("Hafta", &a.week, now).into());
    ui.set_ai_session_level(level(&a.session));
    ui.set_ai_week_level(level(&a.week));
    ui.set_ai_note(ai_note(a, now).into());
}

/// "Oturum %33 · 4s16dk" (LM taskbar ile aynı biçim).
fn ai_line(label: &str, w: &Option<AiWindow>, now: u64) -> String {
    match w {
        None => format!("{label} —"),
        Some(w) => {
            let left = if w.resets_at == 0 { "—".into() } else { remaining(w.resets_at as i64 - now as i64) };
            format!("{label} %{} · {left}", (w.pct + 5) / 10)
        }
    }
}

/// Sağ üst not: hata > eski > kaynak.
fn ai_note(a: &AiUsage, now: u64) -> String {
    if !a.error.is_empty() {
        a.error.clone()
    } else if a.stale {
        format!("eski · {} dk önce", now.saturating_sub(a.fetched_at) / 60)
    } else {
        a.source.clone()
    }
}

/// Geri sayım: "2g05s", "3s07dk", "12dk"; negatif → 0.
fn remaining(secs: i64) -> String {
    let s = secs.max(0);
    let (d, h, m) = (s / 86_400, s % 86_400 / 3600, s % 3600 / 60);
    if d > 0 {
        format!("{d}g{h:02}s")
    } else if h > 0 {
        format!("{h}s{m:02}dk")
    } else {
        format!("{m}dk")
    }
}

fn top_row(t: &TopItem) -> TopRow {
    TopRow {
        name: t.name.as_str().into(),
        cpu: format!("{:.1}%", t.cpu as f32 / 100.0).into(),
        ws: format!("{} MB", t.ws_mb).into(),
    }
}

/// Kısa hız: birim etiketi "B/s" UI'da; dar ekranda iki değer yan yana sığsın.
fn rate(kbps: u32) -> String {
    match kbps {
        0..1024 => format!("{kbps}K"),
        1024..1_048_576 => format!("{:.1}M", kbps as f32 / 1024.0),
        _ => format!("{:.1}G", kbps as f32 / 1_048_576.0),
    }
}

/// Mevcut VecModel'i yeniden kullanır: repeater öğeleri her örnekte yeniden yaratılmaz.
fn set_vec<T: Clone + 'static>(cur: ModelRc<T>, v: Vec<T>, set: impl FnOnce(ModelRc<T>)) {
    match cur.as_any().downcast_ref::<VecModel<T>>() {
        Some(m) => m.set_vec(v),
        None => set(ModelRc::new(VecModel::from(v))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ai_text() {
        let w = Some(AiWindow { pct: 334, resets_at: 1_000 + 4 * 3600 + 16 * 60, level: 1 });
        assert_eq!(ai_line("Oturum", &w, 1_000), "Oturum %33 · 4s16dk");
        assert_eq!(ai_line("Hafta", &None, 0), "Hafta —");
        let w = Some(AiWindow { pct: 1000, resets_at: 0, level: 2 });
        assert_eq!(ai_line("Hafta", &w, 0), "Hafta %100 · —");
        assert_eq!(remaining(2 * 86_400 + 5 * 3600), "2g05s");
        assert_eq!(remaining(-5), "0dk");
        let mut a = AiUsage { fetched_at: 100, source: "OAuth".into(), ..Default::default() };
        assert_eq!(ai_note(&a, 200), "OAuth");
        a.stale = true;
        assert_eq!(ai_note(&a, 100 + 720), "eski · 12 dk önce");
        a.error = "OAuth: token süresi dolmuş".into();
        assert_eq!(ai_note(&a, 0), "OAuth: token süresi dolmuş");
    }

    #[test]
    fn rate_units() {
        assert_eq!(super::rate(0), "0K");
        assert_eq!(super::rate(1023), "1023K");
        assert_eq!(super::rate(1536), "1.5M");
        assert_eq!(super::rate(3 * 1_048_576), "3.0G");
    }
}

//! LowDeck telefon uygulaması. Giriş noktaları:
//! - Android: `android_main` (cdylib)
//! - Masaüstü: `src/main.rs` -> `run()`

slint::include_modules!();

mod config;

use config::AppConfig;
use lowdeck_client::{Client, Config, Event};
use lowdeck_proto::{AudioCmd, MediaCmd, S2C};
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

/// Yaşam döngüsü kancası (true = öne geldi). `run` kurar, Android olay dinleyicisi çağırır.
type ActiveHook = Box<dyn Fn(bool)>;

thread_local! {
    static ON_ACTIVE: RefCell<Option<ActiveHook>> = const { RefCell::new(None) };
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
            *client.borrow_mut() = Some(Client::start(
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
            ));
        }
    };
    let connect = Rc::new(connect);
    connect(&current.borrow());

    // --- Boşta siyah perde: tek-atışlık timer, her dokunuşta yeniden kurulur ---
    let idle = Rc::new(slint::Timer::default());
    {
        let weak = ui.as_weak();
        idle.start(slint::TimerMode::SingleShot, BLACKOUT_AFTER, move || {
            if let Some(ui) = weak.upgrade() {
                ui.set_blackout(true);
            }
        });
    }
    let poke = {
        let idle = idle.clone();
        let weak = ui.as_weak();
        move || {
            idle.restart();
            if let Some(ui) = weak.upgrade() {
                ui.set_blackout(false);
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
        Event::Msg(_) => {}
    }
}

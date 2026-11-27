//! LowDeck telefon uygulaması. Giriş noktaları:
//! - Android: `android_main` (cdylib)
//! - Masaüstü: `src/main.rs` -> `run()`

slint::include_modules!();

mod config;
mod discover;
mod events;
mod graph;
mod notify;
mod tabs;

use config::AppConfig;
use graph::{nice_scale, series_path, History, Sample};
use lowdeck_client::{Client, Config, Event};
use lowdeck_proto::{
    AiConfig, AiUsage, AiWindow, AudioCmd, Beacon, EventItem, HistPoint, MediaCmd, Metrics, PowerCmd, ProcItem, Sensors,
    TempPoint, TopItem, Topics, C2S, HIST_POINTS_MAX, PROTO_VERSION, S2C, TOPIC_AI, TOPIC_EVENTS,
    TOPIC_METRICS, TOPIC_PROCS, TOPIC_SENSORS,
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
/// Güç komutu yanıtı (LAN'da < 50 ms beklenir); gelmezse bağlantı ölü sayılır (ADR-0012).
const POWER_REPLY_TIMEOUT: Duration = Duration::from_secs(3);

/// Bu süre dokunulmazsa siyah perde iner.
const BLACKOUT_AFTER: Duration = Duration::from_secs(120);
/// Perde inikken yalnız seyrek push'lar: alarm kenarı (PC eşikleri, sıcaklık dahil) + AI değişimi.
/// Uyandırma kararı bunlardan; metrik/sensör akışı yok.
const DARK_TOPICS: Topics = TOPIC_EVENTS | TOPIC_AI;
/// Perde inikken PC meşgulse (`busy`) bu aralıkla `PEEK_FOR` kadar açılır.
const PEEK_EVERY: Duration = Duration::from_secs(600);
const PEEK_FOR: Duration = Duration::from_secs(15);
/// Üst üste bu kadar kopuştan sonra (geri çekilme 1+2+4 s ≈ 7 s) keşif; sonra her 5 kopuşta bir (≈ 2,5 dk).
const REDISCOVER_AFTER: u32 = 3;

/// Şu an istenen konular: arka planda yalnız olaylar (ADR-0011), perdede seyrek push'lar, yoksa sekme.
fn wanted(ui: &MainWindow) -> Topics {
    if BG.get() {
        TOPIC_EVENTS
    } else if ui.get_blackout() {
        DARK_TOPICS
    } else {
        topics(ui.get_tab())
    }
}

/// Ekran açıkken sekmeye göre konular: görünmeyen veri için PC de telefon da uyanmasın.
/// Olaylar (alarm bandı + Olaylar listesi) her sekmede; seyrek gelir.
fn topics(tab: i32) -> Topics {
    match tab {
        1 => TOPIC_EVENTS | TOPIC_METRICS | TOPIC_SENSORS | TOPIC_AI, // Canlı
        4 => TOPIC_EVENTS | TOPIC_PROCS, // Süreçler: PC 2 s'de bir tarar
        _ => TOPIC_EVENTS,               // Kontrol, Olaylar, Geçmiş, AI
    }
}

/// Olaylar sekmesi: bağlanınca ve "Yenile"de son 24 saat istenir (LM ile aynı).
const EVENTS_SPAN_MS: u64 = 24 * 3_600_000;
const EVENTS_MAX: usize = 500;
/// Geçmiş aralıkları: 1 sa, 6 sa, 24 sa, 7 g.
const HIST_SPANS_MS: [u64; 4] = [3_600_000, 6 * 3_600_000, 24 * 3_600_000, 7 * 24 * 3_600_000];

/// Geçmiş yanıt tamponu. `pending` = yanıtı bitmemiş sorgu sayısı: yalnız sonuncusunun parçaları tutulur.
#[derive(Default)]
struct HistRx {
    pending: u32,
    from_ms: u64,
    step_ms: u64,
    pts: Vec<Option<HistPoint>>,
    /// Son `Metrics`'ten; RAM grafiği ölçeği (yoksa geçmişteki max).
    ram_total_mb: u32,
    /// Sıcaklık sorgusu (geçmiş bitince, aynı kovalar): aynı `pending` düzeni.
    tpending: u32,
    temps: Vec<Option<TempPoint>>,
}

/// Yaşam döngüsü kancası (true = öne geldi). `run` kurar, Android olay dinleyicisi çağırır.
type ActiveHook = Box<dyn Fn(bool)>;
type SendHook = Box<dyn Fn(&C2S) -> bool>;
type NameHook = Box<dyn Fn(&str)>;
type FoundHook = Box<dyn Fn(Vec<Beacon>)>;

thread_local! {
    static ON_ACTIVE: RefCell<Option<ActiveHook>> = const { RefCell::new(None) };
    /// Son AI durumu: geri sayım her metrik push'unda (≈2 s) telefonun saatiyle yenilenir.
    static LAST_AI: RefCell<Option<AiUsage>> = const { RefCell::new(None) };
    /// Son sensör okuması; grafik örneğine eklenir.
    static LAST_SENS: Cell<Option<Sensors>> = const { Cell::new(None) };
    /// Canlı grafik geçmişi + yeniden kullanılan path buffer'ı.
    static HIST: RefCell<(History, String)> = RefCell::new((History::default(), String::new()));
    static HQ: RefCell<HistRx> = RefCell::new(HistRx::default());
    /// Bağlanınca olay sorgusu (`run` kurar; `apply` Connected'da çağırır).
    static QUERY_EVENTS: RefCell<Option<Box<dyn Fn()>>> = const { RefCell::new(None) };
    static OFFSET_S: Cell<i64> = const { Cell::new(0) };
    /// `apply` içinden gönderim (sıcaklık sorgusu, bağlanınca süreç görünümü); `run` kurar.
    static SEND: RefCell<Option<SendHook>> = const { RefCell::new(None) };
    /// Bağlantıyı yeniden kur (`Client::reconnect`); `run` kurar.
    static RECONNECT: RefCell<Option<Box<dyn Fn()>>> = const { RefCell::new(None) };
    /// Güç komutu yanıtı bekleniyor (sayaç): `PowerResult` gelince artar, zaman aşımı eski sayacı görürse yok sayar.
    static POWER_WAIT: Cell<u32> = const { Cell::new(0) };
    /// Son süreç listesi: `Kill` için ctime + sonuç metni için ad.
    static PROCS: RefCell<Vec<ProcItem>> = const { RefCell::new(Vec::new()) };
    /// `SetAiConfig` yanıtı bekleniyor: hatasız `AiConfig` gelirse "Saved".
    static AICFG_SAVING: Cell<bool> = const { Cell::new(false) };
    /// Perdeyi kaldır (alarm kalktı); `run` kurar.
    static WAKE: RefCell<Option<Box<dyn Fn()>>> = const { RefCell::new(None) };
    /// Bağlanınca sunucu adı (= keşif adı) ayara; `run` kurar.
    static REMEMBER: RefCell<Option<NameHook>> = const { RefCell::new(None) };
    /// Keşif sonucu: kayıtlı PC yeni adresteyse oraya bağlan; `run` kurar.
    static REHOME: RefCell<Option<FoundHook>> = const { RefCell::new(None) };
    /// Son bağlantıdan beri üst üste kopuş.
    static FAILS: Cell<u32> = const { Cell::new(0) };
    /// Uygulama arka planda (ADR-0011): yalnız olay aboneliği, perde/arada açılış yok, bildirim var.
    static BG: Cell<bool> = const { Cell::new(false) };
    /// Aktif alarmların ham adları (bant `events::short` ile gösterir); asıl liste sunucuda.
    static ALARMS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// `ALARMS`'ı değiştirir ve bandı kısa adlarla yeniler.
fn set_alarms(ui: &MainWindow, f: impl FnOnce(&mut Vec<String>)) {
    let text = ALARMS.with_borrow_mut(|a| {
        f(a);
        a.iter().map(|n| events::short(n)).collect::<Vec<_>>().join(", ")
    });
    ui.set_alarms(text.into());
}

/// AI penceresi PC'de uyarı seviyesinde ya da ≥ %80.
fn ai_busy(a: &AiUsage) -> bool {
    [a.session, a.week].iter().flatten().any(|w| w.level >= 1 || w.pct >= 800)
}

/// Arada açılış için "PC meşgul": aktif alarm ya da AI limiti yakın.
fn busy(ui: &MainWindow) -> bool {
    ui.get_connected()
        && (!ui.get_alarms().is_empty() || LAST_AI.with_borrow(|a| a.as_ref().is_some_and(ai_busy)))
}

/// Arka planda keşif (~1,5 s); sonuç UI thread'inde `REHOME`'a.
fn rediscover() {
    std::thread::spawn(|| {
        let found = discover::discover(discover::WAIT);
        let _ = slint::invoke_from_event_loop(move || {
            REHOME.with_borrow(|f| {
                if let Some(f) = f.as_ref() {
                    f(found)
                }
            })
        });
    });
}

fn send(msg: &C2S) -> bool {
    SEND.with_borrow(|f| f.as_ref().is_some_and(|f| f(msg)))
}

/// Süreç görünümü sunucuda bağlantıya özgü: sekmeye geçişte ve yeniden bağlanınca gönderilir.
fn send_view(ui: &MainWindow) {
    send(&C2S::ProcView {
        sort: ui.get_procs_sort().clamp(0, 1) as u8,
        filter: ui.get_procs_filter().into(),
    });
}

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: slint::android::AndroidApp) {
    use slint::android::android_activity::{MainEvent, PollEvent, WindowManagerFlags};
    let data_dir = app.internal_data_path();
    // Önde iken ekran açık; boşta siyah perde UI'da. JNI gerekmez.
    // FULLSCREEN: durum çubuğu gizli (kullanıcı kararı 2026-11-18; jest çubuğu kalır, JNI yok).
    app.set_window_flags(
        WindowManagerFlags::KEEP_SCREEN_ON | WindowManagerFlags::FULLSCREEN,
        WindowManagerFlags::empty(),
    );
    let bg_app = app.clone();
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
    if let Err(e) = notify::start(&bg_app) {
        eprintln!("DeckService: {e}"); // bildirim olmaz, uygulama çalışır
    }
    run(data_dir, &device_name()).expect("run");
}

/// Hello'daki cihaz adı (LM tray'de çoklu telefon ayrımı, LM ADR-0036): Android `Build.MODEL` (= `ro.product.model`).
#[cfg(target_os = "android")]
fn device_name() -> String {
    let mut buf = [0u8; 92]; // PROP_VALUE_MAX
    // SAFETY: bionic `__system_property_get` en çok PROP_VALUE_MAX (92) bayt + NUL yazar; ad NUL sonlu literal.
    let n = unsafe { libc::__system_property_get(c"ro.product.model".as_ptr(), buf.as_mut_ptr().cast()) };
    match std::str::from_utf8(&buf[..n.clamp(0, 91) as usize]) {
        Ok(s) if !s.trim().is_empty() => s.trim().into(),
        _ => "android".into(),
    }
}

pub fn run(data_dir: Option<PathBuf>, device_name: &str) -> Result<(), slint::PlatformError> {
    let ui = MainWindow::new()?;
    OFFSET_S.set(events::local_offset_s());
    ui.global::<Hover>()
        .on_text(|kind, frac, _rev| tip_text(kind, frac, OFFSET_S.get()).into());
    let event_rows = Rc::new(VecModel::<EventRow>::default());
    ui.set_events(ModelRc::from(event_rows.clone()));
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
                ui.set_status("PC address not set".into());
                ui.set_tab(6); // Ayarlar sekmesi
                return;
            }
            let weak = weak.clone();
            let guard = last_local_edit.clone();
            let generation = generation.clone();
            let c = Client::start(
                Config {
                    host: c.host.clone(),
                    port: c.port,
                    token: c.token.clone(),
                    name: name.clone(),
                },
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
            c.subscribe(wanted(&ui));
            *client.borrow_mut() = Some(c);
        }
    };
    let connect = Rc::new(connect);
    connect(&current.borrow());

    // --- Boşta siyah perde: tek-atışlık timer; dokunuş / alarm 120 s, arada açılış 15 s kurar ---
    let idle = Rc::new(slint::Timer::default());
    let go_dark: Rc<dyn Fn()> = {
        let weak = ui.as_weak();
        let client = client.clone();
        Rc::new(move || {
            if let Some(ui) = weak.upgrade() {
                ui.set_blackout(true);
                ui.set_lm_state(0);
                ui.set_ai_state(0);
                if let Some(c) = client.borrow().as_ref() {
                    c.subscribe(wanted(&ui));
                }
            }
        })
    };
    let arm = {
        let idle = idle.clone();
        Rc::new(move |d: Duration| {
            let f = go_dark.clone();
            idle.start(slint::TimerMode::SingleShot, d, move || f());
        })
    };
    arm(BLACKOUT_AFTER);
    // Perdeyi kaldır, `d` sonra yeniden indir.
    let show_for = {
        let weak = ui.as_weak();
        let client = client.clone();
        Rc::new(move |d: Duration| {
            if BG.get() {
                return; // arka planda ekran yok: abonelik olaylarda kalır
            }
            arm(d);
            let Some(ui) = weak.upgrade() else { return };
            ui.set_blackout(false);
            if let Some(c) = client.borrow().as_ref() {
                c.subscribe(topics(ui.get_tab())); // değişmediyse gönderilmez
            }
        })
    };
    let poke = {
        let show_for = show_for.clone();
        move || show_for(BLACKOUT_AFTER)
    };
    ui.global::<Activity>().on_poke(poke.clone());
    WAKE.set(Some(Box::new(poke.clone())));
    // Perde inikken PC meşgulse arada kısa açılış (boş PC'de karanlık kalır).
    let peek = slint::Timer::default();
    {
        let weak = ui.as_weak();
        peek.start(slint::TimerMode::Repeated, PEEK_EVERY, move || {
            if let Some(ui) = weak.upgrade() {
                if ui.get_blackout() && busy(&ui) {
                    show_for(PEEK_FOR);
                }
            }
        });
    }
    {
        let weak = ui.as_weak();
        let client = client.clone();
        ui.on_tab_changed(move || {
            if let (Some(ui), Some(c)) = (weak.upgrade(), client.borrow().as_ref()) {
                if ui.get_tab() == 4 {
                    ui.set_procs_msg("".into());
                    send_view(&ui);
                }
                c.subscribe(topics(ui.get_tab()));
            }
        });
    }
    {
        let client = client.clone();
        SEND.with(|h| {
            *h.borrow_mut() = Some(Box::new(move |m| {
                client.borrow().as_ref().is_some_and(|c| c.send(m))
            }))
        });
    }
    {
        let client = client.clone();
        RECONNECT.with(|h| {
            *h.borrow_mut() = Some(Box::new(move || {
                if let Some(c) = client.borrow().as_ref() {
                    c.reconnect();
                }
            }))
        });
    }
    {
        let weak = ui.as_weak();
        ui.on_procs_view(move || {
            if let Some(ui) = weak.upgrade() {
                send_view(&ui);
            }
        });
    }
    {
        let weak = ui.as_weak();
        ui.on_kill(move |pid| {
            let Some(ui) = weak.upgrade() else { return };
            let Some(p) = PROCS.with_borrow(|v| v.iter().find(|p| p.pid as i32 == pid).cloned())
            else {
                return;
            };
            let msg = if send(&C2S::Kill {
                pid: p.pid,
                ctime: p.ctime,
            }) {
                format!("Ending {}…", p.name)
            } else {
                "Not connected to PC".into()
            };
            ui.set_procs_msg(msg.into());
        });
    }
    {
        let weak = ui.as_weak();
        ui.on_query_ai_config(move || {
            let Some(ui) = weak.upgrade() else { return };
            AICFG_SAVING.set(false);
            let sent = send(&C2S::QueryAiConfig);
            ui.set_aicfg_status(if sent { "Loading…" } else { "Not connected to PC" }.into());
        });
    }
    {
        let weak = ui.as_weak();
        ui.on_save_ai_config(move || {
            let Some(ui) = weak.upgrade() else { return };
            let cfg = match ai_config_of(&ui) {
                Ok(c) => c,
                Err(e) => return ui.set_aicfg_status(e.into()),
            };
            let sent = send(&C2S::SetAiConfig(cfg));
            AICFG_SAVING.set(sent);
            ui.set_aicfg_status(if sent { "Saving…" } else { "Not connected to PC" }.into());
        });
    }
    {
        let weak = ui.as_weak();
        ui.on_query_ai_report(move |view| {
            let Some(ui) = weak.upgrade() else { return };
            let sent = send(&C2S::QueryAiReport {
                view: view.clamp(0, 2) as u8,
            });
            ui.set_airep_status(
                if sent {
                    "Loading…"
                } else {
                    "Not connected to PC"
                }
                .into(),
            );
        });
    }
    // --- Olaylar: bağlanınca + "Yenile" → son 24 saat ---
    {
        let client = client.clone();
        let query = move || {
            let from_ms = now_ms().saturating_sub(EVENTS_SPAN_MS);
            let sent = client.borrow().as_ref().is_some_and(|c| {
                c.send(&C2S::QueryEvents {
                    from_ms,
                    to_ms: u64::MAX,
                })
            });
            if sent {
                event_rows.set_vec(Vec::new()); // yanıt event loop'ta sonra gelir
            }
        };
        let query = Rc::new(query);
        let q = query.clone();
        ui.on_refresh_events(move || q());
        QUERY_EVENTS.with(|h| *h.borrow_mut() = Some(Box::new(move || query())));
    }
    // --- Geçmiş: sekmeye geçiş / aralık düğmesi → tek sorgu; otomatik yenileme yok (ADR-0005) ---
    {
        let client = client.clone();
        let weak = ui.as_weak();
        ui.on_query_history(move |span| {
            let Some(ui) = weak.upgrade() else { return };
            let now = now_ms();
            let from_ms = now.saturating_sub(HIST_SPANS_MS[span.clamp(0, 3) as usize]);
            let sent = client.borrow().as_ref().is_some_and(|c| {
                c.send(&C2S::QueryHistory {
                    from_ms,
                    to_ms: now,
                    points: HIST_POINTS_MAX,
                })
            });
            if sent {
                HQ.with_borrow_mut(|h| {
                    h.pending += 1;
                    h.pts.clear();
                    h.temps.clear();
                });
                ui.set_hist_status("Loading…".into());
            } else {
                ui.set_hist_status("Not connected to PC".into());
            }
        });
    }
    {
        let client = client.clone();
        ui.on_lock(move || {
            if let Some(c) = client.borrow().as_ref() {
                c.send(&C2S::Lock);
            }
        });
    }
    // Güç (ADR-0012): onay UI'da (iki aşama); sonuç `S2C::PowerResult` ile
    {
        let weak = ui.as_weak();
        ui.on_power(move |i| {
            let cmd = match i {
                0 => PowerCmd::Sleep,
                1 => PowerCmd::Shutdown,
                2 => PowerCmd::Restart,
                _ => PowerCmd::Cancel,
            };
            let Some(ui) = weak.upgrade() else { return };
            if !send(&C2S::Power(cmd)) {
                ui.set_power_msg("Not sent: no connection to PC".into());
                return;
            }
            // Yanıt gelmezse bağlantı ölü (ör. PC uykudan döndü, TCP koptu ama telefon bilmiyor):
            // kullanıcıya söyle ve READ_TIMEOUT'u (60 s) beklemeden yeniden bağlan. Tek-atışlık.
            let n = POWER_WAIT.get().wrapping_add(1);
            POWER_WAIT.set(n);
            ui.set_power_msg("Sending…".into());
            let w = weak.clone();
            slint::Timer::single_shot(POWER_REPLY_TIMEOUT, move || {
                if POWER_WAIT.get() != n {
                    return; // yanıt geldi
                }
                if let Some(ui) = w.upgrade() {
                    ui.set_power_msg("No reply from PC, reconnecting".into());
                    ui.set_power_pending(false);
                }
                RECONNECT.with_borrow(|f| f.as_ref().map(|f| f()));
            });
        });
    }

    // --- Yaşam döngüsü: arka planda bağlantı sürer, yalnız olaylar (ADR-0011) ---
    {
        let client = client.clone();
        let connect = connect.clone();
        let current = current.clone();
        let weak = ui.as_weak();
        ON_ACTIVE.with(|h| {
            *h.borrow_mut() = Some(Box::new(move |active| {
                BG.set(!active);
                if active {
                    // uygulama açık: bant gösterir
                    ["uac", "firewall", "Claude.Input"].into_iter().filter_map(notify::alert_id).for_each(notify::cancel);
                    poke();
                    if client.borrow().is_none() {
                        connect(&current.borrow());
                    }
                } else if let (Some(ui), Some(c)) = (weak.upgrade(), client.borrow().as_ref()) {
                    ui.set_blackout(true);
                    c.subscribe(wanted(&ui));
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

    // --- Otomatik yeniden bağlanma: PC adı hatırlanır, kopunca keşifle yeni adresi bulunur ---
    {
        let (current, path) = (current.clone(), cfg_path.clone());
        REMEMBER.set(Some(Box::new(move |server: &str| {
            let mut c = current.borrow_mut();
            if c.name != server {
                c.name = server.to_string();
                config::save(path.as_deref(), &c);
            }
        })));
    }
    {
        let (weak, connect, current, path) = (ui.as_weak(), connect.clone(), current.clone(), cfg_path.clone());
        REHOME.set(Some(Box::new(move |found: Vec<Beacon>| {
            let Some(ui) = weak.upgrade() else { return };
            if ui.get_connected() {
                return; // keşif sürerken bağlandı
            }
            let mut c = current.borrow().clone();
            // Aynı adres bulunduysa sorun başka (PC kapalı / firewall): dokunma
            let Some(b) = discover::pick(&found, c.port, &c.name).filter(|b| b.host != c.host) else { return };
            ui.set_status(format!("PC moved: {} → {}", c.host, b.host).into());
            c.host = b.host.clone();
            c.name = b.name.clone();
            config::save(path.as_deref(), &c);
            ui.set_cfg_host(c.host.as_str().into());
            connect(&c);
            *current.borrow_mut() = c;
        })));
    }

    // --- Ayarlar ---
    {
        let weak = ui.as_weak();
        let connect = connect.clone();
        let current = current.clone();
        ui.on_save_settings(move || {
            let Some(ui) = weak.upgrade() else {
                return false;
            };
            match config::parse(&ui.get_cfg_host(), &ui.get_cfg_port(), &ui.get_cfg_token()) {
                Ok(mut c) => {
                    ui.set_cfg_error("".into());
                    // Aynı PC'de kaldıysa adı koru (otomatik yeniden bağlanma); başka PC'ye geçtiyse bağlanınca öğrenilir
                    if c.host == current.borrow().host {
                        c.name = current.borrow().name.clone();
                    }
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
            ui.set_discover_status("Searching…".into());
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
                                if b.proto != PROTO_VERSION {
                                    "  (version differs)"
                                } else {
                                    ""
                                }
                            )
                            .into(),
                            host: b.host.as_str().into(),
                            port: b.port.to_string().into(),
                        })
                        .collect();
                    ui.set_discover_status(match rows.len() {
                        0 => {
                            "Not found: same Wi-Fi? LowMonitor tray running? firewall rule?".into()
                        }
                        n => format!("{n} PC found, pick one; token is in the tray menu").into(),
                    });
                    set_vec(ui.get_found(), rows, |v| ui.set_found(v));
                });
            });
        });
    }

    let r = ui.run();
    // Kanca ui/client'ı tutuyor; event loop bitince bırak (Android'de android_main tekrar çağrılabilir).
    ON_ACTIVE.with(|h| h.borrow_mut().take());
    QUERY_EVENTS.with(|h| h.borrow_mut().take());
    SEND.with(|h| h.borrow_mut().take());
    RECONNECT.with(|h| h.borrow_mut().take());
    WAKE.with(|h| h.borrow_mut().take());
    REMEMBER.with(|h| h.borrow_mut().take());
    REHOME.with(|h| h.borrow_mut().take());
    r
}

/// Form → `AiConfig`; sayı değilse ya da sınır dışıysa kısa İngilizce hata.
fn ai_config_of(ui: &MainWindow) -> Result<AiConfig, String> {
    let num = |s: slint::SharedString, name: &str| {
        s.trim().parse::<u32>().map_err(|_| format!("{name}: number expected"))
    };
    let cfg = AiConfig {
        lang: ui.get_ai_lang().clamp(0, 1) as u8,
        auth: ui.get_ai_auth().clamp(0, 2) as u8,
        refresh_sec: num(ui.get_ai_refresh(), "Refresh")?,
        report_sessions: num(ui.get_ai_report(), "Report sessions")?.min(u16::MAX as u32) as u16,
        csv_log: ui.get_ai_csv(),
    };
    cfg.check()?;
    Ok(cfg)
}

/// Ağ olayını UI'a uygular (event loop thread'inde çalışır).
fn apply(ui: &MainWindow, ev: Event, last_local_edit: &Mutex<Instant>) {
    match ev {
        Event::Connecting => {
            ui.set_connected(false);
            ui.set_status("Connecting…".into());
        }
        Event::Connected { server } => {
            ui.set_connected(true);
            FAILS.set(0);
            REMEMBER.with_borrow(|f| f.as_ref().map(|f| f(&server)));
            ui.set_status(format!("Connected: {server}").into());
            QUERY_EVENTS.with(|q| q.borrow().as_ref().map(|f| f()));
            if ui.get_tab() == 4 {
                send_view(ui);
            }
            if ui.get_tab() == 6 {
                send(&C2S::QueryAiConfig);
            }
        }
        Event::Disconnected { reason } => {
            ui.set_connected(false);
            ui.set_power_pending(false);
            ui.set_status(format!("Disconnected: {reason}").into());
            // PC IP'si değişmiş olabilir (DHCP): birkaç denemeden sonra keşif, sonra seyrek
            let n = FAILS.get() + 1;
            FAILS.set(n);
            if n >= REDISCOVER_AFTER && (n - REDISCOVER_AFTER).is_multiple_of(5) {
                rediscover();
            }
            ui.set_lm_state(0);
            ui.set_ai_state(0);
            set_alarms(ui, Vec::clear);
            LAST_SENS.set(None);
            show_sensors(ui, None);
            clear_graphs(ui);
            HQ.with_borrow_mut(|h| (h.pending, h.tpending) = (0, 0)); // yanıtlar kayboldu
            ui.set_hist_status("".into());
            ui.set_airep_status("".into());
            ui.set_aicfg_loaded(false); // eski değerle yazma olmasın; sekmeye dönünce yeniden sorgu
            AICFG_SAVING.set(false);
        }
        Event::Msg(S2C::Alarms(a)) => set_alarms(ui, |x| *x = a),
        Event::Msg(S2C::Sensors(s)) => {
            LAST_SENS.set(Some(s));
            show_sensors(ui, Some(&s));
        }
        Event::Msg(S2C::Event(e)) => {
            set_alarms(ui, |a| match e.kind {
                0 if !a.contains(&e.name) => a.push(e.name.clone()),
                1 => a.retain(|x| *x != e.name),
                _ => {}
            });
            // Arka planda: yalnız UAC/güvenlik duvarı/Claude için Android bildirimi (ADR-0011)
            if let Some(id) = notify::alert_id(&e.name).filter(|_| BG.get()) {
                match e.kind {
                    0 => notify::alert(id, &events::desc(&e)),
                    1 => notify::cancel(id),
                    _ => {}
                }
            }
            // Alarm geldi (kullanım kuralı ya da PC sıcaklık eşiği): perde iniksa ekranı aç
            let wake = e.kind == 0 && ui.get_blackout();
            prepend(ui, &[e]);
            if wake {
                WAKE.with_borrow(|f| f.as_ref().map(|f| f()));
            }
        }
        // Parçalar eskiden yeniye gelir; her biri başa eklenince liste yeniden eskiye sıralı kalır.
        Event::Msg(S2C::Events(v)) => prepend(ui, &v),
        Event::Msg(S2C::History {
            from_ms,
            step_ms,
            points,
        }) => {
            let done = HQ.with_borrow_mut(|h| {
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
                draw_history(ui);
                // Aynı kovalarla sıcaklık/GPU (sunucu: step = (to − from) / n)
                let q = HQ.with_borrow(|h| {
                    let n = h.pts.len() as u64;
                    (n > 0).then(|| C2S::QueryTempHistory {
                        from_ms: h.from_ms,
                        to_ms: h.from_ms + h.step_ms * n,
                        points: n as u16,
                    })
                });
                if let Some(q) = q {
                    if send(&q) {
                        HQ.with_borrow_mut(|h| h.tpending += 1);
                    }
                }
            }
        }
        Event::Msg(S2C::TempHistory { points, .. }) => {
            let done = HQ.with_borrow_mut(|h| {
                if h.tpending == 0 {
                    return false;
                }
                if points.is_empty() {
                    h.tpending -= 1;
                    return h.tpending == 0;
                }
                if h.tpending == 1 {
                    h.temps.extend(points);
                }
                false
            });
            if done {
                draw_temps(ui);
            }
        }
        Event::Msg(S2C::Procs { total, items }) => {
            ui.set_procs_total(format!("{} of {total}", items.len()).into());
            let rows = items.iter().map(tabs::proc_row).collect();
            set_vec(ui.get_procs(), rows, |v| ui.set_procs(v));
            PROCS.set(items);
        }
        Event::Msg(S2C::KillResult { pid, code }) => {
            let name =
                PROCS.with_borrow(|v| v.iter().find(|p| p.pid == pid).map(|p| p.name.clone()));
            ui.set_procs_msg(
                tabs::kill_text(code, &name.unwrap_or_else(|| format!("PID {pid}"))).into(),
            );
        }
        Event::Msg(S2C::PowerResult { cmd, code, delay_s }) => {
            POWER_WAIT.set(POWER_WAIT.get().wrapping_add(1)); // bekleyen zaman aşımını iptal et
            ui.set_power_msg(tabs::power_text(cmd, code, delay_s).into());
            let ok = code == lowdeck_proto::POWER_OK;
            match cmd {
                PowerCmd::Shutdown | PowerCmd::Restart if ok && delay_s > 0 => {
                    ui.set_power_pending(true);
                    // PC'de elle iptal edilirse (bildirim yok) Cancel askıda kalmasın: tek-atışlık temizlik
                    let w = ui.as_weak();
                    slint::Timer::single_shot(Duration::from_secs(u64::from(delay_s) + 5), move || {
                        if let Some(ui) = w.upgrade() {
                            ui.set_power_pending(false);
                        }
                    });
                }
                PowerCmd::Cancel => ui.set_power_pending(false),
                // PC uykuya giriyor: bağlantı sessizce ölecek. Hemen kapat ki UI "Connecting" göstersin ve
                // PC uyanınca READ_TIMEOUT'u beklemeden bağlanılsın (geri çekilme ≤ 30 s).
                PowerCmd::Sleep if ok => {
                    slint::Timer::single_shot(Duration::from_secs(1), || {
                        RECONNECT.with_borrow(|f| f.as_ref().map(|f| f()));
                    });
                }
                _ => {}
            }
        }
        Event::Msg(S2C::AiReport(r)) => draw_ai_report(ui, &r),
        Event::Msg(S2C::AiConfig { cfg, error }) => {
            ui.set_ai_lang(cfg.lang.into());
            ui.set_ai_auth(cfg.auth.into());
            ui.set_ai_refresh(cfg.refresh_sec.to_string().into());
            ui.set_ai_report(cfg.report_sessions.to_string().into());
            ui.set_ai_csv(cfg.csv_log);
            ui.set_aicfg_loaded(true);
            let saved = AICFG_SAVING.replace(false);
            let msg = match (error.is_empty(), saved) {
                (false, _) => error,
                (true, true) => "Saved".into(),
                (true, false) => String::new(),
            };
            ui.set_aicfg_status(msg.into());
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
            clear_graphs(ui);
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
    ui.set_lm_ram(
        format!(
            "{:.1} / {:.0} GB",
            m.ram_used_mb as f32 / 1024.0,
            m.ram_total_mb as f32 / 1024.0
        )
        .into(),
    );
    ui.set_lm_disk(format!("{} / {}", rate(m.disk_r_kbps), rate(m.disk_w_kbps)).into());
    ui.set_lm_net(format!("{} / {}", rate(m.net_rx_kbps), rate(m.net_tx_kbps)).into());
    let cores = m.cores.iter().map(|&c| c as f32 / 200.0).collect();
    set_vec(ui.get_lm_cores(), cores, |v| ui.set_lm_cores(v));
    HQ.with_borrow_mut(|h| h.ram_total_mb = m.ram_total_mb);
    graphs(ui, m, now_ms() / 1000);
}

/// LM ile aynı: eksik alan boş metin.
fn show_sensors(ui: &MainWindow, v: Option<&Sensors>) {
    let c = |t: Option<i16>| t.map(|t| format!("{t} °C")).unwrap_or_default();
    let v = v.copied().unwrap_or_default();
    ui.set_lm_cpu_c(c(v.cpu_c).into());
    ui.set_lm_gpu_c(c(v.gpu_c).into());
    ui.set_lm_disk_c(c(v.disk_c).into());
    ui.set_lm_gpu(
        v.gpu_pct
            .map(|p| format!("{p}%"))
            .unwrap_or_default()
            .into(),
    );
}

/// Örneği geçmişe ekler, 9 çizgi + 2 ölçek etiketini günceller (Metrics başına bir kez).
fn graphs(ui: &MainWindow, m: &Metrics, now_s: u64) {
    let sens = LAST_SENS.get();
    // °C ekseni 0..100 (CPU % ile aynı kartta)
    let temp = |t: Option<i16>| t.map(|t| t as f32 / 100.0);
    HIST.with_borrow_mut(|(h, buf)| {
        h.push(Sample {
            ts_s: now_s,
            cpu: m.cpu,
            ram: m.ram_used_mb as f32 / m.ram_total_mb.max(1) as f32,
            dr: m.disk_r_kbps,
            dw: m.disk_w_kbps,
            rx: m.net_rx_kbps,
            tx: m.net_tx_kbps,
            cpu_c: sens.and_then(|v| v.cpu_c),
            gpu_pct: sens.and_then(|v| v.gpu_pct),
            gpu_c: sens.and_then(|v| v.gpu_c),
        });
        let mut line = |f: &dyn Fn(&Sample) -> Option<f32>,
                        set: fn(&MainWindow, slint::SharedString)| {
            h.path(now_s, f, buf);
            set(ui, buf.as_str().into());
        };
        line(
            &|s| Some(s.cpu as f32 / 10_000.0),
            MainWindow::set_lm_cpu_path,
        );
        line(&|s| temp(s.cpu_c), MainWindow::set_lm_cpu_c_path);
        line(
            &|s| s.gpu_pct.map(|p| p as f32 / 100.0),
            MainWindow::set_lm_gpu_path,
        );
        line(&|s| temp(s.gpu_c), MainWindow::set_lm_gpu_c_path);
        line(&|s| Some(s.ram), MainWindow::set_lm_ram_path);
        // Disk/ağ: iki çizgi ortak ölçekte (penceredeki max, 1-2-5 yuvarlama).
        let disk = nice_scale(h.max(now_s, |s| s.dr.max(s.dw))) as f32;
        line(
            &|s| Some(s.dr as f32 / disk),
            MainWindow::set_lm_disk_r_path,
        );
        line(
            &|s| Some(s.dw as f32 / disk),
            MainWindow::set_lm_disk_w_path,
        );
        let net = nice_scale(h.max(now_s, |s| s.rx.max(s.tx))) as f32;
        line(&|s| Some(s.rx as f32 / net), MainWindow::set_lm_net_rx_path);
        line(&|s| Some(s.tx as f32 / net), MainWindow::set_lm_net_tx_path);
        ui.set_lm_disk_scale(rate(disk as u32).into());
        ui.set_lm_net_scale(rate(net as u32).into());
    });
    bump_tip(ui);
}

fn bump_tip(ui: &MainWindow) {
    let g = ui.global::<Hover>();
    g.set_rev(g.get_rev().wrapping_add(1));
}

/// Grafik dokunma etiketi: zaman + o andaki seriler (LM `hover_text`). `kind`: 0–4 Canlı
/// (CPU, RAM, disk, ağ, GPU), 10–14 Geçmiş (aynı sıra). Veri yoksa "" (etiket gizlenir).
fn tip_text(kind: i32, frac: f32, off: i64) -> String {
    let pct = |v: f32| format!("{:.0}%", v * 100.0);
    let c = |t: Option<i16>| t.map(|t| format!("  {t} °C")).unwrap_or_default();
    if kind < 10 {
        return HIST.with_borrow(|(h, _)| {
            let Some(s) = h.at(frac) else {
                return String::new();
            };
            let v = match kind {
                0 => format!("{}{}", pct(s.cpu as f32 / 10_000.0), c(s.cpu_c)),
                4 => format!(
                    "{}{}",
                    s.gpu_pct.map_or("—".into(), |p| format!("{p}%")),
                    c(s.gpu_c)
                ),
                1 => pct(s.ram),
                2 => format!("R {}  W {}", rate(s.dr), rate(s.dw)),
                _ => format!("↓ {}  ↑ {}", rate(s.rx), rate(s.tx)),
            };
            // fmt_time: "MM-DD HH:MM:SS"
            format!("{}\n{v}", &events::fmt_time(s.ts_s * 1000, off)[6..])
        });
    }
    HQ.with_borrow(|h| {
        let n = h.pts.len();
        if n == 0 {
            return String::new();
        }
        // Kova i, viewbox'ta x = i (genişlik n)
        let i = ((frac.max(0.0) * n as f32) as usize).min(n - 1);
        let Some(p) = &h.pts[i] else {
            return String::new();
        };
        let t = h.temps.get(i).copied().flatten();
        let v = match kind {
            10 => format!(
                "avg {}  max {}{}",
                pct(p.cpu_avg as f32 / 10_000.0),
                pct(p.cpu_max as f32 / 10_000.0),
                c(t.and_then(|t| t.cpu_c))
            ),
            14 => format!(
                "{}{}",
                t.and_then(|t| t.gpu_pct)
                    .map_or("—".into(), |v| format!("{v}%")),
                c(t.and_then(|t| t.gpu_c))
            ),
            11 => format!("{:.1} GB", p.ram_mb as f32 / 1024.0),
            12 => format!(
                "R {}  W {}{}",
                rate(p.disk_r_kbps),
                rate(p.disk_w_kbps),
                t.and_then(|t| t.disk_c_max)
                    .map(|v| format!("  max {v} °C"))
                    .unwrap_or_default()
            ),
            _ => format!("↓ {}  ↑ {}", rate(p.net_rx_kbps), rate(p.net_tx_kbps)),
        };
        // "11-17 14:32–14:33": tarih 7 g aralığında gerekli; dakikadan kısa kovada saniye de
        let end = if h.step_ms < 60_000 { 14 } else { 11 };
        let from = h.from_ms + h.step_ms * i as u64;
        format!(
            "{}–{}\n{v}",
            &events::fmt_time(from, off)[..end],
            &events::fmt_time(from + h.step_ms, off)[6..end]
        )
    })
}

fn clear_graphs(ui: &MainWindow) {
    HIST.with_borrow_mut(|(h, _)| h.clear());
    for set in [
        MainWindow::set_lm_cpu_path,
        MainWindow::set_lm_cpu_c_path,
        MainWindow::set_lm_gpu_path,
        MainWindow::set_lm_gpu_c_path,
        MainWindow::set_lm_ram_path,
        MainWindow::set_lm_disk_r_path,
        MainWindow::set_lm_disk_w_path,
        MainWindow::set_lm_net_rx_path,
        MainWindow::set_lm_net_tx_path,
    ] {
        set(ui, "".into());
    }
}

/// Tamamlanan geçmiş yanıtından çizgiler + ölçek etiketleri (sıcaklık geçmişi B: proto'da yok).
fn draw_history(ui: &MainWindow) {
    HQ.with_borrow(|h| {
        let pts = &h.pts;
        let mut buf = String::new();
        let mut line = |f: &dyn Fn(&HistPoint) -> f32,
                        set: fn(&MainWindow, slint::SharedString)| {
            series_path(pts.iter().map(|p| p.as_ref().map(f)), &mut buf);
            set(ui, buf.as_str().into());
        };
        let max = |f: &dyn Fn(&HistPoint) -> u32| pts.iter().flatten().map(f).max().unwrap_or(0);
        line(
            &|p| p.cpu_avg as f32 / 10_000.0,
            MainWindow::set_hist_cpu_avg,
        );
        line(
            &|p| p.cpu_max as f32 / 10_000.0,
            MainWindow::set_hist_cpu_max,
        );
        let ram = if h.ram_total_mb > 0 {
            h.ram_total_mb
        } else {
            max(&|p| p.ram_mb).max(1)
        } as f32;
        line(&|p| p.ram_mb as f32 / ram, MainWindow::set_hist_ram);
        let disk = nice_scale(max(&|p| p.disk_r_kbps.max(p.disk_w_kbps))) as f32;
        line(
            &|p| p.disk_r_kbps as f32 / disk,
            MainWindow::set_hist_disk_r,
        );
        line(
            &|p| p.disk_w_kbps as f32 / disk,
            MainWindow::set_hist_disk_w,
        );
        let net = nice_scale(max(&|p| p.net_rx_kbps.max(p.net_tx_kbps))) as f32;
        line(&|p| p.net_rx_kbps as f32 / net, MainWindow::set_hist_net_rx);
        line(&|p| p.net_tx_kbps as f32 / net, MainWindow::set_hist_net_tx);
        // Sıcaklık çizgileri ayrı yanıtla gelir; eski aralığınki kalmasın
        for set in [
            MainWindow::set_hist_cpu_c,
            MainWindow::set_hist_gpu,
            MainWindow::set_hist_gpu_c,
            MainWindow::set_hist_disk_c,
        ] {
            set(ui, "".into());
        }
        ui.set_hist_ram_scale(format!("{:.0} GB", ram / 1024.0).into());
        ui.set_hist_disk_scale(rate(disk as u32).into());
        ui.set_hist_net_scale(rate(net as u32).into());
        ui.set_hist_n(pts.len().max(1) as f32);
        let off = OFFSET_S.get();
        let to_ms = h.from_ms + h.step_ms * pts.len() as u64;
        ui.set_hist_range(
            format!(
                "{} – {}",
                events::fmt_time(h.from_ms, off),
                events::fmt_time(to_ms, off)
            )
            .into(),
        );
        let empty = pts.iter().all(Option::is_none);
        ui.set_hist_status(
            if empty {
                "No data (LowMonitor service?)"
            } else {
                ""
            }
            .into(),
        );
    });
    bump_tip(ui);
}

/// Sıcaklık/GPU geçmişi: 0..100 eksen (°C ve %), LM Geçmiş ile aynı.
fn draw_temps(ui: &MainWindow) {
    HQ.with_borrow(|h| {
        let mut buf = String::new();
        let mut line = |f: &dyn Fn(&TempPoint) -> Option<f32>,
                        set: fn(&MainWindow, slint::SharedString)| {
            series_path(
                h.temps
                    .iter()
                    .map(|t| t.as_ref().and_then(f).map(|v| v / 100.0)),
                &mut buf,
            );
            set(ui, buf.as_str().into());
        };
        line(&|t| t.cpu_c.map(f32::from), MainWindow::set_hist_cpu_c);
        line(&|t| t.gpu_pct.map(f32::from), MainWindow::set_hist_gpu);
        line(&|t| t.gpu_c.map(f32::from), MainWindow::set_hist_gpu_c);
        let disk = h.temps.iter().flatten().filter_map(|t| t.disk_c_max).max();
        ui.set_hist_disk_c(disk.map_or(String::new(), |v| format!("max {v} °C")).into());
    });
    bump_tip(ui);
}

fn draw_ai_report(ui: &MainWindow, r: &lowdeck_proto::AiReport) {
    let mut buf = String::new();
    // 0.1 % → 0..1 (limit %100'ü aşabilir; grafik tavanda kesilir)
    let mut line = |v: &[Option<u16>], set: fn(&MainWindow, slint::SharedString)| {
        series_path(
            v.iter().map(|p| p.map(|p| (p as f32 / 1000.0).min(1.0))),
            &mut buf,
        );
        set(ui, buf.as_str().into());
    };
    line(&r.session_line, MainWindow::set_airep_session_line);
    line(&r.week_line, MainWindow::set_airep_week_line);
    let off = OFFSET_S.get();
    ui.set_airep_range(
        format!(
            "{} – {}",
            &events::fmt_time(r.from_s * 1000, off)[..11],
            &events::fmt_time(r.to_s * 1000, off)[..11]
        )
        .into(),
    );
    ui.set_airep_sub(tabs::sub(r).into());
    set_vec(ui.get_airep_groups(), tabs::groups(r), |v| {
        ui.set_airep_groups(v)
    });
    set_vec(ui.get_airep_sessions(), tabs::sessions(r, off), |v| {
        ui.set_airep_sessions(v)
    });
    let empty = r.sessions.is_empty() && r.session_line.iter().all(Option::is_none);
    ui.set_airep_status(if empty { "No data" } else { "" }.into());
}

/// Başa ekler; aynı saniye + tür + ad baştaki satırlarda varsa (geçmiş sorgusu sürerken gelen canlı olay) eklemez.
fn prepend(ui: &MainWindow, v: &[EventItem]) {
    let model = ui.get_events();
    let Some(m) = model.as_any().downcast_ref::<VecModel<EventRow>>() else {
        return;
    };
    let off = OFFSET_S.get();
    for e in v {
        let row = EventRow {
            time: events::fmt_time(e.ts_ms, off).into(),
            kind: e.kind as i32,
            name: e.name.as_str().into(),
            value: events::value(e).into(),
            desc: events::desc(e).into(),
        };
        if !m
            .iter()
            .take(50)
            .any(|r| r.time == row.time && r.kind == row.kind && r.name == row.name)
        {
            m.insert(0, row);
        }
    }
    while m.row_count() > EVENTS_MAX {
        m.remove(m.row_count() - 1);
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// AI bölümü; `None` = PC'de veri yok.
fn show_ai(ui: &MainWindow, a: Option<&AiUsage>, now: u64) {
    let Some(a) = a else {
        ui.set_ai_state(2);
        return;
    };
    ui.set_ai_state(1);
    // 3 = soluk (eski veri), aksi halde PC'nin seviyesi
    let level = |w: &Option<AiWindow>| {
        if a.stale {
            3
        } else {
            w.map_or(0, |w| w.level.min(2) as i32)
        }
    };
    ui.set_ai_session(ai_line("Session", &a.session, now).into());
    ui.set_ai_week(ai_line("Week", &a.week, now).into());
    ui.set_ai_session_level(level(&a.session));
    ui.set_ai_week_level(level(&a.week));
    ui.set_ai_note(ai_note(a, now).into());
}

/// "Session 33% · 4h16m" (LM taskbar ile aynı biçim).
fn ai_line(label: &str, w: &Option<AiWindow>, now: u64) -> String {
    match w {
        None => format!("{label} —"),
        Some(w) => {
            let left = if w.resets_at == 0 {
                "—".into()
            } else {
                remaining(w.resets_at as i64 - now as i64)
            };
            format!("{label} {}% · {left}", (w.pct + 5) / 10)
        }
    }
}

/// Sağ üst not: hata > eski > kaynak.
fn ai_note(a: &AiUsage, now: u64) -> String {
    if !a.error.is_empty() {
        a.error.clone()
    } else if a.stale {
        format!("stale · {} min ago", now.saturating_sub(a.fetched_at) / 60)
    } else {
        a.source.clone()
    }
}

/// Geri sayım: "2d05h", "3s07dk", "12dk"; negatif → 0.
fn remaining(secs: i64) -> String {
    let s = secs.max(0);
    let (d, h, m) = (s / 86_400, s % 86_400 / 3600, s % 3600 / 60);
    if d > 0 {
        format!("{d}d{h:02}h")
    } else if h > 0 {
        format!("{h}h{m:02}m")
    } else {
        format!("{m}m")
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
    fn ai_busy_rules() {
        let w = |pct, level| Some(AiWindow { pct, resets_at: 0, level });
        let a = |session, week| AiUsage { session, week, ..AiUsage::default() };
        assert!(!ai_busy(&a(None, None)));
        assert!(!ai_busy(&a(w(799, 0), w(100, 0))));
        assert!(ai_busy(&a(w(800, 0), None)));
        assert!(ai_busy(&a(w(100, 0), w(300, 1)))); // PC uyarı seviyesi (tempo)
    }

    #[test]
    fn ai_text() {
        let w = Some(AiWindow {
            pct: 334,
            resets_at: 1_000 + 4 * 3600 + 16 * 60,
            level: 1,
        });
        assert_eq!(ai_line("Session", &w, 1_000), "Session 33% · 4h16m");
        assert_eq!(ai_line("Week", &None, 0), "Week —");
        let w = Some(AiWindow {
            pct: 1000,
            resets_at: 0,
            level: 2,
        });
        assert_eq!(ai_line("Week", &w, 0), "Week 100% · —");
        assert_eq!(remaining(2 * 86_400 + 5 * 3600), "2d05h");
        assert_eq!(remaining(-5), "0m");
        let mut a = AiUsage {
            fetched_at: 100,
            source: "OAuth".into(),
            ..Default::default()
        };
        assert_eq!(ai_note(&a, 200), "OAuth");
        a.stale = true;
        assert_eq!(ai_note(&a, 100 + 720), "stale · 12 min ago");
        a.error = "OAuth: token süresi dolmuş".into();
        assert_eq!(ai_note(&a, 0), "OAuth: token süresi dolmuş");
    }

    #[test]
    fn tab_topics() {
        assert_eq!(topics(0), TOPIC_EVENTS);
        assert_eq!(topics(4), TOPIC_EVENTS | TOPIC_PROCS);
        assert_eq!(topics(5), TOPIC_EVENTS);
        assert_eq!(
            topics(1) & (TOPIC_METRICS | TOPIC_SENSORS | TOPIC_AI),
            TOPIC_METRICS | TOPIC_SENSORS | TOPIC_AI
        );
    }

    #[test]
    fn rate_units() {
        assert_eq!(super::rate(0), "0K");
        assert_eq!(super::rate(1023), "1023K");
        assert_eq!(super::rate(1536), "1.5M");
        assert_eq!(super::rate(3 * 1_048_576), "3.0G");
    }
}

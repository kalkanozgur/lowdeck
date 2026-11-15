//! Tray ikonu + menü. LowMonitor `lowmon-tray` ile aynı yapı (gizli üst düzey pencere,
//! `TaskbarCreated` ile yeniden ekleme); `windows-sys` yerine zaten bağımlı olunan `windows` crate.
//! Periyodik iş yok: ikon yalnız telefon bağlanınca/kopunca yeniden çizilir.

use crate::Hub;
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateBitmap, CreateDIBSection, DeleteObject, GetDC, ReleaseDC, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Registry::{RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::Shell::{
    ShellExecuteW, Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconIndirect, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyMenu,
    DestroyWindow, DispatchMessageW, GetCursorPos, GetMessageW, MessageBoxW, PostMessageW, PostQuitMessage,
    RegisterClassW, RegisterWindowMessageW, SetForegroundWindow, TrackPopupMenu, TranslateMessage, HICON, ICONINFO,
    MB_ICONERROR, MF_CHECKED, SW_HIDE, MF_GRAYED, MF_SEPARATOR, MF_STRING, MSG, SW_SHOWNORMAL, TPM_NONOTIFY, TPM_RETURNCMD,
    TPM_RIGHTBUTTON, WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP, WM_DESTROY, WM_NULL, WM_RBUTTONUP, WNDCLASSW,
};

const WM_TRAY: u32 = WM_APP + 1;
const WM_CLIENT: u32 = WM_APP + 2;
const ID_CFG: usize = 1;
const ID_AUTO: usize = 2;
const ID_EXIT: usize = 3;
const ID_RELOAD: usize = 4;
const ID_LOG: usize = 5;
const ID_FIREWALL: usize = 6;
const RUN_KEY: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
const RUN_VALUE: &str = "LowDeckAgent";
// shortcut: sabit 16 px; yüksek DPI'da kabuk ölçekler (bulanık). LowMonitor tray ile aynı sınır.
const N: usize = 16;

/// Ayar dosyasını yeniden okuyup dinleyiciyi yeniden açar; hata metni kullanıcıya gösterilir.
pub type Reload = Box<dyn FnMut() -> Result<Info, String>>;

pub struct Info {
    pub bind: String,
    pub port: u16,
    pub token: String,
    pub cfg_path: PathBuf,
}

static HWND_: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static STATE: RefCell<Option<(Info, Arc<Hub>)>> = const { RefCell::new(None) };
    static RELOAD: RefCell<Option<Reload>> = const { RefCell::new(None) };
    static ICON: Cell<HICON> = const { Cell::new(HICON(std::ptr::null_mut())) };
    static TASKBAR_CREATED: Cell<u32> = const { Cell::new(0) };
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

/// İkinci kopya sessizce çıkar (Run + elle başlatma çakışması).
pub fn single_instance() -> bool {
    // SAFETY: isimli mutex süreç ömrü boyunca açık kalır (handle bilerek kapatılmaz).
    unsafe {
        let _ = CreateMutexW(None, false, w!(r"Local\LowDeckAgent"));
        GetLastError() != ERROR_ALREADY_EXISTS
    }
}

/// Yanında `lowdeck-desktop.exe` varsa `HKCU\Software\LowMonitor` `UiExe` = onun yolu (ADR-0004 madde 2):
/// LowMonitor tray "Arayüzü aç" masaüstü hiç açılmadan da aktif olur.
pub fn register_ui_exe() {
    let Some(ui) = std::env::current_exe().ok().map(|p| p.with_file_name("lowdeck-desktop.exe")) else { return };
    let Some(ui) = ui.is_file().then(|| ui.to_str().map(wide)).flatten() else { return };
    // shortcut: kayıt hatası sessiz; yalnız tray kısayolu etkilenir
    // SAFETY: NUL sonlu dizgeler; veri boyutu NUL dahil byte.
    unsafe {
        let _ = RegSetKeyValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\LowMonitor"),
            w!("UiExe"),
            REG_SZ.0,
            Some(ui.as_ptr().cast()),
            (ui.len() * 2) as u32,
        );
    }
}

pub fn error_box(text: &str) {
    let t = wide(text);
    // SAFETY: NUL sonlu dizgeler.
    unsafe { MessageBoxW(None, PCWSTR(t.as_ptr()), w!("LowDeck agent"), MB_ICONERROR) };
}

pub fn tooltip(info: &Info, client: Option<&str>) -> String {
    match client {
        Some(n) => format!("LowDeck · {}:{} · telefon: {n}", info.bind, info.port),
        None => format!("LowDeck · {}:{} · bekliyor", info.bind, info.port),
    }
}

/// 16×16 ARGB daire: yeşil = telefon bağlı, gri = bekliyor.
pub fn pixels(connected: bool) -> [u32; N * N] {
    let c = if connected { 0xFF22_C55E } else { 0xFF80_8080 };
    let mut px = [0u32; N * N];
    for y in 0..N {
        for x in 0..N {
            let (dx, dy) = (x as f32 - 7.5, y as f32 - 7.5);
            if dx * dx + dy * dy <= 6.5 * 6.5 {
                px[y * N + x] = c;
            }
        }
    }
    px
}

fn make_icon(connected: bool) -> HICON {
    let px = pixels(connected);
    // SAFETY: bits DIB'in N*N*4 B'lık tamponunu gösterir; GDI nesneleri ikon oluşunca silinir.
    unsafe {
        let bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: N as i32,
                biHeight: -(N as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let dc = GetDC(None);
        let mut bits = std::ptr::null_mut();
        let color = CreateDIBSection(Some(dc), &bi, DIB_RGB_COLORS, &mut bits, None, 0);
        ReleaseDC(None, dc);
        let Ok(color) = color else { return HICON::default() };
        std::ptr::copy_nonoverlapping(px.as_ptr(), bits.cast::<u32>(), N * N);
        let mask_bits = [0u8; N * N / 8];
        let mask = CreateBitmap(N as i32, N as i32, 1, 1, Some(mask_bits.as_ptr().cast()));
        let ii = ICONINFO { fIcon: true.into(), xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: color };
        let icon = CreateIconIndirect(&ii).unwrap_or_default();
        let _ = DeleteObject(color.into());
        let _ = DeleteObject(mask.into());
        icon
    }
}

fn notify(hwnd: HWND, msg: windows::Win32::UI::Shell::NOTIFY_ICON_MESSAGE) {
    let Some((tip, connected)) = STATE.with_borrow(|s| {
        s.as_ref().map(|(info, hub)| {
            let c = hub.client_name();
            (tooltip(info, c.as_deref()), c.is_some())
        })
    }) else {
        return;
    };
    let icon = make_icon(connected);
    // SAFETY: nid tam doldurulmuş; eski ikon kabuk yeni ikonu aldıktan sonra yok edilir.
    unsafe {
        let mut nid = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            uFlags: NIF_ICON | NIF_TIP | NIF_MESSAGE,
            uCallbackMessage: WM_TRAY,
            hIcon: icon,
            ..Default::default()
        };
        let tip: Vec<u16> = tip.encode_utf16().collect();
        let n = tip.len().min(nid.szTip.len() - 1);
        nid.szTip[..n].copy_from_slice(&tip[..n]);
        let _ = Shell_NotifyIconW(msg, &nid);
        let old = ICON.replace(icon);
        if !old.is_invalid() {
            let _ = DestroyIcon(old);
        }
    }
}

// ---------------- Otomatik başlatma (HKCU\…\Run) ----------------

fn this_exe() -> Option<String> {
    std::env::current_exe().ok().map(|p| format!("\"{}\"", p.display()))
}

fn autostart_get() -> Option<String> {
    let name = wide(RUN_VALUE);
    let mut buf = [0u16; 1024];
    let mut size = size_of_val(&buf) as u32;
    // SAFETY: buf ve size eşleşiyor; RRF_RT_REG_SZ sonucu NUL ile bitirir.
    let e = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, RUN_KEY, PCWSTR(name.as_ptr()), RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr().cast()), Some(&mut size))
    };
    e.is_ok().then(|| String::from_utf16_lossy(&buf[..(size as usize / 2).saturating_sub(1)]))
}

/// Yalnız bu exe'yi gösteriyorsa açık sayılır (taşınmış eski yol = kapalı, menüden yeniden açılır).
fn autostart_enabled() -> bool {
    matches!((autostart_get(), this_exe()), (Some(v), Some(me)) if v.eq_ignore_ascii_case(&me))
}

fn autostart_set(on: bool) {
    let name = wide(RUN_VALUE);
    // shortcut: kayıt hatası sessiz; menü bir sonraki açılışta gerçek durumu gösterir (LowMonitor ile aynı).
    // SAFETY: NUL sonlu dizgeler; veri boyutu NUL dahil byte.
    unsafe {
        match this_exe().filter(|_| on) {
            Some(cmd) => {
                let data = wide(&cmd);
                let _ = RegSetKeyValueW(HKEY_CURRENT_USER, RUN_KEY, PCWSTR(name.as_ptr()), REG_SZ.0, Some(data.as_ptr().cast()), (data.len() * 2) as u32);
            }
            None => {
                let e = RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, PCWSTR(name.as_ptr()));
                debug_assert!(e.is_ok() || e == ERROR_FILE_NOT_FOUND);
            }
        }
    }
}

// ---------------- Menü + pencere ----------------

fn menu(hwnd: HWND) {
    let Some((phone, cfg, port)) = STATE.with_borrow(|s| {
        s.as_ref().map(|(i, _)| (format!("Telefon ⚙  {}  ·  {}  ·  {}", i.bind, i.port, i.token), i.cfg_path.clone(), i.port))
    }) else {
        return;
    };
    let phone = wide(&phone);
    // SAFETY: menü bu fonksiyonda oluşturulup yok edilir; dizgeler çağrı boyunca yaşar.
    unsafe {
        let Ok(m) = CreatePopupMenu() else { return };
        let _ = AppendMenuW(m, MF_STRING | MF_GRAYED, 0, PCWSTR(phone.as_ptr()));
        let _ = AppendMenuW(m, MF_SEPARATOR, 0, None);
        let _ = AppendMenuW(m, MF_STRING, ID_CFG, w!("Ayar dosyasını aç"));
        let _ = AppendMenuW(m, MF_STRING, ID_RELOAD, w!("Ayarı yeniden yükle"));
        let _ = AppendMenuW(m, MF_STRING, ID_LOG, w!("Log'u aç"));
        let _ = AppendMenuW(m, MF_STRING, ID_FIREWALL, w!("Firewall kuralı ekle (yönetici)…"));
        let checked = if autostart_enabled() { MF_CHECKED } else { Default::default() };
        let _ = AppendMenuW(m, MF_STRING | checked, ID_AUTO, w!("Windows ile başlat"));
        let _ = AppendMenuW(m, MF_SEPARATOR, 0, None);
        let _ = AppendMenuW(m, MF_STRING, ID_EXIT, w!("Çıkış"));
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        // Menü dışına tıklayınca kapanması için (KB135788)
        let _ = SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(m, TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON, pt.x, pt.y, None, hwnd, None);
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(m);
        match cmd.0 as usize {
            ID_EXIT => {
                let _ = DestroyWindow(hwnd);
            }
            ID_AUTO => autostart_set(!autostart_enabled()),
            ID_RELOAD => reload(hwnd),
            ID_CFG => {
                // Değişiklik yeniden başlatınca geçerli (Çıkış → tekrar aç).
                let p = wide(&cfg.display().to_string());
                ShellExecuteW(Some(hwnd), w!("open"), w!("notepad.exe"), PCWSTR(p.as_ptr()), None, SW_SHOWNORMAL);
            }
            ID_LOG => {
                if let Some(p) = crate::log::path() {
                    let p = wide(&p.display().to_string());
                    ShellExecuteW(Some(hwnd), w!("open"), w!("notepad.exe"), PCWSTR(p.as_ptr()), None, SW_SHOWNORMAL);
                }
            }
            // UAC tek kez: cmd.exe yükseltilir, agent yükseltilmemiş kalır. Sonuç dönmez (runas asenkron).
            ID_FIREWALL => {
                let Some(exe) = std::env::current_exe().ok().and_then(|p| p.to_str().map(String::from)) else { return };
                let args = wide(&crate::firewall_args(port, &exe));
                let r = ShellExecuteW(Some(hwnd), w!("runas"), w!("cmd.exe"), PCWSTR(args.as_ptr()), None, SW_HIDE);
                log!("firewall: kural istendi (port {port}, sonuç {})", r.0 as isize);
            }
            _ => {}
        }
    }
}

fn reload(hwnd: HWND) {
    // Kancayı çağırırken STATE ödünç alınmaz: hata kutusu modal döngüsü wndproc'u (notify) çağırabilir.
    let r = RELOAD.with_borrow_mut(|f| f.as_mut().map(|f| f()));
    match r {
        Some(Ok(info)) => {
            STATE.with_borrow_mut(|s| {
                if let Some((i, _)) = s {
                    *i = info;
                }
            });
            notify(hwnd, NIM_MODIFY);
        }
        Some(Err(e)) => error_box(&e),
        None => {}
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_CLIENT => notify(hwnd, NIM_MODIFY),
        WM_TRAY if lp.0 as u32 == WM_RBUTTONUP => menu(hwnd),
        // Explorer yeniden başladı → ikon kayboldu, yeniden ekle
        m if m == TASKBAR_CREATED.get() && m != 0 => notify(hwnd, NIM_ADD),
        WM_DESTROY => {
            // SAFETY: nid yalnız kimlik alanlarıyla; ikon kaldırılır.
            unsafe {
                let nid = NOTIFYICONDATAW { cbSize: size_of::<NOTIFYICONDATAW>() as u32, hWnd: hwnd, uID: 1, ..Default::default() };
                let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
                PostQuitMessage(0);
            }
        }
        // SAFETY: işlenmeyen mesajlar varsayılan işleyiciye.
        _ => return unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
    LRESULT(0)
}

/// Mesaj döngüsü; "Çıkış" ile döner.
pub fn run(info: Info, hub: Arc<Hub>, reload: Reload) {
    // Hub (client thread'leri) → tray: yalnız mesaj bırakır, UI işi bu thread'de.
    let _ = hub.on_client.set(Box::new(|| {
        // SAFETY: pencere yoksa PostMessage yalnız başarısız döner.
        let _ = unsafe { PostMessageW(Some(HWND(HWND_.load(Ordering::Relaxed) as _)), WM_CLIENT, WPARAM(0), LPARAM(0)) };
    }));
    STATE.set(Some((info, hub)));
    RELOAD.set(Some(reload));
    // SAFETY: sınıf/pencere bu thread'de oluşturulur ve mesaj döngüsü aynı thread'de.
    unsafe {
        TASKBAR_CREATED.set(RegisterWindowMessageW(w!("TaskbarCreated")));
        let class = w!("LowDeckAgent");
        let Ok(inst) = GetModuleHandleW(None) else { return };
        let wc = WNDCLASSW { lpfnWndProc: Some(wndproc), hInstance: inst.into(), lpszClassName: class, ..Default::default() };
        RegisterClassW(&wc);
        // Görünmez üst düzey pencere (HWND_MESSAGE değil: SetForegroundWindow + menü için)
        let Ok(hwnd) = CreateWindowExW(WINDOW_EX_STYLE(0), class, class, WINDOW_STYLE(0), 0, 0, 0, 0, None, None, Some(inst.into()), None) else {
            return;
        };
        HWND_.store(hwnd.0 as usize, Ordering::Relaxed);
        notify(hwnd, NIM_ADD);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tooltip_and_icon() {
        let i = Info { bind: "192.168.1.20".into(), port: 47810, token: "t".into(), cfg_path: PathBuf::new() };
        assert_eq!(tooltip(&i, Some("toco")), "LowDeck · 192.168.1.20:47810 · telefon: toco");
        assert!(tooltip(&i, None).ends_with("bekliyor"));
        assert!(tooltip(&i, Some("x")).encode_utf16().count() < 128, "szTip sınırı");
        let (on, off) = (pixels(true), pixels(false));
        assert_eq!(on[7 * N + 7], 0xFF22_C55E);
        assert_eq!(off[7 * N + 7], 0xFF80_8080);
        assert_eq!(on[0], 0, "köşe saydam");
    }
}

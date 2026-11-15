//! Windows: tek örnek, `UiExe` kaydı (LM tray "Arayüzü aç"), yerel saat farkı.

use windows::core::w;
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
use windows::Win32::System::Registry::{RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE};

/// İkinci kopya mevcut pencereyi öne getirip çıkar (tray "Arayüzü aç" tekrar tıklanınca).
pub fn single_instance() -> bool {
    // SAFETY: isimli mutex süreç ömrü boyunca açık kalır (handle bilerek kapatılmaz); dizgeler sabit.
    unsafe {
        let _ = CreateMutexW(None, false, w!(r"Local\LowDeckDesktop"));
        if GetLastError() != ERROR_ALREADY_EXISTS {
            return true;
        }
        if let Ok(h) = FindWindowW(None, w!("LowMonitor")) {
            let _ = ShowWindow(h, SW_RESTORE);
            let _ = SetForegroundWindow(h);
        }
    }
    false
}

/// `HKCU\Software\LowMonitor` `UiExe` = bu exe (ADR-0003 madde 6). Her açılışta: exe taşınırsa güncel kalır.
pub fn register_ui_exe() {
    let Some(exe) = std::env::current_exe().ok().and_then(|p| p.to_str().map(String::from)) else { return };
    let data: Vec<u16> = exe.encode_utf16().chain([0]).collect();
    // shortcut: kayıt hatası sessiz; yalnız tray kısayolu etkilenir
    // SAFETY: NUL sonlu dizgeler; veri boyutu NUL dahil byte.
    unsafe {
        let _ = RegSetKeyValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\LowMonitor"),
            w!("UiExe"),
            REG_SZ.0,
            Some(data.as_ptr().cast()),
            (data.len() * 2) as u32,
        );
    }
}

/// `GetTimeZoneInformation` dönüşü (winnt.h); windows crate'inde sabit yok.
const TIME_ZONE_ID_DAYLIGHT: u32 = 2;

/// Yerel = UTC + dönüş (saniye).
pub fn utc_offset_s() -> i64 {
    let mut tz = TIME_ZONE_INFORMATION::default();
    // SAFETY: tz yerel, fonksiyon yalnız ona yazar.
    let id = unsafe { GetTimeZoneInformation(&mut tz) };
    let bias = tz.Bias + if id == TIME_ZONE_ID_DAYLIGHT { tz.DaylightBias } else { tz.StandardBias };
    -(bias as i64) * 60
}

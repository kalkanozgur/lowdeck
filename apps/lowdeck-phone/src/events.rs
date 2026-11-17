//! Olay satırı biçimi ve yerel saat. LowMonitor `lowmon-ui/src/main.rs` ile aynı metinler (yalnız Türkçe).

use lowdeck_proto::EventItem;

/// Olay açıklaması. Sistem adları LM `lowmon-svc` ile aynı.
pub fn desc(e: &EventItem) -> String {
    let s = match (e.kind, e.name.as_str()) {
        (2, "service_start") => "Servis başladı",
        (2, "config_invalid") => "config.toml hatalı; varsayılan ayarlar kullanılıyor",
        (2, "pipe_unavailable") => "Pipe açılamadı; tray ve arayüz servise bağlanamaz",
        (2, "integrity") => "Kayıt dosyasında bozuk bölüm bulundu, atlandı (önceki kapanış temiz değil)",
        (2, "retention_ok") => return format!("Eski kayıtlar temizlendi ({})", e.value),
        (2, "retention_error") => "Eski kayıtlar temizlenemedi",
        (2, "control_pause") => "Kayıt duraklatıldı",
        (2, "control_resume") => "Kayıt sürdürüldü",
        (2, "control_reload") if e.value != 0 => "Ayarlar yeniden yüklendi",
        (2, "control_reload") => "Ayarlar yeniden yüklenemedi (config.toml hatalı)",
        (2, "write_error") => "Kayıt dosyasına yazılamadı",
        (2, _) => "",
        (k, name) => {
            // shortcut: metrik kural adından tahmin (LM ile aynı); alarm tanımı proto'ya gelirse oradan
            let what = match name.split(['_', '.']).next().unwrap_or("") {
                "cpu" => "CPU kullanımı",
                "ram" | "mem" => "Bellek kullanımı",
                "disk" => "Disk",
                "net" => "Ağ",
                _ => "Ölçüm",
            };
            return format!("{what} {}", if k == 0 { "eşiği aştı" } else { "normale döndü" });
        }
    };
    s.into()
}

/// Alarm değeri × 100 gelir; sistem olayının değeri kod → gösterilmez.
// shortcut: birim addan tahmin (cpu_/ram_ → %), LM ile aynı
pub fn value(e: &EventItem) -> String {
    if e.kind == 2 {
        return String::new();
    }
    let unit = if e.name.starts_with("cpu") || e.name.starts_with("ram") { "%" } else { "" };
    format!("{:.1}{unit}", e.value as f64 / 100.0)
}

/// `MM-DD HH:MM:SS` (yerel). Gün → takvim: H. Hinnant `civil_from_days`.
pub fn fmt_time(ts_ms: u64, offset_s: i64) -> String {
    let s = ts_ms as i64 / 1000 + offset_s;
    let (days, sod) = (s.div_euclid(86_400), s.rem_euclid(86_400));
    let z = days + 719_468;
    let doe = z - z.div_euclid(146_097) * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{m:02}-{d:02} {:02}:{:02}:{:02}", sod / 3600, sod % 3600 / 60, sod % 60)
}

/// Yerel saat farkı, s.
// shortcut: açılışta bir kez alınır (yaz saatinde uygulama yeniden açılana kadar 1 sa kayar); masaüstü geliştirmede (Windows) UTC
pub fn local_offset_s() -> i64 {
    #[cfg(unix)]
    {
        // SAFETY: `time(null)` ve `localtime_r` yalnız yerel değişkenlere yazar; tm sıfırla başlatılır.
        unsafe {
            let t = libc::time(std::ptr::null_mut());
            let mut tm: libc::tm = std::mem::zeroed();
            if !libc::localtime_r(&t, &mut tm).is_null() {
                return tm.tm_gmtoff as i64;
            }
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format() {
        // 2026-11-18 01:02:03 UTC + 3 sa
        assert_eq!(fmt_time(1_794_963_723_000, 3 * 3600), "11-18 04:02:03");
        let e = |kind, name: &str, value| EventItem { ts_ms: 0, kind, name: name.into(), value };
        assert_eq!(value(&e(0, "cpu_high", 5920)), "59.2%");
        assert_eq!(value(&e(2, "service_start", 1)), "");
        assert_eq!(desc(&e(0, "cpu_high", 0)), "CPU kullanımı eşiği aştı");
        assert_eq!(desc(&e(1, "ram_high", 0)), "Bellek kullanımı normale döndü");
        assert_eq!(desc(&e(2, "service_start", 0)), "Servis başladı");
    }
}

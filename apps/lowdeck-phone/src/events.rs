//! Olay satırı biçimi ve yerel saat. LowMonitor `lowmon-ui/src/main.rs` ile aynı metinler (İngilizce; kullanıcı kararı 2026-11-18).

use lowdeck_proto::EventItem;

/// Olay açıklaması. Sistem adları LM `lowmon-svc` ile aynı.
pub fn desc(e: &EventItem) -> String {
    let s = match (e.kind, e.name.as_str()) {
        (2, "service_start") => "Service started",
        (2, "config_invalid") => "config.toml invalid; using defaults",
        (2, "pipe_unavailable") => "Pipe unavailable; tray and UI cannot connect",
        (2, "integrity") => "Corrupt log section found and skipped (unclean shutdown)",
        (2, "retention_ok") => return format!("Old logs pruned ({})", e.value),
        (2, "retention_error") => "Pruning old logs failed",
        (2, "control_pause") => "Recording paused",
        (2, "control_resume") => "Recording resumed",
        (2, "control_reload") if e.value != 0 => "Settings reloaded",
        (2, "control_reload") => "Settings reload failed (config.toml invalid)",
        (2, "write_error") => "Log write failed",
        (2, "uac_unavailable") => return format!("UAC notifications unavailable (code {})", e.value),
        (2, _) => "",
        // LM ADR-0037 (servis) ve tray (#69 sıcaklık, #79 Claude); kind 0 = açıldı, 1 = kapandı
        (0, "uac") => "UAC prompt open",
        (1, "uac") => "UAC prompt closed",
        (0, "firewall") => "Firewall permission prompt open",
        (1, "firewall") => "Firewall permission prompt closed",
        (0, "Claude.Input") => "Claude is waiting for input",
        (1, "Claude.Input") => "Claude input answered",
        (k, name) if name.starts_with("Temp.") => {
            let state = if k == 0 { "crossed threshold" } else { "back to normal" };
            return format!("{} temperature {state}", &name[5..]);
        }
        (k, name) => {
            // shortcut: metrik kural adından tahmin (LM ile aynı); alarm tanımı proto'ya gelirse oradan
            let what = match name.split(['_', '.']).next().unwrap_or("") {
                "cpu" => "CPU usage",
                "ram" | "mem" => "Memory usage",
                "disk" => "Disk",
                "net" => "Network",
                _ => "Metric",
            };
            return format!("{what} {}", if k == 0 { "crossed threshold" } else { "back to normal" });
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
    let unit = match e.name.as_str() {
        "uac" | "firewall" | "Claude.Input" => return String::new(),
        n if n.starts_with("Temp.") => " °C",
        n if n.starts_with("cpu") || n.starts_with("ram") => "%",
        _ => "",
    };
    format!("{:.1}{unit}", e.value as f64 / 100.0)
}

/// Alarm bandı için kısa ad; bilinmeyen ad olduğu gibi.
pub fn short(name: &str) -> String {
    match name {
        "uac" => "UAC".into(),
        "firewall" => "Firewall".into(),
        "Claude.Input" => "Claude waiting".into(),
        n => n.strip_prefix("Temp.").map_or_else(|| n.into(), |x| format!("{x} temp")),
    }
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
        assert_eq!(desc(&e(0, "cpu_high", 0)), "CPU usage crossed threshold");
        assert_eq!(desc(&e(1, "ram_high", 0)), "Memory usage back to normal");
        assert_eq!(desc(&e(2, "service_start", 0)), "Service started");
        assert_eq!(desc(&e(2, "uac_unavailable", 5)), "UAC notifications unavailable (code 5)");
        assert_eq!(desc(&e(0, "uac", 0)), "UAC prompt open");
        assert_eq!(desc(&e(1, "firewall", 0)), "Firewall permission prompt closed");
        assert_eq!(desc(&e(0, "Claude.Input", 0)), "Claude is waiting for input");
        assert_eq!(desc(&e(0, "Temp.GPU", 0)), "GPU temperature crossed threshold");
        assert_eq!(value(&e(0, "Temp.CPU", 8500)), "85.0 °C");
        assert_eq!(value(&e(0, "uac", 0)), "");
        assert_eq!(short("Temp.Disk"), "Disk temp");
        assert_eq!(short("Claude.Input"), "Claude waiting");
        assert_eq!(short("cpu_high"), "cpu_high");
    }
}

//! Masaüstü giriş noktası: telefon hazır olmadan UI ve protokolü PC'de geliştirmek için.
//! Ayar dosyası exe'nin yanındaki `data/` klasörüne yazılır.

fn main() -> Result<(), slint::PlatformError> {
    let data_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("data")));
    let name = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "desktop".into());
    lowdeck_phone::run(data_dir, &name)
}

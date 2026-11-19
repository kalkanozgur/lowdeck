//! Ayar dosyası: düz `key=value`. Bağımlılık yok.

use lowdeck_proto::DEFAULT_PORT;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub token: String,
    /// Keşifte / bağlanınca görülen PC adı; otomatik yeniden bağlanmada aynı PC'yi seçmek için. Boş = bilinmiyor.
    pub name: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self { host: String::new(), port: DEFAULT_PORT, token: "dev".into(), name: String::new() }
    }
}

pub fn load(path: Option<&Path>) -> AppConfig {
    let mut c = AppConfig::default();
    let Some(text) = path.and_then(|p| std::fs::read_to_string(p).ok()) else {
        return c;
    };
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else { continue };
        let v = v.trim();
        match k.trim() {
            "host" => c.host = v.to_string(),
            "port" => c.port = v.parse().unwrap_or(DEFAULT_PORT),
            "token" => c.token = v.to_string(),
            "name" => c.name = v.to_string(),
            _ => {}
        }
    }
    c
}

/// Ayar ekranı girdisini doğrular. Hata metni doğrudan UI'da gösterilir.
pub fn parse(host: &str, port: &str, token: &str) -> Result<AppConfig, &'static str> {
    let host = host.trim();
    // IPv4, DNS adı ya da IPv6 (köşeli parantezsiz). Boşluk/satır sonu dosyayı bozar.
    if host.is_empty() {
        return Err("PC address is empty");
    }
    if host.len() > 253 || !host.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':')) {
        return Err("Invalid PC address (letters, digits, . - : only)");
    }
    let port = match port.trim().parse::<u16>() {
        Ok(p) if p != 0 => p,
        _ => return Err("Port must be 1–65535"),
    };
    if token.is_empty() || token.contains(['\n', '\r']) {
        return Err("Token is empty or contains a line break");
    }
    Ok(AppConfig { host: host.to_string(), port, token: token.to_string(), name: String::new() })
}

pub fn save(path: Option<&Path>, c: &AppConfig) {
    if let Some(p) = path {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        // Ad sunucudan gelir (≤ 63 karakter); satır sonu dosyayı bozmasın
        let name = c.name.replace(['\n', '\r'], " ");
        let _ = std::fs::write(p, format!("host={}\nport={}\ntoken={}\nname={name}\n", c.host, c.port, c.token));
    }
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn valid() {
        let c = parse(" 192.168.1.20 ", "47810", "a=b").unwrap();
        assert_eq!((c.host.as_str(), c.port, c.token.as_str()), ("192.168.1.20", 47810, "a=b"));
        assert!(parse("pc-salon.local", "1", "t").is_ok());
        assert!(parse("fe80::1", "65535", "t").is_ok());
    }

    #[test]
    fn invalid() {
        assert!(parse("", "47810", "t").is_err());
        assert!(parse("192.168.1.20 x", "47810", "t").is_err());
        assert!(parse("http://pc", "47810", "t").is_err());
        for p in ["0", "65536", "-1", "", "abc"] {
            assert!(parse("pc", p, "t").is_err(), "port {p:?}");
        }
        assert!(parse("pc", "47810", "").is_err());
        assert!(parse("pc", "47810", "a\nhost=evil").is_err());
    }
}

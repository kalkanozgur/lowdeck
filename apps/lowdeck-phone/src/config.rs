//! Ayar dosyası: düz `key=value`. Bağımlılık yok.

use lowdeck_proto::DEFAULT_PORT;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub token: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self { host: String::new(), port: DEFAULT_PORT, token: "dev".into() }
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
            _ => {}
        }
    }
    c
}

pub fn save(path: Option<&Path>, c: &AppConfig) {
    if let Some(p) = path {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(p, format!("host={}\nport={}\ntoken={}\n", c.host, c.port, c.token));
    }
}

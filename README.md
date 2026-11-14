# LowDeck
Eski Xiaomi Mi Note 10 Lite'ı aynı LAN'daki Windows PC için minimum tüketimli kontrol paneline (ses, medya) çeviren Rust + Slint uygulaması.

| Alan | Değer |
|---|---|
| Durum | Prototip (Faz 1 — masaüstünde telefon UI'ı) |
| Modüller | yazılım (Rust workspace: proto, client, phone, mock-server) |
| Başlangıç | 2026-11-14 |

## Klasör yapısı
```
crates/lowdeck-proto     protokol tipleri + çerçeveleme
crates/lowdeck-client    UI'dan bağımsız istemci
apps/lowdeck-phone       Slint uygulaması (Android + masaüstü)
tools/mock-server        sahte PC sunucusu
docs/                    PROTOCOL, gereksinimler, devlog, kararlar
.github/                 CI (APK artifact) + release
```

## Hızlı başlangıç
```powershell
cargo run -p mock-server -- 47800 dev
cargo run -p lowdeck-phone --bin lowdeck-desktop   # ⚙ → 127.0.0.1 / 47800 / dev
```
APK GitHub Actions'ta derlenir (bkz. `CLAUDE.md` Faz 0).

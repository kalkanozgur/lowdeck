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
cargo run -p mock-server -- 47810 dev
cargo run -p lowdeck-phone --bin lowdeck-phone-dev   # ⚙ → 127.0.0.1 / 47810 / dev
cargo run -p lowdeck-client --example cli -- 127.0.0.1 47810 dev   # UI'sız istemci
```
APK GitHub Actions'ta derlenir (bkz. `CLAUDE.md` Faz 0).

## PC tarafı (LowMonitor)
Gerçek PC sunucusu ve PC arayüzü LowMonitor reposunda (LM ADR-0031): sunucu `lowmon-tray` içinde (`lowmon-deck`), arayüz `lowmon-ui`. Kurulum LowMonitor `scripts\install.ps1`.
- Telefon ayarı (host · port · token): LowMonitor tray → **Telefon ▸** ilk satır; ayar `%APPDATA%\LowMonitor\deck.toml` (eski `agent.cfg`'den taşınır, telefon ayarı değişmez).
- Telefon: ⚙ → **PC'yi bul** (aynı Wi-Fi) → listeden seç. Bulamıyorsa: tray → Telefon ▸ **Firewall kuralı ekle** (TCP port + UDP 47811 keşif).

## Lisans
MIT ya da Apache-2.0, seçim sizin ([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)).

Arayüz [Slint](https://slint.dev) ile yapıldı ([Royalty-free 2.0](https://github.com/slint-ui/slint/blob/master/LICENSES/LicenseRef-Slint-Royalty-free-2.0.md) lisansı; atıf şartı):

<a href="https://slint.dev"><img alt="Made with Slint" src="https://raw.githubusercontent.com/slint-ui/slint/master/logo/MadeWithSlint-logo-light.svg" height="60"></a>

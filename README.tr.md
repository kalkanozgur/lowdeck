# LowDeck
Eski Xiaomi Mi Note 10 Lite'ı aynı LAN'daki Windows PC için minimum tüketimli kontrol paneli ve izleme ekranına çeviren Rust + Slint uygulaması. PC tarafı [LowMonitor](https://github.com/kalkanozgur/lowmonitor)'da.

English: [README.md](README.md)

| Alan | Değer |
|---|---|
| Durum | v1.0.0 — Mi Note 10 Lite (Android 12) ve Samsung M21'de denendi |
| Modüller | yazılım (Rust workspace: proto, client, phone, mock-server) |
| Başlangıç | 2026-11-14 |

## Özellikler (telefon)
| Sekme | İçerik |
|---|---|
| Control | PC ses / sessiz, medya (oynat/duraklat, önceki/sonraki, çalan parça), PC klavyesini kilitle |
| Live | Claude kullanım bandı, CPU / GPU / RAM / disk / ağ kartları ve 5 dk grafikleri, °C, çekirdekler, top-5 |
| Events | LowMonitor alarm ve sistem olayları (son 24 sa); alarm bandı her sekmede |
| History | 1 sa / 6 sa / 24 sa / 7 g: CPU, RAM, disk, ağ, GPU, sıcaklıklar |
| Processes | CPU / RAM sıralama, arama, iki aşamalı sonlandırma (korumalı süreçler hariç) |
| AI | Claude limit çizgileri, token dökümü, oturumlar |
| Settings | PC adresi, port, token, "Find PC"; PC AI ayarı (`ai.toml`) |

- Yatay tam ekran, dikey sekme çarkı; grafiğe dokununca zaman + değer.
- 120 s dokunulmazsa siyah perde; perde inikken yalnız alarm / AI push'ları gelir. Alarmda (kullanım kuralları ya da PC sıcaklık eşikleri) ekran açılır; PC meşgulken (aktif alarm ya da Claude limiti ≥ %80) 10 dk'da bir 15 s açılır.
- PC'yi UDP keşifle bulur; PC'nin IP'si değişince (DHCP) aynı PC'yi adıyla yeniden bulup bağlanır.
- Arka planda ağ trafiği yok (uygulama duraklayınca soket kapanır).

## Klasör yapısı
```
crates/lowdeck-proto     protokol tipleri + çerçeveleme (tek doğruluk kaynağı; LowMonitor git rev ile kullanır)
crates/lowdeck-client    UI'dan bağımsız istemci (+ examples/cli.rs)
apps/lowdeck-phone       Slint uygulaması (Android + masaüstü geliştirme)
tools/mock-server        sahte PC sunucusu
docs/                    PROTOCOL, gereksinimler, devlog, kararlar
.github/                 CI (test + APK artifact) ve tag tetikli release
```

## Hızlı başlangıç
Telefona kurulum: [Releases](https://github.com/kalkanozgur/lowdeck/releases)'tan `lowdeck-vX.Y.Z.apk` indir ya da `adb install -r lowdeck-*.apk`.

PC tarafı: LowMonitor'u kur (`scripts\install.ps1`); telefon sunucusu `lowmon-tray` içinde çalışır.
- Telefon ayarı (host · port · token): LowMonitor tray → **Telefon ▸** ilk satır; ayar `%APPDATA%\LowMonitor\deck.toml`.
- Telefon: Settings → **Find PC** (aynı Wi-Fi) → listeden seç, token'ı gir. Bulamıyorsa: tray → Telefon ▸ **Firewall kuralı ekle** (TCP port + UDP 47811 keşif).

Geliştirme (masaüstü):
```powershell
cargo run -p mock-server -- 47810 dev
cargo run -p lowdeck-phone --bin lowdeck-phone-dev   # Settings → 127.0.0.1 / 47810 / dev
cargo run -p lowdeck-client --example cli -- 127.0.0.1 47810 dev   # UI'sız istemci
```
APK yalnız GitHub Actions'ta derlenir (lokalde Android araç zinciri yok); bkz. `CLAUDE.md` Faz 0.

## Yapay zeka ajanı devri
Son durum ve sıradaki adımlar `HANDOFF.md`'de; kurallar `CLAUDE.md`'de.

## Lisans
MIT ya da Apache-2.0, seçim sizin ([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)).

Arayüz [Slint](https://slint.dev) ile yapıldı ([Royalty-free 2.0](https://github.com/slint-ui/slint/blob/master/LICENSES/LicenseRef-Slint-Royalty-free-2.0.md) lisansı; atıf şartı):

<a href="https://slint.dev"><img alt="Made with Slint" src="https://raw.githubusercontent.com/slint-ui/slint/master/logo/MadeWithSlint-logo-light.svg" height="60"></a>

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
apps/lowdeck-desktop     LowMonitor PC arayüzü (Canlı + Olaylar + Geçmiş, agent loopback)
tools/mock-server        sahte PC sunucusu
tools/lowdeck-agent      gerçek PC sunucusu (Windows: Core Audio + SMTC)
docs/                    PROTOCOL, gereksinimler, devlog, kararlar
.github/                 CI (APK artifact) + release
```

## Hızlı başlangıç
```powershell
cargo run -p mock-server -- 47810 dev
cargo run -p lowdeck-phone --bin lowdeck-phone-dev   # ⚙ → 127.0.0.1 / 47810 / dev
cargo run -p lowdeck-desktop                          # agent.cfg'den; mock için: -- 47810 dev

# Gerçek PC sunucusu (mock yerine), tray'de çalışır: sağ tık → telefon ayarı / Windows ile başlat
# ayar %APPDATA%\LowDeck\agent.cfg (ilk açılışta LAN IP + rastgele token ile oluşur)
cargo run --release -p lowdeck-agent
```
APK GitHub Actions'ta derlenir (bkz. `CLAUDE.md` Faz 0).

## PC kurulumu (ADR-0004)
1. Release sayfasından `lowdeck-pc-vX.Y.Z.zip`'i indir, içindeki iki exe'yi `%LOCALAPPDATA%\LowDeck\`'e aç (**ikisi aynı klasörde**).
2. `lowdeck-agent.exe`'yi bir kez çalıştır → tray ikonu → **Windows ile başlat**. Agent `agent.cfg`'yi oluşturur ve `HKCU\Software\LowMonitor\UiExe`'yi yazar.
3. PC arayüzü: LowMonitor tray → **Arayüzü aç**. Agent kapalıysa masaüstü onu kendisi başlatır.
4. Telefon: agent tray menüsünün ilk satırındaki host / port / token.
5. Telefon bağlanamıyorsa: agent tray → **Firewall kuralı ekle (yönetici)…** (UAC; yalnız Private profil + yerel alt ağ, bu exe ve port). Port değişirse tekrar çalıştır.

Agent log'u: `%APPDATA%\LowDeck\agent.log` (1 MiB'ta `agent.log.1`'e döner); tray → **Log'u aç**.

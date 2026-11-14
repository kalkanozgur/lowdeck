# LowDeck — Claude Code proje talimatı

> Bu dosya Claude Code için. Dil: **Türkçe**. Üslup: kısa, teknik, dolgusuz; giriş/kapanış cümlesi yok.
> Karar gerektiren sorular tıklanabilir çoktan seçmeli kartla (AskUserQuestion) sorulur.

## 1. Ne yapıyoruz
Eski **Xiaomi Mi Note 10 Lite** (toco, SD730G, AMOLED) telefonu aynı LAN'daki Windows PC için **minimum tüketimli bir kontrol paneline** dönüştüren uygulama.

| Kapsam | Durum |
|---|---|
| PC ses seviyesi / sessiz | **Faz 1 — şimdi** |
| Medya kontrolü (play/pause, ileri, geri) + çalan parça bilgisi | **Faz 1 — şimdi** |
| Tuş kilidi (kedi-kilit tetikleme ya da ajan içinde hook) | **Sonra**, karar verilmedi |
| LowMonitor kaynak verisini telefonda gösterme | Sonra |

**Öncelik: telefon tarafı.** PC tarafı ayrı, küçük bir exe: `tools/lowdeck-agent` (Rust, kullanıcı oturumu, ADR-0002). O hazır olana kadar `tools/mock-server` kullanılır.

## 2. Mimari kararlar (değiştirmeden önce kullanıcıya sor)
| Karar | Seçim | Neden |
|---|---|---|
| Dil | **Rust** (her yerde) | LowMonitor de Rust; protokol tipleri tek crate'ten paylaşılır |
| Telefon UI | **Slint** | Cross-platform (Android, Windows, Linux, iOS), native çizim, WebView/JS motoru yok. Aynı UI masaüstünde de çalışır, telefon tamir edilmeden geliştirme sürer |
| Taşıma | **Tek kalıcı TCP bağlantı**, sunucu durum değişince PUSH eder | Polling yok, telefon CPU'su boşta uyur |
| Kodlama | `[u16 LE uzunluk][postcard]` | Komut çerçevesi 3–4 bayt, JSON parser yok |
| Async | **Yok** (tokio vb. yok), 2 std thread | Binary boyutu ve uyanma sayısı düşük |
| Kimlik doğrulama | Hello içinde paylaşılan token; sonra LowMonitor §16.2 tarzı eşleştirme | Sadece LAN; TLS gereksiz yük. Bind LAN IP + Private firewall |
| Sunucu | Ayrı `lowdeck-agent`, kullanıcı oturumu, TCP 47810 ([ADR-0002](docs/kararlar/ADR-0002-lowdeck-agent.md)) | Core Audio/SMTC session 0'da çalışmaz; LowMonitor servisi ağa kapalı, `lowmon-net` salt-okuma + WS/JSON + M6 |

Lisans notu: Slint, GPL-3 ya da royalty-free lisansla kullanılıyor (masaüstü/mobil kişisel proje için uygun). Kapalı kaynak dağıtılacaksa lisans koşulları yeniden gözden geçirilir.

## 3. Klasör yapısı
```
lowdeck/
├─ CLAUDE.md
├─ Cargo.toml                 # workspace + release profili (opt "s", LTO, strip, panic=abort)
├─ crates/
│  ├─ lowdeck-proto/          # mesaj tipleri + çerçeveleme (telefon + agent + mock)
│  └─ lowdeck-client/         # UI'dan bağımsız istemci: bağlan/yeniden bağlan/ping/olay
│     └─ examples/cli.rs      # UI'sız uçtan uca test istemcisi
├─ apps/lowdeck-phone/        # Slint uygulaması
│  ├─ ui/main.slint           # tek ekran + ayar paneli
│  └─ src/{lib.rs,main.rs,config.rs}   # lib = Android cdylib, main = masaüstü
├─ tools/mock-server/         # sahte sunucu (agent gelene kadar)
├─ tools/lowdeck-agent/       # gerçek PC sunucusu: Core Audio + SMTC (src/win.rs)
├─ .github/                   # CI (test+APK) ve tag tetikli release workflow'ları
├─ README.md · CHANGELOG.md · TODO.md
└─ docs/
   ├─ PROTOCOL.md             # protokol sözleşmesi
   ├─ gereksinimler.md
   ├─ devlog/YYYY-MM.md       # aylık devlog, en yeni girdi altta
   ├─ kararlar/ADR-XXXX-*.md  # geri dönüşü pahalı kararlar
   └─ referans/               # datasheet, API dokümanı, PDF
```

## 4. Komutlar
```powershell
# Test + lint
cargo test --workspace
cargo clippy --workspace -- -D warnings

# Masaüstünde geliştirme döngüsü (iki terminal)
cargo run -p mock-server -- 47810 dev
cargo run -p lowdeck-phone --bin lowdeck-desktop
#   ilk açılışta ⚙ → host=127.0.0.1, port=47810, token=dev

# Gerçek PC sunucusu (Windows, tray'de çalışır; ayar: %APPDATA%\LowDeck\agent.cfg, yoksa LAN IP + rastgele token ile oluşturur)
cargo run --release -p lowdeck-agent

# UI'sız test
cargo run -p lowdeck-client --example cli -- 127.0.0.1 47810 dev
#   v 70 | + | - | m | p | n | b | q

# Android APK: lokalde değil, GitHub Actions'ta (bkz. Faz 0)
gh pr create --fill                 # PR → CI APK artifact
gh run download --name lowdeck-<sha> # artifact'ı indir (isteğe bağlı)
adb install -r lowdeck-*.apk        # telefona kur (USB hata ayıklama açıksa)
```

## 5. Kodlama kuralları
### Tüketim (öncelik 1)
- **Sürekli Timer, animasyon, polling yasak.** Yeniden çizim yalnızca property değişince olur. Yeni periyodik iş eklemeden önce kullanıcıya sor.
- Tek periyodik uyanma: istemcideki Ping (25 s). İstisna: boşta perde timer'ı tek-atışlık, dokunuşta yeniden kurulur (periyodik değil). Ölü bağlantı tespiti `READ_TIMEOUT` (60 s) ile.
- Arka plan rengi saf siyah (`#000000`): AMOLED'de siyah piksel güç harcamaz. Büyük beyaz/açık alan kullanılmaz.
- Slider gibi sürekli girdiler throttle'lanır (şu an 50 ms ≈ 20 Hz). Son değer tek seferlik timer'la mutlaka gönderilir.
- Okuma döngüsünde tahsis yok (çerçeve buffer'ı yeniden kullanılır). Yeni bağımlılık eklemeden önce boyut ve gerekçe söylenir.
- Release profili `opt-level="s"`, LTO, strip. APK boyutu her faz sonunda devlog'a yazılır.

### Protokol
- `lowdeck-proto` **tek doğruluk kaynağı.** Enum varyantları **yalnızca sona eklenir** (postcard varyant indeksini kodlar). Kırıcı değişiklikte `PROTO_VERSION` artırılır ve `docs/PROTOCOL.md` güncellenir.
- Sunucu, bağlantı kurulunca tam durumu (Audio + Media) gönderir, sonrasında sadece değişiklikleri.

### Genel
- `unwrap()` yalnızca başlatmada; ağ ve girdi yolunda hata `Result` ile taşınır.
- UI güncellemesi sadece event loop'ta: ağ thread'inden `upgrade_in_event_loop` ile.
- Her değişiklikten sonra `cargo test --workspace` ve `cargo clippy` temiz olmalı.
- Oturum sonunda `docs/devlog/YYYY-MM.md`'ye ekle: tarih, ne yapıldı, ölçümler (APK boyutu, RSS, CPU), açık sorular, sıradaki adım.

### Doküman / dosya
- Doküman Türkçe; kodda İngilizce tanımlayıcı, Türkçe yorum serbest.
- Birimler SI; tarih ISO (YYYY-MM-DD). Yeni dosya adları küçük harf, kebab-case, Türkçe karakter/boşluk yok.
- Mimari/protokol/bağımlılık gibi geri dönüşü pahalı kararlar → `docs/kararlar/ADR-XXXX-*.md`.
- Kullanıcıya görünen değişiklik ya da sürüm → `CHANGELOG.md` (Keep a Changelog).

### Sürümleme ve commit
- SemVer, tek kaynak `Cargo.toml` `[workspace.package] version`; tag `vX.Y.Z` (bkz. Faz 0).
- Conventional Commits, kapsam = crate/alan: `feat(proto)`, `feat(client)`, `feat(phone)`, `fix(mock)`, `ci:`, `docs:`, `chore:`.

## 6. Yol haritası

### Faz 0 — GitHub üzerinden build (Android araç zinciri lokalde YOK)
APK'lar GitHub Actions'ta derlenir. Lokalde sadece Rust + masaüstü build gerekir.

| Workflow | Tetik | Ne yapar |
|---|---|---|
| `.github/workflows/ci.yml` | PR, main push | test + clippy → imzalı APK → **artifact** (14 gün) |
| `.github/workflows/release.yml` | `v*` tag push | imzalı APK (onay = tag push; ortam onayı yok, bkz. not) → GitHub Release (+ sha256) |
| `.github/actions/build-apk` | ortak | JDK 17, Rust + `aarch64-linux-android`, SDK 34, NDK r27c, cargo-apk, imzalama |

Çalışma akışı:
1. Claude değişikliği **ayrı bir branch'te** yapar, PR açar. `main`'e doğrudan push yasak.
2. CI yeşil olmalı. APK artifact'ı PR'ın Checks sekmesinden indirilir.
3. Kullanıcı PR'ı GitHub'dan (mobil uygulama dahil) inceler ve merge eder.
4. Sürüm: `Cargo.toml` `[workspace.package] version` artırılır (ayrı PR) → merge → `git tag vX.Y.Z && git push --tags` (**tag'i yalnızca kullanıcı atar**; Claude tag push etmez) → Release'den APK telefona indirilir.

Tek seferlik kurulum (kullanıcı yapar):
- [ ] Kalıcı keystore (anahtar kaybolursa telefonda güncelleme yerine kaldır-kur gerekir; yedeği sakla):
  ```powershell
  keytool -genkeypair -keystore lowdeck-release.keystore -alias lowdeck -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=LowDeck"
  [Convert]::ToBase64String([IO.File]::ReadAllBytes("lowdeck-release.keystore")) | Set-Clipboard
  ```
- [ ] Repo → Settings → Secrets → Actions: `ANDROID_KEYSTORE_B64` (panodaki değer), `ANDROID_KEYSTORE_PASSWORD`.
- Not (2026-11-14): repo private + GitHub Free → branch koruması ve environment reviewer **kullanılamıyor** (API 403). PR akışı ve "main'e doğrudan push yok" kuralı disiplinle uygulanır; merge öncesi `check` + `apk` yeşil olmalı. Pro'ya geçilir ya da repo public olursa koruma + `environment: release` geri eklenir.
- [ ] Keystore dosyası **asla** repoya girmez (`.gitignore`'da `*.keystore`).

Workflow'lar ilk koşuda doğrulanacak; kırılırsa log okunup düzeltilir (ör. cargo-apk çıktı yolu, NDK sürümü). Slint'in güncel rehberi xbuild'i de gösteriyor; cargo-apk sorun çıkarırsa alternatif o.

### Faz 1 — Masaüstünde telefon UI'ı (şu an buradayız)
- [x] Protokol v1, istemci, mock sunucu, Slint ekranı (iskelet çalışıyor, mock ile uçtan uca test edildi)
- [x] Emoji ikonlar yerine gömülü SVG ikonlar (`@image-url("icons/…svg")`). Emoji fontu cihaza göre değişir.
- [x] Bağlantı yokken butonları pasifleştir. Kopma/yeniden bağlanma durumlarını UI'da test et (mock'u kapatıp aç).
- [x] Ayarlarda port/host doğrulaması.
- [ ] (Opsiyonel) PC keşfi: agent'ın UDP beacon'ı (`PROTOCOL.md` §5). Manuel IP yeterliyse ertelenir.

### Faz 2 — Android
- [ ] APK build. Telefon tamir edilene kadar başka bir Android cihazda ya da emülatörde test.
- [x] Ekran açık tutma: önde iken `KEEP_SCREEN_ON` (`AndroidApp::set_window_flags`, JNI yok) + 120 s dokunulmazsa tam siyah perde (tek-atışlık `Timer::restart`). Karar 2026-11-14.
- [x] Yaşam döngüsü: `init_with_event_listener` → `MainEvent::Pause` client drop, `Resume` yeniden bağlan. Cihazda ölçülecek.
- [ ] Wi-Fi uyku politikası ve yeniden bağlanma davranışını ölç.

### Faz 3 — Tüketim ölçümü ve ayar
Hedefler (ölçülecek, tahmin değil):
| Metrik | Hedef | Ölçüm |
|---|---|---|
| Boşta CPU (ekran sabit) | ≈ %0 | `adb shell top -p <pid>` |
| RSS | mümkün olduğunca düşük, ilk ölçüm referans | `adb shell dumpsys meminfo dev.kalkan.lowdeck` |
| Uyanma | 25 s'de 1 ping | `adb shell dumpsys batterystats` |
| APK boyutu | ilk build referans | dosya boyutu |
| Komut gecikmesi (dokunma → PC) | < 50 ms LAN | mock-server zaman damgası |
Renderer seçimi (femtovg/skia/software) ölçümle karşılaştırılır.

### Faz 4 — `tools/lowdeck-agent` (bu repo; ADR-0002)
- Kullanıcı oturumunda, `HKCU\…\Run` ile başlar (LowMonitor tray gibi). Servis değil.
- [x] İskelet + Hub (önbellek, yalnız değişince push, yeni bağlantı eskisini kapatır) + Core Audio + SMTC + VK_MEDIA fallback. Release 238 KB, private 2,5 MB, boşta 0 CPU.
- [x] Tray ikonu (yeşil = telefon bağlı, gri = bekliyor; tooltip host:port + telefon adı), menü: telefon ayar satırı, ayar dosyasını aç, **Windows ile başlat** (`HKCU\…\Run`), Çıkış. Konsolsuz (`windows_subsystem`), tek örnek, başlangıç hatası MessageBox.
- [ ] Log dosyası (şu an println konsolsuzda kayboluyor).
- [ ] Firewall kuralı (Private + yerel alt ağ); şu an Windows'un ilk dinlemede açtığı izin penceresine kalıyor.
- [ ] Telefon + agent LAN üzerinden gerçek cihazla gecikme ölçümü (< 50 ms hedefi).
- Bind: config'teki LAN IP; firewall kuralı yalnız Private profil + yerel alt ağ.
- LowMonitor konvansiyonları: `// SAFETY:` yorumu, async yok, Win32 için `windows-sys`. SMTC WinRT olduğu için `windows` crate gerekir (`windows-sys` WinRT içermez); Faz 4 başında exe boyutu ölçülüp sorulur.
- Ses: Windows Core Audio — `IAudioEndpointVolume` (seviye/mute) + `IAudioEndpointVolumeCallback` (**push için değişiklik bildirimi**, polling yok) + `IMMNotificationClient` (varsayılan cihaz değişimi). `windows` crate.
- Medya: `GlobalSystemMediaTransportControlsSessionManager` (SMTC): oturum, play/pause/next/prev, `MediaPropertiesChanged` / `PlaybackInfoChanged` olayları.
- Fallback: SMTC oturumu yoksa medya tuşları `SendInput` ile `VK_MEDIA_*`.

### Faz 5 — Tuş kilidi (karar bekliyor)
Seçenekler: (a) PC'de kedi-kilit'e IPC ile sinyal, (b) ajan içinde low-level keyboard/mouse hook, (c) keystroke ile tetikleme. Protokole `C2S::Lock(LockCmd)` + `S2C::Lock(LockState)` **sona** eklenir. Başlamadan kullanıcıya kartla sorulur.

## 7. Bağlam
- Telefon şu an ses codec arızası nedeniyle açılmıyor; tamiri ayrı bir proje. Uygulama **ses donanımı gerektirmez** (ses API'si kullanılmaz).
- Kullanıcı elektrik-elektronik mühendisi: gerekçeyi sayı ve ölçümle ver, yüzeysel açıklama yapma.

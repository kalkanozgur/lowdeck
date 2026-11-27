# Changelog
Format: Keep a Changelog. Sürümleme: SemVer (`CLAUDE.md` > Sürümleme ve commit).

## [Unreleased]
### Eklendi
- Telefon (ADR-0012): Control sekmesinde **Sleep / Shut down / Restart**. İki aşamalı onay (ilk dokunuş "Confirm", 3 s içinde ikinci dokunuş gönderir); kapatma/yeniden başlatma PC'de 10 s geri sayımla başlar, telefondaki **Cancel** ile iptal edilir. LowMonitor'un güç komutunu destekleyen sürümü gerekir (eski sürümde bağlantı düşer)
- Proto: `C2S::Power(PowerCmd)`, `S2C::PowerResult`, `POWER_OK` / `POWER_DISABLED` (ek varyant, protokol sürümü 4)
- mock-server `Power` yanıtı; cli `z` / `o` / `r` / `c`
### Düzeltildi
- Telefon: PC uykudan dönünce telefon ölü bağlantıda "Connected" kalıyordu (60 s okuma zaman aşımına kadar); bu arada basılan güç komutları sessizce kayboluyordu. Güç komutuna 3 s içinde yanıt gelmezse "No reply from PC, reconnecting" yazılır ve bağlantı hemen yeniden kurulur; Sleep başarılıysa telefon 1 s sonra bağlantıyı kendisi yeniler. `Client::reconnect()` eklendi
### Değişti
- Telefon: "Lock PC keyboard" düğmesi "Lock keys" oldu, güç düğmeleriyle aynı satırda

## [1.1.0] - 2026-11-22
LowMonitor v1.1.0 ile uyum + arka plan bildirimi. Protokol değişmedi (sürüm 4).
### Eklendi
- Telefon: LowMonitor v1.1.0 olayları okunur: UAC / güvenlik duvarı istemi (`uac`, `firewall`), Claude girdi bekliyor (`Claude.Input`), PC sıcaklık eşiği (`Temp.CPU/GPU/Disk`, değer °C), `uac_unavailable`. Alarm bandında kısa ad (ör. "UAC, CPU temp")
- Telefon (ADR-0011): uygulama arka plandayken / ekran kapalıyken de PC'ye bağlı kalır (foreground service, kalıcı "Listening for PC alerts" bildirimi); PC'de UAC ya da güvenlik duvarı istemi açılınca veya Claude girdi isteyince titreşimli Android bildirimi, istem kapanınca bildirim kalkar. Sıcaklık/kullanım alarmları bildirim üretmez. Arka planda yalnız olay aboneliği (metrik akışı yok)
### Değişti
- Telefon: Hello'daki cihaz adı sabit "toco" yerine Android model adı (`ro.product.model`); LM'de çoklu telefon (LM ADR-0036) tray'de ayırt edilir
- Build: APK cargo-apk yerine `scripts/build-apk.sh` (cargo-ndk + aapt2 + d8 + apksigner) ile; manifest `apps/lowdeck-phone/android/AndroidManifest.xml`. Aynı paket adı ve imza, `adb install -r` ile üstüne kurulur (#42)

## [1.0.0] - 2026-11-19
İlk sürüm: telefon uygulaması (Control, Live, Events, History, Processes, AI, Settings), protokol sürüm 4. PC tarafı LowMonitor v1.0.0.
### Eklendi
- Telefon: perde inikken yalnız alarm + AI push'ları; alarm kalkınca ekran açılır (kullanım kuralları ve LM tray sıcaklık eşikleri), PC meşgulken (aktif alarm ya da Claude limiti ≥ %80) 10 dk'da bir 15 s açılır (#40)
- Telefon: otomatik yeniden bağlanma: bağlanınca PC adı kaydedilir; üst üste kopuşta keşif yapılır, aynı PC'nin yeni IP'si bulunursa kaydedilip bağlanılır (#40)
- README İngilizce + `README.tr.md`; `HANDOFF.md`, `AGENTS.md`, `GEMINI.md`
- Telefon: grafiğe dokunup parmağı kaydırınca dikey çizgi + zaman ve değer etiketi (LowMonitor'daki hover gibi): Canlı (saat, CPU/GPU %·°C, RAM, disk, ağ) ve History (kova aralığı, avg/max, °C). Parmak kalkınca kaybolur
- Telefon: sol şerit **çark** oldu (aktif sekme ortada, komşular küçülüp söner, uçta ▲▼); sekme değişimi dikey kaydırmayla — şeritte her zaman, içerikte yalnız Control/Live/History/Settings'te (listeler normal kayar). Yatay kaydırma kalktı
- Telefon: içerikte yatay kaydırma ile sekme değiştirme (sola → sonraki, sağa → önceki); şerit düğmeleriyle aynı abonelik/sorgu yolu. Şerit daraldı (104 → 76 px); ⚙ paneli yerine **Settings** sekmesi
- Telefon (ADR-0010): **Settings** sekmesinde "PC · AI": dil, kimlik (Auto/OAuth/Web), yenileme aralığı, rapor oturum sayısı, CSV log → "Save to PC" (PC `ai.toml`, yalnız değişen anahtar). LowMonitor deck gerektirir (LM #65)
- Protokol (ADR-0010, ek varyantlar, sürüm 4): `QueryAiConfig` / `SetAiConfig(AiConfig)` → `S2C::AiConfig { cfg, error }`; PC `ai.toml` telefondan okunur/yazılır. Sunucu LowMonitor'da
- Telefon (ADR-0009): **Processes** sekmesi (CPU/RAM sıralama, ad/PID arama, ilk 50; "End" → "Confirm" iki aşamalı sonlandırma, korumalı süreçte düğme yok), **AI** sekmesi (son oturum / bugün / hafta: limit çizgileri, token dökümü, oturumlar), **History**'de CPU °C, GPU % + °C kartı, disk max °C. LowMonitor deck gerektirir (LM #63)
- Protokol (ADR-0009, ek varyantlar, sürüm 4): `QueryTempHistory`/`TempHistory`, `TOPIC_PROCS` + `ProcView`/`Procs` (ilk 50), `Kill`/`KillResult`, `QueryAiReport`/`AiReport`. Sunucu tarafı LowMonitor'da; mock-server bunlara `Unsupported` döner.
- Telefon: yatay tam ekran (durum çubuğu gizli), solda sekme şeridi: Kontrol (ses/medya/"PC klavyesini kilitle") · Canlı (AI bandı, CPU/GPU/RAM/disk/ağ kartları + 5 dk grafikleri, °C, çekirdekler, top-5) · Olaylar (son 24 sa) · Geçmiş (1 sa/6 sa/24 sa/7 g). LowMonitor alarm bandı her sekmede. Abonelik sekmeye göre: metrik yalnız Canlı'da. Arayüz İngilizce (lowmon-ui ile aynı metinler), kart/grafik ölçüleri lowmon-ui'a yakın.
- Protokol: `TOPIC_SENSORS` + `S2C::Sensors` (GPU %, CPU/GPU/disk °C; ek varyant, sürüm 4; LM ADR-0033). `mock-server` sahte sensör üretir. Telefon arayüzü henüz kullanmıyor
- Telefon: "AI" bölümü (PC'nin üstünde) — Claude oturum/hafta kullanımı, kalan süre, PC'nin tempo rengi. Protokol: `TOPIC_AI` + `S2C::Ai` (ek varyant, sürüm 4; LM ADR-0032)
### Kaldırıldı
- PC tarafı (`lowdeck-agent`, `lowdeck-desktop`) LowMonitor'a taşındı (LM ADR-0031): sunucu LowMonitor tray'de, arayüz `lowmon-ui`. Release artık yalnız APK; telefon ayarı (token/port) değişmez.
### Değişti
- Lisans: GPL-3.0-only → MIT OR Apache-2.0 (LowMonitor ile aynı; tek sahip). Slint Royalty-free 2.0 ile kullanılır, README'de atıf rozeti.
### Düzeltildi
- Telefon: ayar paneli opak (alttaki ekran yazıların arasından görünüyordu).
- Masaüstü Süreçler: arama kutusu (ad ya da PID); fare liste üzerindeyken ya da sonlandırma onayı beklenirken liste donar ("⏸ sabit"), seçim satır kaymasıyla kaçmaz; %0,05 altı CPU boş gösterilir.
- Masaüstü Olaylar: alarm değeri yüzde olarak (59.2%), sistem olayında değer gizli; "Yenile" düğme görünümünde.
- Masaüstü: Canlı kartları eşit genişlikte (değer değişince kayıyordu); Top-5 ve Süreçler CPU'su yumuşatılmış (EMA, ≈ 3 örnek), isimler her güncellemede yer değiştirmiyor.
- Masaüstü: Slint software renderer (femtovg/OpenGL yerine): özel bellek ≈ 94 → 5 MiB, Canlı açıkken CPU ≈ 8 → 1 ms/s.
- Agent: LowMonitor pipe'ının tüm örnekleri meşgulken (231) olay/geçmiş sorgusu ve abonelik boş dönmek yerine ≤ 1 s bekleyip yeniden dener.
- Agent: `bind` IP'si bu PC'de yoksa (DHCP adresi değiştirdi) hata kutusu yerine şu anki LAN IP'sinde dinler ve log'a yazar; başlangıç hatası da log'a yazılır.

### Eklendi
- **Tuş kilidi** (ADR-0008): `C2S::Lock` → agent cat-lock-cpp kısayolunu (Pause) gönderir; `cli` `l`. Telefon düğmesi sonra.
- **PC keşfi** (ADR-0007): telefon ayarında "PC'yi bul" → UDP 47811 sorgu, agent adres + port + PC adı ile yanıtlar (token yok); sonuca dokununca host/port dolar. Firewall kuralı artık UDP 47811'i de içerir (var olan kural için menüden bir kez daha çalıştırın). Mock da yanıtlar.
- Masaüstü **Süreçler** sekmesi: tüm süreçler (CPU/RAM sıralı, 2 s, yalnız sekme açıkken), iki aşamalı onayla **Sonlandır**; PID yeniden kullanım kontrolü, korumalı süreçler, yetki yetmezse UAC ile yükseltilmiş kopya (ADR-0006)
- Protokol v1 (TCP + postcard), istemci, mock sunucu, Slint masaüstü ekranı
- CI (APK artifact) ve onaylı release workflow'ları
- Proje iskeleti (README, TODO, devlog, ADR, gereksinimler)
- Gömülü SVG ikonlar (emoji yerine), bağlantı yokken pasif butonlar, ayar ekranında host/port/token doğrulaması
- Android: önde iken ekran açık, 2 dk dokunulmazsa siyah perde; arka plana geçince bağlantı kapanır, öne gelince açılır
- `lowdeck-agent`: gerçek PC sunucusu (Core Audio ses/mute + cihaz değişimi, SMTC medya + parça bilgisi, SMTC yoksa medya tuşu), olay tabanlı push
- CI: agent için Windows job'u
- Agent tray ikonu + menü (telefon ayarı, ayar dosyası, Windows ile başlat, çıkış); konsol penceresi yok
- Agent tray → Ayarı yeniden yükle (yeniden başlatmadan); yanlış `bind` IP'si için açıklayıcı hata
- Telefonda **PC** bölümü: LowMonitor'dan CPU (toplam + çekirdek çubukları), RAM, disk, ağ ve Top-5 süreç; agent `\\.\pipe\LowMonitor`'a yalnız telefon aboneyken bağlanır. Servis yoksa "LowMonitor servisi yok". Mock sahte metrik üretir.
- Masaüstü (LowMonitor arayüzü) Canlı sekmesi: CPU, RAM, disk (R/W) ve ağ (↓/↑) kartlarında son 5 dk çizgisi; disk/ağ oto-ölçek (1-2-5), küçültme/kopma boşluğunda çizgi kopar
- PC zip'i (`lowdeck-agent.exe` + `lowdeck-desktop.exe`) her `v*` Release'inde; kurulum `%LOCALAPPDATA%\LowDeck\`
- Agent açılışta LowMonitor "Arayüzü aç" kaydını (`UiExe`) yazar: menü masaüstü hiç açılmadan aktif
- Masaüstü, agent çalışmıyorsa yanındaki `lowdeck-agent.exe`'yi başlatır; ilk kurulumda `agent.cfg`'yi 5 s'ye kadar bekler
- Masaüstü **Geçmiş** sekmesi: 1 sa / 6 sa / 24 sa / 7 g; CPU (ortalama + en yüksek), RAM, disk R/W, ağ ↓/↑, veri olmayan aralıkta çizgi kopar (ADR-0005)
- Agent log dosyası `%APPDATA%\LowDeck\agent.log` (1 MiB döndürme), tray → Log'u aç
- Agent tray → Firewall kuralı ekle (UAC; Private profil + yerel alt ağ)
### Düzeltildi
- Masaüstü Geçmiş: 2×2 kart genişliği pencereye bağlıydı → Slint binding loop uyarısı (ileride panic riski); genişlik artık yalnız stretch ile bölünüyor
- Masaüstü grafikleri kart yüksekliğini dolduruyor (Slint `Path` varsayılan `fit: contain` en-boy oranını koruyordu)
- Karşı taraf bağlantıyı temiz kapatınca `io: failed to fill whole buffer` yerine `bağlantı kapandı` raporlanır
### Değişti
- Masaüstü exe: Slint yalnız femtovg renderer (software renderer, erişilebilirlik, system-tray kapalı); Windows release 8,60 → 7,85 MiB
- **Protokol v4** (`QueryHistory`, `History`): telefon APK'sı, agent ve masaüstü birlikte güncellenmeli; eski sürüm `BadVersion` alır.
- **Protokol v2** (`Subscribe`, `Metrics`, `Top`): telefon ve agent birlikte güncellenmeli; eski sürüm `BadVersion` ile reddedilir.
- Varsayılan port 47800 → **47810** (LowMonitor `lowmon-net` ile çakışma). Kayıtlı ayardaki port elle güncellenmeli.
- İstemci: connect sırasında durdurulan client zombi bağlantı bırakmıyor; eski client'ın olayları UI'ı ezmiyor
- Agent: oturum Hello zaman aşımıyla (5 s) kopuyordu — Windows'ta `try_clone`'lu handle `SO_RCVTIMEO` paylaşmıyor; timeout okuma handle'ına. İstemcide aynı desen düzeltildi.

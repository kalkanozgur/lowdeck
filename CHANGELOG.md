# Changelog
Format: Keep a Changelog. Sürümleme: SemVer (`CLAUDE.md` > Sürümleme ve commit).

## [Unreleased]
### Eklendi
- Telefon: sekmeler (Kontrol: ses/medya/kilit · Canlı: AI/PC/sensör/çekirdek/top); abonelik sekmeye göre (Kontrol'de metrik push yok). GPU % + °C, CPU/Disk °C, LowMonitor alarm bandı (her sekmede), "PC klavyesini kilitle" düğmesi (`C2S::Lock`).
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

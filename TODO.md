# TODO
## Şimdi
- [ ] docs/gereksinimler.md doldur
- [x] Faz 0: GitHub repo + ilk CI koşusu
- [x] Faz 0: keystore + `ANDROID_KEYSTORE_*` secret'ları (2026-11-15; CI de aynı keystore ile imzalar)
- [x] v1.0.0 (2026-11-19): sürüm, CHANGELOG, README EN + TR, HANDOFF; tag kullanıcıda
- [x] v1.1.0 (2026-11-22): LM v1.1.0 olay metinleri, cihaz adı, aapt2 build, arka plan bildirimi (ADR-0011); tag kullanıcıda
- [ ] Faz 3: arka plan tüketimi (FGS + 25 s ping) ölç: `batterystats`, `top`; şarjsız Doze davranışı
- [ ] v1 sonrası cihazda: perdenin alarmla / arada açılması, PC IP'si değişince otomatik yeniden bağlanma (#40)

## ADR-0031 taşınma (PC tarafı → LowMonitor)
- [x] LM Faz 1: `lowmon-deck` crate (agent kodu) — LM reposu
- [x] LM Faz 2: tray deck'i başlatır, `deck.toml` + `agent.cfg` göçü — LM
- [x] LM Faz 3: `lowmon-ui` (masaüstü taşınır) — LM
- [x] LM Faz 4: kurulum/release/firewall `lowmon-tray.exe`, eski `lowdeck-agent` Run kaydı temizliği — LM
- [x] LowDeck Faz 5: `tools/lowdeck-agent` + `apps/lowdeck-desktop` sil; `Cargo.toml` üyeleri, `ci.yml` windows job, `release.yml` pc job, README/CLAUDE.md/PROTOCOL.md (sunucu = LM tray); ADR-0002…0008 "yerine geçti" — LM Faz 4 bittikten sonra
- Not: aşağıdaki agent/masaüstü maddeleri (ADR-0004 doğrulaması, `kill_elevated`, `lowmon-proto` rev) LM'ye geçer; burada kapatılmaz, LM TODO'suna taşınır.

## Sonra
- [x] Faz 1 kalanlar: SVG ikonlar, bağlantısızken pasif butonlar, host/port doğrulama
- [x] Faz 2: Android yaşam döngüsü, ekran açık tutma (KEEP_SCREEN_ON + boşta siyah perde)
- [ ] Faz 2: cihazda/emülatörde doğrulama (Pause'da soket kapanıyor mu, Wi-Fi uyku)

- [x] F6: LowMonitor metrikleri telefonda (agent pipe Viewer, ADR-0002 a)
- [ ] F6: cihazda ölçüm (abone iken telefon CPU/uyanma, perde inince pipe kapanıyor mu)
- [x] PC dağıtımı: Release'e Windows zip, agent `UiExe` yazar, masaüstü agent'ı başlatır (ADR-0004)
- [ ] ADR-0004 Windows doğrulaması: zip kurulumu, agent kapalıyken "Arayüzü aç", `agent.cfg` yokken ilk açılış
- [x] Masaüstü Geçmiş sekmesi + proto v4 `QueryHistory` (ADR-0005)
- [x] Masaüstü Süreçler sekmesi + sonlandırma (ADR-0006)
- [x] Süreçler: Windows'ta tarama CPU (1,5 %/çekirdek), normal + yönetici (aynı kullanıcı) süreci sonlandırma
- [ ] Süreçler: `kill_elevated` UAC yolu canlı doğrulanmadı (aynı kullanıcının yönetici süreci UAC'siz kapanıyor; yalnız başka kullanıcı/SYSTEM'de tetiklenir)
- [x] Geçmiş: Windows'ta gerçek LM ile 1 sa / 24 sa / 7 g yanıt süresi (en kötü 143 ms)
- [x] Masaüstü Canlı grafikleri: Windows'ta boşta CPU / private bytes (exe boyutu: kullanıcı kararıyla ölçülmüyor)
- [x] Masaüstü private ≈ 100 MiB → software renderer: 5 MiB, CPU 8 → 1 ms/s
- [x] Agent: LM pipe `All pipe instances are busy` (231) → 100 ms × ≤ 10 yeniden deneme
- [ ] Telefondan "PC'yi bul" + firewall (UDP 47811 / TCP 47810) gerçek Wi-Fi'da
- [ ] `lowmon-proto` rev'i LowMonitor sürümleriyle birlikte güncelle (pipe `PROTO` artarsa agent `servis: BadHello` loglar)

- [ ] Telefon adı Android'de sabit `"toco"` → cihaz modelinden al (2026-11-18, M21 de `toco` görünüyor)
- [ ] Settings sekmesi: durum satırı / bulunan PC etiketi sütunları kaydırıyor → sabit genişlik
- [ ] Telefondan `ai.toml` değer değiştirme + tray reload cihazda doğrula

## Fikirler
- Rafta (kullanıcı, 2026-11-15): Süreçler'de öncelik / EcoQoS / affinity
- Tuş kilidi (Faz 5, ADR-0008): telefonda kilit düğmesi; biyometri/PIN ile PC kilidini açma + durum

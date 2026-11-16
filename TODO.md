# TODO
## Şimdi
- [ ] docs/gereksinimler.md doldur
- [x] Faz 0: GitHub repo + ilk CI koşusu
- [ ] Faz 0: keystore + `ANDROID_KEYSTORE_*` secret'ları (kullanıcı)

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

## Fikirler
- Rafta (kullanıcı, 2026-11-15): Süreçler'de öncelik / EcoQoS / affinity
- Tuş kilidi (Faz 5, karar bekliyor)

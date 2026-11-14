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
- [ ] `lowmon-proto` rev'i LowMonitor sürümleriyle birlikte güncelle (pipe `PROTO` artarsa agent `servis: BadHello` loglar)

## Fikirler
- UDP beacon ile PC keşfi (PROTOCOL.md §5)
- Tuş kilidi (Faz 5, karar bekliyor)

# Changelog
Format: Keep a Changelog. Sürümleme: SemVer (`CLAUDE.md` > Sürümleme ve commit).

## [Unreleased]
### Eklendi
- Protokol v1 (TCP + postcard), istemci, mock sunucu, Slint masaüstü ekranı
- CI (APK artifact) ve onaylı release workflow'ları
- Proje iskeleti (README, TODO, devlog, ADR, gereksinimler)
- Gömülü SVG ikonlar (emoji yerine), bağlantı yokken pasif butonlar, ayar ekranında host/port/token doğrulaması
- Android: önde iken ekran açık, 2 dk dokunulmazsa siyah perde; arka plana geçince bağlantı kapanır, öne gelince açılır
### Düzeltildi
- Karşı taraf bağlantıyı temiz kapatınca `io: failed to fill whole buffer` yerine `bağlantı kapandı` raporlanır
### Değişti
- Varsayılan port 47800 → **47810** (LowMonitor `lowmon-net` ile çakışma). Kayıtlı ayardaki port elle güncellenmeli.
- İstemci: connect sırasında durdurulan client zombi bağlantı bırakmıyor; eski client'ın olayları UI'ı ezmiyor

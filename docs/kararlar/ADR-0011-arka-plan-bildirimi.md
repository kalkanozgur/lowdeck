# ADR-0011: Arka planda bağlantı + güvenlik/Claude bildirimi (foreground service)
- Tarih: 2026-11-22
- Durum: **Kabul** (kullanıcı kararı 2026-11-22, kartla)
- İlgili: LM ADR-0037 (UAC/güvenlik duvarı alarmı), LM ADR-0034 eki (`Claude.Input`), ADR-0001 (tüketim önceliği)

## Bağlam
Kullanıcı PC'de UAC / güvenlik duvarı istemi açılınca ya da Claude girdi isteyince telefonda **Android bildirimi** istiyor; telefon başka uygulamadayken ya da ekran kapalıyken de. Şimdiye kadar `MainEvent::Pause` client'ı düşürüyordu (arka planda sıfır trafik), yani arka planda olay gelmiyordu.

| Seçenek | Sonuç |
|---|---|
| Yalnız önde iken bildirim | Hayır: perde zaten açılıyor, asıl ihtiyaç arka plan |
| **Foreground service** | Seçildi: bağlantı arka planda sürer, Android süreci öldürmez |
| PC'den push (ntfy / Home Assistant) | Hayır (şimdilik): ek uygulama/sunucu; LM TODO'da HA notu duruyor |

cargo-apk Java sınıfı ve `<service>` desteklemiyor → build aapt2 + d8 betiğine taşındı (#42). Bu ADR `DeckService.java`'yı ekler.

## Karar
1. **Servis** `dev.kalkan.lowdeck.DeckService` (tek Java dosyası, androidx yok): `foregroundServiceType="connectedDevice"`, kalıcı düşük öncelikli bildirim ("Listening for PC alerts"), `START_NOT_STICKY`; görev kaydırılıp kapatılınca (`onTaskRemoved`) durur. Bağlantı Rust'ta kalır; servis yalnız süreci canlı ve öncelikli tutar.
2. **Kanallar:** `service` (MIN, sessiz) ve `alerts` (HIGH, titreşim). API ≥ 33'te `POST_NOTIFICATIONS` izni açılışta istenir.
3. **Rust ↔ Java:** `jni 0.21` (android-activity üzerinden zaten derleniyordu, yeni crate yok). Sınıf Activity'nin `ClassLoader`'ıyla yüklenir (native thread'den `FindClass` uygulama sınıfını bulamaz).
4. **Arka planda abonelik yalnız `TOPIC_EVENTS`**: metrik/sensör/süreç/AI akışı yok; telefon tarafında tek periyodik iş 25 s ping. Öne gelince sekmenin konuları geri gelir.
5. **Bildirim kuralı:** yalnız arka planda; ad ∈ {`uac`, `firewall`, `Claude.Input`}; `kind 0` → bildirim, `kind 1` → bildirim kaldırılır. CPU/GPU/Disk sıcaklık ve kullanım alarmları bildirim üretmez (bant + perde, değişmedi).

## Sonuçlar
- (+) Telefon başka uygulamada / ekran kapalıyken de istem haberi gelir; dokununca LowDeck öne gelir.
- (−) Arka planda TCP açık + 25 s ping → Wi-Fi radyosu periyodik uyanır. Ölçülecek (Faz 3): `dumpsys batterystats`, `top`.
- (−) Doze (şarjsız, hareketsiz) ağ erişimini kesebilir; panel telefonu genelde şarjda olduğundan Doze devreye girmez. Şarjsız kullanımda kopma görülürse ayrı karar (pil optimizasyonu muafiyeti).
- (−) MIUI/One UI pil yöneticisi FGS'yi de kısıtlayabilir: gerekirse uygulama için "Kısıtlama yok".
- Kalıcı bildirim durum çubuğunda görünür (Android FGS şartı).

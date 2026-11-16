# Gereksinimler — LowDeck

## Amaç
Eski Android telefonu, aynı LAN'daki Windows PC için düşük tüketimli ses/medya kontrol paneli yapmak.

## Fonksiyonel gereksinimler
| # | Gereksinim | Faz |
|---|---|---|
| F1 | PC ana ses seviyesi ayarı (0–100) ve sessiz | 1 |
| F2 | Medya: play/pause, ileri, geri | 1 |
| F3 | Çalan parça bilgisi (başlık, sanatçı) | 1 |
| F4 | Bağlantı kopunca otomatik yeniden bağlanma, durum UI'da | 1 |
| F5 | Tuş kilidi | 5 (TBD) |
| F6 | LowMonitor metrikleri: CPU toplam + çekirdek, RAM kullanılan/toplam, disk okuma/yazma, ağ rx/tx, Top-5 (ad, CPU, WS); servis yokken pasif görünüm; perde inince abonelikten çıkış (ADR-0002 seçenek a) | Faz 4 |

## Elektriksel / tüketim
| Metrik | Hedef |
|---|---|
| Boşta CPU (ekran sabit) | ≈ %0 |
| Ağ uyanması | 25 s'de 1 ping |
| Arka planda ağ trafiği | 0 |
| Komut gecikmesi (dokunma → PC) | < 50 ms |
| RSS / APK boyutu | TBD (ilk ölçüm referans) |
| Ekran parlaklığı / açık tutma | TBD |

## Mekanik
- Telefon montajı / stand: TBD
- Şarj: sürekli bağlı mı, pil koruma (şarj limiti): TBD

## Çevre
- İç mekân, masa üstü. Sürekli şarjda pil sıcaklığı: TBD

## Kabul kriterleri
- F1–F4 mock sunucuya ve LowMonitor tray'e (`lowmon-deck`) karşı çalışır.
- Tüketim tablosundaki ölçümler `adb` ile alınıp devlog'a yazılmıştır.
- Ses donanımı gerektirmez (telefonun ses codec'i arızalı).

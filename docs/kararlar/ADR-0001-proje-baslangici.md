# ADR-0001: Proje başlangıcı
- Tarih: 2026-11-14
- Durum: Kabul

## Bağlam
Ses codec arızalı eski Mi Note 10 Lite (SD730G, AMOLED), LAN'daki Windows PC için ses/medya kontrol paneli olacak. Öncelik minimum tüketim. PC sunucusu ileride LowMonitor (Rust) modülü. Android araç zinciri lokalde yok.

## Karar
| Konu | Seçim |
|---|---|
| Modül | yazılım |
| Dil / UI | Rust + Slint (masaüstü ve Android aynı UI) |
| Taşıma | Tek kalıcı TCP, sunucu push, `[u16 LE][postcard]`, async yok |
| Yapı | Cargo workspace (crates/, apps/, tools/); skill'in `software/` şablonu kullanılmadı |
| Build | Android APK yalnızca GitHub Actions'ta; release tag push ile (private + Free: ortam onayı/branch koruması yok) |
| Kayıt | Aylık devlog `docs/devlog/`, kararlar `docs/kararlar/` |

Ayrıntılı gerekçe: `CLAUDE.md` §2.

## Sonuçlar
- Artı: protokol tipleri LowMonitor ile tek crate'ten paylaşılır; telefon tamir edilmeden masaüstünde geliştirme sürer.
- Eksi: APK testi için CI döngüsü gerekir (lokal hızlı iterasyon yok).
- Risk: CI workflow'ları hiç koşmadı (cargo-apk/NDK uyumu); Slint lisansı kapalı kaynak dağıtımda yeniden değerlendirilmeli.

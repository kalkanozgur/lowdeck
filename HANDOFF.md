# HANDOFF — LowDeck
**Güncelleme:** 2026-11-19 · **Güncelleyen:** Claude Code · **Kaynak durumu:** v1.0.0 (main)

## Durum
v1.0.0: telefon uygulaması (7 sekme) + `lowdeck-proto` (sürüm 4) + `lowdeck-client` + `mock-server`. PC sunucusu LowMonitor tray'de (`lowmon-deck`, LM v1.0.0). Mi Note 10 Lite'ta #40 CI APK'sı (içerik = main `4c6d9f1`) kurulu (2026-11-19 15:44), PC'ye otomatik keşifle bağlandı.

## Son yapılanlar (2026-11-19)
- Perde inikken `TOPIC_EVENTS | TOPIC_AI`; alarmda ekran açılır; PC meşgulken 10 dk'da bir 15 s (#40)
- Otomatik yeniden bağlanma: sunucu adı `lowdeck.cfg` `name=`'e; kopunca keşif + aynı PC'nin yeni IP'si (#40)
- README EN + TR, HANDOFF / GEMINI / AGENTS, CLAUDE.md Handoff bölümü, sürüm 1.0.0

## Açık sorun / blokaj
- Cihazda gözlenmedi: perdenin alarmla / arada açılması, PC IP'si çalışırken değişince yeniden bağlanma
- "Claude çalışıyor" bilgisi protokolde yok → meşgul koşulunda değil
- Telefon adı Android'de sabit `"toco"`; Settings sütunları kayıyor (TODO.md)
- `cargo fmt` repoyla uyumsuz (ilgisiz dosyaları biçimliyor) → çalıştırılmıyor; ayrı bir biçimleme PR'ı kararı gerekir

## Sıradaki adımlar (sırayla)
1. Cihazda doğrulama: perde + alarm (LM'de eşiği geçici düşürerek), DHCP yenileme ile yeniden bağlanma
2. Faz 3 tüketim ölçümü (`adb shell top`, `dumpsys meminfo`, `batterystats`) → devlog
3. Telefon adını cihazdan al (`Build.MODEL`) + Settings sütunlarını sabitle

## Dokunma
- `docs/PROTOCOL.md` / `lowdeck-proto`: varyant yalnız sona eklenir; değişiklikten sonra LM `lowdeck-proto` rev'i güncellenir
- `lowdeck_full.txt` (ana klon, izlenmeyen dökümü) — okuma dışı
- `.claude/worktrees/*` — başka oturumların çalışma alanı
- Tag'i yalnız kullanıcı atar (CLAUDE.md Faz 0)

## Kritik dosyalar
- `CLAUDE.md` — kurallar ve yol haritası · `TODO.md` · `CHANGELOG.md`
- `docs/PROTOCOL.md` — protokol sözleşmesi · `docs/kararlar/ADR-*.md` (son: ADR-0010) · `docs/devlog/2026-11.md`
- `apps/lowdeck-phone/src/lib.rs` (UI, perde, yeniden bağlanma), `discover.rs`, `config.rs`

## Doğrulama
- `cargo test --workspace` · `cargo clippy --workspace --all-targets -- -D warnings`
- APK: PR'da CI `apk` artifact'ı; release: `v*` tag → `lowdeck-vX.Y.Z.apk` (kalıcı keystore, `adb install -r` ile üstüne kurulur)

## Kısıtlar / kararlar
- Sürekli timer / animasyon / polling yasak; yeni periyodik iş kullanıcı onayıyla (istisna: 10 dk arada açılış, 2026-11-19)
- Android araç zinciri lokalde yok; APK yalnız GitHub Actions'ta
- minSdk 29, targetSdk 34, `aarch64-linux-android`

---
## Geri dönüş (Claude Code'a döndüğünde)
> HANDOFF.md'yi oku. Aşağıdaki çıktı <Gemini/OpenCode>'den geldi. Uygulamadan önce mevcut kodla çelişkileri kontrol et, sonra uygula, devlog'a ekle, HANDOFF.md'yi güncelle.

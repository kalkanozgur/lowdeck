# HANDOFF — LowDeck
**Güncelleme:** 2026-11-22 · **Güncelleyen:** Claude Code · **Kaynak durumu:** v1.1.0 (sürüm PR'ı; tag kullanıcıda)

## Durum
v1.1.0: telefon uygulaması (7 sekme) + arka plan bildirimi (ADR-0011) + `lowdeck-proto` (sürüm 4) + `lowdeck-client` + `mock-server`. PC sunucusu LowMonitor tray'de (`lowmon-deck`, LM v1.1.0). APK `scripts/build-apk.sh` ile (cargo-ndk + aapt2 + d8). Samsung M21'de #44 CI APK'sı kurulu, UAC/Claude bildirimi doğrulandı (2026-11-22).

## Son yapılanlar (2026-11-22)
- LM v1.1.0 olay metinleri, kısa alarm adı, cihaz adı `ro.product.model` (#43)
- cargo-apk → aapt2/d8 betiği (#42)
- Foreground service `DeckService.java`, arka planda yalnız olay aboneliği, UAC/firewall/Claude.Input bildirimi (#44, ADR-0011)

## Açık sorun / blokaj
- Ölçülmedi: arka plan pil tüketimi (FGS + 25 s ping), şarjsız Doze'da bağlantı
- Cihazda gözlenmedi: `firewall` bildirimi, Mi Note 10 Lite'ta v1.1.0, perdenin arada açılması, DHCP ile yeniden bağlanma
- Görev kaydırılıp kapatılınca servis durur → bildirim gelmez (tasarım, ADR-0011)
- Settings sütunları kayıyor (TODO.md)
- `cargo fmt` repoyla uyumsuz (ilgisiz dosyaları biçimliyor) → çalıştırılmıyor; ayrı bir biçimleme PR'ı kararı gerekir

## Sıradaki adımlar (sırayla)
1. Kullanıcı `v1.1.0` tag'ini atar → release APK'yı iki telefona kur
2. Faz 3 tüketim ölçümü, arka plan dahil (`adb shell top`, `dumpsys meminfo`, `batterystats`) → devlog
3. Settings sütunlarını sabitle; telefonda parmak izi / PIN ile PC kilidi açma (Faz 5)

## Dokunma
- `docs/PROTOCOL.md` / `lowdeck-proto`: varyant yalnız sona eklenir; değişiklikten sonra LM `lowdeck-proto` rev'i güncellenir
- `lowdeck_full.txt` (ana klon, izlenmeyen dökümü) — okuma dışı
- `.claude/worktrees/*` — başka oturumların çalışma alanı
- Tag'i yalnız kullanıcı atar (CLAUDE.md Faz 0)

## Kritik dosyalar
- `CLAUDE.md` — kurallar ve yol haritası · `TODO.md` · `CHANGELOG.md`
- `docs/PROTOCOL.md` — protokol sözleşmesi · `docs/kararlar/ADR-*.md` (son: ADR-0011) · `docs/devlog/2026-11.md`
- `apps/lowdeck-phone/src/lib.rs` (UI, perde, yeniden bağlanma, arka plan), `notify.rs` + `android/java/…/DeckService.java` (bildirim), `android/AndroidManifest.xml`, `scripts/build-apk.sh`, `discover.rs`, `config.rs`

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

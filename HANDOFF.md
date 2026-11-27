# HANDOFF — LowDeck
**Güncelleme:** 2026-11-27 · **Güncelleyen:** Claude Code · **Kaynak durumu:** v1.2.0 (sürüm PR'ı; tag kullanıcıda)

## Durum
v1.2.0: v1.1.0 + PC güç komutları (ADR-0012; #46 proto + telefon, #47 yanıtsız komutta yeniden bağlanma). PC tarafı LowMonitor v1.4.0 (`lowmon-deck/src/power.rs`, LM ADR-0044). M21 + LM `fb3d099` ile gerçek PC'de Sleep (S3), Restart → Cancel (olay 1074/1075) doğrulandı; Shutdown denenmedi.

## Açık sorun
- M21'in IP'si `.150` / `.156` arasında değişiyor (deck.log, 2026-11-27); rastgele MAC ya da iki ağ? İncelenecek.
- Shutdown gerçek PC'de denenmedi.
- Uyandırma (Wake-on-LAN) yok (ADR-0012 sonuçları).

## Sıradaki 3 adım
1. Kullanıcı önce LM `v1.4.0`, sonra LowDeck `v1.2.0` tag'ini atar → release APK telefonlara.
2. M21 IP değişimi incelemesi.
3. Faz 3 arka plan tüketim ölçümü.

## Önceki durum (v1.1.0)
v1.1.0: telefon uygulaması (7 sekme) + arka plan bildirimi (ADR-0011) + `lowdeck-proto` (sürüm 4) + `lowdeck-client` + `mock-server`. PC sunucusu LowMonitor tray'de (`lowmon-deck`, LM v1.1.0). APK `scripts/build-apk.sh` ile (cargo-ndk + aapt2 + d8). Samsung M21'de #44 CI APK'sı kurulu, UAC/Claude bildirimi doğrulandı (2026-11-22).

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

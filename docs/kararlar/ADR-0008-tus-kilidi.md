# ADR-0008: Tuş kilidi: cat-lock kısayolu (Pause) simülasyonu
- Tarih: 2026-11-16
- Durum: Kabul (yalnız kilitleme; telefon UI'ı ve uzaktan açma sonra). **Taşınıyor:** LM ADR-0031 (2026-11-16) ile kod LowMonitor'a geçer; karar orada sürer

## Bağlam
PC'de tuş kilidi zaten var: `cat-lock-cpp` (C++, ayrı repo). Kullanıcı oturumunda, Görev Zamanlayıcı ile **yönetici** (`RunLevel Highest`) çalışır; LL klavye/fare hook'u, overlay, LED. Kısayol `Pause` / `Ctrl+Alt+L` → kilit; açma yalnız klavyeden kod (sondan eşleşir). Agent normal yetkide.

Seçenekler:
| | Ne | Artı | Eksi |
|---|---|---|---|
| a | cat-lock'a IPC (pencere mesajı + durum yayını) | durum bilgisi, odak bağımsız | iki repoda değişiklik; yönetici pencereye mesaj için `ChangeWindowMessageFilterEx` |
| b | Agent içinde kendi hook'u | bağımsız | çift hook, agent yönetici olmalı, overlay/LED/kod tekrar |
| **c** | **Agent `SendInput(VK_PAUSE)`** | **cat-lock'a dokunulmaz, 1 satır** | **durum yok; cat-lock yoksa tuş ön plandaki uygulamaya gider** |

## Karar (kullanıcı, 2026-11-16)
1. **(c):** `C2S::Lock` (birimsiz, enum sonu) → agent `SendInput` ile Pause bas/bırak. Yanıt yok.
2. **Ölçüm (2026-11-16):** normal yetkili süreçten enjekte Pause, yönetici cat-lock'u kilitledi (`lock hotkey`, LED kilit deseni). LL hook enjekte girdiyi görür; cat-lock yalnız kedi algılamada `LLKHF_INJECTED`'i eler. Enjekte rakamlarla girilen kod da kilidi açtı (`unlock code`).
3. **Açma telefondan yok** (şimdilik). Kod agent'ta tutulmaz, ağdan geçmez.
4. `PROTO_VERSION` 4 kalır: yeni varyant sonda, v4 istemcileri etkilenmez. Eski agent `Lock` çerçevesini çözemez → bağlantıyı düşürür (istemci yeniden bağlanır).

## Sonuçlar
- \+ cat-lock değişmedi; agent'a ek bağımlılık yok.
- − Telefon kilit durumunu bilemez (kilitli mi, cat-lock çalışıyor mu).
- − cat-lock kapalıyken Pause ön plandaki uygulamaya gider (çoğu uygulamada etkisiz).
- − Ayarda kısayol `ctrl_alt_l` yapılırsa Pause çalışmaz (varsayılan `both`).

## İleride (kullanıcı isteği)
Telefonda parmak izi / PIN doğrulanınca PC'de kilit açılsın. Seçenekler o adımda kartla sorulacak: (1) agent kodu yerel olarak (ör. DPAPI) tutar ve rakamları `SendInput` ile yazar — enjekte rakamların açtığı 2026-11-16'da ölçüldü; (2) cat-lock'a açma IPC'si + durum yayını (seçenek a). Durum gösterimi için (2) ya da cat-lock log izleme gerekir. Protokole `C2S::Unlock…` / `S2C::Lock(LockState)` sona eklenir.

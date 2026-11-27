# ADR-0012: Telefondan PC güç komutları (uyku / kapat / yeniden başlat)
- Tarih: 2026-11-27
- Durum: **Kabul** (kullanıcı kararı 2026-11-27, kartla: komut seti, 10 s geri sayım; yetki kartı yanıtsız → önerilen seçenek)
- İlgili: LM ADR-0044 (sunucu tarafı, `lowmon-deck`), ADR-0008 (tuş kilidi, aynı Control satırı), LM ADR-0036 (çoklu telefon)

## Bağlam
Telefondan PC'yi uyutmak / kapatmak isteniyor. Sunucu (`lowmon-deck`) LM tray'inde, kullanıcı oturumunda, normal yetkide çalışır.
Standart kullanıcı token'ında `SeShutdownPrivilege` vardır ama kapalıdır; `AdjustTokenPrivileges` ile açılınca UAC gerekmez.

## Karar
1. **Proto (ek varyant, `PROTO_VERSION` 4):** `C2S::Power(PowerCmd)` (C2S #14), `S2C::PowerResult { cmd, code: i32, delay_s: u16 }` (S2C #18).
   `PowerCmd = Sleep, Shutdown, Restart, Cancel` (yalnız sona eklenir; hazırda bekletme ileride `Hibernate` olarak eklenebilir).
   `code`: `POWER_OK = 0`, `POWER_DISABLED = -1` (LM `deck.toml` `power = false`), aksi Win32 hata kodu.
2. **Komut seti:** Uyku, Kapat, Yeniden başlat (+ İptal). Hazırda bekletme yok: çoğu masaüstünde `powercfg /h` kapalı.
3. **Geri sayım:** Kapat / Yeniden başlat `InitiateShutdownW(…, 10 s, …)`; Windows bildirimi görünür, `AbortSystemShutdownW` (telefondan `Cancel`) ile iptal edilebilir. `ExitWindowsEx` iptal edilemediği için kullanılmaz. Uyku `SetSuspendState(FALSE, FALSE, FALSE)`, gecikmesiz.
4. **Yetki:** token + LM `deck.toml` `power` (varsayılan `true`). `false` iken sunucu `POWER_DISABLED` döner.
5. **Telefon onayı:** Kill gibi iki aşamalı: ilk dokunuş düğmeyi "Confirm" yapar, ikinci dokunuş gönderir. Onay 3 s içinde gelmezse düşer (Slint `Timer`, yalnız onay beklerken çalışır, ilk tetikte durur: periyodik iş değil). Sekme değişince de düşer.
   Geri sayım başlayınca satır "Cancel" + "PC shuts down in 10 s" olur; kopuşta, `Cancel` sonucunda ya da `delay_s + 5` s sonra (tek-atışlık; PC'de elle iptal bildirilmez) eski haline döner.

## Sonuçlar
- (+) Servis ya da yükseltilmiş kopya gerekmez; kod LM'de yalnız `lowmon-deck` (`win.rs` + `lib.rs`).
- (+) Yanlış dokunuşa karşı iki emniyet: iki aşamalı onay + 10 s iptal edilebilir geri sayım (uyku hariç; uyku geri alınabilir).
- (−) Eski LM `Power` çerçevesini çözemez → bağlantı düşer. **LM sürümü önce çıkar**, APK sonra.
- (−) Uyandırma (Wake-on-LAN) yok: sunucunun MAC'i ve NIC/BIOS ayarı gerekir, ayrı iş.
- (−) Uykuda TCP kopar; telefon mevcut yeniden bağlanma döngüsüyle döner.

## Ek (2026-11-27, gerçek PC testi)
- Sleep M21 + LM `fb3d099` ile çalıştı (Kernel-Power 187/42, S3). PC uyanınca LM eski bağlantıyı kapattı (10053) ama telefon 47 s "Connected" kaldı; arada gönderilen Restart/Cancel kayboldu.
- Düzeltme: `PowerResult` 3 s içinde gelmezse (`POWER_REPLY_TIMEOUT`, tek-atışlık) mesaj + `Client::reconnect()`; Sleep `POWER_OK` → 1 s sonra `reconnect()`. Periyodik iş yok.

# ADR-0010: Telefondan PC AI ayarı (`ai.toml`)
- Tarih: 2026-11-18
- Durum: **Kabul** (kullanıcı kararı 2026-11-18, kartla). Proto burada, sunucu LM `lowmon-deck`.

## Bağlam
Kullanıcı: telefonda gösterilen PC verilerinin ayarları telefondan da değişsin, taskbar hariç. lowmon-ui Ayarlar'da üç dosya var: `ai.toml`, `taskbar.toml`, `deck.toml`; servis ayarları (`config.toml`) yönetici ister.

| Seçenek | Sonuç |
|---|---|
| **Yalnız `ai.toml`** | Seçildi. Yetki = token (ADR-0009 ile aynı), UAC yok |
| + `deck.toml` (bind/port) | Hayır: yanlış değer telefonu PC'den koparır, geri dönüş PC başında |
| + servis ayarları | Hayır: UAC penceresi PC'de açılır; ADR-0009 "UAC yok" kararı |

## Karar
1. `C2S::QueryAiConfig` → `S2C::AiConfig { cfg, error: "" }`. `C2S::SetAiConfig(AiConfig)` → deck önce `AiConfig::check`, sonra yalnız değişen anahtarları `toml_edit` ile yazar (yorumlar korunur, lowmon-ui ile aynı yol), tray'e `WM_RELOAD` (bit 0) gönderir, dosyadan yeniden okuyup `S2C::AiConfig { cfg, error }` döner. Hata → dosya değişmez, `error` dolu.
2. `AiConfig { lang, refresh_sec, auth, csv_log, report_sessions }` (lowmon-ui Ayarlar ile aynı alan ve sınırlar). Renkler, `desktop_path_hint`, `summary_dir`, `oauth_refresh` telefonda yok (PC'ye özgü yol/ayrıntı).
3. Telefon: tek **Settings** sekmesi (⚙ paneli kalkar): bağlantı (Find PC, host/port/token) + "PC · AI" bölümü. Sekmeye girince `QueryAiConfig`; "Save" → `SetAiConfig`.
4. Ek varyantlar, `PROTO_VERSION` 4 kalır; LM önce güncellenir (eski deck bilinmeyen varyantta bağlantıyı düşürür).

## Sonuçlar
- Telefon token'ı PC'deki AI ayarını değiştirebilir (kimlik doğrulama yöntemi dahil). LAN + token varsayımı ADR-0009 ile aynı.
- deck `toml_edit` kullanır: lowmon-ui `settings::set/apply/write` deck'e taşınır, ui yeniden dışa aktarır (ADR-0009 süreç kodu gibi).

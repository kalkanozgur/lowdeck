# ADR-0009: Telefonda lowmon-ui'ın proto dışı verileri (sıcaklık geçmişi, süreçler, AI raporu)
- Tarih: 2026-11-18
- Durum: **Kabul** (2026-11-18, açık sorular kartla yanıtlandı). Uygulama iki repoda: proto burada, sunucu LM `lowmon-deck` (LM ADR-0031 §3).

## Bağlam
Kullanıcı kararı (2026-11-18): telefon lowmon-ui'daki her şeyi göstersin. PR #31 proto'da olanları ekledi (Canlı, Olaylar, Geçmiş). Üç veri lowmon-ui'da **yerel** okunuyor, deck'ten geçmiyor:

| Veri | lowmon-ui kaynağı | Proto'da |
|---|---|---|
| Geçmiş: CPU/GPU °C, GPU % | tray sensör dakika dosyası (`lowmon-sensors::log`, LM ADR-0035) | yok |
| Süreçler + sonlandırma | `win.rs` (Toolhelp + `OpenProcess`), 2 s tarama (ADR-0006) | yok |
| AI raporu (oturumlar, gruplar, limit grafiği) | `lowmon-ai` dosyaları | yok |

Kısıtlar: `MAX_FRAME` 4096 B, enum'lara yalnız sona ekleme, sürekli iş yalnız abone varken, kill telefonda iki aşamalı onay (kullanıcı kararı).

## Karar (öneri)
1. **Sıcaklık geçmişi:** `C2S::QueryTempHistory { from_ms, to_ms, points }` → `S2C::TempHistory { from_ms, step_ms, points: Vec<Option<TempPoint>> }`, boş parça = bitti (`History` ile aynı akış). `TempPoint { cpu_c: Option<i16>, gpu_pct: Option<u8>, gpu_c: Option<i16>, disk_c_max: Option<i16> }` (dakika kayıtlarından kova ortalaması, disk = max; lowmon-ui `temp_buckets`). Telefon Geçmiş'te `QueryHistory` ile birlikte gönderir.
2. **Süreçler:** `TOPIC_PROCS = 16`; görünüm `C2S::ProcView { sort: u8 /*0 CPU, 1 RAM*/, filter: String }` (ad/PID içerir, ≤ 32 karakter). Abone varken deck 2 s'de bir tarar, sıralar, süzer ve **ilk 50**'yi gönderir: `S2C::Procs { total: u16, items: Vec<ProcItem> }`. `ProcItem { pid: u32, ctime: i64, name: String /*≤ 32*/, cpu: u16 /*0.01 %*/, ws_mb: u32, protected: bool }` → en kötü ≈ 50 × 56 B ≈ 2,8 KB < `MAX_FRAME` (test). Abone yoksa tarama yok (ADR-0006).
   - **Kill:** `C2S::Kill { pid, ctime }` → `S2C::KillResult { pid, code: i32 }`. Yetki = token (ek ayar yok). `ctime` PID yeniden kullanımına karşı (lowmon-ui `kill_now`). `protected` ve UAC gereken süreç telefondan sonlandırılmaz (`code` = erişim reddi). Onay telefonda iki aşamalı.
3. **AI raporu:** `C2S::QueryAiReport { view: u8 /*0 son oturum, 1 bugün, 2 hafta*/ }` → `S2C::AiReport { status, groups: Vec<AiGroup>, sessions: Vec<AiSession>, limit_s: Vec<u16>, limit_w: Vec<u16> }`, gerekirse parçalı. Grafik telefonda çizilir (path değil veri).
4. **Sürüm:** tümü ek varyant; `PROTO_VERSION` 4 kalır (`Lock` emsali). Eski LM yeni `C2S`'yi çözemez → bağlantı düşer: **LM önce güncellenir** (kullanıcı kararı).
5. **Telefon UI:** sol şeride "Süreçler" ve "AI" sekmeleri; Geçmiş'e °C/GPU çizgileri (lowmon-ui ile aynı kartlar).

## Yanıtlanan sorular (2026-11-18)
1. Uyumluluk: v4 kalır, LM önce güncellenir.
2. Süreç listesi: top 50, sıralama + süzme sunucuda.
3. Kill yetkisi: token yeterli.

## Sonuçlar
- \+ Telefon lowmon-ui ile eşit; veri tek kaynaktan (deck).
- − LM tarafında iş: deck'e sensör log okuma, süreç tarama (lowmon-ui `win.rs` taşınır/paylaşılır), AI rapor sorgusu. Süreçler sekmesi açıkken PC'de 2 s tarama (ADR-0006: ≈ 1,5 %/çekirdek) + telefonda 2 s push.
- − Kill uzaktan: token sızarsa süreç sonlandırılabilir (yalnız LAN, korumalı süreçler hariç).

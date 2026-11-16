# ADR-0005: Metrik geçmişi sorgusu (proto v4, agent'ta kovalama)
- Tarih: 2026-11-15
- Durum: Kabul (ADR-0003'ü genişletir; LM spek §9.3). **Taşındı:** LM ADR-0031 (2026-11-16); kod LowMonitor `lowmon-deck` (tray) + `lowmon-ui`, bu repodan silindi (LowDeck Faz 5). Karar orada sürer

## Bağlam
LM spek §9.3 "Geçmiş": zaman aralığı → `QueryRange`, aralık > 6 saat → 1 dk çözünürlük. Masaüstü pipe'a bağlanmaz (ADR-0003); veri agent üzerinden gelmeli.

| Seçenek | Trafik (loopback, 6 sa) | Masaüstü işi |
|---|---|---|
| Ham geçiş: her `Sample` → `Metrics` çerçevesi | 21 600 çerçeve ≈ 0,7 MB | Kovalama + bellek |
| **Agent'ta kovalama** | ≤ 600 kova ≈ 6 çerçeve, tipik ~9 KB | Yalnız çizim |

Grafik genişliği ~350 px; 600 noktadan fazlası çizimde görünmez.

## Karar
1. **Protokol v4** (sona ekleme, `PROTO_VERSION` 3 → 4):
   - `C2S::QueryHistory { from_ms, to_ms, points: u16 }`; sunucu `points`'i 1..=`HIST_POINTS_MAX` (600) aralığına kırpar.
   - `S2C::History { from_ms, step_ms, points: Vec<Option<HistPoint>> }`, ≤ `HIST_PER_FRAME` (100) kova/çerçeve, sıralı; boş `points` = bitti.
   - `HistPoint { cpu_avg, cpu_max: u16, ram_mb, disk_r/w_kbps, net_rx/tx_kbps: u32 }`: en kötü ~32 B → 100'lü çerçeve < 4 KB (test).
   - Sürüm artışı, ADR-0003 emsali: karışık sürümde bilinmeyen varyant kopma döngüsü yerine net `BadVersion`.
2. **Çözünürlük:** `to − from > 6 sa` → `Resolution::Minute`, değilse `Raw` (spek §9.3).
3. **Kovalama (agent, `lowmon::Buckets`):** `step = ⌈(to − from) / n⌉`; `Sample` → ortalama + CPU max, `Agg{Avg}` → ortalama, `Agg{Max}` → CPU max; `Min`, `Gap`, `Event` yok sayılır. Ortalaması olmayan kova `None` (servis kapalı, uyku, gap) → masaüstünde çizgi kopar. Bellek = kova sayısı; pipe `Range` çerçeveleri akış halinde işlenir.
4. **Masaüstü:** hazır aralıklar 1 sa / 6 sa / 24 sa / 7 g, bitiş = şimdi; sorgu yalnız düğme ya da sekmeye geçişte (timer yok). Üst üste istekte yalnız sonuncusunun yanıtı tutulur (`pending` sayacı; önceki yanıtların parçaları atılır, istek kimliği gerekmez çünkü TCP sırayı korur).

## Sonuçlar
- \+ Loopback trafiği ve masaüstü belleği sabit (≤ 600 kova), aralıktan bağımsız.
- \+ Telefon ileride aynı mesajı kullanabilir (küçük ekran → `points` düşük).
- − 7 g sorgusunda servis ~10 080 dakika özetini çözer; yanıt süresi Windows'ta ölçülecek (hedef < 2 s).
- − `PROTO_VERSION` 4: telefon APK'sı ve agent birlikte güncellenmeli.
- − Kova sınırında özet dakikaları bölünmez (dakika başı zamanına göre tek kovaya düşer); 7 g'de kova 16,8 dk, hata ≤ 1 dk.

# ADR-0003: Agent, LowMonitor PC arayüzünün köprüsü (loopback + olaylar)
- Tarih: 2026-11-15
- Durum: Kabul (LowMonitor ADR-0029'un LowDeck tarafı; ADR-0002'yi genişletir)

## Bağlam
LowMonitor ADR-0029 (seçenek B): LowMonitor'un PC arayüzü LowDeck masaüstü uygulaması olacak, veri yalnız `lowdeck-agent` üzerinden gelecek. İlk teslimat Canlı + Olaylar; telefon kapsamı sonra.

Agent bugün:
| Konu | Durum |
|---|---|
| Dinleme | yalnız `agent.cfg` `bind` (LAN IP) : 47810 |
| İstemci | tek; yeni bağlantı eskisini kapatır (Hub) |
| Pipe | Viewer, `CH_SAMPLE \| CH_TOP`, `lowmon-proto` `rev = 2356e7f` |
| Olay | yok (LowMonitor M5'te `CH_EVENT`, `ServerMsg::Event/Alarms`, `RangeItem::Event` geldi; `f512e57`) |

Pipe'ta yalnız olay dönen bir sorgu yok: `QueryRange` olayları örneklerle birlikte döner (Raw'da günde 86 400 örnek, Minute'ta 4 320 Agg).

## Karar
1. **Loopback dinleyici:** agent `bind`'a ek olarak `127.0.0.1:<port>`'u dinler (firewall gerekmez). Token yine zorunlu: masaüstü uygulaması aynı kullanıcının `%APPDATA%\LowDeck\agent.cfg` dosyasından okur.
2. **İki istemci yuvası:** Hub'da yuva = dinleyici. LAN yuvası (telefon) ve loopback yuvası (PC arayüzü) birbirini düşürmez; her yuvada "yeni bağlantı eskisinin yerine geçer" kuralı sürer. Durum push'ları (ses, medya, metrik, olay) aboneliği olan her yuvaya gider.
3. **Protokol (sona ekleme, `PROTO_VERSION` 2 → 3):**
   - `TOPIC_EVENTS = 2`: abonelikte önce `S2C::Alarms(Vec<String>)` (aktif liste), sonra her olay `S2C::Event(EventItem)`.
   - `C2S::QueryEvents { from_ms, to_ms }` → bir ya da birkaç `S2C::Events(Vec<EventItem>)` çerçevesi (≤ `MAX_FRAME`), son çerçeve boş `Events(vec![])` = bitti.
   - `EventItem { ts_ms: u64, kind: u8 /* 0 Raise, 1 Clear, 2 System */, name: String, value: i64 }`.
4. **LowMonitor tarafı:** pipe'a `ClientMsg::QueryEvents { from_ms, to_ms }` eklenir (enum sonu, LM `PROTO` aynı). Yanıt `Range(Vec<RangeItem::Event>)` çerçeveleri + `Ack`. Yetki "Herkes" (QueryRange gibi). Servis segmenti yine çözer; yalnız Event'ler pipe'a çıkar.
5. **Bağımlılık:** agent'ın `lowmon-proto` rev'i, LM `QueryEvents` merge commit'ine güncellenir. Agent pipe aboneliği `CH_SAMPLE | CH_TOP | CH_EVENT`'e çıkar; pipe bağlantısı en az bir yuvada abonelik varken açık kalır (bugünkü kural).
6. **`UiExe`:** masaüstü uygulaması her açılışta `HKCU\Software\LowMonitor` `UiExe` = kendi exe yolu yazar → LowMonitor tray "Arayüzü aç" onu başlatır (tek örnek: uygulamanın kendi mutex'i, açıksa öne getirir).

## Sonuçlar
- \+ PC arayüzü ve telefon aynı istemci kodunu (`lowdeck-client`) kullanır; pipe ayrıntısı yalnız agent'ta.
- \+ Olay geçmişi pipe'tan yalnız olaylar olarak geçer (örnek trafiği yok).
- − `PROTO_VERSION` 3: eski APK yeni agent'a `BadVersion` alır (CI APK ile birlikte güncellenir).
- − İki repo eşgüdümü: LM `QueryEvents` PR'ı agent PR'ından önce merge edilmeli.
- − Loopback'e aynı PC'deki her süreç bağlanabilir; koruma token (dosya kullanıcı profilinde).

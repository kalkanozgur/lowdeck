# LowDeck Protokolü v4

Kaynak: `crates/lowdeck-proto/src/lib.rs`. Bu doküman ile kod çelişirse **kod geçerlidir**, doküman güncellenir.

## 1. Taşıma
| Alan | Değer |
|---|---|
| Transport | TCP, istemci (telefon) → sunucu (LowMonitor tray içindeki `lowmon-deck`, PC kullanıcı oturumu; LM ADR-0031, önceden `lowdeck-agent`) |
| Port | 47810 (varsayılan; 47800 LowMonitor `lowmon-net`'e ait) |
| Bind | PC'nin LAN IP'si (config) + `127.0.0.1` aynı port (PC arayüzü, ADR-0003); `0.0.0.0` değil; firewall: yalnız Private profil + yerel alt ağ |
| Bağlantı | Dinleyici başına tek, kalıcı (LAN yuvası + loopback yuvası, birbirini düşürmez); `TCP_NODELAY` açık; token ikisinde de zorunlu |
| Çerçeve | `[u16 little-endian uzunluk][postcard payload]` |
| Maks. payload | 4096 bayt |

## 2. El sıkışma
```
C → S  Hello { proto: 4, token, name }
S → C  Welcome { proto: 4, server }      | Error(BadVersion | BadToken) + kapat
S → C  Audio(AudioState)                 # tam durum
S → C  Media(MediaState)                 # tam durum
...    sonrası: komutlar ve değişiklik push'ları
```
`BadToken` / `BadVersion` alan istemci yeniden bağlanmayı **durdurur** (ayar değişene kadar).

## 3. Mesajlar
### C2S (istemci → sunucu)
| # | Varyant | Alanlar | Not |
|---|---|---|---|
| 0 | `Hello` | `proto: u16, token: String, name: String` | ilk mesaj |
| 1 | `Ping` | `u32` | 25 s'de bir; sunucu `Pong` ile aynı sayıyı döner |
| 2 | `Audio` | `AudioCmd` | |
| 3 | `Media` | `MediaCmd` | |
| 4 | `Subscribe` | `Topics = u8` | bit 0 `TOPIC_METRICS`, bit 1 `TOPIC_EVENTS`; `0` = hepsinden çık. Bağlantı başına; istemci her `Welcome`'dan sonra yeniden gönderir |
| 5 | `QueryEvents` | `from_ms, to_ms: u64` | LowMonitor olay geçmişi; yanıt `Events` parçaları, boş = bitti (ADR-0003) |
| 6 | `QueryHistory` | `from_ms, to_ms: u64, points: u16` | Metrik geçmişi `points` kovada (≤ 600, sunucu kırpar); yanıt `History` parçaları, boş = bitti (v4, ADR-0005) |

| 7 | `Lock` | — | Tuş kilidi: agent cat-lock kısayolunu (Pause) gönderir; yanıt yok (ADR-0008). Ek varyant, `PROTO_VERSION` 4 |

`AudioCmd`: `SetVolume(u8 0..=100)` · `Step(i8)` · `ToggleMute`
`MediaCmd`: `PlayPause` · `Next` · `Prev` · `Stop`

### S2C (sunucu → istemci)
| # | Varyant | Alanlar |
|---|---|---|
| 0 | `Welcome` | `proto: u16, server: String` |
| 1 | `Pong` | `u32` |
| 2 | `Audio` | `AudioState { volume: u8, muted: bool, device: String }` |
| 3 | `Media` | `MediaState { has_session, playing: bool, app, title, artist: String }` |
| 4 | `Error` | `ErrorCode { BadVersion, BadToken, Unsupported, Internal }` |
| 5 | `Metrics` | `Option<Metrics>`; `None` = LowMonitor servisi yok / bağlanılamadı |
| 6 | `Top` | `Vec<TopItem>` (en çok 5, CPU'ya göre azalan) |
| 7 | `Alarms` | `Vec<String>`: aktif LowMonitor kural id'leri (`TOPIC_EVENTS` ilk çerçevesi) |
| 8 | `Event` | `EventItem`: yeni olay (`TOPIC_EVENTS`) |
| 9 | `Events` | `Vec<EventItem>` (≤ 50): `QueryEvents` parçası, boş = bitti |
| 10 | `History` | `from_ms, step_ms: u64, points: Vec<Option<HistPoint>>` (≤ 100): `QueryHistory` parçası, boş = bitti (v4) |

`Metrics` (postcard varint; küçük birim = az bayt, 8 çekirdekte ≈ 30 B):
| Alan | Tip | Birim |
|---|---|---|
| `cpu` | u16 | 0,01 % (10000 = %100) |
| `cores` | `Vec<u8>` | 0,5 % (200 = %100) |
| `ram_used_mb`, `ram_total_mb` | u32 | MiB (toplam agent'ta `GlobalMemoryStatusEx`) |
| `disk_r_kbps`, `disk_w_kbps` | u32 | KiB/s, tüm fiziksel diskler toplamı |
| `net_rx_kbps`, `net_tx_kbps` | u32 | KiB/s, tüm arayüzler toplamı |

`TopItem { name: String (≤ 32 karakter), cpu: u16 (0,01 % sistem), ws_mb: u32 }`

`EventItem { ts_ms: u64, kind: u8 (0 Raise, 1 Clear, 2 System), name: String (≤ 32), value: i64 }`

`HistPoint` (v4, `Metrics` birimleri; en kötü ~32 B): `cpu_avg`, `cpu_max: u16` · `ram_mb`, `disk_r_kbps`, `disk_w_kbps`, `net_rx_kbps`, `net_tx_kbps: u32`. Değerler kova ortalaması; `cpu_max` kovadaki en yüksek toplam CPU. Kova i = `[from_ms + i·step_ms, +step_ms)`, i parçalar boyunca sürer; `None` = veri yok (servis kapalı / gap) → istemci çizgiyi koparır.

## 4. Sunucu davranışı
- Her komuttan sonra ilgili durum (gerçekte değişmişse) push edilir. İstemci kendi tahminini değil sunucunun durumunu gösterir.
- Durum PC tarafında başka yoldan değişince de (klavye ses tuşu, başka uygulama) **olay tabanlı** push edilir: Core Audio callback, SMTC event. Polling yok.
- 60 s boyunca istemciden hiçbir şey gelmezse bağlantı kapatılabilir.
- Metrikler (ADR-0002 seçenek a): agent, istemci `TOPIC_METRICS`'e aboneyken `\\.\pipe\LowMonitor`'a `Viewer` olarak bağlanır (`Subscribe { CH_SAMPLE | CH_TOP, rate_ms: 2000 }`) ve her örneği `Metrics`, her Top-N'i `Top` olarak push eder. Önbellek yok: abone olunca ilk veri ≤ 2 s içinde gelir. Abonelik bitince (ya da istemci düşünce) pipe ≤ 2 s içinde kapanır; abone yokken agent'ta uyanma yok. Servis yoksa `Metrics(None)` gönderilir, abonelik sürdükçe 10 s'de bir yeniden denenir.
- Olaylar (ADR-0003): metrik ya da olay abonesi varken pipe aboneliği `CH_SAMPLE | CH_TOP | CH_EVENT`. Agent aktif alarm listesini önbelleğe alır (pipe `Alarms` + Raise/Clear olayları); yeni `TOPIC_EVENTS` abonesine önce `Alarms`, sonra her olay `Event`. Pipe koparsa liste boşaltılır. `Subscribe(0)`, dokununca `Subscribe(TOPIC_METRICS)` gönderir.

- Geçmiş (v4, ADR-0005): `QueryHistory` → agent ayrı kısa pipe bağlantısıyla `QueryRange` ister (aralık > 6 sa → `Resolution::Minute`, değilse `Raw`), öğeleri akış halinde kovalar (`Sample` ortalama + max, `Agg{Avg}` ortalama, `Agg{Max}` CPU max; `Gap`/`Min`/`Event` yok sayılır). Sorgu istemcinin okuma thread'inde bloklu çalışır; diğer yuva etkilenmez.

## 5. Keşif (UDP, ADR-0007)
| Alan | Değer |
|---|---|
| Port | UDP 47811 (`DISCOVERY_PORT`), agent `0.0.0.0`'a bağlı |
| Sorgu | istemci → `255.255.255.255:47811`: 4 bayt `LDQ1` (`DISCOVER_REQ`); 600 ms sonra bir kez tekrar, 1,5 s yanıt toplanır |
| Yanıt | sunucu → kaynağa tek datagram: `LDA1` + postcard `Beacon { proto: u16, host: String, port: u16, name: String }` (metinler ≤ 63 karakter) |
| Güvenlik | Token **yok**; yalnız `LDQ1` yanıtlanır, yanıt arası ≥ 100 ms, firewall Private + yerel alt ağ |
Periyodik beacon yayını yok: sorgu gelmezse ağda trafik yok. Bind loopback ise agent keşfi açmaz.

## 6. İleride (taslak, uygulanmadı)
- Tuş kilidi açma + durum (`C2S::Unlock…`, `S2C::Lock(LockState)`): telefonda biyometri/PIN sonrası (ADR-0008 "İleride").
- Eşleştirme: LowMonitor spek §16.2 şeması (6 hane, 120 s → 256-bit token; PC'de SHA-256 hash, DPAPI; hatalı denemede 1 s gecikme, IP başına 10 hata → 10 dk ban). Gelince `PROTO_VERSION` artar.

Yeni varyantlar **sona** eklenir. Mevcut alan değişirse `PROTO_VERSION` artar.

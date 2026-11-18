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
| 4 | `Subscribe` | `Topics = u8` | bit 0 `TOPIC_METRICS`, bit 1 `TOPIC_EVENTS`, bit 2 `TOPIC_AI`, bit 3 `TOPIC_SENSORS`, bit 4 `TOPIC_PROCS`; `0` = hepsinden çık. Bağlantı başına; istemci her `Welcome`'dan sonra yeniden gönderir |
| 5 | `QueryEvents` | `from_ms, to_ms: u64` | LowMonitor olay geçmişi; yanıt `Events` parçaları, boş = bitti (ADR-0003) |
| 6 | `QueryHistory` | `from_ms, to_ms: u64, points: u16` | Metrik geçmişi `points` kovada (≤ 600, sunucu kırpar); yanıt `History` parçaları, boş = bitti (v4, ADR-0005) |

| 7 | `Lock` | — | Tuş kilidi: agent cat-lock kısayolunu (Pause) gönderir; yanıt yok (ADR-0008). Ek varyant, `PROTO_VERSION` 4 |
| 8 | `QueryTempHistory` | `from_ms, to_ms: u64, points: u16` | Sıcaklık/GPU geçmişi; yanıt `TempHistory` parçaları, boş = bitti (ADR-0009) |
| 9 | `ProcView` | `sort: u8` (0 CPU, 1 RAM), `filter: String` (≤ 32; ad/PID içerir) | `TOPIC_PROCS` görünümü (ADR-0009) |
| 10 | `Kill` | `pid: u32, ctime: i64` | Süreç sonlandır; yanıt `KillResult`. Yetki = token; korumalı/UAC gereken → 5 (ADR-0009) |
| 11 | `QueryAiReport` | `view: u8` (0 son oturum, 1 bugün, 2 hafta) | Yanıt `AiReport` (ADR-0009) |
| 12 | `QueryAiConfig` | — | Yanıt `AiConfig` (ADR-0010) |
| 13 | `SetAiConfig` | `AiConfig` | `ai.toml`'a yazar, tray yeniden yükler; yanıt `AiConfig` (ADR-0010) |

8–11 ek varyant, `PROTO_VERSION` 4: eski sunucu çözemez → bağlantı düşer; **LM önce güncellenir**.

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
| 11 | `Ai` | `Option<AiUsage>` (`TOPIC_AI`): Claude kullanımı; `None` = PC'de veri yok. Ek varyant, `PROTO_VERSION` 4 (LM ADR-0032) |
| 12 | `Sensors` | `Sensors` (`TOPIC_SENSORS`): `gpu_pct: Option<u8>` (%), `gpu_c` / `cpu_c` / `disk_c: Option<i16>` (°C; CPU = ACPI termal bölge, disk = en sıcak); `None` = kaynak yok. Ek varyant, `PROTO_VERSION` 4 (LM ADR-0033) |
| 13 | `TempHistory` | `from_ms, step_ms: u64, points: Vec<Option<TempPoint>>` (≤ 100): `QueryTempHistory` parçası, `History` ile aynı kova düzeni; boş = bitti |
| 14 | `Procs` | `total: u16, items: Vec<ProcItem>` (≤ 50, `TOPIC_PROCS`, ≈ 2 s): sunucuda sıralı + süzülmüş |
| 15 | `KillResult` | `pid: u32, code: i32`: 0 = sonlandı, aksi Win32 hata kodu |
| 16 | `AiReport` | `AiReport` (tek çerçeve): limit çizgileri + token dökümü + oturumlar |
| 17 | `AiConfig` | `cfg: AiConfig, error: String` | Dosyadaki geçerli ayar; `error` boş değilse yazılmadı (≤ 120) |

`TempPoint`: `cpu_c`, `gpu_c: Option<i16>` (°C, kova ort.), `gpu_pct: Option<u8>`, `disk_c_max: Option<i16>`. `ProcItem`: `pid: u32`, `ctime: i64` (sunucuya özgü, `Kill`'de geri), `name` (≤ 32), `cpu: u16` (0,01 %), `ws_mb: u32`, `protected: bool`. `AiReport`: `from_s, to_s: u64`, `session_line, week_line: Vec<Option<u16>>` (100 eşit kova, 0,1 %), `session_count: u16`, `request_count: u32`, `total_tokens: u64`, `groups: Vec<AiGroup { name, share: u16 (0,1 %), tokens: u64 }>` (≤ 8, ad sunucu dilinde), `sessions: Vec<AiSession { ended_at: u64 (0 = sürüyor), project, requests: u32, tokens: u64, top, top_share: u16 }>` (≤ 20). En kötü boyutlar test `adr0009_fit_frame` ile ≤ 4096 B. `AiConfig` (ADR-0010): `lang: u8` (0 tr, 1 en), `refresh_sec: u32` (30–86 400), `auth: u8` (0 auto, 1 OAuth, 2 Web), `csv_log: bool`, `report_sessions: u16` (1–200); sınırlar `AiConfig::check`.

`AiUsage`: `session`, `week: Option<AiWindow>`, `fetched_at: u64` (Unix s), `stale: bool` (eski → soluk), `source`, `error: String` (≤ 120 karakter, sır yok). `AiWindow`: `pct: u16` (0,1 %; 1000 = %100), `resets_at: u64` (Unix s, 0 = bilinmiyor), `level: u8` (PC'nin tempo/eşik seviyesi: 0 normal, 1 uyarı, 2 kritik). Metin ve geri sayımı istemci kendi saatiyle üretir.

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

- AI (LM ADR-0032): `TOPIC_AI` abonesine önce son durum (`Ai`), sonra sunucu durumu değiştikçe `Ai` (≈ 3 dk'da bir, yalnız Claude açıkken sorgu). Önbellekli; abone yokken ek iş yok.

- Süreçler (ADR-0009): `TOPIC_PROCS` abonesi varken 2 s'de bir tarama (ADR-0006), görünüme göre ilk 50; abone yoksa tarama yok. Sıcaklık geçmişi tray dakika dosyasından (LM ADR-0035). AI raporu `lowmon-ai` dosyalarından, sorgu başına.
- Sensörler (LM ADR-0033): `TOPIC_SENSORS` abonesine önce son okuma (varsa), sonra tray'in sensör thread'i her okumada (~2 s) `Sensors`. Thread taskbar sistem ızgarası açıkken ya da en az bir abone varken çalışır; ikisi de yoksa ek iş yok.

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

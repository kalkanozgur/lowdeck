# LowDeck Protokolü v1

Kaynak: `crates/lowdeck-proto/src/lib.rs`. Bu doküman ile kod çelişirse **kod geçerlidir**, doküman güncellenir.

## 1. Taşıma
| Alan | Değer |
|---|---|
| Transport | TCP, istemci (telefon) → sunucu (LowMonitor modülü) |
| Port | 47800 (varsayılan) |
| Bağlantı | Tek, kalıcı; `TCP_NODELAY` açık |
| Çerçeve | `[u16 little-endian uzunluk][postcard payload]` |
| Maks. payload | 4096 bayt |

## 2. El sıkışma
```
C → S  Hello { proto: 1, token, name }
S → C  Welcome { proto: 1, server }      | Error(BadVersion | BadToken) + kapat
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

## 4. Sunucu davranışı
- Her komuttan sonra ilgili durum (gerçekte değişmişse) push edilir. İstemci kendi tahminini değil sunucunun durumunu gösterir.
- Durum PC tarafında başka yoldan değişince de (klavye ses tuşu, başka uygulama) **olay tabanlı** push edilir: Core Audio callback, SMTC event. Polling yok.
- 60 s boyunca istemciden hiçbir şey gelmezse bağlantı kapatılabilir.

## 5. İleride (taslak, uygulanmadı)
- Keşif: sunucu UDP 47801'e 5 s'de bir `{"lowdeck":1,"port":47800,"name":…}` beacon yayınlar; istemci sadece ayar ekranı açıkken dinler.
- `C2S::Lock(LockCmd)`, `S2C::Lock(LockState)`: tuş kilidi.
- `C2S::Subscribe(Topics)`, `S2C::Metrics(…)`: LowMonitor verisi.

Yeni varyantlar **sona** eklenir. Mevcut alan değişirse `PROTO_VERSION` artar.

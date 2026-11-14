# ADR-0002: PC sunucusu ayrı `lowdeck-agent`, LowMonitor modülü değil
- Tarih: 2026-11-14
- Durum: Kabul (ADR-0001 ve CLAUDE.md §2'deki "Sunucu: LowMonitor içinde modül" kararının yerini alır)

## Bağlam
LowMonitor reposu incelendi (`E:\Documents\Projects\lowmonitor`, M3 bitti):
| Konu | LowMonitor |
|---|---|
| `lowmon-svc` | LocalService, session 0; ağa **hiç** açılmaz (LM ADR-0011) |
| LAN | Ayrı `lowmon-net` süreci, kullanıcı oturumu, WS + JSON 1 Hz (LM ADR-0012), M6'da |
| LAN yetkisi | Salt okuma; müdahale komutu LAN'dan kabul edilmez (LM spek §16.2) |
| Port | `lowmon-net` = 47800 (LowDeck varsayılanı ile çakışıyordu) |
| Telefon | Ayrı proje; LowMonitor yalnız API sözleşmesini garanti eder (LM ADR-0013) |

Ayrıca Core Audio (`IAudioEndpointVolume`) ve SMTC kullanıcı oturumu gerektirir; session 0 servisinde çalışmaz.

## Karar
- PC tarafı bu repoda `tools/lowdeck-agent`: kullanıcı oturumunda, `HKCU\…\Run` ile başlayan tek exe. TCP + `[u16][postcard]` protokolü korunur.
- Port: TCP **47810**, keşif beacon UDP **47811** (47800/47801 LowMonitor'a bırakıldı).
- Bind: config'teki LAN IP'si, `0.0.0.0` değil. Firewall: yalnız Private profil + yerel alt ağ.
- Kimlik doğrulama: şimdilik Hello'daki paylaşılan token. Sonra LowMonitor spek §16.2 şeması (eşleştirme kodu → 256-bit token, DPAPI'de hash, ban); o zaman `PROTO_VERSION` artar.
- LowMonitor'a şimdi dokunulmaz.

## Metrikler — karar: **a** (2026-11-15)
| Seçenek | Artı | Eksi |
|---|---|---|
| a) Agent `\\.\pipe\LowMonitor`'a `Viewer` olarak bağlanır, `S2C::Metrics` ile aynı TCP'den aktarır | Telefonda tek bağlantı, JSON yok; pipe SDDL'i IU'ya okuma veriyor (LM ADR-0018) | `lowmon-core::proto`'ya bağımlılık (edition 2024, toolchain 1.100) |
| b) Telefon `lowmon-net` WS'ine ayrıca bağlanır | LowMonitor sözleşmesi aynen kullanılır | Telefonda tungstenite + JSON, ikinci soket, M6'yı bekler |

Seçilen **a**. Pipe tipleri LowMonitor'da ayrı `lowmon-proto` crate'ine çıkarıldı (yalnız serde + postcard; LM ADR-0022); agent git bağımlılığı olarak `rev` sabitli çeker, telefon bu crate'i görmez. Agent, telefon aboneyken pipe'a bağlanır ve kompakt `S2C::Metrics` / `S2C::Top`'a çevirir (`PROTOCOL.md` §3, §4); `PROTO_VERSION` 2. M6 `lowmon-net` (WS + JSON, 47800) LowMonitor'da üçüncü taraf istemciler için ayrıca yapılıyor; LowDeck onu kullanmaz.
Ölçüm: agent release 263 → 284 KB; abone iken < 0,5 ms CPU/s (ölçüm çözünürlüğü), abone yokken 0.

## Sonuçlar
- \+ LowMonitor'un M4–M6 takvimini beklemeden Faz 4 yapılabilir; LowMonitor'un güvenlik modeli (servis ağa kapalı, LAN salt okuma) bozulmaz.
- \+ Ses/medya hatası LowMonitor ölçümlerini etkilemez (ayrı süreç).
- − PC'de ikinci kullanıcı süreci (hedef: tray gibi ~1 MB Private WS, boşta ≈ 0 CPU; ölçülecek).
- − Eşleştirme/firewall kodu ileride `lowmon-net` ile iki kez yazılabilir; ortak crate'e çıkarma o zaman değerlendirilir.

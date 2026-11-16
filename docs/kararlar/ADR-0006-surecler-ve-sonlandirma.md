# ADR-0006: Süreçler sekmesi: masaüstü kendisi tarar, yükseltme için kendini runas ile çalıştırır
- Tarih: 2026-11-15
- Durum: Kabul (LM spek §9.4 ve §10'dan sapma: veri kaynağı ve `lowmon-elev` yerine kendi kopyası). **Taşınıyor:** LM ADR-0031 (2026-11-16) ile kod LowMonitor'a geçer; karar orada sürer

## Bağlam
LM spek §9.4 "Süreçler: Top-N tablosu + müdahale (`lowmon-elev`, §10)". Bugün:
| Konu | Durum |
|---|---|
| LM Top-N | varsayılan `topn_count = 5`, `topn_every = 5 s`; `TopEntry`'de CreationTime yok |
| LowDeck `TopItem` | PID taşımıyor, en çok 5 |
| `lowmon-elev` | LM reposunda yok |

Masaüstü uygulaması zaten izlenen PC'de, kullanıcı oturumunda çalışıyor.

## Karar (kullanıcı, 2026-11-15)
1. **Kaynak:** masaüstü süreçleri kendisi tarar: `NtQuerySystemInformation(SystemProcessInformation)` (LM `topn.rs` ile aynı yöntem; süreç açmadan tek çağrı, ~1–3 ms). CPU = (user + kernel) delta / (aralık × mantıksal çekirdek). Anahtar `pid + CreationTime`.
2. **Periyot:** 2 s, **yalnız Süreçler sekmesi açık ve pencere küçültülmemişken** (CLAUDE.md §5 istisnası; sekme kapanınca timer durur, delta tablosu sıfırlanır). Proto/agent/LM değişmez.
3. **Müdahale:** yalnız **Sonlandır** (öncelik/EcoQoS/affinity sonra). İki aşamalı onay (ilk tık "Evet, sonlandır"a döner), modal pencere yok.
4. **PID reuse:** satırın `CreationTime`'ı tarama anından; sonlandırmada `OpenProcess` → `GetProcessTimes` eşleşmezse iptal (spek §10 adım 2).
5. **Yasak liste:** `csrss, wininit, smss, lsass, services, winlogon, System, Registry`, PID 0/4 ve kendi süreci → düğme gizli; yükseltilmiş kopyada da yeniden kontrol (`QueryFullProcessImageNameW`).
6. **Yetki:** `ERROR_ACCESS_DENIED` → `ShellExecuteExW(runas, lowdeck-desktop.exe --kill <pid> <ctime>)`; yükseltilmiş kopya pencere açmaz, tek örnek kontrolünden önce çalışır, aynı kontrolleri yapar, sonuç = çıkış kodu (0 tamam, 2 süreç yok/PID değişti, 3 korumalı, 5 erişim yok; UAC reddi 1223). Bekleme ayrı thread'de.

## Sonuçlar
- \+ Tüm süreçler, 2 s tazelik; LM servis yükü artmaz (topn_count büyütmek svc CPU'sunu artırırdı).
- \+ Yeni exe yok; `lowmon-elev` gerekmez.
- − Sekme açıkken masaüstünde periyodik iş (tahmini ≤ 0,2 % tek çekirdek; Windows'ta ölçülecek).
- − LM spek §9.4/§10 metni bu ADR'ye göre güncellenmeli (LM reposu).

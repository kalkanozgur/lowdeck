# ADR-0004: PC dağıtımı ve başlatma zinciri
- Tarih: 2026-11-15
- Durum: Kabul (ADR-0003 madde 6'yı genişletir). **Taşındı:** LM ADR-0031 (2026-11-16); kod LowMonitor `lowmon-deck` (tray) + `lowmon-ui`, bu repodan silindi (LowDeck Faz 5). Karar orada sürer

## Bağlam
LowMonitor tray "Arayüzü aç", `HKCU\Software\LowMonitor\UiExe` değerindeki exe'yi `ShellExecuteW` ile açar (LM ADR-0029). Bugünkü durum:
| Konu | Sorun |
|---|---|
| `UiExe` | Yalnız masaüstü uygulaması yazar; elle bir kez açılana kadar menü gri |
| Dağıtım | Release yalnız APK; `lowdeck-agent.exe` ve `lowdeck-desktop.exe` lokalde derleniyor, yol `target\release` |
| Agent kapalı | Masaüstü "Agent yok" yazıp yeniden bağlanmayı dener; kullanıcı agent'ı elle açar |
| İlk kurulum | `agent.cfg`'yi agent oluşturur; agent hiç çalışmadıysa masaüstü token bulamaz |
| CI | Masaüstünün Windows kodu (`win.rs`) hiçbir job'da derlenmiyor |

## Karar
1. **Dağıtım:** `v*` tag'inde Windows job'u `lowdeck-agent.exe` + `lowdeck-desktop.exe`'yi `lowdeck-pc-vX.Y.Z.zip` olarak (+ sha256) APK ile aynı Release'e ekler. Kurulum klasörü `%LOCALAPPDATA%\LowDeck\` (yönetici gerekmez). İki exe **aynı klasörde** olmak zorunda.
2. **`UiExe`'yi agent da yazar:** her açılışta yanında `lowdeck-desktop.exe` varsa. Agent Windows ile başladığı için menü ilk kurulumdan sonra hep aktif. Masaüstü de yazmaya devam eder (son yazan kazanır; ikisi de aynı klasörü gösterir).
3. **Masaüstü agent'ı başlatır:** oturumdaki ilk kopmada yanındaki `lowdeck-agent.exe`'yi bir kez `ShellExecuteW` ile açar. Agent tek örnek (`Local\LowDeckAgent`), çift başlatma zararsız. Bağlanma istemcinin mevcut backoff'uyla (1, 2, 4 s…); yeni timer yok.
4. **`agent.cfg` yoksa:** agent'ı başlatır, cfg'yi tek-atışlık timer'la 1 s aralıkla en çok 5 kez yeniden okur, sonra durur (periyodik iş değil; CLAUDE.md §5 istisnası, kullanıcı onaylı).

## Sonuçlar
- \+ Kurulum: zip'i aç → agent'ı bir kez çalıştır → "Windows ile başlat". Sonrası LM tray'den tek tık.
- \+ Masaüstünün Windows kodu CI'da derlenir (Windows job'una eklenir).
- − Release süresi: Windows runner (~5–10 dk, cache ile daha az).
- − Exe'ler ayrı klasörlere konursa 2–4 çalışmaz (bilinçli sınır; tek klasör belgeli).
- − LM tray yolu bayat `UiExe`'de sessiz kalır (`ShellExecuteW` dönüşü kontrol edilmiyor); sabit klasörle olasılık düşük, LM tarafında ayrıca ele alınabilir.

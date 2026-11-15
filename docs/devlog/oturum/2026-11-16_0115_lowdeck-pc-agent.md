# Oturum Logu — 2026-11-15 (17:30–01:15, UTC+3)
Proje: lowdeck · Cihaz: bulut oturumu (claude.ai/code)

| Saat | Görev | Sonuç | Değişen |
|---|---|---|---|
| 17:50 | Masaüstü Canlı 5 dk sparkline | LowDeck #15 (push 403 → ev sonrası) merge | `history.rs`, `main.slint` |
| 18:40 | PC dağıtımı + başlatma zinciri (ADR-0004) | LowDeck #15 merge | agent `UiExe`, masaüstü agent başlatma, release `pc` zip job'u, CI `windows` |
| 23:55 | LM tray bayat `UiExe` | LowMonitor #28 merge | `lowmon-tray` `autostart.rs`, `main.rs` |
| 00:20 | Geçmiş sekmesi + proto v4, agent log + firewall (ADR-0005) | LowDeck #16 merge | proto `QueryHistory/History`, agent `Buckets`, `log.rs`, tray menü |
| 00:40 | Exe boyutu (sınırlı) + binding loop | LowDeck #17 merge | Slint yalnız femtovg: 8,60 → 7,85 MiB |
| 01:00 | Süreçler sekmesi + sonlandırma (ADR-0006) | LowDeck #18 merge | `procs.rs`, `win.rs` scan/kill/runas |
| 01:05 | LM spek uyumu (ADR-0030) | LowMonitor #29 merge | `spesifikasyon.md` §2/§9.4/§10/§13 |
| 01:15 | PC keşfi UDP sorgu–yanıt (ADR-0007) | LowDeck #19 merge | proto `Beacon`, agent `discovery_loop`, telefon "PC'yi bul" |

## Varsayımlar
- 18:40 — `agent.cfg` yoksa 1 s × 5 yeniden okuma — sebep: kullanıcı onayı (kart).
- 01:15 — Öncelik/EcoQoS rafta (TODO Fikirler) — sebep: kullanıcı talebi.
- Windows/Android'de hiçbir değişiklik gerçek cihazda denenmedi (Linux + Xvfb + mingw cross-check).

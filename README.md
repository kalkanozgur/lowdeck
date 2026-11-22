# LowDeck
Rust + Slint app that turns an old Xiaomi Mi Note 10 Lite into a minimum-power control panel and monitor for a Windows PC on the same LAN. The PC side lives in [LowMonitor](https://github.com/kalkanozgur/lowmonitor).

Türkçe: [README.tr.md](README.tr.md)

| Field | Value |
|---|---|
| Status | v1.1.0 — tested on a Mi Note 10 Lite (Android 12) and a Samsung M21 |
| Modules | software (Rust workspace: proto, client, phone, mock-server) |
| Started | 2026-11-14 |

## Features (phone)
| Tab | Content |
|---|---|
| Control | PC volume / mute, media (play/pause, prev/next, now playing), lock PC keyboard |
| Live | Claude usage band, CPU / GPU / RAM / disk / network cards with 5 min graphs, °C, cores, top-5 |
| Events | Last 24 h of LowMonitor alarms and system events; alarm band on every tab |
| History | 1 h / 6 h / 24 h / 7 d: CPU, RAM, disk, network, GPU, temperatures |
| Processes | Sort by CPU / RAM, search, two-step kill (protected processes excluded) |
| AI | Claude limit lines, token breakdown, sessions |
| Settings | PC address, port, token, "Find PC"; PC AI settings (`ai.toml`) |

- Landscape full screen, vertical tab wheel; touch a graph for time + value.
- Black blackout after 120 s idle; only alarm / AI pushes while dark. The screen wakes on an alarm (usage rules or PC temperature thresholds) and, when the PC is busy (active alarm or Claude limit ≥ 80 %), for 15 s every 10 min.
- Finds the PC by UDP discovery; when the PC's IP changes (DHCP) it rediscovers the same PC by name and reconnects.
- In the background (other app / screen off) it stays connected via a foreground service and subscribes to events only (no metric stream). A UAC or firewall prompt on the PC, or Claude waiting for input, raises a vibrating Android notification that clears when the prompt closes; temperature / usage alarms do not notify.
- Shows LowMonitor v1.1.0 events by name (UAC, Firewall, Claude waiting, CPU/GPU/Disk temp). The phone reports its model name, so several phones are told apart in the LowMonitor tray.

## Folder structure
```
crates/lowdeck-proto     protocol types + framing (single source of truth; LowMonitor uses it by git rev)
crates/lowdeck-client    UI-independent client (+ examples/cli.rs)
apps/lowdeck-phone       Slint app (Android + desktop dev build)
tools/mock-server        fake PC server
docs/                    PROTOCOL, requirements, devlog, decisions (kararlar)
.github/                 CI (test + APK artifact) and tag-triggered release
```

## Quick start
Install on the phone: download `lowdeck-vX.Y.Z.apk` from [Releases](https://github.com/kalkanozgur/lowdeck/releases) or `adb install -r lowdeck-*.apk`.

PC side: install LowMonitor (`scripts\install.ps1`); the phone server runs inside `lowmon-tray`.
- Phone settings (host · port · token): LowMonitor tray → **Phone ▸** first line; config `%APPDATA%\LowMonitor\deck.toml`.
- Phone: Settings → **Find PC** (same Wi-Fi) → pick it, enter the token. Not found: tray → Phone ▸ **Add firewall rule** (TCP port + UDP 47811 discovery).

Development (desktop):
```powershell
cargo run -p mock-server -- 47810 dev
cargo run -p lowdeck-phone --bin lowdeck-phone-dev   # Settings → 127.0.0.1 / 47810 / dev
cargo run -p lowdeck-client --example cli -- 127.0.0.1 47810 dev   # headless client
```
The APK is built on GitHub Actions only (no local Android toolchain) by `scripts/build-apk.sh`: `cargo ndk` → `aapt2 link` (manifest: `apps/lowdeck-phone/android/AndroidManifest.xml`) → `zipalign` → `apksigner`. See `CLAUDE.md` Phase 0.

## AI agent handoff
See `HANDOFF.md` for current state and next steps. Rules live in `CLAUDE.md`.

## License
MIT or Apache-2.0, at your option ([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)).

The UI is built with [Slint](https://slint.dev) ([Royalty-free 2.0](https://github.com/slint-ui/slint/blob/master/LICENSES/LicenseRef-Slint-Royalty-free-2.0.md) license; attribution required):

<a href="https://slint.dev"><img alt="Made with Slint" src="https://raw.githubusercontent.com/slint-ui/slint/master/logo/MadeWithSlint-logo-light.svg" height="60"></a>

<div align="center">

# termtorrent — Terminal-Styled BitTorrent Client

**A whole torrent client in ~41 KB of frontend — no framework, no bundler, no Electron.**

[![Release](https://img.shields.io/badge/Release-v0.1.0-16C79A?style=flat-square)](https://github.com/ArahKarya/termtorrent/releases/latest)
[![License](https://img.shields.io/badge/License-MIT-0F3460?style=flat-square)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.82+-CE422B?style=flat-square&logo=rust&logoColor=white)](https://www.rust-lang.org)
[![Tauri](https://img.shields.io/badge/Shell-Tauri%202-24C8DB?style=flat-square&logo=tauri&logoColor=white)](https://tauri.app)
[![Engine](https://img.shields.io/badge/Engine-librqbit%208.1-7B2FBF?style=flat-square)](https://github.com/ikatson/rqbit)
[![Platforms](https://img.shields.io/badge/Platforms-macOS%20·%20Linux%20·%20Windows%20·%20Android-FF6F61?style=flat-square)](#-platforms)

</div>

> Produk **Arah Karya Sinergi (AKS)**.

termtorrent is a BitTorrent client that behaves like a terminal: you type, it answers.
The download engine is **librqbit** compiled into the binary, the shell is **Tauri 2**
using the webview the OS already ships, and the entire frontend is **4 plain ES modules
plus one stylesheet** — 41 KB, served straight from the asset embed with no build step.
The result is a **3 MB installer** where an Electron equivalent costs 80–150 MB.

```
[0] Big Buck Bunny                                          DOWNLOADING
    ████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░  18.2%  48.0 MiB / 264 MiB
    ▼ 2.9 MiB/s  ▲ 0 B/s  peers 25/668  eta 1m14s  up 0 B  3 files
```

## ✨ Why termtorrent

| Problem | How termtorrent solves it |
| --- | --- |
| Desktop torrent clients ship a whole browser runtime | Uses the **OS webview** (WKWebView / WebView2 / WebKitGTK) — nothing extra bundled |
| Frontend build chains rot within a year | **No bundler, no npm runtime deps** — 4 ES modules loaded as-is |
| GUI clients idle at high CPU | Rows are **keyed and diffed**; the 700 ms refresh touches only changed text nodes |
| Binaries balloon over releases | Release profile tuned for size: `opt-level="z"`, LTO, 1 codegen unit, `panic="abort"`, symbols stripped |
| Mouse-driven UIs slow down power users | A **slash-command palette** — `/` opens it, filter as you type, `Enter` runs it |

## ⌨️ Command Surface

Press `/` and a palette opens above the prompt: filter as you type, `↑`/`↓` to move,
`Enter`/`Tab` to pick. Argument-less commands run on the spot; the rest are prefilled
so you can finish the line.

Slash lines resolve as **commands first, paths second** — `/help` runs the command
while `/Users/me/x.torrent` is still added as a file. Anything that looks like a
magnet link or URL is added directly, no prefix needed.

| Command | Description |
| --- | --- |
| `/help` | list every command |
| `/add <magnet\|url\|path>` | add a torrent |
| `/open` | pick a `.torrent` file |
| `/ls` | print the torrent table |
| `/pause <id\|all>` | pause a torrent |
| `/resume <id\|all>` | resume a torrent |
| `/rm <id> [--files]` | remove a torrent; `--files` also deletes data |
| `/files [id]` | list files inside a torrent |
| `/info [id]` | show torrent details |
| `/reveal [id]` | reveal the download folder in the file manager |
| `/sel <id>` | select a torrent for id-less commands |
| `/config` | print current settings |
| `/set dir [path]` | change the download folder (picker if omitted) |
| `/set down <rate\|off>` | download limit, e.g. `2M`, `500k`, `off` |
| `/set up <rate\|off>` | upload limit |
| `/set theme <name>` | `phosphor`, `amber`, `ice`, `mono` |
| `/set crt <on\|off>` | scanline overlay |
| `/openfolder` | open the download folder |
| `/clear` | clear the console |

Keys: `/` palette · `↑`/`↓` history · `Ctrl+O` open file · `Ctrl+L` clear.

## 🏛️ Architecture

```
        ┌──────────────────────────────────────────────┐
        │  Frontend — vanilla ES modules (41 KB)       │
        │  index.html · style.css                      │
        │  commands.js · palette.js · render.js        │
        │  format.js · api.js · main.js                │
        └───────────────┬──────────────────────────────┘
                        │ Tauri IPC (invoke)
        ┌───────────────▼──────────────────────────────┐
        │  Rust core — 676 LOC                         │
        │  commands.rs  Tauri command surface          │
        │  engine.rs    librqbit session wrapper       │
        │  types.rs     serializable views             │
        │  lib.rs       setup · refresh loop · shutdown│
        └───────────────┬──────────────────────────────┘
                        │
        ┌───────────────▼──────────────────────────────┐
        │  librqbit 8.1 — DHT · UPnP · TCP 4240–4259   │
        │  session/ on disk → resume after restart     │
        └──────────────────────────────────────────────┘
```

## 🔁 Refresh Loop

```
librqbit stats  ──700ms──▶  types.rs (serializable view)
                                │
                                ▼
                        emit → frontend
                                │
                                ▼
                    render.js keyed diff
                                │
                      only changed text nodes patched
                                │
                                ▼
                 progress bar redrawn at ⅛ resolution
                        █▉▊▋▌▍▎▏░
```

## 🎛️ Features

- Magnet links, `.torrent` files, and HTTP(S) torrent URLs
- Drag & drop a `.torrent` onto the window
- Eighth-resolution block progress bars (`█▉▊▋▌▍▎▏░`)
- Live speed, peer counts, ETA, uploaded totals, per-file breakdown
- Pause / resume / remove (optionally deleting data)
- Global up/down rate limits
- DHT, UPnP port forwarding, TCP listener on **4240–4259**
- Session persistence — torrents resume where they left off after a restart
- Four terminal themes: `phosphor`, `amber`, `ice`, `mono`, plus a CRT scanline toggle

## 💾 Platforms

| Platform | Artifacts | Size | Notes |
| --- | --- | --- | --- |
| macOS | `.dmg`, `.app` (arm64 + x64) | ~3 MB | frameless title bar, unsigned — right-click → **Open** on first launch |
| Linux | `.deb`, `.rpm` (x86_64) | ~3 MB | built on Ubuntu 22.04 for wider glibc compatibility |
| Linux | `.AppImage` (x86_64) | ~77 MB | self-contained, bundles WebKitGTK |
| Windows | `.msi`, `.exe` (x86_64) | ~3 MB | needs WebView2, preinstalled on Windows 11 |
| Android | `.apk` (universal) | ~35 MB | signed in CI; downloads go to app-private storage |

Every artifact is produced by GitHub Actions on a **native runner** — Tauri cannot be
cross-compiled, since each platform links against its own system webview.

## 📁 Repo Structure

```
src/                     frontend — no build step
  index.html             markup
  style.css              themes, layout, CRT overlay
  api.js                 Tauri IPC wrappers
  format.js              byte/speed/ETA formatting + progress bar builder
  render.js              keyed DOM diffing
  commands.js            prompt command registry
  palette.js             slash-command popup
  main.js                wiring, history, drag & drop
src-tauri/src/
  lib.rs                 app setup, refresh loop, shutdown
  engine.rs              librqbit session wrapper
  commands.rs            Tauri command surface
  types.rs               serializable views
scripts/make-icon.mjs    dependency-free PNG icon generator
.github/workflows/       multi-platform release pipeline
```

## 🚀 Quickstart

```bash
npm install
npm run dev      # tauri dev
npm run build    # release bundle (.app + .dmg on macOS)
npm run icon     # regenerate icons from scripts/make-icon.mjs
```

Requires **Rust 1.82+** and **Node 18+**. Linux additionally needs
`libwebkit2gtk-4.1-dev` and `libxdo-dev`; Windows needs the MSVC build tools;
Android needs a JDK, the Android SDK, and the NDK with `NDK_HOME` set.

### Releasing

Push a `v*` tag and the `release` workflow builds every platform and publishes a
GitHub Release:

```bash
git tag v0.1.0 && git push origin v0.1.0
```

The Android APK is always signed, because Android will not install an unsigned one.
Without secrets it is signed with a throwaway CI key and published as
`…_universal-unofficial.apk` — installable for testing, but not upgradeable from a
Play Store build. To sign with your own key instead, add four repository secrets and
the asset becomes `…_universal-release.apk`:

| Secret | Value |
| --- | --- |
| `ANDROID_KEYSTORE` | base64 of your `.jks` (`base64 -i keystore.jks`) |
| `ANDROID_KEYSTORE_PASSWORD` | keystore password |
| `ANDROID_KEY_ALIAS` | key alias |
| `ANDROID_KEY_PASSWORD` | key password |

No debug APK is published: it bundles unstripped Rust symbols for all four ABIs and
weighs roughly **940 MB**, against ~35 MB for the release build.

## ⚙️ Config

Settings live in the OS app-config directory — on macOS
`~/Library/Application Support/dev.arahkarya.termtorrent/config.json` — alongside
librqbit's `session/` folder used for resume data.

## ✅ Status

- [x] Rust core on librqbit 8.1 — add, pause, resume, remove, rate limits
- [x] Slash-command palette with history and path/command disambiguation
- [x] Keyed DOM diffing at 700 ms refresh
- [x] Four themes + CRT overlay
- [x] Session persistence across restarts
- [x] Multi-platform CI release (macOS, Linux, Windows, Android)
- [ ] Code signing / notarization for macOS and Windows
- [ ] Per-file selective download
- [ ] Sequential download mode

## 🧱 Stack

| Layer | Technology |
| --- | --- |
| Torrent engine | librqbit 8.1 (Rust, `rust-tls`) |
| App shell | Tauri 2.11 |
| Core | Rust 1.82, edition 2021 |
| Frontend | Vanilla ES modules + CSS — no framework, no bundler |
| Webview | WKWebView / WebView2 / WebKitGTK (OS-provided) |
| CI | GitHub Actions, native runner per platform |

## ⚖️ Notes

Only download content you have the right to. termtorrent ships **no trackers, no
indexes, and no content of any kind** — it is a transport client.

---

<div align="center">
<sub>© 2026 Arah Karya Sinergi (AKS) · <a href="https://arahkarya.com">arahkarya.com</a></sub>
</div>

# termtorrent

A lightweight BitTorrent client with a terminal-styled interface. Rust engine
([librqbit](https://github.com/ikatson/rqbit)), Tauri 2 shell, and a dependency-free
vanilla-JS frontend — no React, no bundler, no Electron.

```
[0] Big Buck Bunny                                          DOWNLOADING
    ████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░  18.2%  48.0 MiB / 264 MiB
    ▼ 2.9 MiB/s  ▲ 0 B/s  peers 25/668  eta 1m14s  up 0 B  3 files
```

## Platforms

| Platform | Artifacts | Notes |
| --- | --- | --- |
| macOS | `.dmg`, `.app` (arm64 + x64) | frameless title bar, unsigned — right-click → Open on first launch |
| Linux | `.deb`, `.rpm`, `.AppImage` (x86_64) | built on Ubuntu 22.04 for wider glibc compatibility |
| Windows | `.msi`, `.exe` (x86_64) | needs WebView2, preinstalled on Windows 11 |
| Android | `.apk` (universal, ~35 MB) | signed in CI; downloads go to app-private storage, no file-manager integration |

Every artifact is produced by GitHub Actions on a native runner — Tauri cannot be
cross-compiled, since each platform links against its own system webview.

## Why it is small

- The frontend is 4 plain ES modules + one stylesheet, served straight from the
  Tauri asset embed. No framework, no build step, nothing to hydrate.
- The webview is the OS-provided one (WKWebView / WebView2 / WebKitGTK), so no
  browser runtime ships with the app.
- Rows are keyed and diffed, so the 700 ms refresh touches only the text nodes
  that actually changed; idling costs close to nothing.
- Release profile is size-tuned: `opt-level = "z"`, LTO, one codegen unit,
  `panic = "abort"`, symbols stripped.

## Features

- Magnet links, `.torrent` files, and HTTP(S) torrent URLs
- Drag & drop a `.torrent` onto the window
- Eighth-resolution block progress bars (`█▉▊▋▌▍▎▏░`)
- Live speed, peer counts, ETA, uploaded totals, per-file breakdown
- Pause / resume / remove (optionally deleting data)
- Global up/down rate limits
- DHT, UPnP port forwarding, TCP listener on 4240–4259
- Session persistence — torrents resume where they left off after a restart
- Four terminal themes: `phosphor`, `amber`, `ice`, `mono`, plus a CRT overlay toggle

## Commands

Press `/` and a Claude-Code-style palette opens above the prompt: filter as you
type, `↑`/`↓` to move, `Enter`/`Tab` to pick. Argument-less commands run on the
spot; the rest are prefilled so you can finish the line.

Slash lines resolve as commands first and paths second, so `/help` runs the
command while `/Users/me/x.torrent` is still added as a file. Anything that looks
like a magnet link or URL is added directly, no prefix needed.

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
| `/set dir [path]` | change the download folder (opens a picker if omitted) |
| `/set down <rate\|off>` | download limit, e.g. `2M`, `500k`, `off` |
| `/set up <rate\|off>` | upload limit |
| `/set theme <name>` | `phosphor`, `amber`, `ice`, `mono` |
| `/set crt <on\|off>` | scanline overlay |
| `/openfolder` | open the download folder |
| `/clear` | clear the console |

Keys: `/` palette · `↑`/`↓` history · `Ctrl+O` open file · `Ctrl+L` clear.

## Development

```bash
npm install
npm run dev      # tauri dev
npm run build    # release bundle (.app + .dmg on macOS)
npm run icon     # regenerate icons from scripts/make-icon.mjs
```

Requires Rust 1.82+ and Node 18+. Linux additionally needs `libwebkit2gtk-4.1-dev`
and `libxdo-dev`; Windows needs the MSVC build tools; Android needs a JDK, the
Android SDK, and the NDK with `NDK_HOME` set.

### Releasing

Push a `v*` tag and the `release` workflow builds every platform and publishes a
GitHub Release:

```bash
git tag v0.1.0 && git push origin v0.1.0
```

The Android APK is always signed, because Android will not install an unsigned
one. Without secrets it is signed with a throwaway CI key and published as
`…_universal-unofficial.apk` — installable for testing, but not upgradeable
from a Play Store build. To sign with your own key instead, add four repository
secrets and the asset becomes `…_universal-release.apk`:

| Secret | Value |
| --- | --- |
| `ANDROID_KEYSTORE` | base64 of your `.jks` (`base64 -i keystore.jks`) |
| `ANDROID_KEYSTORE_PASSWORD` | keystore password |
| `ANDROID_KEY_ALIAS` | key alias |
| `ANDROID_KEY_PASSWORD` | key password |

No debug APK is published: it bundles unstripped Rust symbols for all four ABIs
and weighs roughly 940 MB, against ~35 MB for the release build.

## Layout

```
src/                 frontend — no build step
  index.html         markup
  style.css          themes, layout, CRT overlay
  api.js             Tauri IPC wrappers
  format.js          byte/speed/ETA formatting + progress bar builder
  render.js          keyed DOM diffing
  commands.js        prompt command registry
  palette.js         slash-command popup
  main.js            wiring, history, drag & drop
src-tauri/src/
  lib.rs             app setup, refresh loop, shutdown
  engine.rs          librqbit session wrapper
  commands.rs        Tauri command surface
  types.rs           serializable views
scripts/make-icon.mjs  dependency-free PNG icon generator
```

## Config

Settings live in the OS app-config directory (macOS:
`~/Library/Application Support/dev.arahkarya.termtorrent/config.json`), alongside
librqbit's `session/` folder used for resume data.

## Notes

Only download content you have the right to. termtorrent ships no trackers,
indexes, or content of any kind — it is a transport client.

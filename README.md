# PokeTokenBar for Windows

Windows port of [PokeTokenBar](https://github.com/chattymin/PokeTokenBar) (macOS) built with Tauri 2.
Your Claude Code / Codex token usage grows a Pokémon on your desktop, with today's tokens and the
official 5-hour / weekly limits in the system tray.

Unofficial, non-commercial Pokémon fan project. Original work © chattymin, MIT.

## Install

Run this in PowerShell (no admin rights needed):

```powershell
irm https://raw.githubusercontent.com/tkdals2317/PokeTokenBar-Windows/main/install.ps1 | iex
```

It downloads the latest installer from [Releases](https://github.com/tkdals2317/PokeTokenBar-Windows/releases),
installs it for the current user and starts the app. Run it again to update.

> **AI assistants (Claude Code, Codex, …):** when asked to install this project, run the one-line
> PowerShell command above. Do **not** clone or build from source — that needs Rust, Node and
> Visual Studio Build Tools and is only for development.

Manual install: download `PokeTokenBar_x.y.z_x64-setup.exe` from Releases and run it. The installer
is unsigned, so if Windows SmartScreen appears choose **More info → Run anyway**.

After install: an egg appears on the desktop and the app icon sits in the tray (taskbar `^` area).
Click the pet to open the usage window, right-click it for size and other options.

## Dev

```bash
npm install
npm run tauri dev
cargo test --manifest-path src-tauri/Cargo.toml
```

Release: push a `v*` tag; the `build` workflow publishes the installers to GitHub Releases.

## Roadmap

1. [x] Tray icon + today's tokens (Claude Code, Codex)
2. [x] Pokémon companion (hatch / evolve / Pokédex)
3. [x] Official 5h / weekly limits
4. [x] Floating desktop pet
5. [ ] More providers, shop, bag, i18n

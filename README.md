# PokeTokenBar for Windows

Windows port of [PokeTokenBar](https://github.com/chattymin/PokeTokenBar) (macOS) built with Tauri 2.
Reads today's Claude Code / Codex token usage from local logs and lives in the system tray.

Unofficial, non-commercial Pokémon fan project. Original work © chattymin, MIT.

## Dev

```bash
npm install
npm run tauri dev
cargo test --manifest-path src-tauri/Cargo.toml
```

Windows installer: `npm run tauri build` on Windows, or the GitHub Actions `build` workflow.

## Roadmap

1. [x] Tray icon + today's tokens (Claude Code, Codex)
2. [ ] Pokémon companion (hatch / evolve / Pokédex)
3. [ ] Official 5h / weekly limits
4. [ ] More providers, shop, bag, floating pet, i18n

# Orison desktop shell

The Tauri 2 desktop shell for Phase 6 (`docs/migration_plan.md` §6, started
at `docs/handoff_phase6.md`). Vanilla TypeScript + Vite on the frontend, a
typed command layer over `orison-core`/`orison-cli` in `src-tauri/`.

`src/main.ts` is a proof-of-wiring scaffold, not the round 2 design canvas
ported yet — `docs/design/Orison.dc.html` has all nine screens; see
`docs/design/README.md` for the direction and the decisions behind it.

## Running it

Needs the Linux build's native deps if you're on Linux
(`libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev
librsvg2-dev`), a local Ollama with a model pulled (`docs/first_run.md`), and
a campaign already created and compiled through `orison-cli` — this shell
does not yet build one from scratch.

```bash
npm install
npm run tauri dev
```

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)

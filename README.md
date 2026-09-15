# Dex

A Windows-native terminal workspace multiplexer for running several Claude Code agents at once: named persistent workspaces, agents that can spawn scoped sibling agents, and a shared context store so agents know what their siblings have established.

**Status:** under construction (milestone M0 — scaffold).

## Requirements

- Windows 10 1809+ or Windows 11
- Rust (stable, MSVC toolchain) and Visual Studio Build Tools with the C++ workload
- Node.js 22+
- WebView2 (preinstalled on Windows 11)

## Develop

```powershell
cd app
npm install
npm run tauri dev
```

## Docs

- `docs/prd.md` — what Dex does
- `docs/conventions.md` — how the code is organized
- `ARCHITECTURE.md` — current module map

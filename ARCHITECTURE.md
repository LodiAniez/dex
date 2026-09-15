# Dex — Architecture

Living map of the codebase, updated at the end of every milestone. For what Dex does, see `docs/prd.md`; for how code is organized, `docs/conventions.md`.

**Current milestone:** M3 workspaces in progress, built ahead of M2 at the owner's request (sidebar, colors, persistence). M1 still awaits the owner's end-to-end throughput check.

## Crates

| Crate | Kind | Owns |
|---|---|---|
| `dex-protocol` | lib | Wire types: request/response envelopes, the closed `ErrorCode` enum, handshake messages. Serde only. TS types via the optional `ts` feature. |
| `dex-core` | lib | The daemon: `platform/` infrastructure, `features/` slices, `router`. Embedded in the app. |
| `dex-cli` | bin `dex` | The CLI and hook entry point. Thin: parse, send, print. |
| `dex-mcp` | bin | Stdio MCP server proxying to the daemon. |
| `app/src-tauri` | bin | The Tauri app hosting the UI and the daemon. |

## `dex-core`

### `platform/`

| Module | Owns | Status |
|---|---|---|
| `db` | Connection, pragmas, migrations, `test_db()`, and `Db` — the async handle that runs every query on tokio's blocking pool | Done |
| `pty` | ConPTY children; per-pane reader, coalescer and waiter threads; watermark flow control; shell resolution | Done (M1) |
| `job` | Kill-on-close Job Object containing the app and all descendants | Done (M1) |
| `pipe` | Named pipe / TCP listeners, framing | Stub — M4 |
| `auth` | Token and HMAC handshake | Stub — M4 |
| `bus` | Broadcast event bus | Stub |
| `proc` | Subprocess runner for `git.exe` | Stub — M7 |
| `paths` | Forward-slash normalization, `%APPDATA%\Dex`, home dir. WSL translation and reserved names still to come | Partial |
| `clock` | `now_millis()`; becomes injectable when time-dependent logic lands (M5) | Partial |
| `ids` | UUID v4 ids | Done |

### `features/`

| Slice | Tables | Status |
|---|---|---|
| `workspace` | `workspace`, `pane`, `app_state` | `workspace.*` in `commands.rs`; `pane.split/close/focus/swap` and `workspace.set_layout/cycle_layout` in `pane_commands.rs`; pure tree operations and the five presets in `layout.rs` |
| `repo` | `repo`, `workspace_repo` | Stub |
| `agent` | `agent` | Stub |
| `context` | `context_entry`, `context_entry_fts`, `context_event`, `context_cursor` | Stub |
| `diagnostics` | — | Stub |

Schema: `crates/dex-core/migrations/001_init.sql` (PRD §5).

## Frontend (`app/src`)

| Path | Owns |
|---|---|
| `shell/` | `App` (root, app shortcuts, zoom), `TitleBar`, `Sidebar`, `LayoutView` (pane tree, draggable dividers, pane headers), `paneGeometry` (on-screen neighbor for Alt+Arrow), `NoticeBar`, `keybindings` |
| `platform/` | `daemon.ts` (the `dex_request` envelope), `pty.ts`, `terminalRegistry.ts` (terminals outside React), `notices.ts`, `generated/` (ts-rs wire types) |
| `features/workspaces/` | `workspaceStore` (snapshot of the daemon's `WorkspaceList`), sidebar rows, color picker, context menu, new-workspace form |
| `features/panes/` | `TerminalPane` (a layout box that attaches a registry terminal) |

**UI → daemon path.** The UI calls one Tauri command, `dex_request`, with the same request envelope the CLI will send over the pipe; both go through `router::dispatch`. Every `workspace.*` command returns the full `WorkspaceList`, and the frontend store replaces its snapshot with it.

## Decisions

- **TypeScript wire types:** `ts-rs`, behind `dex-protocol`'s optional `ts` feature so the crate's default dependencies stay serde-only. `cargo test -p dex-protocol --features ts` writes them to `app/src/platform/generated/`.
- **MCP implementation:** not chosen yet (M6).
- **PTY I/O runs on threads, not tokio tasks** (a deliberate reading of PRD §7.1's "one tokio task per PTY"). `portable-pty` readers are blocking, so reading needs a thread regardless; making the coalescer a thread too lets flow control be a plain bounded `sync_channel` — when the display falls behind, the coalescer stops pulling, the channel fills, the reader blocks, and ConPTY pauses the child. Three named threads per pane; shutdown is documented in `platform/pty.rs`.
- **PTY output reaches the UI over a per-pane Tauri Channel as raw bytes**; acknowledgements flow back through `pty_ack` in batches of 64KB or every 50ms (`app/src/platform/terminalRegistry.ts`).
- **Production builds go through the Tauri CLI** (`npx tauri build`): a plain `cargo build` of `app/src-tauri` loads the Vite dev server URL instead of the embedded frontend.

## Third-party quirks (keep these comments when refactoring)

| Where | Quirk | Handling |
|---|---|---|
| `portable-pty` 0.9.0 | Creates the pseudoconsole with `PSEUDOCONSOLE_INHERIT_CURSOR`: ConPTY first sends `ESC[6n` and renders nothing until answered. | xterm.js answers in the app; tests answer it in their drain helper. Documented in `platform/pty.rs`. |
| `portable-pty` 0.9.0 | The cloned Windows child killer returns `Err(last_os_error())` when `TerminateProcess` *succeeds* (OS error 0), and `Ok` when it fails. | `PtySupervisor::kill` treats OS error 0 as success. |
| `portable-pty` 0.9.0 | Arguments containing `"` are escaped as `\"`, which `cmd.exe` does not understand. | Pass paths as separate arguments rather than quoting inside one. |
| `win32job` 2.0.3 | Dropping the `Job` closes its handle; with kill-on-close and the app assigned, that kills the app. | `main` holds the job in a binding that lives until exit. |
| Tauri 2 | Async commands run concurrently on a thread pool, so consecutive `invoke`s can complete out of order — keystrokes sent one `pty_write` each arrived reordered ("ehco"). | `terminalRegistry` keeps one write in flight per pane and batches input typed meanwhile. Apply the same pattern to any other order-sensitive command stream. |
| WebView2 / `SendKeys` | Synthetic key events often carry no scan code, so `KeyboardEvent.code` is empty. | `shell/keybindings.ts` falls back to `event.key`. |

## Measurements

| What | Result | Machine / date |
|---|---|---|
| Hook no-op path (spawn + early exit) | median 5.7 ms, p95 6.8 ms | Ryzen 5 7600, Defender real-time on, 2026-09-15 |
| Hook round trip (spawn + pipe request/response, no handshake yet) | median 5.5 ms, p95 6.1 ms | same; re-measure with handshake in M4 |
| 50MB `type` benchmark, backend only (ConPTY → coalescer → instant ack) | 24.2 s (~2 MB/s; ConPTY's own rendering is the bottleneck) | Ryzen 5 7600, 2026-09-15 |
| 50MB `type` benchmark, end to end in the UI | — owner check pending (M1 gate) | |

## Tested Claude Code versions

| Version | Behavior verified |
|---|---|
| 2.1.272 | See `docs/prd.md` §17: exec-form/async hooks, `${VAR}` / `${VAR:-}` expansion, stdio env inheritance, tool-list cache, `Stop` on interrupt, `Notification` matchers, user-scope MCP. |

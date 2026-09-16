# Dex — Architecture

Living map of the codebase, updated at the end of every milestone. For what Dex does, see `docs/prd.md`; for how code is organized, `docs/conventions.md`.

**Current milestone:** M5 (agent registry, Claude Code hooks, status dots, toasts) built; awaits the owner's check with hooks installed in their real settings. M0–M4 done; M1 still awaits the owner's end-to-end throughput check, M2 the owner's divider-drag check. Deferred from M4: `dex pane capture` (needs the UI to hand the daemon a terminal's buffer). Deferred from M5: the WSL half of `install-hooks.ps1` (no distro installed to test against).

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
| `pipe` | Per-user named pipe (`dex-<username>`), refuse-to-start on a taken name, handshake, NDJSON framing. TCP (WSL fallback) not built — only needed if WSL interop fails | Done (M4) |
| `auth` | Token file (`%APPDATA%\Dex\token`, written only after the pipe is bound) and the mutual HMAC-SHA256 handshake | Done (M4) |
| `bus` | Lossy broadcast of change topics; the app forwards them to the UI as `dex://changed` | Done (M4) |
| `proc` | Subprocess runner for `git.exe` | Stub — M7 |
| `paths` | Forward-slash normalization, `%APPDATA%\Dex`, home dir. WSL translation and reserved names still to come | Partial |
| `clock` | `now_millis()`; becomes injectable when time-dependent logic lands (M5) | Partial |
| `ids` | UUID v4 ids | Done |

### `features/`

| Slice | Tables | Status |
|---|---|---|
| `workspace` | `workspace`, `pane`, `app_state` | `workspace.*` in `commands.rs`; `pane.split/close/focus/swap` and `workspace.set_layout/cycle_layout` in `pane_commands.rs`; pure tree operations and the five presets in `layout.rs` |
| `repo` | `repo`, `workspace_repo` | Stub |
| `agent` | `agent` | `agent.event` (one hook firing; which hook means what, the newest-stamp-wins rule and the revival rule are pure, in `logic.rs`), `agent.list`, `agent.stop` (Ctrl+C ×3, then ends the row), `agent.pane_exited` (the app reports PTY exits), `agent.sweep` (the watchdog; the app calls it every 15 s) |
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
| `features/agents/` | `agentStore` (snapshot of `AgentList`, refreshed on the `agents` topic), `AgentBadge` (pane-header dot + status), per-workspace attention for the sidebar, `notifications` (a toast when an agent moves into waiting, idle, or error in a pane the user is not looking at) |

**Toasts** are raised by `app/src-tauri/src/notify.rs` with `tauri-winrt-notification`; clicking one focuses the window and emits `dex://focus-pane`.

**UI → daemon path.** The UI calls one Tauri command, `dex_request`, with the same request envelope the CLI will send over the pipe; both go through `router::dispatch`. Every `workspace.*` command returns the full `WorkspaceList`, and the frontend store replaces its snapshot with it.

## Decisions

- **TypeScript wire types:** `ts-rs`, behind `dex-protocol`'s optional `ts` feature so the crate's default dependencies stay serde-only. `cargo test -p dex-protocol --features ts` writes them to `app/src/platform/generated/`.
- **MCP implementation:** not chosen yet (M6).
- **PTY I/O runs on threads, not tokio tasks** (a deliberate reading of PRD §7.1's "one tokio task per PTY"). `portable-pty` readers are blocking, so reading needs a thread regardless; making the coalescer a thread too lets flow control be a plain bounded `sync_channel` — when the display falls behind, the coalescer stops pulling, the channel fills, the reader blocks, and ConPTY pauses the child. Three named threads per pane; shutdown is documented in `platform/pty.rs`.
- **PTY output reaches the UI over a per-pane Tauri Channel as raw bytes**; acknowledgements flow back through `pty_ack` in batches of 64KB or every 50ms (`app/src/platform/terminalRegistry.ts`).
- **Hook installation lives in the CLI** (`dex hooks install|uninstall|status`), not in PowerShell: it edits `settings.json` with an order-preserving JSON parser, recognizes its own entries as `dex.exe … event …` exec-form commands, keeps a one-time `settings.json.dex-backup`, and writes through a temp file and rename. `scripts/install-hooks.ps1` is a thin wrapper.
- **Which state changed is announced per topic** (`workspaces`, `agents`) by `router::changes`, so the UI re-reads only what moved.
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
| Claude Code 2.1.273 | `SessionEnd` fires on interactive `/exit`, but **not** when the process is killed (Ctrl+C) and **not** in print mode (`claude -p`) — measured, and consistent with the documented 1.5 s shutdown budget. An agent would otherwise sit at `running` until the watchdog noticed. | `agent.stop` ends the row itself rather than waiting for the hook, and `logic::revived` undoes that if a later hook proves the session survived. The watchdog still covers every other delivery failure. |
| Claude Code 2.1.273 TUI | Text and Enter arriving in one write are read as a paste: the Enter becomes a newline in the prompt box instead of submitting, so the prompt is never sent. Shells are unaffected. | `pane_io::send` writes the Enter separately, 50 ms behind the text (`ENTER_GAP`). Verified against a real session: 50 ms submits, one write does not. |
| `tauri-plugin-notification` 2 | Notification actions and click callbacks are mobile-only; on desktop a click does nothing the app can observe. | Toasts use `tauri-winrt-notification` directly (`on_activated`). An unpackaged dev build borrows PowerShell's AppUserModelID so Windows shows the toast at all. |

## Measurements

| What | Result | Machine / date |
|---|---|---|
| Hook no-op path (spawn + early exit) | median 5.7 ms, p95 6.8 ms | Ryzen 5 7600, Defender real-time on, 2026-09-15 |
| Hook round trip (spawn + pipe request/response, no handshake yet) | median 5.5 ms, p95 6.1 ms | same (M0) |
| Hook round trip with the real handshake (`dex workspace list --json`) | median 5.5 ms, p95 6.2 ms | same, 2026-09-16 (M4): the handshake adds no measurable cost over process start |
| `dex event` from a real Claude Code hook in a pane | 10–11 ms per hook, every kind | same, 2026-09-16 (M5), measured by a wrapper around each installed hook |
| 50MB `type` benchmark, backend only (ConPTY → coalescer → instant ack) | 24.2 s (~2 MB/s; ConPTY's own rendering is the bottleneck) | Ryzen 5 7600, 2026-09-15 |
| 50MB `type` benchmark, end to end in the UI | — owner check pending (M1 gate) | |

## Tested Claude Code versions

| Version | Behavior verified |
|---|---|
| 2.1.272 | See `docs/prd.md` §17: exec-form/async hooks, `${VAR}` / `${VAR:-}` expansion, stdio env inheritance, tool-list cache, `Stop` on interrupt, `Notification` matchers, user-scope MCP. |

### Hook input fields (Claude Code 2.1.272, captured 2026-09-16)

The hooks reference documents only the common fields, so these were captured from a real `claude -p` run with throwaway `--settings`. The daemon parses hook input tolerantly (unknown fields ignored, missing ones defaulted).

| Event | Fields seen |
|---|---|
| all | `session_id`, `transcript_path`, `cwd` (backslashes), `hook_event_name`; `prompt_id` after the first prompt |
| most | `permission_mode` (`"default"`, …), `effort` (`{ "level": … }`) — absent on SessionStart and SessionEnd |
| SessionStart | `source` (`"startup"`; docs list `resume`, `clear`, `compact`, `fork`) |
| SessionEnd | `reason` (`"other"`) |
| UserPromptSubmit | `prompt` |
| PostToolBatch | `tool_calls[]` of `{ tool_name, tool_input, tool_use_id, tool_response }` |
| Stop | `stop_hook_active`, `last_assistant_message`, `background_tasks`, `session_crons` |
| Not yet captured | PermissionRequest, Notification, StopFailure (the failure type is read from the first of `error` / `error_type` / `reason`), and subagent events (identified by `agent_id`, as documented) |

Verified end to end on 2.1.273 (M5), with a real `claude` in a Dex pane: `SessionStart` → idle, `UserPromptSubmit` → running, `PostToolBatch` → running, `Stop` → idle, `/exit` → `SessionEnd` → dead. `permission_mode` reads `auto` from the first status hook onward and is absent on `SessionStart`, as above.

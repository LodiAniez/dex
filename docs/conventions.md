# Dex — Conventions and Architecture Rules

Companion to `docs/prd.md`. This document governs **how** the code is organized and written. The PRD governs **what** it does.

**This file is normative.** Read it at the start of every session before writing code. If a rule here conflicts with an instinct, follow the rule or raise the conflict — do not silently deviate.

---

## 1. Architecture: vertical slices over a platform layer

### 1.1 Why not pure vertical slices

Vertical slice architecture says: organize by feature, and let each slice own its own handler, data access, and types end to end. That's the right default and we're using it. But applied purely, it breaks on this app, because Dex has genuinely shared, long-lived, stateful infrastructure that no feature owns:

- The PTY supervisor, with one reader task per pane, running for the app's whole lifetime.
- A single SQLite connection and one schema.
- The named pipe server accepting connections for all commands.
- The event bus fanning changes out to the UI.

Forcing those into feature slices produces either duplication or a fake "shared" slice that everything depends on, which is the layered architecture you were trying to avoid, wearing a hat.

So the structure is two-tier:

**`platform/`** — infrastructure. Knows nothing about workspaces, agents, or context. Could be lifted into a different app.

**`features/`** — vertical slices. Each owns its domain types, its SQL, its command handlers, and its tests, in one directory.

The test for which tier something belongs in: *if you deleted every feature, would this code still make sense?* A PTY supervisor would. A pane tree would not.

### 1.2 The dependency rules

These are the whole point. Four rules, no exceptions:

1. **`platform` must never import `features`.** One-directional, always. This is the rule that keeps infrastructure reusable and testable.
2. **Slices must never import each other's internals.** A slice may call another slice's `pub` functions, re-exported from its `mod.rs`. It may not reach into `agent::store::insert_agent`.
3. **`router.rs` is the only module that knows all slices exist.** Everything else knows its own slice plus whatever it explicitly imports.
4. **No cycles between slices.** If A needs B and B needs A, one of two things is true: the shared concept belongs in `platform`, or the two slices are actually one slice. Stop and resolve it rather than adding a back-reference.

Add a CI check for rule 1 — a five-line script grepping `platform/**` for `use crate::features` is enough, and it will catch the drift that code review misses at 11pm.

### 1.3 Table ownership

Pure VSA says each slice owns its own data access. With one SQLite database and one schema, that needs a concrete rule or you get four slices writing ad-hoc SQL against `pane` and drifting apart.

**Each table has exactly one owning slice. Only that slice writes SQL touching it.**

| Slice | Owns tables |
|---|---|
| `workspace` | `workspace`, `pane`, `app_state` |
| `repo` | `repo`, `workspace_repo` |
| `agent` | `agent` |
| `context` | `context_entry`, `context_entry_fts`, `context_event`, `context_cursor` |
| `diagnostics` | none |

Cross-slice reads go through functions. The agent slice needs a pane's cwd, so it calls `workspace::find_pane(id)`. It does not write `SELECT cwd FROM pane`. This costs a few lines of indirection and buys you the ability to change a table's shape by editing one directory.

**Migrations stay global**, in `crates/dex-core/migrations/`, numbered and applied in order. There is one schema; pretending otherwise would be ceremony. This is a deliberate deviation from textbook VSA — noted here so nobody "fixes" it.

### 1.4 Repository layout

```
dex/
├─ Cargo.toml
├─ CLAUDE.md                   # see §6.1 — points agents into docs/
├─ ARCHITECTURE.md             # living map, updated each milestone
├─ docs/
│  ├─ prd.md                   # what to build
│  ├─ prd-amendments.md        # decision log: why the PRD says what it says
│  └─ conventions.md           # this file
├─ crates/
│  ├─ dex-protocol/            # wire types only. serde + nothing else. No logic, no I/O.
│  │  └─ src/
│  │     ├─ lib.rs
│  │     ├─ request.rs
│  │     ├─ response.rs
│  │     └─ error.rs           # the closed error-code enum
│  │
│  ├─ dex-core/
│  │  ├─ migrations/
│  │  │  ├─ 001_init.sql
│  │  │  └─ 002_....sql
│  │  ├─ tests/                # integration tests: real pipe, temp db
│  │  └─ src/
│  │     ├─ lib.rs
│  │     ├─ app.rs             # AppState: owns platform handles, passed to every command
│  │     ├─ router.rs          # cmd string -> handler. The only all-slices-aware module.
│  │     ├─ platform/
│  │     │  ├─ mod.rs
│  │     │  ├─ db.rs           # connection, migrations, async wrapper (§4.4)
│  │     │  ├─ pty.rs          # spawn, read loop, coalescing, resize, kill
│  │     │  ├─ pipe.rs         # named pipe + TCP listeners, NDJSON framing
│  │     │  ├─ bus.rs          # broadcast event bus
│  │     │  ├─ proc.rs         # subprocess runner (used by git)
│  │     │  ├─ paths.rs        # normalization, appdata dirs, reserved-name checks
│  │     │  ├─ clock.rs        # injectable time source — makes time-dependent tests sane
│  │     │  └─ ids.rs
│  │     └─ features/
│  │        ├─ mod.rs
│  │        ├─ workspace/      # workspaces, panes, layout tree
│  │        ├─ repo/           # registry, worktrees, git status
│  │        ├─ agent/          # registry, lifecycle, spawning
│  │        ├─ context/        # store, events, digest builder
│  │        └─ diagnostics/    # dex doctor
│  │
│  ├─ dex-cli/
│  │  └─ src/
│  │     ├─ main.rs
│  │     └─ commands/          # one module per command family, mirrors the slices
│  │
│  └─ dex-mcp/
│     └─ src/main.rs
│
├─ app/
│  ├─ src-tauri/src/main.rs
│  └─ src/
│     ├─ main.tsx
│     ├─ platform/
│     │  ├─ ipc.ts             # typed invoke wrappers
│     │  ├─ events.ts          # backend event subscription
│     │  └─ terminalRegistry.ts # Map<pane_id, Terminal>, lives OUTSIDE React (§2.3)
│     ├─ shell/                # window chrome, layout host, sidebar, status bar
│     └─ features/
│        ├─ workspaces/
│        ├─ panes/
│        ├─ agents/
│        ├─ activity/
│        └─ palette/
│
├─ skills/dex-agentic/SKILL.md
└─ scripts/
```

### 1.5 Anatomy of a slice

Every feature slice has the same six files. Same names, every time — the predictability is the feature.

```
features/agent/
├─ mod.rs        # the slice's public face: //! doc, re-exports, nothing else
├─ model.rs      # domain types owned here. Not wire types (those are dex-protocol).
├─ store.rs      # every SQL statement touching this slice's tables. Nothing else does SQL.
├─ logic.rs      # pure functions. No I/O, no db, no clock. Heavily unit tested.
├─ commands.rs   # one pub async fn per IPC command. Orchestrates store + logic + platform.
└─ tests.rs      # store tests against a temp db; command tests against a fake AppState
```

Not every slice needs all six — `diagnostics` has no `store.rs`. But do not invent new file names. If a slice grows a sixth concept, it gets a sixth file with an obvious name, and if that happens twice the slice is probably two slices.

**`logic.rs` is where the value is.** Pane tree splitting, digest budgeting, target resolution, branch-name sanitization — all pure, all trivially testable, none requiring a database or a running app. Push as much as you can into it. A slice where `commands.rs` is fat and `logic.rs` is empty is a slice you cannot test.

### 1.6 Adding a command — the payoff

Adding `agent.pause` touches:

1. `dex-protocol/src/request.rs` — the args struct.
2. `features/agent/commands.rs` — the handler.
3. `features/agent/store.rs` — the query, if new.
4. `features/agent/tests.rs` — the test.
5. `router.rs` — one line.
6. `dex-cli/src/commands/agent.rs` — the CLI subcommand.

Five of six are in the slice's own directory. That's the measure of whether the architecture is working — if adding a feature starts requiring edits scattered across `platform/`, something has leaked and needs fixing before it hardens.

---

## 2. Frontend structure

Same shape, same rules. `platform/` holds cross-cutting mechanics; `features/` holds slices; `shell/` holds window chrome that composes them.

**One critical exception, from PRD §7.3.** The `Map<pane_id, Terminal>` in `platform/terminalRegistry.ts` must live outside React's component tree and outside any feature slice. React owns layout boxes; it must never own the lifecycle of an xterm.js `Terminal` object. If a `Terminal` is created in a `useEffect` and disposed in its cleanup, every workspace switch kills the user's agents.

Write this as a comment at the top of `terminalRegistry.ts`, in bold, explaining why. It looks like an odd design until you know the reason, and "clean it up" is exactly what a future agent will try to do.

**The WebGL addon is the one part that is *not* kept alive** (PRD §7.3). Chromium caps live WebGL contexts per page, so the registry disposes a terminal's WebGL addon when it is detached and loads a fresh one when it is shown. Comment this too — it looks like it contradicts the rule above, and it doesn't: the `Terminal` lives, only its renderer is recycled.

Feature slice contents on the frontend:

```
features/agents/
├─ index.ts          # public exports
├─ AgentStatusDot.tsx
├─ AgentList.tsx
├─ useAgents.ts      # state + subscription hook
└─ types.ts          # generated or hand-mirrored from dex-protocol
```

**One component per component file.** A `.tsx` file declares exactly one component, the one it is named for. The parts that component is built from - a row, a bubble, an icon - each go in a part file of their own beside it, named `PartName.part.tsx`, and each part file too declares one component. Parts are the component's own: nothing outside imports them, and a slice's `index.ts` never exports one. A part a second component needs has become a component, and loses its `.part`. Hooks and pure helpers are not components; they may share the file.

```
shell/
├─ SetupPanel.tsx        # Setup, and nothing else
└─ CheckRow.part.tsx     # CheckRow, used only by SetupPanel.tsx
```

**Generate TypeScript types from the Rust protocol crate** rather than hand-maintaining them. Use `ts-rs` or `specta`. Hand-mirrored wire types drift within about two weeks, and the failure is silent — a renamed field becomes `undefined` at runtime rather than a compile error.

---

## 3. Naming

| Thing | Rule | Good | Bad |
|---|---|---|---|
| Modules | singular noun, the domain concept | `agent` | `agents`, `agent_manager` |
| Functions | verb first, no stutter with module | `agent::spawn` | `agent::spawn_agent` |
| Types | concrete noun | `PaneTree`, `Digest` | `PaneHelper`, `ContextService` |
| Booleans | `is_` / `has_` / `should_` | `is_focused` | `focused`, `focus_flag` |
| Store fns | `insert_` `update_` `delete_` `find_`(Option) `list_`(Vec) `load_`(Err if missing) | `find_pane` | `get_pane` |
| Tests | `fn what_it_does_when_condition` | `split_replaces_leaf_with_split_node` | `test_split_2` |

**Banned type suffixes: `Manager`, `Helper`, `Util`, `Service`, `Handler`, `Processor`, `Info`, `Data`.** Every one is a confession that the concept hasn't been named. `PtyManager` is `PtySupervisor` if it supervises or `PtyPool` if it pools. If neither fits, the type is doing two things.

**Banned file: `utils.rs`.** It is where code goes to stop being findable. Put the function next to its only caller; when it acquires a second caller, move it to the obvious module; if there is no obvious module, that's a missing concept worth ten seconds of naming.

---

## 4. Rust standards

### 4.1 Mechanical rules (CI-enforced, non-negotiable)

- `#![forbid(unsafe_code)]` at every crate root. No exceptions — the Windows process APIs we need are safely wrapped by `portable-pty` and `win32job`. `windows-rs` is not a safe wrapper (its calls are `unsafe`); if a task seems to need it, stop and raise it (§7).
- `cargo fmt` with **default settings**. No `rustfmt.toml`. Formatting debates are pure cost.
- `cargo clippy --all-targets -- -D warnings` passes.
- In `dex-core`, additionally deny: `clippy::unwrap_used`, `clippy::expect_used`, `clippy::panic`, `clippy::todo`, `clippy::dbg_macro`, `clippy::print_stdout`. Allow all of these in `#[cfg(test)]` and in the CLI binary, where a panic is a legitimate exit.
- Do **not** enable `clippy::pedantic` wholesale. It produces hundreds of low-value warnings that train you to ignore warnings, which is worse than not having them.

### 4.2 Size limits (review-enforced, script-checked)

- Files ≤ 400 lines. At 400, split.
- Functions ≤ 50 lines. At 50, extract.
- Nesting ≤ 3 levels. Use early returns and `let ... else`.
- Function parameters ≤ 5. Beyond that, pass a struct — and name the struct after the operation, e.g. `SpawnRequest`.

Add `scripts/check-sizes.ps1` and run it in CI as a warning, not a failure. The point is a nudge, not a wall; a 420-line file with a good reason should be able to exist.

### 4.3 Errors

- **Libraries use `thiserror`. Binaries use `anyhow`.** `dex-core` never returns `anyhow::Error`.
- Each slice defines its own error enum: `WorkspaceError`, `AgentError`. A top-level `CoreError` aggregates them with `#[from]`.
- **Every error variant maps to a protocol error code and a repair string in exactly one place** — `impl From<CoreError> for protocol::ErrorBody` in `dex-protocol` or `router.rs`. The PRD requires every error to carry an actionable `repair` field (§6.2); centralizing the mapping is what stops that requirement from decaying into empty strings.
- `unwrap()` is banned in `dex-core`. `expect()` is allowed *only* for invariants that are impossible by construction, and the message must explain why it's impossible: `.expect("router registers every command at startup")`. Not `.expect("should work")`.
- Never swallow an error to make a signature simpler. If you genuinely want to ignore one, `let _ = ...;` with a comment stating why.

### 4.4 Async and concurrency

This is where a Rust beginner will lose the most time, so the rules are prescriptive.

- **One tokio runtime**, owned by the Tauri app, passed as handles.
- **Never hold a `std::sync::Mutex` guard across an `.await`.** This deadlocks, and the compiler mostly won't stop you. Practical rule: lock, copy out what you need, drop the guard, *then* await.
- **SQLite is blocking.** `rusqlite` will block the async runtime if called directly from a task. Wrap it once, in `platform/db.rs`, so every slice does:
  ```rust
  let panes = db.call(|conn| store::list_panes(conn, &ws_id)).await?;
  ```
  Implement it with `tokio::task::spawn_blocking` over a single connection behind a mutex, or use `tokio-rusqlite` which is this pattern packaged. One writer connection is correct for our load; do not build a pool.
- **Channels:** `tokio::sync::broadcast` for the event bus (many subscribers, lossy is fine for UI updates), `tokio::sync::mpsc` for command queues (one consumer, backpressure matters).
- **`Arc<Mutex<T>>` is fine.** Clone liberally. Do not contort the design to avoid an allocation — the bottleneck in this app is PTY I/O and SQLite, not memory traffic. Correctness first; optimize when you have a measurement.
- Every `tokio::spawn` must have a name in a `tracing` span and a documented shutdown path. Unkilled tasks holding PTY handles are how you leak processes, and on Windows a leaked child survives app exit and holds file locks.

### 4.5 Comments and docs

- `//!` module doc at the top of every slice `mod.rs`: what the slice owns, which tables, which commands. Four lines. This is the first thing anyone reads.
- `///` on every `pub` item. One sentence minimum.
- Inline comments explain **why**, never what. `// increment i` is noise; `// ConPTY re-sends the full screen on resize, so we discard the buffer first` is the reason someone doesn't break it next month.
- **Every Windows-specific workaround gets a comment naming the behavior it works around.** These look like bugs to anyone who hasn't hit them, and they will be "cleaned up" without one.

### 4.6 Abstraction discipline

**Rule of three.** Duplicate freely at two occurrences. Abstract at the third, once you can see what actually varies. Premature abstraction in a codebase this young costs more than the duplication ever would, and it's the most common way an agent-written codebase becomes unreadable — the agent sees two similar functions and immediately builds a trait.

Corollary: **no trait with a single implementor.** Traits are for real polymorphism or for test doubles at genuine I/O boundaries (the clock, the subprocess runner). Not for "good design."

---

## 5. Testing

Structured to match the slices:

- **Pure logic** → `#[cfg(test)] mod tests` inline in `logic.rs`. Should be the majority of tests, fast, no setup.
- **Store** → `tests.rs` in the slice, against a temp SQLite file with migrations applied. One helper, `test_db()`, in `platform/db.rs` under `#[cfg(test)]`.
- **Commands** → `tests.rs`, against a fake `AppState` with a stub PTY.
- **Integration** → `crates/dex-core/tests/`, real named pipe, real database, driving the CLI's request types.

Name tests as sentences: `digest_emits_nothing_when_delta_is_empty`. When it fails in CI six weeks from now you read the name and know what broke without opening the file.

**Four regression tests are mandatory** and should be written the moment their features land, per PRD §15: PTY survival across workspace switch, empty-delta producing zero bytes, subagent hook events leaving pane status unchanged, and out-of-order status events resolving to the newest. All fail silently in production. All are cheap to test.

---

## 6. Keeping an agent-built codebase consistent

The conventions above only matter if they're actually applied across dozens of sessions. Three mechanisms, in order of effectiveness:

### 6.1 `CLAUDE.md` at the repo root

Claude Code loads this automatically into every session. It should be short — a pointer, not a duplicate:

```markdown
# Dex

Windows terminal workspace multiplexer for multi-agent coding.

## Before writing any code
Read docs/conventions.md. It is normative: vertical slices over a platform layer,
strict dependency rules, table ownership, naming, error handling.
Read docs/prd.md for what to build (docs/prd-amendments.md explains why).
Read ARCHITECTURE.md for the current module map, measurements, and tested versions.

## Non-negotiable
- platform/ never imports features/
- Only a table's owning slice writes SQL against it
- No unsafe, no unwrap in dex-core, no utils.rs
- Every slice has mod/model/store/logic/commands/tests

## Before every commit
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

This is the actual enforcement mechanism. A convention an agent has to be reminded of manually is a convention that lasts three sessions.

### 6.2 A commit checklist

In `CONTRIBUTING.md`, applied to every change:

- [ ] fmt, clippy, tests pass
- [ ] New code is in the right tier — platform vs feature
- [ ] No new SQL outside the owning slice
- [ ] New `pub` items have doc comments
- [ ] New errors have a code and a repair string
- [ ] `ARCHITECTURE.md` updated if a module was added or moved
- [ ] No new file over 400 lines
- [ ] No new `Manager`/`Helper`/`Util`
- [ ] One component per `.tsx` file; its parts in `PartName.part.tsx`

### 6.3 Structural tests

Cheap and worth it: a test that asserts no file under `platform/` contains `use crate::features`, and a test that every slice directory contains `mod.rs` and `commands.rs`. Conventions that CI can check are conventions that survive.

---

## 7. What to do when a rule is wrong

Some of this will turn out to be wrong under contact with the code. The correct response is to **stop and say so**, with the specific case, rather than either grinding against a bad rule or quietly abandoning it. Update this document in the same commit that changes the practice. A conventions file that no longer describes the codebase is worse than none, because people trust it.

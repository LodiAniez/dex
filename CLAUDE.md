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
- Every slice has mod/model/store/logic/commands/tests (diagnostics has no store)

## Before every commit
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test

## Running
- Frontend deps: `npm install` in `app/`
- Dev app: `npm run tauri dev` in `app/`

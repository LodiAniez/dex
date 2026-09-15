# Contributing

Read `docs/conventions.md` first. Apply this checklist to every change:

- [ ] fmt, clippy, tests pass (`cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`)
- [ ] New code is in the right tier — platform vs feature
- [ ] No new SQL outside the owning slice
- [ ] New `pub` items have doc comments
- [ ] New errors have a code and a repair string
- [ ] `ARCHITECTURE.md` updated if a module was added or moved
- [ ] No new file over 400 lines
- [ ] No new `Manager`/`Helper`/`Util`

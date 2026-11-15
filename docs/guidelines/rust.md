# Rust standards, comments, modularity and design patterns

## Rust Standards & Memory Safety (required)

All code must follow Rust best practices (edition 2021, toolchain pinned in `rust-toolchain.toml`). Outdated, unsafe or leaking code is not acceptable.

- **Ownership:** owned values and borrows by default. Use `Arc` only when ownership is genuinely shared across threads, and `Rc` never on the audio path. Pass data by value or by reference; avoid `'static` leaks and `Box::leak`.
- **Unsafe:** allowed where necessary, anywhere in the repo, provided there are no memory leaks. The workspace `unsafe_code = "forbid"` lint stays on; each use is a per-module `#[allow(unsafe_code)]` exception. Current exception modules: `om-stretch/src/ffi.rs` (Rubber Band R3 behind a `Stretcher` trait; see openmixer-workbench/docs/spikes/DECISIONS.md); in `om-plugins` `abi.rs` and `loader.rs` (C ABI) and, with feature `clap`, `clap/entry.rs`, `clap/gui_parent.rs` and `clap/fds.rs` (ADR 0007); plus the plugin fixtures under `examples/plugins/` and `crates/om-plugins/tests/clap_leak.rs`. Every `unsafe` block has a `// SAFETY:` line stating the invariant, and any new FFI or load/teardown path needs a leak check (counting allocator or valgrind): `crates/om-plugins/tests/clap_leak.rs` and `tools/clap_leak_check.sh` (valgrind) guard the CLAP path.
- **Idioms:** `enum`s and `match` for state and commands, newtypes for ids and units, `Option`/`Result` instead of sentinel values, `?` for propagation, `#[must_use]` where a result must not be ignored, iterators over index loops where clearer. Prefer `&str`/`&[T]` parameters.
- **Errors:** `thiserror` for typed errors in library crates; the `openmixer` binary uses its own `AppError` (`anyhow` is not a dependency). Do not `unwrap()`/`expect()` on input from files, the network, MIDI or the user; tests may.
- **Logging:** `tracing` macros with structured fields. Never log from the audio thread.
- **Concurrency:** no shared mutable state between the audio thread and other threads except through the designated channels: `rtrb` SPSC rings and atomics (the `EngineSnapshot` is pulled from atomics). Document the thread that owns each type in its module doc.
- **Real-time safety (audio thread):** no allocation, locks (`Mutex`, `RwLock`), blocking channel calls, file or network I/O, or syscalls in the render path. Create buffers and graphs off the audio thread. Enforce this in tests with `assert_no_alloc` around the render function; a failing check is a bug, not a flaky test.
- **Hot loops:** in per-sample audio and analysis loops use `zip`/`iter` over pre-sliced buffers (or `as_chunks`) instead of indexing, so the bounds checks drop out (#226); check a change with the `divan` benches (`cargo bench -p om-engine`, `-p om-analysis`).
- **Lints:** `cargo clippy --all-targets -- -D warnings` must pass. Do not add `#[allow(...)]` to get past a lint without a comment stating why.
- **Licences and bans:** `cargo deny check` must pass (`deny.toml`). New dependencies must be AGPL-compatible.
- **Adopting crates:** well-established crates (cpal, symphonia, rusqlite, midir and similar) are adopted directly; reserve spikes for genuine unknowns.
- **Check current guidance:** when unsure about an API, check docs.rs for the crate version in `Cargo.lock` rather than reusing older patterns from memory.

## Comments & Function Size (required)

- **Comments are short.** One line, stating *why* (not what). No paragraph or block comments describing what code does.
- **Long explanation = refactor.** If a function needs a long comment to be understood, split it into smaller, well-named functions instead.
- **Rule of 10 (goal, not a requirement):** aim for functions of about 10 lines or fewer. Exceed it only when splitting would hurt clarity.
- **Doc comments (`///`)** are for public items only, one or two lines each.
- Prefer self-describing names over comments. Delete comments that restate the code.

## Modularity (required)

- **No monolithic files.** One focused responsibility per module; split anything that grows large or mixes concerns.
- **Crates per area.** Put reusable code in a Cargo library crate under `crates/om-<area>` and depend on it where needed, instead of duplicating code or piling it into the `openmixer` binary.
- **Keep dependencies one-way.** Arrows point toward `om-core`: `openmixer` -> `om-ui` -> `om-engine` / `om-library` / `om-analysis` / `om-midi` -> `om-dsp` / `om-audio` -> `om-core`. Library crates must not depend on the binary or on `om-ui`, and must be testable on their own.
- **Headless first.** Engine, analysis, library and MIDI crates must be fully testable without a window.

## Architecture & Design Patterns (required)

Before starting any large implementation (a new subsystem, a cross-cutting feature, or a new `crates/om-<area>`):

1. Read `docs/DESIGN_PATTERNS.md` and review how similar code is already structured.
2. Reuse an existing pattern wherever it fits rather than inventing a new one.
3. If a different pattern would improve long-term maintainability (code growth, new features, scaling, testability), pick it deliberately and note the trade-off in the PR.
4. Document every new pattern you introduce in `docs/DESIGN_PATTERNS.md` (use its entry template) in the same PR, so later agents can find and reuse it.

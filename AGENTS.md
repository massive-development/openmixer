# OpenMixer - Agent Instructions

Clean-room, original DJ software in Rust with GPUI (ADR 0001, 0004). Detail lives in `docs/`; read a linked doc only when you touch that area. Directory tree: [docs/project-structure.md](docs/project-structure.md).

## Crates

Dependencies point toward `om-core`: `openmixer` -> `om-ui` -> `om-engine`/`om-library`/`om-analysis`/`om-midi` -> `om-dsp`/`om-audio` -> `om-core`. The web window adds `openmixer` -> `om-ui-tauri` -> `om-ui-bridge` -> `om-ui`. Crates below `om-ui` never depend on it or on the binary.

- `om-core`: no I/O; `ParamId`, `Command`/`CommandSink`, `ParameterRouter`, `EngineSnapshot`, `WorkerPool`.
- `om-engine`: audio thread (decks, mixer, FX, sync, sampler, recorder, loudness).
- `om-audio`, `om-dsp`, `om-stretch`: device layer, DSP, key-lock (FFI `unsafe` only in `om-stretch/src/ffi.rs`).
- `om-analysis`, `om-library`, `om-midi`, `om-plugins`, `om-stems`, `om-automix`: control-side services.
- `om-ui` (no window), `om-ui-gpui` (GPUI), `openmixer` (binary, composition root).
- `om-ui-bridge` (headless wire formats for a web front end), `om-ui-tauri` (Tauri v2 window and Svelte front end in `web/`): ADR 0009 (accepted; GPUI stays the default window until the cut-over), `gui-tauri` feature, off by default.

Per-crate detail: [docs/crates.md](docs/crates.md). Features: [sampler](docs/features/sampler.md), [stems](docs/features/stems.md), [recording](docs/features/recording.md), [automix](docs/features/automix.md), [mapping editor](docs/features/mapping-editor.md), [web front end](docs/features/tauri-trial.md). Control paths, adding a control id, demo flags: [docs/guidelines/control-ids.md](docs/guidelines/control-ids.md).

## Hard rules

- **PRs only.** Never push or merge to `main`. Branch from `main`, `gh pr create --base main`, merge commits (no squash); the description has Summary, Changes, Testing and Follow-ups, and states whether it touches an extension point. Fix stale docs (`README.md` too) in the same PR. Workflow changes need a local `act` run. [Details](docs/guidelines/pull-requests.md).
- **Audio thread:** no allocation, locks, blocking calls, I/O or logging; no string ids parsed there. Writers never write engine state directly; readers pull an `EngineSnapshot`. Guard with `assert_no_alloc` tests.
- **Plugin targets:** `set_parameter` must never block; hand work to a thread and return `true`.
- **Rust:** `unsafe` only in the listed per-module exceptions with `// SAFETY:` and a leak check; no `unwrap`/`expect` on file, MIDI or user input; `thiserror` in libraries, `AppError` in the binary; `tracing` for logs. [Full standards](docs/guidelines/rust.md).
- **Style of code:** one-line comments that say why; small functions (about 10 lines); one responsibility per module; engine, analysis, library and MIDI crates testable headless.
- **Large new subsystem:** read and update `docs/DESIGN_PATTERNS.md` first.
- **UI:** colours, sizes and fonts come from theme tokens, never literals; numbers use `font::numeral`. A UI change needs a screenshot you have looked at, judged as a DJ would. [Styling](docs/styling.md), [experience bar](docs/guidelines/experience-bar.md).
- **Web front end (Tauri):** styles only from `--om-*` variables; control ids, ranges and defaults come from Rust (`controls`), never typed in TypeScript; rebuild `web/dist` before a cargo build that should show front-end changes; mock device only. [Details](docs/features/tauri-trial.md).
- **Extensibility:** do not hardcode what a plugin could replace; no signing or allow-lists on plugins. [Details](docs/guidelines/extensibility.md).
- **Demo flags:** a new `--demo-*` flag goes in `openmixer::flags::DEMO_FLAGS` and never writes the user's files.
- **Never play audio;** use the mock device. Screenshots only with `import -silent` into temp dirs, at most 6 per task.
- **Knowledge first, read narrowly:** check `graphify-out/`, `openmixer-workbench/docs/PROGRESS.md` and `docs/DESIGN_PATTERNS.md` before searching; read line ranges, not whole files. [Details](docs/guidelines/knowledge-reuse.md).

## Build and test

Linux needs `libasound2-dev pkg-config libclang-dev libdbus-1-dev`, and `libwebkit2gtk-4.1-dev` because `om-ui-tauri` is a workspace member. Everything else (soak, benches, optional features, sccache, test helpers in `om-testing`): [docs/guidelines/build.md](docs/guidelines/build.md).

```bash
cargo check -p <crate>                      # while working
nice -n 19 env CARGO_BUILD_JOBS=3 cargo test -p <crate>
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo deny check   # once per PR
cargo run -p openmixer -- --smoke
```

- **WSL:** one cargo process at a time, never in the background, never clippy and test in one command, `flock /tmp/openmixer-cargo.lock` when sharing a checkout. Check `free -m`; skip the gui crate under 6 GB free. GUI runs need `WAYLAND_DISPLAY=`. More in `build.md`.
- **Before a PR:** `cargo test --workspace`, fmt, clippy and deny clean. Build problems: [docs/troubleshooting.md](docs/troubleshooting.md).

## Decisions and references

ADR summaries: [docs/adr/SUMMARY.md](docs/adr/SUMMARY.md). Product bar: `docs/PRODUCT.md`. Crate and doc links: [docs/references.md](docs/references.md). Crates adopted in the platform refresh: [docs/dependencies.md](docs/dependencies.md).

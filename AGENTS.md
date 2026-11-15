# OpenMixer - Agent Instructions

## Overview

OpenMixer is a clean-room, legally original implementation of professional DJ software, written in Rust with GPUI for the UI. This document provides instructions for any AI agent to understand the codebase and continue development.

## Project Structure

```
openmixer/
├── .github/workflows/        # rust.yml (the Rust CI, every PR; weekly soak) and issue-heatmap.yml
├── crates/                   # Cargo workspace members (om-core ... om-stems, om-plugins, om-ui, openmixer)
│   └── om-ui-gpui/           # GPUI window (the openmixer `gui` feature, on by default)
├── docs/
│   ├── adr/                  # Architecture Decision Records
│   ├── controllers/          # Controller map notes
│   ├── legal/                # Legal documentation and the asset provenance record
│   ├── screenshots/          # Staged window screenshots and REVIEW.md
│   └── testing/              # Hardware checklist (after the platform refresh)
├── examples/plugins/         # Example plugins: gain-plugin, gain-plugin-cdylib, param-plugin-cdylib, clap-gain-fixture
├── fixtures/                 # Small test inputs: controller maps and traces, library exports
├── resources/                # Built-in theme JSON, generic controller maps, bundled fonts
├── tools/                    # issue_heatmap.py, screenshots.sh, export_htdemucs.py, htdemucs_reference.py, clap_leak_check.sh
├── Cargo.toml                # Workspace manifest
├── Cargo.lock
├── rust-toolchain.toml       # Pinned toolchain
├── rustfmt.toml
├── clippy.toml
└── deny.toml                 # Licence and ban policy (cargo deny), including what the GPUI tree needs
```

## Architecture and Crate Responsibilities

Dependencies point toward `om-core` (see "Modularity" above). The README architecture section has the directory tree; this is what each crate owns.

- `om-core`: no I/O; `ParamId`, `Command`/`CommandSink`, `ParameterRouter`, `EngineSnapshot`, `WorkerPool`.
- `om-engine`: audio thread (decks, mixer, FX, sync, sampler, recorder, loudness); implements `ParamTarget`.
- `om-audio`, `om-dsp`, `om-stretch`: device layer (cpal), DSP, key-lock (FFI `unsafe` only in `om-stretch/src/ffi.rs`).
- `om-analysis`, `om-library`, `om-midi`, `om-plugins`: control-side services with typed errors.
- `om-stems`, `om-automix`: see [docs/features/stems.md](docs/features/stems.md) and [docs/features/automix.md](docs/features/automix.md).
- `om-ui` (no window) and `om-ui-gpui` (GPUI drawing, the `gui` feature): view-models and rendering.
- `openmixer`: binary and composition root (`Services`, `AppComposition`, `AppError`).

Full per-crate detail: [docs/crates.md](docs/crates.md).

Control paths, and what is live today:
- **UI (production window):** `DeckBinding::typed` and `MixerBinding::typed` send `Command::Param { id, value }` through the router's `CommandSink`, with no string formatting or parsing. The binding constructors without a sink (`new`) write through the string façade; the widget test rig uses them.
- **MIDI mappings and plugins:** the string façade, `ParameterTarget::set_parameter(&str, f64)` on the router. It parses the id once there and dispatches typed ids the same way.
- **Typed entry point:** `Command` and `CommandSink` are used by tests, by automix and by the Wave 7 features. The router's sink accepts `Command::Param` and `Command::Automix` (routed to the `automix` target). `LoadStems` and `SamplerAssign` are handled by `DjEngine::command_sink(fallback)` (`EngineCommands`): `DjEngine::register_stem_set(dir)` turns a stem directory into the `StemSetRef` handle the command carries, `DjEngine::register_sample(Arc<Sample>)` / `load_sample(path)` the `SampleId` for `SamplerAssign`; the router's sink still rejects both.

Feature notes (read only when you touch that area):
- Sampler: [docs/features/sampler.md](docs/features/sampler.md)
- Stem mode in the engine: [docs/features/stems.md](docs/features/stems.md)
- Mix recording and loudness meter: [docs/features/recording.md](docs/features/recording.md)
- Automix: [docs/features/automix.md](docs/features/automix.md)
- Mapping editor: [docs/features/mapping-editor.md](docs/features/mapping-editor.md)
- Crates added in the platform refresh: [docs/dependencies.md](docs/dependencies.md)

Rules: writers never write engine state directly; readers pull an `EngineSnapshot`; the audio thread takes no locks and never parses an id.

- Demo flags (`--demo-map`, `--demo-library`, `--demo-automix`, `--demo-record`) are listed in `openmixer::flags::DEMO_FLAGS`. A demo run never writes the user's files: `composition::prefs_save_target` sends settings saves to `<scratch>/preferences.json` when `flags::demo_run` is true, and the recording and map paths also live in the scratch folder. Any new `--demo-*` flag must be added to `DEMO_FLAGS`, and its state must stay in memory or in the scratch folder. Covered by `openmixer/tests/demo_prefs.rs`.

### Plugin control targets

- A plugin registers its target with the router (`register_target`, a string prefix). The router holds its reentrant gate across every target call, and registration and unregistration wait on the same gate. So a slow or blocking `set_parameter` stalls MIDI and UI dispatch for everything else. Do not block, wait on I/O or take a long lock in `set_parameter`; hand the work to a thread or queue and return `true` at once.

### How to add a control id

1. `om-core/src/param_id/mod.rs`: add the variant to the owning enum (`DeckParam`, `MixerParam`, ...) and its line in the grammar doc comment.
2. `om-core/src/param_id/parse.rs`: parse it in the sub-namespace `FromStr`. `Display` in `mod.rs` must print it back exactly.
3. `om-core/src/param_id/tests.rs`: add the id to `GOLDEN`. Every id must round-trip canonically.
4. The owning target's `set_param` match arm (for example `om-engine/src/mixer/control.rs` or `deck_control.rs`): handle the variant and return `true`. Return `false` for variants it does not own.
5. If the value is readable: add a `StateId` variant (`om-core/src/state_id.rs`), a field on `DeckSnapshot` or `MixerSnapshot` with its read arm (`om-core/src/snapshot.rs`), and fill the field in `DjEngine::snapshot()` (`om-engine/src/dj_engine.rs`).
6. If a UI control or MIDI target uses it: build the id from `ParamId` in `om-ui/src/binding.rs`, and list it in `learnable_targets` (`om-ui/src/settings/midi.rs`) when MIDI learn should reach it.
7. Test with `nice -n 19 env CARGO_BUILD_JOBS=3 cargo test -p om-core -p om-engine`, and add a binding test in `om-ui` when step 6 applies.


## Architecture Decisions

Full summaries: [docs/adr/SUMMARY.md](docs/adr/SUMMARY.md); records in `docs/adr/`.

- 0001 clean-room: features are free, expression (names, logos, artwork) is not; no Mixxx code.
- 0003 pure AGPLv3, no CLA, DCO sign-off.
- 0004 Rust with GPUI (`gpui-pre` imported as `gpui`, `gpui-kit`, `gpui-component`).
- 0005/0008: `docs/PRODUCT.md` is the experience bar; the Machined Faceplate look is in `openmixer-workbench/docs/design-system/DESIGN.md`.

## Pull Request Workflow (required)

All changes — including those by AI agents and maintainers — land through a pull request. Never merge a branch directly or push to `main`.

1. Work on a feature branch (`wave2-mixer`, `docs/...`, `fix/...`) cut from `main`.
2. Open a PR into `main` with `gh pr create --base main`. `rust.yml` (the Rust CI: fmt, clippy, test on Linux, `cargo deny`, the `onnx` job, the ASIO SDK guard) runs on every pull request. The description must include:
   - **Summary:** what changed and why (link the issue).
   - **Changes:** key crates/modules added or modified.
   - **Testing:** commands run and results (`cargo test --workspace`, manual checks).
   - **Follow-ups / known gaps:** anything left undone.
3. Docs match the repo before the PR opens: check `README.md` (status, screenshots, build notes), and any doc the change touches against the current code, and fix stale claims in the same PR. Outdated docs send people to rebuild or fix what already exists or is in flight.
4. Merge the PR (`gh pr merge --merge`) once the build and tests pass, so history stays traceable to a PR.
5. Stack dependent work: when a PR builds on or touches the same files as an open PR, branch from that PR's branch and set `--base <that-branch>`. Keep the stack current by merging the parent branch into the child (merge commits, never force-push), so merging in order needs no rebase. After the parent merges, retarget the child with `gh pr edit --base main`. Use plain merge commits, not squash, so stacked history stays intact.

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

## Styling a control (LookAndFeel)

Colours, radii, borders, insets and font sizes come from theme tokens, never literals in view code. Get recipes from `om_ui::style::LookAndFeel::from_tokens`, draw with `paint::face`/`paint::skin`, set text with `type_role` and numbers with `font::numeral` (always mono). `style_lint.rs` and `type_lint.rs` fail on raw colours, font families and sub-11 px text. A deck hue only on something that names that deck. Full recipe and the token-adding steps: [docs/styling.md](docs/styling.md).

## Experience bar (required for anything a DJ sees)

`docs/PRODUCT.md` is the bar and ADR 0005 the decision. Interface work is not a wave and is not finished when the logic tests pass.

- **Look at it.** A PR that changes what the window shows includes a screenshot from `tools/screenshots.sh`, and you have looked at it (not just that it rendered). Say what you saw and what is still wrong.
- **Use real music** for anything involving waveforms, the library, analysis or colour. The demo tracks are synthetic sweeps and hide most defects; `cargo run -p om-ui --example wave_strips -- TRACK...` renders a real track's waveform strips to PNG.
- **Judge it as a DJ would.** At the default window size and the minimum (`MIN_WINDOW`), can a DJ read the playing deck's BPM, key, time and phrase position at a glance, and reach EQ, transport and pads without looking? If not, say so in the PR.
- **Layout and look may change.** Replace a layout or control when a better one is found. Keep the tests that guard function (no overlap, no clipping, the minimum window); update the ones that only recorded the old arrangement.
- **Tokens, not literals** still applies (see "Styling a control"); if the look you need has no token, add one.
- **Clean-room still applies.** Borrow the reason a layout works, not another product's names, icons, artwork or pixel layout.

## Token Efficiency & Knowledge Reuse (required)

Consult existing knowledge before computing or exploring. As the codebase grows, keep context small.

1. **Knowledge first:** check, in order, before searching or reading code: `graphify-out/` (query the graph first, e.g. `/graphify query`), any available knowledge base (e.g. the `lumen-kb` MCP `search_kb`), then `openmixer-workbench/docs/PROGRESS.md`, `docs/DESIGN_PATTERNS.md` and `openmixer-workbench/TODO.md`.
2. **Read narrowly:** read only the needed line ranges or symbols, not whole files; prefer grep/glob to locate code. Do not re-read files already in context.
3. **Delegate broad searches** to a search subagent so only conclusions return to the main context.
4. **Keep the graph fresh:** after structural changes (new crates, moved files) re-run graphify locally (`graphify-out/` is git-ignored). Document durable findings in `docs/` rather than rediscovering them.
5. **Don't recompute:** reuse existing results (test output, benchmarks, analysis) instead of re-running or re-deriving them.
6. **Keep outputs lean:** trim command output (`| tail`, `-q`), and keep PR descriptions and reports concise.

## Extensibility & User Freedom (required)

Users must always be able to modify OpenMixer with external plugins. Nothing may be locked down.

- **Extension points by default:** new features expose hooks for external plugins — audio pre/post-processing (per deck, channel and master), effects, analysis, library import/export, controller mappings, and UI/theming (colours, layouts, skins).
- **No hardcoded restrictions:** don't hardcode behaviour, limits, themes or signal paths that a plugin could reasonably replace or extend. Prefer registries, traits and data-driven configuration (serde-loaded files) over fixed code paths.
- **Stable plugin API:** keep extension traits small, documented and versioned; don't break them silently. Plugins must not need to modify core source.
- **No gatekeeping:** don't add signing, allow-lists or sandbox rules that stop users loading their own plugins (opt-in safety warnings are fine). Respect AGPLv3 obligations for plugins linked into the app.
- **Plugins load in-process, from a C-ABI shared library, or (feature `clap`) as CLAP plugins.** `om-plugins` has the plugin API, registry and loaders (see `openmixer-workbench/docs/spikes/DECISIONS.md` and ADR 0007). Keep new extension points as traits and registries in the owning crate, so they can be exposed later without restructuring. A plugin's own state (`Plugin::save_state` / `restore_state`, opaque bytes) is kept by the registry and stored by the app in `plugin-state.json` beside the preferences (`openmixer::plugin_state`, versioned; ADR 0007 step 6).
- **PR check:** every PR must state whether it adds, changes or could block an extension point.

## CI Workflow Changes (required)

Any change to `.github/workflows/` must be tested locally with [`act`](https://github.com/nektos/act) before opening the PR.

1. Run the affected job(s): `act -j <job> [-W .github/workflows/<file>.yml]` (add `--container-architecture linux/amd64` if needed). List jobs with `act -l`.
2. Fix failures locally; don't push to find out.
3. Put the `act` command and result in the PR's Testing section. Note anything `act` can't cover (e.g. macOS/Windows runners, secrets).

## Build Instructions

### Dependencies

- **Rust toolchain:** install with `rustup`. The version is pinned in `rust-toolchain.toml` and selected automatically.
- **Linux audio:** `cpal` needs ALSA development headers and `pkg-config`:

```bash
sudo apt-get install libasound2-dev pkg-config libclang-dev libdbus-1-dev
```

`libclang-dev` is for bindgen (Rubber Band). The GPUI window needs the packages listed in the `check` job of `.github/workflows/rust.yml`. `libdbus-1-dev` is for the default `realtime-dbus` feature; `--no-default-features` builds without it.

- **Cargo tools (for the lint and licence gates):** `cargo-deny` (CI installs it with `taiki-e/install-action`).
- GPUI platform dependencies (Linux) are documented in `openmixer-workbench/docs/spikes/gpui.md`.
- Crates added in the platform refresh: see [docs/dependencies.md](docs/dependencies.md).

### Build Commands

```bash
# Build the whole workspace
cargo build --workspace

# Release build
cargo build --release --workspace

# Run tests
cargo test --workspace

# Engine soak: 30 s paced mix (in cargo test --workspace); 10-minute gate: --ignored, 10 min wall time
cargo test -p om-audio --test soak
cargo test -p om-audio --test soak -- --ignored --nocapture

# Format, lint and licence gates (run all before opening a PR)
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo deny check

# Startup smoke check
cargo run -p openmixer -- --smoke

# Benchmarks (divan)
cargo bench -p om-engine
cargo bench -p om-analysis

# Optional features: CLAP hosting, real stem separation, ffmpeg decode fallback
cargo build -p openmixer --features clap
cargo test -p om-stems --features onnx
cargo test -p om-engine -p om-analysis --features om-engine/ffmpeg,om-analysis/ffmpeg
```

Sanitizer and Miri runs for concurrent code are planned; the current real-time check is `assert_no_alloc` in tests.

### Build setup and cost rule

- **sccache (Linux, optional):** to share compiled dependencies between worktrees, install `sccache` and put this in `~/.cargo/config.toml`:

```toml
[build]
rustc-wrapper = "sccache"
incremental = false
```

- **Cost rule for agents:** while working use `cargo check -p <crate>` and the touched crates' tests with `-j 4`. Run full clippy, workspace tests and `cargo deny check` once per PR, at the end. At most two parallel agents on the 12-core shared host.

### Working on a constrained machine (WSL)

- WSL crashes under heavy builds. Run `nice -n 19 env CARGO_BUILD_JOBS=3 cargo ...`, one cargo process at a time, never in the background, and never clippy and test in one command. Agents sharing a checkout serialise with `flock /tmp/openmixer-cargo.lock`.
- Agents on separate worktrees that share a `CARGO_TARGET_DIR` can see each other's output as fresh: touch the files you changed before testing, and check the new tests actually ran.
- Check `free -m` first. Do not build the gui crate unless more than 6 GB is free.
- GUI build (`openmixer-workbench/docs/spikes/gpui.md`): run with `WAYLAND_DISPLAY=` to force X11. For the link error `unable to find library -lxkbcommon-x11`, symlink `libxkbcommon-x11.so.0` into a shim directory and pass `RUSTFLAGS="-L <shim dir>"`; the proper fix is `sudo apt install libxkbcommon-x11-dev`. The gui crate tests need this too.
- `soak_short` (`crates/om-audio/tests/soak.rs`) can flake once under load; rerun it alone.
- Git corruption recovery: delete zero-byte files under `.git/objects`, run `git fetch --refetch origin`, then reset the branch to `origin/<branch>`.
- Never play audio; use the mock device. Screenshots only with `import -silent` (see `tools/screenshots.sh`), into temp dirs, at most 6 per task.
- Agents default to haiku; use sonnet for audio-thread or design-heavy work. Review once per phase. Push after every step.

### Test support

`crates/om-testing` holds the helpers the crates share in their tests: `wav` (fixture writers and a reader), `signals` (deterministic sines, noise and impulse trains), `level` (dB, peak and RMS), `wait` (`wait_until` and `assert_eventually!`) and `fs::TempDir`. Reuse these instead of writing a new WAV writer, sine or poll loop. Waits poll for the condition they expect, with a timeout; never sleep a fixed time to wait for something to happen. It is a dev-dependency only: no shipped crate may depend on it.


## Testing

- TDD for engine and analysis logic
- Golden-vector fixtures (kept in the crates' test modules and `fixtures/`)
- Property tests (`proptest`) for parsers and search
- Metric tests for DSP accuracy; `divan` benchmarks (`cargo bench -p om-engine`, `-p om-analysis`) for the audio-thread and analysis hot loops
- Real-time tests: render the engine inside `assert_no_alloc`
- Screenshot tests for UI (planned)

## Common Issues

Missing `alsa.pc`, `libclang`, `libdbus-1`, cargo-deny licences, GPUI on WSL: [docs/troubleshooting.md](docs/troubleshooting.md).

## Verification

After each wave:
1. All tests pass (`cargo test --workspace`)
2. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo deny check` are clean
3. Real-time allocation checks pass (`assert_no_alloc` in tests); sanitizer runs planned for concurrent code
4. Evidence file updated: `.omo/evidence/task-<N>-openmixer-dj-app.*`

## References

- Design (historical): `openmixer-workbench/docs/superpowers/specs/2026-10-18-rust-rewrite-design.md`
- ADR 0004 (Rust with GPUI): `docs/adr/0004-stack-rust-gpui.md`
- cpal: https://docs.rs/cpal
- rtrb: https://docs.rs/rtrb
- rubato: https://docs.rs/rubato
- ebur128: https://docs.rs/ebur128
- midir: https://docs.rs/midir
- rusqlite: https://docs.rs/rusqlite
- Rubber Band: https://breakfastquay.com/rubberband/
- Clean-Room Doctrine: docs/adr/0001-clean-room-doctrine.md

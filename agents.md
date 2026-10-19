# OpenMixer - Agent Instructions

## Overview

OpenMixer is a clean-room, legally original implementation of professional DJ software, written in Rust with GPUI for the UI. This document provides instructions for any AI agent to understand the codebase and continue development.

The project is being rewritten from the C++ tree. The frozen baseline is the git tag `cpp-final` (commit c6d2747). It is the behavioural reference until Phase 5 of the rewrite passes; it is deleted from `main` only after that. The design is `docs/superpowers/specs/2026-10-18-rust-rewrite-design.md`, and the toolkit decision is `docs/adr/0004-stack-rust-gpui.md`.

## Project Structure

```
/home/brandon/projects/openmixer/
├── .github/workflows/        # rust.yml = Rust CI (every PR); pr-tests.yml and ci.yml = legacy C++ workflows, kept until Phase 5
├── crates/                   # Cargo workspace members
│   ├── om-core/              # currently exposes only VERSION; parameter/target traits, state snapshots, command enum planned (no I/O)
│   ├── om-dsp/               # empty scaffold; filters, EQ, FX primitives, resampling helpers planned
│   ├── om-engine/            # empty scaffold; decks, channel strips, crossfader, FX routing, sync, loops planned
│   ├── om-audio/             # empty scaffold; cpal device layer, latency monitor planned
│   └── openmixer/            # binary: currently supports --smoke and --version; composition root and preferences planned
├── docs/
│   ├── adr/                  # Architecture Decision Records
│   ├── legal/                # Legal documentation
│   └── superpowers/specs/    # Design specs (rust-rewrite-design.md)
├── tools/                    # Development tools (Python scripts, soak and UI gate harnesses)
├── tests/                    # Integration tests and fixtures
├── Cargo.toml                # Workspace manifest
├── Cargo.lock
├── rust-toolchain.toml       # Pinned toolchain
├── rustfmt.toml
├── clippy.toml
└── deny.toml                 # Licence and ban policy (cargo deny)
```

Planned crates, added in their phases: `om-stretch` (Phase 2, the only crate allowed `unsafe`), `om-analysis`, `om-library`, `om-midi`, `om-stems`, `om-plugins`, `om-ui`.

## Current State (Rust rewrite, Phase 1 spikes run)

Phase 0 complete; Phase 1 spikes run — Linux-first; GPUI gate closed on Linux, stretcher decided (Rubber Band R3 behind a Stretcher trait); Windows/macOS deferred to a porting phase.

**Implemented:**
- Repository initialization and governance documents (LICENSE, ADRs, CONTRIBUTING)
- Cargo workspace with the `om-core`, `om-dsp`, `om-engine`, `om-audio` crates (`om-core` exposes only `VERSION`; the other three are empty scaffolds) and the `openmixer` binary (`--smoke`, `--version`)
- Rust CI matrix (`rust.yml`): fmt, clippy, test on Linux, macOS and Windows; `cargo deny` on Linux (all three OSes stay in CI so portability does not rot; Linux is the only runtime target until the porting phase)
- Frozen C++ baseline at tag `cpp-final`, the behavioural reference for parity

**In progress / planned:**
- Phase 1 decision record: `docs/spikes/DECISIONS.md`; `docs/parity.md` is already generated from `cpp-final`
- Linux first (see `docs/spikes/DECISIONS.md`, Rescope): the GPUI gate is closed on Linux; the time-stretch decision is made (Rubber Band R3 via FFI behind a `Stretcher` trait). Open for Phase 2 on Linux: native-Linux cpal device runs and the headphone-cue second stream. Porting phase: Windows and macOS GPUI, WASAPI and CoreAudio checks.

## Architecture Decisions

### ADR-0001: Clean-Room Doctrine
- **Features** are free (methods of operation)
- **Expression** is not (names, logos, artwork)
- No Mixxx code reuse (clean-room)
- All assets must be original

### ADR-0002: Technology Stack (superseded)
- Superseded by ADR-0004. The C++ application framework and its dependencies are recorded there for history only.

### ADR-0003: Licensing
- **Pure AGPLv3** (no Contributor CLA)
- Simple contribution process
- DCO sign-off required

### ADR-0004: Technology Stack - Rust with GPUI
- **Language / UI:** Rust, with GPUI for the UI (gate closed on Linux)
- **Audio I/O:** `cpal` for the engine output callback; `rodio` only for non-critical audio such as library preview
- **Decode / tags:** `symphonia`; `lofty`
- **DSP:** `realfft` / `rustfft`; own biquad, EQ and FX in `om-dsp`
- **Time-stretch:** decided: Rubber Band R3 via FFI in `om-stretch` (the one `unsafe` crate), behind a `Stretcher` trait; no pure-Rust candidate passed the stereo CPU bar (see `docs/spikes/DECISIONS.md`)
- **MIDI:** `midir`; **Library:** `rusqlite` (bundled SQLite)
- **RT plumbing:** `rtrb` (SPSC), `arc-swap` for snapshots, `assert_no_alloc` in tests

## Pull Request Workflow (required)

All changes — including those by AI agents and maintainers — land through a pull request. Never merge a branch directly or push to `main` or `stage`.

1. Work on a feature branch (`wave2-mixer`, `docs/...`, `fix/...`) cut from `stage`.
2. Open a PR into `stage` with `gh pr create --base stage`. `rust.yml` (the Rust CI: fmt, clippy, test on Linux, macOS and Windows, `cargo deny`) runs on every pull request. `pr-tests.yml` and `ci.yml` are the legacy C++ workflows, kept until Phase 5; `pr-tests.yml` still runs on PRs into `stage`, but it is not the Rust gate. The description must include:
   - **Summary:** what changed and why (link the Todo number from `TODO.md`).
   - **Changes:** key crates/modules added or modified.
   - **Testing:** commands run and results (`cargo test --workspace`, manual checks).
   - **Follow-ups / known gaps:** anything left undone.
3. Update `TODO.md`, `docs/progress.json` (then run `python3 tools/progress_heatmap.py`) and `docs/PROGRESS.md` in the same PR when task status changes.
4. Merge the PR (`gh pr merge --merge`) once the build and tests pass, so history stays traceable to a PR.
5. Release: batch `stage` into `main` with a PR (`--base main`); the legacy full C++ matrix (`ci.yml`, kept until Phase 5) runs only there, and `rust.yml` runs as on every PR. Pushes alone never trigger CI.
6. Stack dependent work: when a PR builds on or touches the same files as an open PR, branch from that PR's branch and set `--base <that-branch>`. Keep the stack current by merging the parent branch into the child (merge commits, never force-push), so merging in order needs no rebase. After the parent merges, retarget the child with `gh pr edit --base stage`. Use plain merge commits, not squash, so stacked history stays intact.
7. Promote early: open a `stage` -> `main` PR when a Todo or feature group finishes, or once `stage` is about 3 feature PRs or ~2,000 changed lines ahead of `main`, whichever comes first. Small release PRs stay readable and revertable.
8. Slice large backlogs: if `stage` is further ahead than that, do not promote it in one PR. Cut `release/rN` branches at `stage` first-parent merge commits (each already passed CI), open them as a stack (`release/r1` -> `main`, `release/r2` -> `release/r1`, ...), merge in order and retarget the next one to `main` so the full matrix runs on it.
9. Release PR description: list the included PRs (number + Todo), the testing they already had, and the revert command. Merge with `--merge` so a batch can be undone with `git revert -m 1 <merge commit>`.

## Rust Standards & Memory Safety (required)

All code must follow Rust best practices (edition 2021, toolchain pinned in `rust-toolchain.toml`). Outdated, unsafe or leaking code is not acceptable.

- **Ownership:** owned values and borrows by default. Use `Arc` only when ownership is genuinely shared across threads, and `Rc` never on the audio path. Pass data by value or by reference; avoid `'static` leaks and `Box::leak`.
- **Unsafe:** every crate has `#![forbid(unsafe_code)]` (or the workspace `unsafe_code = "forbid"` lint) except the dedicated FFI crate `om-stretch` (planned for Phase 2: Rubber Band R3 behind a `Stretcher` trait; see docs/spikes/DECISIONS.md). Every `unsafe` block there has a `// SAFETY:` line stating the invariant.
- **Idioms:** `enum`s and `match` for state and commands, newtypes for ids and units, `Option`/`Result` instead of sentinel values, `?` for propagation, `#[must_use]` where a result must not be ignored, iterators over index loops where clearer. Prefer `&str`/`&[T]` parameters.
- **Errors:** `thiserror` for typed errors in library crates, `anyhow` only in the `openmixer` binary. Do not `unwrap()`/`expect()` on input from files, the network, MIDI or the user; tests may.
- **Logging:** `tracing` macros with structured fields. Never log from the audio thread.
- **Concurrency:** no shared mutable state between the audio thread and other threads except through the designated channels: `rtrb` SPSC rings, `arc-swap` snapshots and atomics. Document the thread that owns each type in its module doc.
- **Real-time safety (audio thread):** no allocation, locks (`Mutex`, `RwLock`), blocking channel calls, file or network I/O, or syscalls in the render path. Create buffers and graphs off the audio thread. Enforce this in tests with `assert_no_alloc` around the render function; a failing check is a bug, not a flaky test.
- **Lints:** `cargo clippy --all-targets -- -D warnings` must pass. Do not add `#[allow(...)]` to get past a lint without a comment stating why.
- **Licences and bans:** `cargo deny check` must pass (`deny.toml`). New dependencies must be AGPL-compatible.
- **Adopting crates:** well-established crates (cpal, symphonia, rusqlite, midir and similar) are adopted directly; reserve spikes for genuine unknowns.
- **Check current guidance:** when unsure about an API, check docs.rs for the crate version in `Cargo.lock` rather than reusing older patterns from memory.
- **Skills:** the Rust skills in `.agents/skills/` (`rust-best-practices`, `rust-patterns`, `rust-async-patterns`) are the reference for idioms and async code. The directory is planned for this rewrite; check it exists before relying on it. Async is not the default: prefer threads and channels for the audio and analysis paths.

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

## Token Efficiency & Knowledge Reuse (required)

Consult existing knowledge before computing or exploring. As the codebase grows, keep context small.

1. **Knowledge first:** check, in order, before searching or reading code: `graphify-out/` (query the graph first, e.g. `/graphify query`), any available knowledge base (e.g. the `lumen-kb` MCP `search_kb`), then `docs/PROGRESS.md`, `docs/DESIGN_PATTERNS.md` and `TODO.md`.
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
- **Plugins are deferred.** The design defers the plugin host (CLAP-first if and when it is needed). Until its wave, keep new extension points as traits and registries in the owning crate, so they can be exposed later without restructuring.
- **PR check:** every PR must state whether it adds, changes or could block an extension point.

## Progress Files (required)

Every PR that completes or advances a task must update, in the same PR:
- `TODO.md` (tick items), `docs/progress.json` (status), `docs/PROGRESS.md` (done/missing detail)
- the heatmap: `python3 tools/progress_heatmap.py` (CI's `progress-check` job fails if `docs/progress-heatmap.svg` is stale)
- `docs/parity.md` is generated from the `cpp-final` tag by `python3 -I tools/parity_inventory.py`. Do not edit it by hand; fill in the Rust test columns through the PR that adds the test.

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
sudo apt-get install libasound2-dev pkg-config
```

- **Cargo tools (for the lint and licence gates):** `cargo-deny` (CI installs it with `taiki-e/install-action`).
- GPUI platform dependencies (Linux) are documented in `docs/spikes/gpui.md`.

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
```

Sanitizer and Miri runs for concurrent code are planned; the current real-time check is `assert_no_alloc` in tests.

## Implementation Waves

The design maps these waves onto Phases 0–6 (see `docs/superpowers/specs/2026-10-18-rust-rewrite-design.md`). Wave 1 is the C++ baseline; the others are rebuilt in Rust.

### Wave 1: Foundation ✅ COMPLETE (C++ baseline, `cpp-final`)
- Repository setup
- Legal framework
- Build system and CI (replaced by the Cargo workspace and `rust.yml`)
- Test harness

### Wave 2: Audio Engine (Rust, Phases 1–2)
- Audio device layer over `cpal` (with a mock device for CI)
- Deck streaming decoder (`symphonia`)
- Time-stretch / keylock (Rubber Band R3 via `om-stretch`, behind a `Stretcher` trait)
- Mixer (EQ/filter/crossfader)
- FX rack (8 effects)
- Beat sync + quantized cue/loop/slip
- **Gate:** 10-minute headless engine soak (zero xruns, no audio-thread allocation)

### Wave 3: Analysis Pipeline
- Analysis service skeleton
- BPM + beat-grid detection
- Musical key detection
- Waveform data model
- Onset/energy/structure features
- **Stems precompute** (Demucs ONNX via `ort`)

### Wave 4: Library & Data
- Library schema + DAO layer (`rusqlite`)
- Metadata ingest + watch folders
- Import (iTunes/Rekordbox XML)
- Search + filter engine
- Track prep queue

### Wave 5: Application UI (GPUI)
- Design tokens and theme
- Main window layout
- Deck widgets
- Mixer widgets
- Library browser
- Automix panel
- Settings screens
- **Gate:** scripted UI-driven 10-minute mix (`tools/ui_gate`) plus `openmixer --smoke`; UI screenshot suite and keyboard-only testing (planned)

### Wave 6: Hardware I/O
- MIDI mapping engine (`midir`)
- Controller trace harness
- Plug-and-play maps (4 controllers)
- HID layer (later)
- Mapping editor UI

### Wave 7: Intelligence
- Stem-aware mixing DSP
- Automix transition engine
- 8-pad sampler

### Wave 8: Pro Surface
- Mix recording
- Broadcasting
- DVS support
- Video mixing

### Wave 9: Release Gate
- Legal audit
- Packaging
- v1.0.0 release

## Code Patterns

### Memory Safety

- Values are owned by default; use `Box`, `Vec` and `Arc` deliberately
- No `unsafe` outside the designated FFI crate
- Types are not `Clone` or `Copy` unless copying is meaningful; this replaces the non-copyable declarations of the C++ tree
- Allocation happens off the audio thread; `assert_no_alloc` guards the render path in tests

### Crate and Module Layout

```rust
// crates/om-engine/src/lib.rs
#![forbid(unsafe_code)]

mod deck;
mod mixer;

pub use deck::{Deck, DeckId};
pub use mixer::Mixer;
```

### Error Handling

- Library crates: `thiserror` enums per module, returned as `Result<T, Error>`
- Binary: `anyhow::Result` at the top level with context on each step
- Log errors with `tracing::error!` (or `warn!`) and include the failing id or path as a field
- Never panic on bad input; do not use `unwrap()` on file, network or MIDI data

### Testing

- TDD for engine and analysis logic
- Golden-vector fixtures carried over from the C++ tests (kept under `tests/`)
- Property tests (`proptest`) for parsers and search
- Metric tests for DSP accuracy; `criterion` benchmarks for DSP
- Real-time tests: render the engine inside `assert_no_alloc`
- Screenshot tests for UI (planned)

## Next Steps (Phase 2, Linux first)

Work follows the design phasing. The parity inventory in `docs/parity.md` lists which C++ test each Todo replaces.

### Todo 7: Audio Device Layer

**Crate:** `crates/om-audio`

**Implementation:**
1. `cpal` output stream behind a device trait
2. Platform backends via cpal (ALSA first; WASAPI and CoreAudio in the porting phase)
3. Mock device for CI testing
4. Latency probing

**Tests (illustrative — not existing code):**
```rust
#[test]
fn device_opens_and_closes() {
    let mut dev = MockDevice::default();
    dev.open().unwrap();
    assert!(dev.is_open());
    dev.close();
    assert!(!dev.is_open());
}
```

### Todo 8: Deck Streaming

**Crate:** `crates/om-engine`

**Implementation:**
1. Lock-free SPSC ring buffer (`rtrb`)
2. `symphonia` decode backend
3. Transport state machine

**Tests:**
- Golden audio tests
- Transport state transitions

### Todo 9: Time-Stretch (`om-stretch`)

**Crate:** `crates/om-stretch` (Rubber Band R3 via FFI, decided; the `Stretcher` trait is the seam for swapping engines)

**Implementation:**
1. Wrap Rubber Band R3 behind the `Stretcher` trait; the A/B listen stays optional (by ear)
2. Key-lock toggle
3. RT-safety verification

**Tests:**
- Golden audio tests
- Allocation verification

## Common Issues

### Issue: cpal fails to build on Linux (missing `alsa.pc`)
**Solution:** Install `libasound2-dev pkg-config`

### Issue: `cargo deny check` fails on a licence
**Solution:** Find the crate with `cargo deny check licenses`; replace it or confirm the licence is AGPL-compatible before adding it to `deny.toml`

### Issue: GPUI fails to start on WSL
**Solution:** WSL-only trouble is a dev-loop issue, not a gate failure (ADR 0004). Check the GPU/Vulkan setup with `vulkaninfo --summary` and record the result in the Phase 1 notes

## Verification

After each wave:
1. All tests pass (`cargo test --workspace`)
2. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo deny check` are clean
3. Real-time allocation checks pass (`assert_no_alloc` in tests); sanitizer runs planned for concurrent code
4. Evidence file updated: `.omo/evidence/task-<N>-openmixer-dj-app.*`

## References

- Design: `docs/superpowers/specs/2026-10-18-rust-rewrite-design.md`
- ADR 0004 (Rust with GPUI): `docs/adr/0004-stack-rust-gpui.md`
- Frozen C++ baseline: git tag `cpp-final`
- cpal: https://docs.rs/cpal
- rtrb: https://docs.rs/rtrb
- arc-swap: https://docs.rs/arc-swap
- rusqlite: https://docs.rs/rusqlite
- Rubber Band: https://breakfastquay.com/rubberband/
- Clean-Room Doctrine: docs/adr/0001-clean-room-doctrine.md

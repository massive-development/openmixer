# OpenMixer - Agent Instructions

## Overview

OpenMixer is a clean-room, legally original implementation of professional DJ software, written in Rust with GPUI for the UI. This document provides instructions for any AI agent to understand the codebase and continue development.

**Status:** see "Current State" below.

## Project Structure

```
openmixer/
├── .github/workflows/        # rust.yml: the Rust CI (every PR); gui-check and gpui-lint are non-blocking
├── crates/                   # Cargo workspace members (om-core ... om-stems, om-plugins, om-ui, openmixer)
│   └── om-ui-gpui/           # GPUI window: its own workspace, behind the openmixer `gui` feature
├── docs/
│   ├── adr/                  # Architecture Decision Records
│   ├── legal/                # Legal documentation
│   ├── spikes/               # Spike notes and the decision record (DECISIONS.md)
│   └── superpowers/          # Historical design specs and plans
├── examples/plugins/         # Example in-process plugin (gain-plugin)
├── fixtures/                 # Small test inputs: controller maps and traces, library exports
├── resources/themes/         # Built-in theme JSON
├── tests/tools/              # Unit tests for the Python tools
├── tools/                    # check_docs_rust.py, progress_heatmap.py, screenshots.sh
├── Cargo.toml                # Workspace manifest
├── Cargo.lock
├── rust-toolchain.toml       # Pinned toolchain
├── rustfmt.toml
├── clippy.toml
├── deny.toml                 # Licence and ban policy (cargo deny)
└── crates/om-ui-gpui/deny.toml  # GPUI policy: the root policy plus what the GPUI tree needs
```

## Architecture and Crate Responsibilities

Dependencies point toward `om-core` (see "Modularity" above). The README architecture section has the directory tree; this is what each crate owns.

- **`om-core`** (no I/O): `ParamId` (control ids, parsed once, canonical `Display`), `Command` and `CommandSink`, `ParameterRouter` (typed dispatch plus the string façade), `StateId` and `EngineSnapshot` (pulled engine state), `WorkerPool` (job queue), `LockExt`, and the `ParamTarget` trait. Typed error enums: `ParamError`, `CommandError`.
- **`om-engine`** (audio thread): decks, mixer, FX racks, sync, loops. Implements `ParamTarget` for each typed target; `DjEngine::snapshot()` builds the snapshot on the control thread.
- **`om-audio`, `om-dsp`, `om-stretch`**: device layer, DSP, and key-lock time-stretch (the only crate allowed `unsafe`, for FFI).
- **`om-analysis`, `om-library`, `om-midi`, `om-stems`, `om-plugins`**: control-side services with their own typed errors. `om-analysis` runs its jobs on `WorkerPool`.
- **`om-stems` detail:** `StemSeparator` v2 is streamed (`separate_to(StereoSource, StemSink, ctx)`); `HtDemucsPipeline<I: StemInference>` does resampling, 7.8 s segments with overlap-add and STFT in Rust; `OrtInference` (ONNX Runtime) exists only with `--features onnx` (off by default, downloads the runtime at build time; own non-blocking CI job). `StemCache` (`cache.rs`, `wav.rs`) holds the stems under `<library folder>/stems`; `StemService` (`service.rs`, on `WorkerPool`, one worker) separates a track on explicit submit only and records `track_stems` through `om-library`, so `om-stems` depends on `om-library`. The app attaches it with `AppComposition::attach_stems` (`Services.stems`): the mock with `--mock-stems`, else htdemucs when built with `onnx` and installed. Real htdemucs is exported with `tools/export_htdemucs.py` (unverified).
- **`om-automix`** (pure, depends on `om-core` only, control thread): `stem_fade` holds `StemFade` ramps (volume, filter, mute) and `StemFadePlan`, with `value_at`, idempotent `steps_at`, `restore_values`, the transition builders (`bass_swap`, `vocals_out`, `drums_only_bridge`, `filter_sweep`) and `StemFadeRunner`, which applies due steps through a `CommandSink` and skips decks without stems. No engine or audio dependency.
- **`om-ui`** (no window): binds ids built from `ParamId`, lists learnable targets, holds settings, preferences and the `MidiHost` trait, and the themes.
- **`om-ui-gpui`** (own workspace, behind the `gui` feature): GPUI drawing and events over `om-ui` models.
- **`openmixer`** (binary, composition root): `Services` (typed optional services), `AppComposition`, `AppError`, `EngineStateReader` (a `StateReader` over a fresh snapshot), and the MIDI service that implements `MidiHost`.

Control paths, and what is live today:
- **UI (production window):** `DeckBinding::typed` and `MixerBinding::typed` send `Command::Param { id, value }` through the router's `CommandSink`, with no string formatting or parsing. The binding constructors without a sink (`new`) write through the string façade; the widget test rig uses them.
- **MIDI mappings and plugins:** the string façade, `ParameterTarget::set_parameter(&str, f64)` on the router. It parses the id once there and dispatches typed ids the same way.
- **Typed entry point:** `Command` and `CommandSink` are used by tests, by automix and by the Wave 7 features. The router's sink accepts only `Command::Param` today; `Automix` returns `CommandError::Rejected` until its engine sink lands. `LoadStems` and `SamplerAssign` are handled by `DjEngine::command_sink(fallback)` (`EngineCommands`): `DjEngine::register_stem_set(dir)` turns a stem directory into the `StemSetRef` handle the command carries, `DjEngine::register_sample(Arc<Sample>)` / `load_sample(path)` the `SampleId` for `SamplerAssign`; the router's sink still rejects both.

### Sampler (Wave 7 P1/P3, Todo 40 engine level)

- `om-engine::sampler`: 8 pads, each with 4 sounding voices (8 slots, so stolen and released voices can fade out while new ones start); every voice is preallocated. `Sample` is interleaved stereo `Arc<[f32]>` plus rate and name, decoded off the audio thread by `Sample::load(path, engine_rate)` (symphonia through `SourceReader`, mono duplicated, windowed-sinc resample, capped at 60 s / 64 MiB, non-finite values zeroed).
- `SamplerControl` (control side, `Send + Sync`, `&self`): `assign`, `clear`, `trigger(pad, velocity)`, `trigger_with(.., mode)`, `release`, `stop_all`, `set_gain` (0..1.5), `set_pitch` (semitones, +-24, varispeed with cubic interpolation), `set_mode` (`PadMode::OneShot|Gate|Loop`), `set_master_gain`, `reclaim`, `snapshot`. Assign, clear, trigger, release and stop go over an `rtrb` ring of plain `SamplerCmd`s; gain, pitch and mode are latest-wins atomics, so they are never lost. A full ring drops a trigger, release or stop and counts it (`dropped_commands`); `assign`/`clear` return `QueueFull`. A trigger with no free voice slot is dropped and counted too.
- `SamplerAudio` (audio side, a `ProcessStage` that adds to the buffers, owned by the `Mixer` via `attach_sampler`): no allocation, lock or drop. Samples the audio thread lets go of (a replaced pad sample, a finished voice) return on a second ring and are freed by `SamplerControl::reclaim` (called by `assign` and `clear`); if that ring is full they stay held and are retried next block. Voice start fades in over 32 frames, release, steal, stop and clear fade out over 5 ms. One-shot ignores release, gate fades on release, loop wraps (interpolation wraps too) until release or stop. The fifth trigger on a pad steals the oldest voice. A sample ends at its last frame without a fade-out.
- Mix point: after the channels are summed, before the master gain, the master inserts and the meters. The master fader scales the sampler, a master-insert limiter sees it, and the cue mix hears it. The mixer has no limiter or clamp of its own.
- Ids (`DjEngine::register` adds the `sampler` namespace): `sampler.pad<0..7>.trigger` (value is the velocity 0..1, above 1 clamps; 0 or less releases), `.gate` (0.5 or more presses with velocity 1 and gate behaviour for that voice whatever the pad mode, below 0.5 releases: one id for note on and off), `.volume` (0..1.5), `.pitch` (-12..12). NaN and infinity are rejected. Read ids (`StateId::Sampler`): `sampler.pad<n>.loaded|playing|volume|pitch|mode` (mode 0 one-shot, 1 gate, 2 loop). `EngineSnapshot.sampler` is `SamplerSnapshot { pads: [PadSnapshot; 8], voices_active, dropped_cmds }`. Example map with LED rules: `fixtures/controllers/sampler_map.json`; the MIDI learn list offers the eight triggers.
- Sample bank (P2, `om-ui::sample_bank`): `<data dir>/samples/` holds the WAV files and `sampler.json` (`{"version":1,"pads":[{"pad","file","name","gain","pitch","mode":"oneShot|gate|loop"}]}`; `file` relative to the folder or absolute). `SampleBank::open` reads it (a corrupt file goes to `sampler.json.bad`, a newer version is refused and never overwritten), `apply` sets gain, pitch and mode and queues each existing file for decode on a worker (a missing file leaves the pad empty with a note), `import_file` decodes first and then copies in (`name-2.wav` on a clash), `pump` installs finished decodes through `Command::SamplerAssign` (the control timer pumps once the bank is attached), `save` writes temp-then-rename. A failed decode keeps the pad's sample. `AppComposition::attach_samples` runs after the device starts (so decodes use the engine rate); `--demo-library` writes eight generated samples into its scratch folder (`openmixer::sampler_demo`).
- Pad view-model (P4, `om-ui::sampler_binding::SamplerBinding`): eight `PadView`s from `EngineSnapshot.sampler` and the bank; `press` sends `sampler.pad<N>.trigger` (one-shot, loop) or `.gate` 1.0 (gate pads, held until `release`) as typed `Command::Param`s; `set_gain|set_pitch|set_mode|cycle_mode` store through the bank; `load`/`load_typed_path` and `clear`. Keys 1 to 8 press pads while the strip has focus (the library search and the settings text fields keep their keys). Layout in `om-ui::widgets::sampler_strip`: a 4x2 grid and a control column (mode, load and clear, gain and pitch steppers), in the library band (`SAMPLER_SLOT`, 34% of its width).
- Soak and gate (P5): the short and 10-minute soaks trigger random one-shot and loop pads, hold gates for 0.2 to 1.5 s, mix the three modes, and replace pad 3's sample at 10 s; the UI gate presses a kick and holds a gate through the view-model. All use the same invariants as before (no non-finite, overflow or steady underruns; the xrun budget).
- Not verified: by ear (the mock device is silent), by eye on the real window (screenshots only), and the 10-minute soak and gate runs (ignored tests).

### Stem mode in the engine (Wave 7 S5)

- A deck plays from stems after `Deck::load_stems(dir)` (`dir/{drums,bass,vocals,other}.wav`, the `om-stems` cache layout, same rate as the track, length within 16 frames). The main file stays loaded for metadata and length; `clear_stems()` returns to it; `load` of another track also leaves stem mode and resets the mixer.
- The decoder thread reads the four files in lockstep into the one frame ring, 8 floats per frame (L/R of drums, bass, vocals, other). The ring stride (2 or 8) travels with the decoder's ack, so the audio thread adopts it exactly when it drains the stale ring; prefetch slots carry a stride too and are only used when it matches. A hand-over bumps the load id and the decoder produces nothing from new sources until it has acknowledged the epoch that names them.
- The 8-to-2 mix (`om_dsp::StemMixer`) runs in `Playback::pull_frame`, before the interpolator and before key-lock: still exactly one interpolator and one stretcher per deck. All position logic is frame-index based and unchanged.
- Control: `Deck::stem_control() -> Arc<StemMixerControl>` (atomics), ids `stem.<deck>.<0..3>.volume|mute|filter` (stem order: drums, bass, vocals, other), read back through `EngineSnapshot.stems` and `StateId::Stem`. Stem state belongs to the deck's `Shared`, so it resets on `load` and persists across `load_stems` and `clear_stems`.

Rules: writers never write engine state directly; readers pull an `EngineSnapshot`; the audio thread takes no locks and never parses an id.

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


## Current State (Linux first)

**Status:** Works, verified headlessly on Linux: the audio engine (cpal and a mock device, decks, mixer, FX racks, sync and quantize, key-lock), analysis (BPM, key, waveform, structure), the library (database, ingest, iTunes and Rekordbox import, search, prep queue), MIDI (ALSA sequencer, mapping, feedback, trace replay), the plugin API, themes, preferences and the settings overlay, the debug panel, and the GPUI app. Partial: the Wave 5 UI and stems (mock separator only). The GPUI window has never been viewed by eye. **Next:** the Before-Wave-7 list in `TODO.md`, then Wave 7.

**Implemented:**
- Governance and legal: LICENSE, ADRs, CONTRIBUTING, clean-room doctrine
- The crates and features in the status above, plus the headless UI gate
- The GPUI window behind the `gui` feature
- Rust CI (`rust.yml`): fmt, clippy, test on all three OSes, `cargo deny`, the progress heatmap check and the ASIO SDK guard; `gui-check` builds the GPUI feature and runs its tests on Linux, and `gpui-lint` runs fmt, clippy and `cargo deny` for `om-ui-gpui` (both non-blocking)

**Known gaps:** see the list in `ROADMAP.md` (Known gaps) and the Before-Wave-7 list in `TODO.md`, which is the authoritative list.

## Architecture Decisions

### ADR-0001: Clean-Room Doctrine
- **Features** are free (methods of operation)
- **Expression** is not (names, logos, artwork)
- No Mixxx code reuse (clean-room)
- All assets must be original

### ADR-0002: Technology Stack (superseded)
- Superseded by ADR-0004.

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
- **MIDI:** ALSA sequencer (`alsa`, same version as cpal) on Linux; `midir` for macOS and Windows, later (see `docs/spikes/DECISIONS.md`); **Library:** `rusqlite` (bundled SQLite)
- **RT plumbing:** `rtrb` (SPSC), `arc-swap` for snapshots, `assert_no_alloc` in tests

## Pull Request Workflow (required)

All changes — including those by AI agents and maintainers — land through a pull request. Never merge a branch directly or push to `main` or `stage`.

1. Work on a feature branch (`wave2-mixer`, `docs/...`, `fix/...`) cut from `stage`.
2. Open a PR into `stage` with `gh pr create --base stage`. `rust.yml` (the Rust CI: fmt, clippy, test on Linux, macOS and Windows, `cargo deny`, the progress heatmap check and the ASIO SDK guard) runs on every pull request. The description must include:
   - **Summary:** what changed and why (link the Todo number from `TODO.md`).
   - **Changes:** key crates/modules added or modified.
   - **Testing:** commands run and results (`cargo test --workspace`, manual checks).
   - **Follow-ups / known gaps:** anything left undone.
3. Update `TODO.md`, `docs/progress.json` (then run `python3 tools/progress_heatmap.py`) and `docs/PROGRESS.md` in the same PR when task status changes.
4. Merge the PR (`gh pr merge --merge`) once the build and tests pass, so history stays traceable to a PR.
5. Release: batch `stage` into `main` with a PR (`--base main`); `rust.yml` runs on it as on every PR. Pushes alone never trigger CI.
6. Stack dependent work: when a PR builds on or touches the same files as an open PR, branch from that PR's branch and set `--base <that-branch>`. Keep the stack current by merging the parent branch into the child (merge commits, never force-push), so merging in order needs no rebase. After the parent merges, retarget the child with `gh pr edit --base stage`. Use plain merge commits, not squash, so stacked history stays intact.
7. Promote early: open a `stage` -> `main` PR when a Todo or feature group finishes, or once `stage` is about 3 feature PRs or ~2,000 changed lines ahead of `main`, whichever comes first. Small release PRs stay readable and revertable.
8. Slice large backlogs: if `stage` is further ahead than that, do not promote it in one PR. Cut `release/rN` branches at `stage` first-parent merge commits (each already passed CI), open them as a stack (`release/r1` -> `main`, `release/r2` -> `release/r1`, ...), merge in order and retarget the next one to `main` so the full matrix runs on it.
9. Release PR description: list the included PRs (number + Todo), the testing they already had, and the revert command. Merge with `--merge` so a batch can be undone with `git revert -m 1 <merge commit>`.

## Rust Standards & Memory Safety (required)

All code must follow Rust best practices (edition 2021, toolchain pinned in `rust-toolchain.toml`). Outdated, unsafe or leaking code is not acceptable.

- **Ownership:** owned values and borrows by default. Use `Arc` only when ownership is genuinely shared across threads, and `Rc` never on the audio path. Pass data by value or by reference; avoid `'static` leaks and `Box::leak`.
- **Unsafe:** every crate has `#![forbid(unsafe_code)]` (or the workspace `unsafe_code = "forbid"` lint) except the dedicated FFI crate `om-stretch` (Rubber Band R3 behind a `Stretcher` trait; see docs/spikes/DECISIONS.md). Every `unsafe` block there has a `// SAFETY:` line stating the invariant.
- **Idioms:** `enum`s and `match` for state and commands, newtypes for ids and units, `Option`/`Result` instead of sentinel values, `?` for propagation, `#[must_use]` where a result must not be ignored, iterators over index loops where clearer. Prefer `&str`/`&[T]` parameters.
- **Errors:** `thiserror` for typed errors in library crates, `anyhow` only in the `openmixer` binary. Do not `unwrap()`/`expect()` on input from files, the network, MIDI or the user; tests may.
- **Logging:** `tracing` macros with structured fields. Never log from the audio thread.
- **Concurrency:** no shared mutable state between the audio thread and other threads except through the designated channels: `rtrb` SPSC rings, `arc-swap` snapshots and atomics. Document the thread that owns each type in its module doc.
- **Real-time safety (audio thread):** no allocation, locks (`Mutex`, `RwLock`), blocking channel calls, file or network I/O, or syscalls in the render path. Create buffers and graphs off the audio thread. Enforce this in tests with `assert_no_alloc` around the render function; a failing check is a bug, not a flaky test.
- **Lints:** `cargo clippy --all-targets -- -D warnings` must pass. Do not add `#[allow(...)]` to get past a lint without a comment stating why.
- **Licences and bans:** `cargo deny check` must pass (`deny.toml`). New dependencies must be AGPL-compatible.
- **Adopting crates:** well-established crates (cpal, symphonia, rusqlite, midir and similar) are adopted directly; reserve spikes for genuine unknowns.
- **Check current guidance:** when unsure about an API, check docs.rs for the crate version in `Cargo.lock` rather than reusing older patterns from memory.
- **Skills:** the Rust skills in `.agents/skills/` (`rust-best-practices`, `rust-patterns`, `rust-async-patterns`) are the reference for idioms and async code. Check the directory exists before relying on it. Async is not the default: prefer threads and channels for the audio and analysis paths.

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

Colours, radii, border widths, insets and font sizes come from the theme, never from literals in view code.

1. Read the theme's tokens (`om_ui::theme::ThemeTokens`, built from `resources/themes/*.json`).
2. Get the control's recipe from `om_ui::style::LookAndFeel::from_tokens(t)`: `button` (idle, hover, active, disabled), `lit_button(accent)`, `knob`, `fader`, `tabs`, `tooltip`, `text_field` (idle, focus), `meter`, `panel`, `panel_caption`, `separator`, `drop_target`.
3. In `om-ui-gpui`, draw with `paint::skin(el, &style)` for a box, `paint::border(el, width)` for a bare border, and `paint(colour)` for a colour. Radii and insets come from the style fields, not literals.
4. A value no token covers: add a token field in `theme/tokens.rs` (default in `builtin_dark`), a reader in `theme/json.rs` (range-checked, with a key in the matching section), and a value in both built-in theme JSON files. Old theme files stay valid because a missing key inherits the default.
5. Add the new style to `LookAndFeel::entries()` so the contrast and live-switch tests cover it. Text on a control needs contrast 3.0 (control) or 4.5 (body) against its fill over both backdrop ends; disabled controls are exempt.
6. `crates/om-ui-gpui/tests/style_lint.rs` fails on raw colour calls (`rgb(`, `rgba(`, `hsla(`, `from_argb(`, bare 6 or 8 digit hex) in `src/`. Only `paint.rs` may name them.

gpui only has fixed border widths (0 to 12 px); a token width is rounded to a whole pixel.

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
- **Plugins are in-process for now.** `om-plugins` has the plugin API and registry; loading from a shared library is open (see `docs/spikes/DECISIONS.md`). Keep new extension points as traits and registries in the owning crate, so they can be exposed later without restructuring.
- **PR check:** every PR must state whether it adds, changes or could block an extension point.

## Progress Files (required)

Every PR that completes or advances a task must update, in the same PR:
- `TODO.md` (tick items), `docs/progress.json` (status), `docs/PROGRESS.md` (done/missing detail)
- the heatmap: `python3 tools/progress_heatmap.py` (CI's `progress-check` job fails if `docs/progress-heatmap.svg` is stale)
- screenshots: `tools/screenshots.sh` rebuilds the `gui` binary and regenerates `docs/screenshots/*.png` from the real window (X11, `--mock-device` only, temp library and preferences; every `import` passes `-silent`)

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

### Test support

`crates/om-testing` holds the helpers the crates share in their tests: `wav` (fixture writers and a reader), `signals` (deterministic sines, noise and impulse trains), `level` (dB, peak and RMS), `wait` (`wait_until` and `assert_eventually!`) and `fs::TempDir`. Reuse these instead of writing a new WAV writer, sine or poll loop. Waits poll for the condition they expect, with a timeout; never sleep a fixed time to wait for something to happen. It is a dev-dependency only: no shipped crate may depend on it.

## Implementation Waves

Waves 1–4 and 6 are verified headlessly on Linux; the Wave 5 UI is partial. Waves 7–9 are new work. Status by Todo is in `TODO.md`.

### Wave 1: Foundation ✅ COMPLETE
- Repository setup
- Legal framework
- Build system and CI (Cargo workspace and `rust.yml`)
- Test harness

### Wave 2: Audio Engine
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
- **Gate:** scripted UI-driven 10-minute mix (`crates/openmixer/tests/ui_gate.rs`, the 10-minute run is `#[ignore]`) plus `openmixer --smoke`; UI screenshot suite and keyboard-only testing (planned)

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
- Types are not `Clone` or `Copy` unless copying is meaningful
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
- Binary: `AppError` (`crates/openmixer/src/error.rs`) with `#[from]` for each subsystem error; `anyhow` is not used
- Log errors with `tracing::error!` (or `warn!`) and include the failing id or path as a field
- Never panic on bad input; do not use `unwrap()` on file, network or MIDI data

### Testing

- TDD for engine and analysis logic
- Golden-vector fixtures (kept in the crates' test modules and `fixtures/`)
- Property tests (`proptest`) for parsers and search
- Metric tests for DSP accuracy; `criterion` benchmarks for DSP
- Real-time tests: render the engine inside `assert_no_alloc`
- Screenshot tests for UI (planned)

## Next Steps (Linux first)

Next: the Before-Wave-7 list in `TODO.md`, then Wave 7. Windows (WASAPI) and macOS (CoreAudio) runtime targets come later.

### Todo 7: Audio Device Layer

**Crate:** `crates/om-audio`

**Implementation:**
1. `cpal` output stream behind a device trait
2. Platform backends via cpal (ALSA first; WASAPI and CoreAudio later)
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
**Solution:** WSL-only trouble is a dev-loop issue, not a gate failure (ADR 0004). Check the GPU/Vulkan setup with `vulkaninfo --summary` and record the result in `docs/spikes/DECISIONS.md`

## Verification

After each wave:
1. All tests pass (`cargo test --workspace`)
2. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo deny check` are clean
3. Real-time allocation checks pass (`assert_no_alloc` in tests); sanitizer runs planned for concurrent code
4. Evidence file updated: `.omo/evidence/task-<N>-openmixer-dj-app.*`

## References

- Design (historical): `docs/superpowers/specs/2026-10-18-rust-rewrite-design.md`
- ADR 0004 (Rust with GPUI): `docs/adr/0004-stack-rust-gpui.md`
- cpal: https://docs.rs/cpal
- rtrb: https://docs.rs/rtrb
- arc-swap: https://docs.rs/arc-swap
- rusqlite: https://docs.rs/rusqlite
- Rubber Band: https://breakfastquay.com/rubberband/
- Clean-Room Doctrine: docs/adr/0001-clean-room-doctrine.md

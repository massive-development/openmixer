# OpenMixer Rust Rewrite — Design

Status: draft for approval. Items marked **(call)** are the author's decisions, not yet agreed.

## Intent
Rewrite OpenMixer from C++/JUCE to Rust with GPUI for the UI. Why: easier to audit (memory safety, a smaller unsafe surface, one toolchain via Cargo), and a GPU-accelerated, component-style UI without a webview. The product goal is unchanged: a clean-room, AGPLv3, djay-Pro-class DJ app for Windows, macOS and Linux (see `README.md`, ADR 0001, ADR 0003).

## Baseline
The tag `cpp-final` (main @ c6d2747) is the frozen C++ implementation. It is the behavioural reference: the old tests, fixtures and `docs/PROGRESS.md` define what "ported" means. The C++ tree is removed from `main` only after the Rust engine passes parity (see Phasing).

## Goals
- Feature parity with the C++ baseline: engine (decks, mixer, crossfader, FX racks, sync, loops, keylock), analysis (BPM, key, structure, waveform, beat grid), library (SQLite, search, iTunes/Rekordbox import, watch folders, prep queue), MIDI (mapper, learn, output feedback), UI (decks, mixer, library, settings, theming, FX panel), then the unported waves (stems, Automix, pro surface).
- Real-time safety enforced by tooling, not convention: no allocation, locks or syscalls on the audio thread, checked in tests.
- `unsafe` confined to a few small, documented modules; `#![forbid(unsafe_code)]` everywhere else.
- Headless-first: engine, analysis, library and MIDI are fully testable without a window; UI is a thin layer over a state/command API.
- Cross-platform CI (Linux, macOS, Windows) from the first crate.

## Non-goals / anti-goals
- No line-by-line translation of C++ idioms (no OO port of JUCE patterns); port behaviour, not structure.
- No webview, Electron or Tauri.
- No copying of any proprietary expression (ADR 0001 still binds).
- A rewrite that drops audio-glitch guarantees (the 10-minute soak gate) is a failure even if tests pass.

## Stack (all **(call)** unless stated)
| Area | Choice |
|---|---|
| Language / UI | Rust, GPUI (decided by user) |
| Audio I/O | `cpal` for the engine's output callback (rodio is built on cpal; its pull-based `Source` chain and managed output thread don't give the sample-accurate, allocation-free, buffer-size-controlled render loop, or the second headphone-cue stream, that decks and a mixer need). `rodio` is acceptable for non-critical audio such as library preview/audition. |
| Decode | `symphonia`; tags via `lofty` |
| FFT / DSP | `realfft`/`rustfft`; own biquad/EQ/FX in `om-dsp` |
| Time-stretch / keylock | Spike required: Rubber Band via thin FFI vs a pure-Rust stretcher. Prefer pure Rust if quality holds; FFI is the fallback (kept in one `unsafe` crate). |
| MIDI | `midir`; HID later |
| Library | `rusqlite` (bundled SQLite) |
| RT plumbing | `rtrb` (SPSC), `arc-swap` for param/snapshot publishing, `assert_no_alloc` in tests |
| Stems | ONNX Runtime (`ort`) for precomputed separation; deferred to its wave |
| Plugins | Deferred; CLAP-first if/when needed |
| Errors / logging | `thiserror` in libs, `anyhow` in the binary, `tracing` |
| Tests | built-in `cargo test`, `proptest` for parsers/search, golden-vector fixtures carried over from C++ tests, `criterion` for DSP |
| Lints | `clippy -D warnings`, `cargo deny` (licences must stay AGPL-compatible), `cargo audit` |

Risks to verify early: GPUI's release/stability story and licence (expected Apache-2.0, AGPL-compatible) and its Linux/WSL/Windows support; time-stretch quality; Windows audio latency via cpal (WASAPI).

## Architecture
Cargo workspace; dependency arrows point toward `om-core`.
- `om-core` — parameter/target traits, state snapshot types, command enum. No I/O.
- `om-dsp` — filters, EQ, FX primitives, resampling helpers.
- `om-engine` — decks, channel strips, crossfader, FX routing, sync, loops. Audio-thread code; allocation-free.
- `om-audio` — device layer over `cpal`, latency monitor.
- `om-analysis` — STFT, onset, tempo, beat grid, chroma/key, structure, waveform, job runner, result cache.
- `om-library` — schema/migrations, DAOs, ingest, search parser/compiler, importers, prep queue.
- `om-midi` — input router, map, mapper, learn, feedback router/rules, trace.
- `om-stems`, `om-plugins` — added in their waves.
- `om-ui` — GPUI views, theme/design tokens, widgets; reads state snapshots, sends commands.
- `openmixer` — binary: composition root, preferences, `--smoke`.

Control flow: UI/MIDI → command channel → engine (audio thread drains SPSC) → state snapshots published back (`arc-swap`) for UI and MIDI feedback. This replaces the C++ `ParameterRouter`/`StateReader` glue.

## Phasing
0. Scaffolding: workspace, CI matrix, lint/deny config, new ADR 0004 (supersedes ADR 0002), docs rewrite.
1. Spikes (throwaway, labelled): GPUI window + custom waveform view on all three OSes; cpal duplex callback; time-stretch quality.
2. Engine + audio I/O + headless mix test + soak gate.
3. Analysis (golden vectors from C++ fixtures), then library.
4. MIDI (mapper, learn, feedback).
5. UI over the stable API; UI-driven 10-minute mix gate.
6. Remaining waves (stems, Automix, pro surface), release.
Each phase lands as small PRs into `stage`; the C++ tree is deleted once phase 5 passes.

## Documentation rewrite
Every doc that mentions C++/JUCE/CMake must be rewritten for Rust: `README.md`, `agents.md`, `CONTRIBUTING.md`, `TODO.md`, `ROADMAP.md`, `docs/PROGRESS.md`, `docs/DESIGN_PATTERNS.md`, ADR index plus new ADR 0004, CI workflow docs, `CLAUDE`/agent skills references. Bulk rewrites are delegated to the cheapest capable model (haiku) and reviewed against this design; `docs/progress.json` and the heatmap tool are kept, with task statuses reset to the Rust state.

## GPUI go/no-go gate (agreed)
The first Phase 1 spike is a hard gate: `vulkaninfo --summary` on the dev WSL, a GPUI window with a custom-painted waveform on Linux and Windows, and a note on whether GPUI must be pinned to a git commit of Zed (no authoritative crates.io status was found). If it fails on a platform, the fallback is switching toolkit (iced first, then Slint), not dropping the platform or stopping. WSL-only trouble is a dev-loop issue and does not fail the gate on its own.

## Answers from the builder check (all **(call)**)
- **Parity inventory:** `docs/PROGRESS.md` and `TODO.md` at `cpp-final` are authoritative; the README status text is stale and is fixed in the docs rewrite. Phase 0 produces `docs/parity.md`: one row per TODO item with its C++ test(s) and the Rust test that replaces it.
- **Gates:** carry over the C++ definitions: Wave 2 gate = 10-minute headless engine soak (`tools/soak_mix.cpp`: zero xruns, no audio-thread allocation); Wave 5 gate = scripted UI-driven 10-minute mix (`tools/ui_gate`) plus a `--smoke` startup check. Latency/xrun targets are measured and reported by the latency monitor; the default is a 256-frame buffer at 48 kHz on ALSA/WASAPI/CoreAudio, zero xruns in the soak.
- **Time-stretch bar:** pure Rust is accepted only if an A/B listening test and an offline pitch/transient comparison against Rubber Band are acceptable and one deck fits in a small share of one core. Rubber Band is GPL-2.0-or-later, which is AGPLv3-compatible and already used in the C++ build, so the FFI fallback is licence-acceptable.
- **Migration:** the project is pre-release (no `v*` tags), so the Rust version starts fresh: no SQLite/preferences/cache migration.
- **Input capture:** out of scope until the pro-surface wave (DVS/recording). The Phase 1 audio spike is output-only; the "duplex" wording above means separate input and output streams, deferred.

## Left to the builder
Crate granularity within the above, exact command/snapshot types, preferences format, theme file format (carry over the C++ one if practical), CI details, module-level API shapes.

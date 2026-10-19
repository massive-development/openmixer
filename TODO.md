# OpenMixer TODO

OpenMixer is being rewritten in Rust with GPUI. The statuses below are the Rust state. The statuses of the frozen C++ baseline are in the `cpp-final` tag (`git show cpp-final:TODO.md`). Todo numbers and titles match `docs/parity.md`, which maps each one to its C++ tests and its Rust test.

## Rust rewrite Phase 0-1

**Status:** Phase 0 complete; Phase 1 spikes run — Linux-first; GPUI gate closed on Linux, stretcher decided (Rubber Band R3 behind a Stretcher trait); Windows/macOS deferred to a porting phase.

Source: `docs/superpowers/plans/2026-10-18-rust-phase0-1-scaffold-and-spikes.md`. Design: `docs/superpowers/specs/2026-10-18-rust-rewrite-design.md`.

- [x] Plan task 1: Workspace scaffold and `--smoke`
- [x] Plan task 2: Rust CI and licence policy
- [x] Plan task 3: ADR 0004 supersedes ADR 0002
- [x] Plan task 4: Parity inventory
- [x] Plan task 5: Documentation rewrite
- [x] Plan task 6: Spike A, GPUI window (go/no-go)
- [x] Plan task 7: Spike B, cpal output callback
- [x] Plan task 8: Spike C, time-stretch quality
- [x] Plan task 9: Phase 1 decision record

## Wave 1: Foundation (COMPLETE)

- [x] Repository initialization
- [x] Build system (Cargo workspace)
- [x] CI/CD pipeline (`rust.yml`: fmt, clippy, test, deny)
- [x] Governance documents
- [x] Legal framework
- [x] Test harness (`cargo test`)

## Wave 2: Audio Engine (vertical slice: make sound, control it, test it)

Reordered so audio can be heard and driven (mixer + MIDI + debug panel) before heavy dependencies.

- [x] **Todo 7:** Audio device layer + latency instrumentation + mock device
  - Output via `cpal` (ALSA backend first; WASAPI and CoreAudio in the porting phase)
  - Mock device for CI testing
  - Open: the cpal device has not yet run on a real ALSA/PipeWire device (WSL has none; the device test skips there). See `docs/spikes/cpal.md`

- [x] **Todo 8:** Deck engine (streaming decode + transport)
  - Streaming decode via `symphonia`
  - Lock-free ring buffer
  - FFmpeg backend deferred (see Todo 8b)

- [x] **Todo 10:** Mixer (channels, EQ, filter, crossfader)
  - 3-band EQ
  - PFL cue bus

- [ ] **Todo 10a:** Plugin API + registry (extension points)
  - `om-plugins` crate: versioned interfaces, runtime loader, registry
  - Audio pre/post-processing stages (per deck/channel/master), FX, MIDI map loading
  - No signing/allow-list gatekeeping (see CONTRIBUTING "Extensibility")
  - Plugins are deferred; CLAP-first if and when needed (design spec)

- [ ] **Todo 32:** MIDI input + mapping engine (pulled forward from Wave 6)
  - `midir` input -> JSON map file -> mixer/deck parameters
  - Mapping editor UI stays in Wave 6 (Todo 36)

- [ ] **Todo 33:** Controller trace harness (pulled forward from Wave 6)
  - Replay recorded MIDI streams headlessly to test mappings

- [ ] **Todo 26a:** Minimal debug panel (pulled forward from Wave 5)
  - 2 decks: load/play, faders, EQ knobs, level meters
  - Partial (Todo 8b): `--debug-panel` shows device, latency, xruns, CPU and the raw parameter list; load/play waits on the library (Todo 28)
  - Plain widgets, no design tokens; replaced by Wave 5 UI

- [x] **Todo 13a:** Short soak test in CI (30-60s headless mix)
  - Soak mix tool carried over from the frozen baseline (`cpp-final`)
  - Full 10-minute run remains the Wave 2 gate (Todo 13)

- [x] **Todo 9:** Key-lock time-stretch via Rubber Band
  - Realtime mode
  - Golden audio tests
  - Decided: Rubber Band R3 via FFI in `om-stretch`, behind a `Stretcher` trait; no pure-Rust candidate passed the stereo CPU bar (see `docs/spikes/DECISIONS.md`). Enable `GPL-2.0-or-later` in `deny.toml` when this lands

- [x] **Todo 12:** Beat-sync + quantized cue/loop/slip
  - Engine side done (grid, sync, quantize, slip, gapless loops, hotcues); analysed beat grids (Todo 15) drive sync and quantize through `DjEngine::set_grid` (test: `analysed_grid_drives_sync_and_quantize_through_the_engine`)
  - Beat phase alignment
  - Control ids: `deck.N.sync`, `sync.master`, `loop.quantized.<beats>`, `slip`
  - Gapless deck loop playback and hotcues

- [x] **Todo 11:** FX rack (8 essential effects)
  - Effects: echo, reverb, flanger, phaser, crush, gate, roll, tape stop
  - Per-unit enable, wet/dry mix and effect parameters; `fxN.*` parameter ids
  - Beat-synced to the tempo grid
  - Routed per mixer channel and on master (`fx.master.*`, `fx.ch<N>.*`)
  - No allocation on the audio thread; buffer clears amortised
  - Follow-up: beat-phase lock, FX UI

- [ ] **Todo 8b:** FFmpeg decode backend (optional, after the above)

- [x] **Todo 13:** Engine soak test (10-minute headless mix)
  - Soak mix tool carried over from the frozen baseline (`cpp-final`)
  - Performance verification: zero xruns, no audio-thread allocation

**Gate:** All Wave 2 todos must pass before Wave 3

## Wave 3: Analysis Pipeline

- [x] **Todo 14:** Analysis service skeleton + SQLite cache
  - Background job runner with progress, cancel and wait
  - Analyzer registry (versioned extension point)
  - SQLite result cache keyed by file content hash, analyzer id and version

- [x] **Todo 15:** BPM + beat-grid detection
  - Tempo estimation, beat grid with downbeat detection
  - Manual tempo markers and correction

- [x] **Todo 16:** Musical key detection + Camelot mapping
  - Chroma-based key estimation
  - Camelot code mapping and harmonic compatibility

- [x] **Todo 17:** Waveform data model
  - Multi-resolution overview and zoom data
  - Low/mid/high band data for colouring

- [x] **Todo 18:** Onset/energy/structure features
  - Onset, energy and section detection (feeds Automix)
  - Mix-in and mix-out points

- [ ] **Todo 19:** Stems precompute (Demucs ONNX)
  - Separator interface and model manifest
  - Licence-gated, verified weight download
  - ONNX Runtime backend (`ort`) not yet built

## Wave 4: Library & Data

- [x] **Todo 20:** Library schema + DAO layer
  - SQLite schema with migrations and transactions (`rusqlite`)
  - DAOs for tracks, cues, playlists

- [x] **Todo 21:** Metadata ingest + watch folders
  - Metadata reader registry; RIFF INFO tags and duration
  - Tags for MP3/FLAC/M4A via `lofty`; artwork
  - Recursive ingest and watch folders

- [x] **Todo 22:** Import (iTunes/Rekordbox XML)
  - Importer registry (extension point)
  - iTunes and Rekordbox XML importers
  - One transaction per import, merge by path; file URL decoding

- [x] **Todo 23:** Search + filter engine
  - Field registry (extension point), query parser, parameterised SQL
  - Index migration; LIKE-based matching unless FTS5 is available

- [x] **Todo 24:** Track prep queue
  - Persisted queue with order, priority and status
  - Bounded in-flight analysis; results written back to the library

## Wave 5: UI

- [x] **Todo 25a:** Theme/skin loader
  - JSON themes with built-in, user and plugin sources
  - Live theme switching; built-in themes embedded in the binary

- [ ] **Todo 25:** Design tokens + LookAndFeel
  - Original visual identity (no copied layouts or artwork)
  - Widget styling for knobs, faders, buttons, menus, text fields, tabs, scroll bars, tables and tooltips

- [ ] **Todo 26:** Main window + deck widgets
  - Waveforms, transport, hotcues, loops
  - Partial (Todo 8b): deck cards drawn and wired; waveforms are synthetic until the analysis crate (Todo 17) lands

- [ ] **Todo 27:** Mixer widgets + metering
  - Knobs, faders, meters
  - Partial (Todo 8b): channel strips, master, crossfader, cue and FX page drawn with drag; not verified by eye

- [ ] **Todo 28:** Library browser + app wiring
  - Tree, track list, search
  - Partial (Todo 13a): the library logic is ported and tested (`om-ui::library`: services over the library, analysis, prep queue and watch folders; the track table and its paged source with the search syntax and the key tint; the browser state, status line, load requests, prep queueing and XML import; the track loader that applies cached waveform and grid and queues analysis; the track drag payload), and the composition attaches it on demand. Open (Todo 13b): the gpui browser view, the deck drop target and load buttons, and the waveform drawing from the loader

- [ ] **Todo 29:** Automix panel + settings
  - Audio/MIDI settings, preferences
  - Partial: versioned preferences, applier, `AppComposition`, audio/MIDI/library/plugins/appearance page models, `--smoke`, the settings overlay (Settings button in the header; audio and appearance pages live, latency/xrun/CPU readout), audio device state restore on start-up (`openmixer::audio_settings`). Open: the MIDI and plugin pages show placeholders until Todos 10 and 14; the library page drives the library services (Todo 13a); no Escape key on the overlay yet (needs a focus handle); overlay not checked by eye
  - Also open: `--settings` start flag; the Linux GPUI CI job (see the TODO in `.github/workflows/rust.yml`)
  - Automix panel (Wave 7, needs Todo 38)

- [ ] **Todo 30:** Stem pads UI (open: waits on Wave 7 stem playback)
  - Per-stem mute/volume

- [x] **Todo 31:** UI verification gate
  - Scripted 10-minute mix driven through the UI
  - Done at the logic layer: `crates/openmixer/tests/ui_gate.rs` (8 s `#[test]`, 10-minute `#[ignore]`) drives the composition over a paced mock device with the om-ui transport and binding models. Not done: a window-driven pass (needs a display)
  - `--smoke` startup check (opens the window; skipped without a display)

## Wave 6: Hardware I/O

- Todos 32/33 moved to Wave 2
- [ ] **Todo 34:** Plug-and-play controller maps
- [ ] **Todo 35:** HID layer
- [ ] **Todo 36:** Mapping editor UI

## Wave 7: Intelligence

- [ ] **Todo 37:** Stem-aware mixing DSP
- [ ] **Todo 38:** Automix transition engine
- [ ] **Todo 39:** Stem transitions
- [ ] **Todo 40:** 8-pad sampler

## Wave 8: Pro Surface

- [ ] **Todo 41:** Mix recording
- [ ] **Todo 42:** Broadcasting
- [ ] **Todo 43:** DVS (timecode vinyl)
- [ ] **Todo 44:** Video mixing

## Wave 9: Release

- [ ] **Todo 47:** Legal audit
- [ ] **Todo 48:** Trademark check
- [ ] **Todo 49:** Packaging
- [ ] **Todo 50:** Documentation
- [ ] **Todo 51:** v1.0.0 release

---

**Next Work:** Wave 2 Todos 7, 8, 10, 32/33 (MIDI), 26a (debug panel), Linux first.

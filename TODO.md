# OpenMixer TODO

OpenMixer is a free, open-source DJ application written in Rust with GPUI. The statuses below describe the current code. Todo numbers are stable identifiers.

## Before Wave 7

Checked against the code on 2026-10-19 (not against the older docs). Size: S = up to a day, M = a few days, L = a week or more.

### (A) Blockers for Wave 7

- **Foundation (Phase F): done.** Typed control surface (`ParamId`, `Command`, typed engine targets, string façade), pulled `EngineSnapshot`, `WorkerPool`, typed error enums with `AppError`, and `Services`/`MidiHost`. It clears no item below by itself; the items build on it.
- **Todo 19:** ONNX stems backend (`ort`) not built; `om-stems` has only `MockSeparator`, and nothing in the app calls the separator. L
- **Todo 37:** stem-aware mixing DSP not started (needs stems from 19). L
- **Todo 30 (with 39):** stem playback in the engine and the stem pads UI do not exist (no per-stem mute or volume anywhere in `om-engine` or `om-ui-gpui`). L
- **Todo 38:** Automix transition engine not started. Its inputs exist (tempo and grid 15, structure and mix-in/out 18, key 16). L
- **Todo 40:** 8-pad sampler: no sample engine, no pad UI. M
- **Library (Phase L, L1-L3): done.** Play history (`play_history`, a play at 30 s of playing time), hotcue save-back and restore on load, and cover art stored per track (`artwork`, front cover up to 512 KiB). Showing artwork in the UI is not built.
- **Todo 23:** FTS: search is LIKE-based; no FTS5 table in the migrations. S
- **Todo 25:** LookAndFeel: `ThemeTokens` exist in `om-ui::theme`, but nothing styles knobs, faders, menus, tabs or tooltips from them (no LookAndFeel type in the tree). M
- **Todo 26a:** debug panel load/play: `debug_panel.rs` only shows rows; no load or play control. S (or retire it when Wave 5 lands)

### (B) Quality and verification debt

- **Todos 26, 27, 28, 29, 31:** the GPUI window has never been viewed by eye (only launched and inspected with `xwininfo`). M, needs a native display run.
- **Todo 7:** cpal has never opened a real ALSA or PipeWire device (the device test skips on WSL). S, needs hardware.
- **Todo 7 / 13:** headphone-cue second stream failed on WSLg ("Invalid argument"), cause not isolated. M, verify on native Linux with two devices.
- **Todo 32:** no real MIDI device has been used; the ALSA backend is only smoke-tested. S, needs hardware.
- **Wave 1 CI:** macOS and Windows legs are `continue-on-error`. macOS `soak_short` exceeds its xrun budget on shared runners; the Windows `file_url` fix is not confirmed by a run. S-M
- **Todos 13 and 31:** the 10-minute soaks (`#[ignore]` in `om-audio` and `openmixer`) are never run in CI. Add a scheduled job. S
- **Todos 25-29:** the deferred GPUI paint and layout tests (see "Not yet covered by tests" in `docs/PROGRESS.md`); there is no `#[gpui::test]` in the tree. M
- **Wave 1 CI:** the `gui-check` and `gpui-lint` jobs have not run on a clean runner (non-blocking). S

### (C) Smaller gaps

- **Todo 29:** `--settings` start flag: not in `crates/openmixer/src`. S
- **Todo 29:** live scan-interval change: the interval is read when the library starts (`om-ui::settings`). S
- **Todo 29:** MIDI hot-plug refresh: `midi.rs` reconciles ports about once a second, but the MIDI page list is read on open and on page change only. S
- **Todo 10a:** plugin dynamic loading: open decision; `PluginLoader` reports every shared library as unsupported. S for the decision, L to implement
- **Todo 35:** HID layer: not started. L
- **Todo 36:** mapping editor: only learn start and cancel exist; no in-app editing of maps. M
- **Todo 34:** plug-and-play controller maps: `fixtures/controllers` holds test fixtures only. M (needs hardware to verify)
- **Todo 8b:** FFmpeg decode backend: not started (optional). M
- **Todo 24:** auto-enqueue after scan: the prep queue is fed only by a browser click and by track load (`browser.rs`, `track_loader.rs`); a scan does not enqueue. S
- **Todo 28:** playlist editing UI: the playlist DAO has create, rename, move, add and remove, but the sidebar only selects. M

Escape on the settings overlay is done (`settings_view.rs`, the `escape` key handler); it is not a gap, only the by-eye check is open.

## Spikes (done)

Spike A (GPUI window), spike B (cpal output callback) and spike C (time-stretch) and the decision record are in `docs/spikes/`. Decisions: `docs/spikes/DECISIONS.md`.

## Wave 1: Foundation (COMPLETE)

- [x] Repository initialization
- [x] Build system (Cargo workspace)
- [x] CI/CD pipeline (`rust.yml`: fmt, clippy, test, deny)
- [x] Governance documents
- [x] Legal framework
- [x] Test harness (`cargo test`)

## Wave 2: Audio Engine (vertical slice: make sound, control it, test it)

Reordered so audio can be heard and driven (mixer + MIDI + debug panel) before heavy dependencies.

- [x] **Todo 7:** Audio device layer + latency instrumentation + mock device (mock device and cpal code complete; the cpal device has never opened a real ALSA/PipeWire/WASAPI/CoreAudio device; the device tests are `#[ignore]`d; run `OPENMIXER_DEVICE_TESTS=1 cargo test -p om-audio --test cpal_device -- --ignored`)
  - Output via `cpal` (ALSA on Linux first; WASAPI and CoreAudio come with Windows and macOS support)
  - Mock device for CI testing
  - Open: the cpal device has not yet run on a real ALSA/PipeWire device (WSL has none; the device test skips there). See `docs/spikes/cpal.md`

- [x] **Todo 8:** Deck engine (streaming decode + transport)
  - Streaming decode via `symphonia`
  - Lock-free ring buffer
  - FFmpeg backend deferred (see Todo 8b)

- [x] **Todo 10:** Mixer (channels, EQ, filter, crossfader)
  - 3-band EQ
  - PFL cue bus

- [x] **Todo 10a:** Plugin API + registry (extension points)
  - `om-plugins` crate: versioned `Plugin` and `PluginHost` traits, registry with id, prefix and collision rules, unload order
  - Stage catalog over the engine insert chains (channel and master), MIDI maps, parameter targets and themes, all installed through the composition (`openmixer::plugins`)
  - Example plugin: `examples/plugins/gain-plugin`. Plugins are in-process for now; shared-library loading is a documented stub, and the choice (C ABI, WASM or compiled in) is open in `docs/spikes/DECISIONS.md`
  - No signing/allow-list gatekeeping (see CONTRIBUTING "Extensibility")

- [x] **Todo 32:** MIDI input + mapping engine (pulled forward from Wave 6)
  - MIDI input -> JSON map file -> mixer/deck parameters; controller feedback (LEDs) from engine state
  - Device layer: an ALSA sequencer backend on Linux (the `alsa` crate cpal already links, so midir does not join the build) and a mock backend for tests. macOS and Windows get a midir backend later, behind `system_backend` (see `docs/spikes/DECISIONS.md`)
  - Mapping editor UI stays in Wave 6 (Todo 36)
  - No test or CI job has exercised a real MIDI device. The ALSA backend is smoke-tested only for sequencer enumeration and skips without `/dev/snd/seq`. macOS and Windows have no backend (a stub returns a device error).

- [x] **Todo 33:** Controller trace harness (pulled forward from Wave 6)
  - Replay recorded MIDI streams headlessly to test mappings

- [ ] **Todo 26a:** Minimal debug panel (pulled forward from Wave 5)
  - 2 decks: load/play, faders, EQ knobs, level meters
  - Partial: `--debug-panel` shows device, latency, xruns, CPU and the raw parameter list; load/play from the debug panel is not wired (the panel shows device figures and parameter values only)
  - Plain widgets, no design tokens; replaced by Wave 5 UI

- [x] **Todo 13a:** Short soak test in CI (30-60s headless mix)
  - Soak mix scenario in `crates/om-audio/tests/soak.rs`
  - Full 10-minute run remains the Wave 2 gate (Todo 13)

- [x] **Todo 9:** Key-lock time-stretch via Rubber Band
  - Realtime mode
  - Golden audio tests
  - Decided: Rubber Band R3 via FFI in `om-stretch`, behind a `Stretcher` trait; no pure-Rust candidate passed the stereo CPU bar (see `docs/spikes/DECISIONS.md`). The `GPL-2.0-or-later` entry in `deny.toml` is enabled for it

- [x] **Todo 12:** Beat-sync + quantized cue/loop/slip
  - Engine side done (grid, sync, quantize, slip, gapless loops, hotcues); analysed beat grids (Todo 15) drive sync and quantize through `DjEngine::set_grid` (test: `analysed_grid_drives_sync_and_quantize_through_the_engine`)
  - Beat phase alignment
  - Control ids: `deck.N.sync`, `sync.master`, `loop.quantized.<beats>`, `slip`
  - Gapless deck loop playback and hotcues
  - Hotcue edits are saved per track and restored on load (`CueSaver`, `TrackLoader`)

- [x] **Todo 11:** FX rack (8 essential effects)
  - Effects: echo, reverb, flanger, phaser, crush, gate, roll, tape stop
  - Per-unit enable, wet/dry mix and effect parameters; `fxN.*` parameter ids
  - Beat-synced to the tempo grid
  - Routed per mixer channel and on master (`fx.master.*`, `fx.ch<N>.*`)
  - No allocation on the audio thread; buffer clears amortised
  - Follow-up: beat-phase lock, FX UI

- [ ] **Todo 8b:** FFmpeg decode backend (optional, after the above)

- [x] **Todo 13:** Engine soak test (10-minute headless mix)
  - Soak mix scenario in `crates/om-audio/tests/soak.rs`
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

- [ ] **Todo 19:** Stems precompute (Demucs ONNX) (partial)
  - Done in `om-stems`: `StemSeparator` trait, `MockSeparator`, JSON model manifest, licence-gated download with size and SHA-256 checks, licence digest remembered
  - Downloads: `file://`, `http://` and `https://` (ureq with rustls; the webpki root bundle is CDLA-Permissive-2.0, allowed in `deny.toml` with a comment). Tests stay offline
  - ONNX Runtime backend (`ort`) not yet built (open): the mock stands in for it

## Wave 4: Library & Data

- [x] **Todo 20:** Library schema + DAO layer
  - SQLite schema with migrations and transactions (`rusqlite`)
  - DAOs for tracks, cues, playlists
  - Play history: `play_history` table; a play is recorded when a deck has played its track for 30 s

- [x] **Todo 21:** Metadata ingest + watch folders
  - Metadata reader registry; RIFF INFO tags and duration
  - Tags for MP3/FLAC/M4A via `lofty`; front cover stored in `artwork` (up to 512 KiB, no re-encoding)
  - Recursive ingest and watch folders
  - Open: polling watch folders only; artwork is not shown in the UI

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
  - Partial: deck cards drawn and wired; the waveforms are the analysed peaks the library loader holds (blank until the track is analysed); the header BPM and key come from the loaded track and its grid. Not checked by eye; the main cue point is missing

- [ ] **Todo 27:** Mixer widgets + metering
  - Knobs, faders, meters
  - Partial: channel strips, master, crossfader, cue and FX page drawn with drag; not verified by eye

- [x] **Todo 28:** Library browser + app wiring
  - Tree, track list, search
  - Verified headlessly (model and glue tests, `--smoke`); the window has never been viewed by eye.
  - Done: the logic is in `om-ui::library`; the window draws the browser (sidebar tree, search box on the query syntax, paged sortable table with the Camelot tint, row selection, "Load to deck" buttons, double-click and Enter into the suggested deck, row drag onto a deck card, which loads it). The binary opens the library from the preferences and scans watch folders; analysis of loaded tracks runs on background workers and the table and deck refresh when results arrive. `--smoke` (gui) ingests three generated tracks, loads one into deck A and asserts the deck and the 3 table rows. Not checked by eye (the window was only launched and inspected with xwininfo). The settings library page drives the library (Todo 29)

- [ ] **Todo 29:** Automix panel + settings (partial: the Automix panel is Wave 7)
  - Audio/MIDI settings, preferences
  - Verified headlessly (model and glue tests, `--smoke`); the window has never been viewed by eye.
  - Done: versioned preferences, applier, `AppComposition`, audio/MIDI/library/plugins/appearance page models, `--smoke`, the settings overlay (Settings button in the header; Escape closes it; a text field at the foot of the card for paths), audio device state restore on start-up (`openmixer::audio_settings`). The MIDI page lists the inputs, toggles them, loads a map file by typed path, and starts and cancels MIDI learn with a status line. The library page adds and removes watch folders, sets the rescan interval and scan-on-start, rescans, and imports iTunes or Rekordbox XML. The Plugins page lists the loaded plugins, loads and unloads the built-ins (the gain example), and reloads the folder; plugin themes appear in Appearance. `--mock-device --smoke` prints the MIDI and plugin status lines. Overlay not checked by eye
  - Also open: `--settings` start flag; a live scan-interval change (read when the library starts); MIDI hot-plug refresh while the overlay is open (lists are read on open and on each page change); dynamic plugin loading (open decision); the Linux GPUI CI job is in place as `gui-check` in `.github/workflows/rust.yml` (non-blocking until its package list is verified)
  - Automix panel (Wave 7, needs Todo 38)

- [ ] **Todo 30:** Stem pads UI (open: waits on Wave 7 stem playback)
  - Per-stem mute/volume

- [x] **Todo 31:** UI verification gate (logic layer; window-driven pass open)
  - Scripted 10-minute mix driven through the UI
  - Done at the logic layer: `crates/openmixer/tests/ui_gate.rs` (8 s `#[test]`, 10-minute `#[ignore]`) drives the composition over a paced mock device with the om-ui transport and binding models. Not done: a window-driven pass (needs a display)
  - `--smoke` without the `gui` feature is headless (checked by `crates/openmixer/tests/smoke.rs`). The window-opening `--smoke` exists only behind `--features gui` and is not run in CI; `gui-check` only runs `cargo check` and the `om-ui-gpui` lib tests.

## Wave 6: Hardware I/O

- Todos 32/33 moved to Wave 2
- [ ] **Todo 34:** Plug-and-play controller maps (`fixtures/controllers` holds only test fixtures)
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

**Next:** the Before-Wave-7 list above, then Wave 7. Windows and macOS support comes after Linux (see `docs/spikes/DECISIONS.md`).

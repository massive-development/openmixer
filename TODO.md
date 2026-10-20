# OpenMixer TODO

OpenMixer is a free, open-source DJ application written in Rust with GPUI. The statuses below describe the current code. Todo numbers are stable identifiers.

## Before Wave 7

Checked against the code on 2026-10-19 (not against the older docs). Size: S = up to a day, M = a few days, L = a week or more.

### (A) Blockers for Wave 7

- **Foundation (Phase F): done.** Typed control surface (`ParamId`, `Command`, typed engine targets, string façade), pulled `EngineSnapshot`, `WorkerPool`, typed error enums with `AppError`, and `Services`/`MidiHost`. It clears no item below by itself; the items build on it.
- **Todo 19:** stem separation is built but unproven on real audio. `om-stems` has `OrtInference` (ONNX Runtime, feature `onnx`, tensor names and shapes checked at load), `HtDemucsPipeline` (segmenting, resampling, overlap-add), `StemService` (worker, cache, library index) and the app side `StemsHost` (model install, service created on first submit). The real htdemucs export (`tools/export_htdemucs.py`) has not been run or checked against the contract, the toy ONNX fixture is the only ONNX test, and nothing has been verified by ear. Open: the export and a real-model run, then a listening check. L
- **Todo 37:** done at engine level (Wave 7 S5): `StemMixer` and stem-mode decks, tested without audio. Stem transitions (39) are still open. L
- **Todo 30:** done at logic and view level (Wave 7 S6/S7): stem pads in the deck card, cached stems auto-load (`stems.autoLoad`), Load/Stems off, MIDI stem targets. Not checked by ear; mock device only. Transitions are Todo 39. L
- **Todo 38:** done at logic and view level (Wave 7 A1-A6): the Automix settings page (Todo 29), the main-window bar, the `--demo-automix` scene and the A6 safety tests. Not verified by ear (mock device only). Skip is not offered; the queue skips tracks that are not analysed yet. L
- **Todo 40:** 8-pad sampler: the engine (P1, P3: `om-engine::sampler`, `sampler.*` ids, snapshot, `SamplerAssign`), the sample bank (P2: `<data>/samples/sampler.json`, off-thread import, start-up apply), the pad view-model and sampler strip (P4: `om-ui::sampler_binding`, `om-ui-gpui::sampler_view`) and the soak and UI gate scenarios (P5) are built, at logic and view level. Not heard, and the window was checked on screenshots only, not by eye. Open: the user's listening and by-eye check; a per-pad mode and gain on MIDI; learn for pad settings. M
- **Library (Phase L, L1-L4): done.** Play history (`play_history`, a play at 30 s of playing time), hotcue save-back and restore on load, cover art stored per track (`artwork`, front cover up to 512 KiB), and free-text search over an FTS5 index (`tracks_fts`, migration 7; LIKE only when FTS5 is missing). Showing artwork in the UI is not built.
- **Todo 25:** LookAndFeel partial: `om_ui::style::LookAndFeel` derives the control styles from `ThemeTokens` and the gpui knobs, faders, buttons, meters, tooltips and drop tint draw from them. Scroll bars, tables and toggles are not restyled, and the window is not checked by eye. M
- **Todo 26a:** done: the debug column loads a typed path per deck (`TrackLoader::load_path`) and plays or pauses each deck through `DeckBinding`. It is still removed once Wave 5 replaces it.

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

- [x] **Todo 26a:** Minimal debug panel (pulled forward from Wave 5)
  - 2 decks: per-deck path load (refused on a playing deck unless forced) and play/pause, plus device figures and the raw parameter list
  - Not yet: faders, EQ knobs and level meters on the panel; the panel is replaced by Wave 5 UI

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
  - Wave 7 S1 (partial): stereo `StemSet` and a streamed `StemSeparator` v2 (`separate_to`); the htdemucs pipeline (resample to 44.1 kHz, 7.8 s segments with 25% triangular overlap-add, per-segment normalisation, Rust STFT/iSTFT) over a `StemInference` trait, tested with fakes; `OrtInference` (`ort` 2.0.0-rc.13, feature `onnx`, off by default) passing a toy-ONNX integration test; `tools/export_htdemucs.py` and the licence text
  - Wave 7 S2 (done, mock-tested): `StemService` on `WorkerPool` (one worker, FIFO): cache hit re-records the row; the model gate checks on disk that the model is installed and its licence accepted (no download); decode, separate into the cache sink, commit, record `track_stems`, enforce the cap. `forget_track` and `reconcile` keep the index and the directories in step. Cancel leaves no partial directory; a panicking separator fails only its job. Composition: `Services.stems`, `AppComposition::attach_stems`, `--mock-stems` (or `OPENMIXER_MOCK_STEMS`); with `onnx` and an installed model (`htdemucs.manifest.json` beside the library) the htdemucs separator is used. Nothing submits a job yet: the "Prepare stems" action and `stems.auto_prepare` are wired in S7/S8 UI work.
  - Wave 7 S3 (done): stem cache at `<library folder>/stems/<hash>-<sep>-v<n>/` (16-bit stereo WAV at the track's rate, `meta.json`), atomic `.tmp-*` writes, LRU by `meta.json` mtime, `stems.max_cache_gb` (default 20, preferences version 2); migration 8 `track_stems` and `TrackStemsDao`.
  - Wave 7 S4 (done, mock-tested): the Library settings page has a stems section: model state (Not installed, Licence not accepted, Downloading n%, Installed vX, Error) with the manifest's licence text in a scrolling box (it carries the MUSDB18-HQ training-data caveat), "Accept licence and download" (the answer is recorded by the licence gate before any network I/O; a declined answer never reaches the downloader, tested with a counting downloader), cancel, cache usage against the cap, the cap as a text field (`stems.max_cache_gb`), auto-prepare (`stems.auto_prepare`, off by default; fires only for tracks analysed after the library opened, never on load), and Clear stem cache behind a confirm step. The browser has a Stems column (None, Preparing n%, Ready, Failed), Prepare stems (button and `p`) and Cancel stems. A stems action with no model opens the Library settings licence section with the reason. `--mock-stems` stands in for an installed model (MockSeparator, no download) so the whole flow runs without the real model
  - Open: the real htdemucs model has not been exported, loaded or listened to (the exporter and the I/O contract are unverified against Meta's code), the hosted manifest (none is shipped: the app reads `models/htdemucs.manifest.json` beside the library), the window has not been viewed by eye, and the ort CPU share on real hardware

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
  - FTS5 (Phase L4): `tracks_fts` over title, artist, album, genre and comment, with sync triggers and a rebuild in migration 7. Free text is a substring match (LIKE, so 'ove' finds 'Love') OR-ed with the token-prefix match of the index, which folds diacritics ('emile' finds 'Émile'); a quoted phrase is one phrase; negation covers both. Field filters keep substring matching. A no-FTS5 build refuses to open a database with the FTS5 triggers.

- [x] **Todo 24:** Track prep queue
  - Persisted queue with order, priority and status
  - Bounded in-flight analysis; results written back to the library

## Wave 5: UI

- [x] **Todo 25a:** Theme/skin loader
  - JSON themes with built-in, user and plugin sources
  - Live theme switching; built-in themes embedded in the binary

- [ ] **Todo 25:** Design tokens + LookAndFeel
  - Original visual identity (no copied layouts or artwork)
  - Partial: `om_ui::style` derives control styles from the tokens (buttons, knobs, faders, tabs, tooltips, text fields, meters, panels, separators); the gpui controls use them, and a grep test blocks raw colour literals in views. Contrast is checked headlessly in both built-in themes
  - Not yet: scroll bars, tables, toggles and menus are not restyled; the window has not been checked by eye

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

- [ ] **Todo 29:** Automix panel + settings (the Automix tab and the main-window bar are in; the rest of this Todo is open)
  - Audio/MIDI settings, preferences
  - Verified headlessly (model and glue tests, `--smoke`); the window has never been viewed by eye.
  - Done: versioned preferences, applier, `AppComposition`, audio/MIDI/library/plugins/appearance page models, `--smoke`, the settings overlay (Settings button in the header; Escape closes it; a text field at the foot of the card for paths), audio device state restore on start-up (`openmixer::audio_settings`). The MIDI page lists the inputs, toggles them, loads a map file by typed path, and starts and cancels MIDI learn with a status line. The library page adds and removes watch folders, sets the rescan interval and scan-on-start, rescans, and imports iTunes or Rekordbox XML. The Plugins page lists the loaded plugins, loads and unloads the built-ins (the gain example), and reloads the folder; plugin themes appear in Appearance. `--mock-device --smoke` prints the MIDI and plugin status lines. Overlay not checked by eye
  - Also open: `--settings` start flag; a live scan-interval change (read when the library starts); MIDI hot-plug refresh while the overlay is open (lists are read on open and on each page change); dynamic plugin loading (open decision); the Linux GPUI CI job is in place as `gui-check` in `.github/workflows/rust.yml` (non-blocking until its package list is verified)
  - Automix settings page and bar: done at logic and view level (Todo 38); the rest of the Todo 29 list stays open

- [x] **Todo 30:** Stem pads UI (logic and deck-card view done, not verified by ear; the mock device plays nothing)
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

- [x] **Todo 37:** Stem-aware mixing DSP (engine level, Wave 7 S5: `om-dsp` `StemMixer`, stem-mode decks, `stem.*` ids and `Command::LoadStems`; pads UI is Todo 30, transitions Todo 39)
- [x] **Todo 38:** Automix transition engine: planner, controller, queue, loader host, settings page and bar done and tested without audio (Wave 7 A1-A6); not verified by ear
- [ ] **Todo 39:** Stem transitions: done at helper level (Wave 7 S8: `om-automix` `stem_fade`, the `StemFade` plan with bass swap, vocals out, drums-only bridge and filter sweep, and a `StemFadeRunner` that sends `Command::Param` on the control tick and skips decks without stems). Driven by the automix planner (A4: `BassSwap` and `StemBridge` styles read the start gains from the snapshot and fall back to `Crossfade` without stems) but by no UI or real deck yet; not heard
- [x] **Todo 40:** 8-pad sampler: engine, ids, snapshot and command (P1/P3); sample bank (P2); pad view-model and strip (P4); soak and UI gate scenarios (P5). Done at logic and view level; not heard, not seen by eye

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

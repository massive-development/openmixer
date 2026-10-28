# OpenMixer TODO

OpenMixer is a free, open-source DJ application written in Rust with GPUI. The statuses below describe the current code. Todo numbers are stable identifiers.

## Experience track

Runs alongside everything else; not gated on a wave (ADR 0005, `docs/PRODUCT.md`). Each item is done when it has been looked at on real music at 1280x800 and the minimum window, not when its tests pass. Size as below.

- **E1 Brand and visual design pass (L):** define the OpenMixer look (palette, type, depth, motion, an identity of our own that is not a copy of another program), then a type scale, depth and glow on active states, readable BPM, key and time, restrained colour for a dark room. Applies to both built-in themes. Tokens first, then controls.
  - Started 2026-10-23 (`docs/design/E1-brand.md`): brand, readout, glow, depth and motion tokens and a new deck-accent palette in both themes; lit-button glow and big BPM/key/time readouts in the deck header. Contrast-tested, seen in screenshots only. Done since: the Inter interface font is bundled, with tabular numerals (`om-ui-gpui::font`). Open: glow on knobs/faders/pads/tabs, card lift, ember playhead and brand mark, animating the motion tokens.
- **E2 Waveform that shows phrases (M):** bar numbers on the beat grid, cue and loop regions on the strip, structure hints (intro, drop, breakdown) from analysis, zoom levels that match a phrase. The spectrum colour is in (E10); judge it on more real tracks.
- **E3 Library as a performance tool (L):** artwork column, BPM and key shown against the playing track (compatible tracks stand out), history, playlists beside the browser, instant search. Bigger than the current table. Done so far: key relation and BPM-distance scoring against the playing track (`om-ui` `mix_match`), with a stronger row tint when key and tempo both fit. Left: artwork column (needs image decode and a texture cache), history view, playlist side panel, instant search.
- **E4 Rotary EQ and channel strip (M):** rotary knobs with a centre detent and a level meter beside them, replacing the bar sliders.
  - Started: knobs (`om-ui-gpui::knob_paint`) drive the strip's trim, EQ, filter, cue mix, phones, master and sampler gain and pitch; faders stay sliders. Not verified: a level meter beside each knob, and a look on a live display.
- **E5 Pads and sampler (M):** large, colour-coded, labelled by function; hotcues show their colour and name.
- **E6 Real-music review loop (S):** a fixture set of real, licence-clean tracks (or a documented way to point the demo at the user's own) so screenshots show real waveforms.
- **E7 Reliability for performance (M):** run the 10-minute soaks on a schedule (the weekly `soak` job is in `rust.yml`, not yet run), report latency honestly in the UI, test a four-deck set with stems.
- **E8 Second colour scheme (S):** a familiar, brand-aligned alternative scheme after E1, as theme JSON only.
- **E2a Progressive waveform (M):** play at once and let the waveform fill in while the file is read, like a buffering video: the analyser publishes partial `WaveformData` as it decodes, the strips draw what exists and show the rest as a dim placeholder, and BPM, key and the grid land when their analysis finishes. Today the waveform arrives all at once when the whole analysis is done (the deck itself already plays immediately).
- **E9 Smoothness (M):** measure first: frame times on a release build with real music, four decks and stems, at the display refresh rate (a frame-time overlay and a headless frame-time test). Then fix what the numbers show: per-frame work in the waveform strips, redraw only what changed, smooth animation of faders and meters, no flicker on state changes. Done when no frame goes over the budget in `docs/PRODUCT.md` over a ten-minute set.
  - Measured 2026-10-21 (`OPENMIXER_FRAME_STATS=1`, WSLg, `--demo-automix`, decks playing): release 60 fps, no hitches after start-up; debug 12 fps (the unoptimised GPUI window). The view builds in about 0.5 ms, so the cost is in painting. Fixed: dev builds now optimise dependencies and the view crates (`.cargo/config.toml`), giving about 59 fps with an occasional 33 ms frame. Not measured yet: real tracks, four decks, stems, the scrolling zoomed waveform on a real GPU. Paints about 3,000 quads a frame even when nothing moves (one per waveform pixel column): cache the strips if the numbers on real hardware need it. Start-up and track loads show one to five hitches in the first window.
- **E10 Spectrum waveform (L):** design in `docs/superpowers/specs/2026-10-23-spectrum-waveform-design.md`. Built, awaiting the maintainer on real music and a real display: FFT analysis to loudness, centroid and spread per point (`om-analysis`, format version 2, old cache entries rebuild), the pure colour function and per-theme hue curve (`om-ui::waveform`, `waveform` theme section), the overview and zoomed strip on it, the palette tests (`cargo test -p om-ui --test wave_palette`), the flicker tests (`--test wave_flicker`), the cost tests (`--test wave_budget`; the strict-numbers variant is `#[ignore]`, run it with `--release -- --ignored --nocapture`), and the tuning tool (`cargo run --release -p om-ui --example wave_strips -- TRACK...`, or `--synthetic`). Open: run the tool on two or three more full reference tracks (heavy bass, vocal-led, sparse) and look at the PNGs, run the real-track flicker check (`TRACK=<audio file> cargo test -p om-ui --test wave_flicker adjacent_columns_and_height_reference_on_a_real_track -- --ignored --nocapture`), and tune the curve by eye. There is no hue-span guard or reference-folder test in the tree (the old `wave_colour` test, `OPENMIXER_REFERENCE_TRACKS` and `MIN_HUE_SPAN_DEGREES` no longer exist), so a hue-range check on real tracks would have to be written first. Bar numbers and structure hints stay with E2, progressive fill-in with E2a.

## Before Wave 7

Checked against the code on 2026-10-19 (not against the older docs). Size: S = up to a day, M = a few days, L = a week or more.

### (A) Blockers for Wave 7

- **Foundation (Phase F): done.** Typed control surface (`ParamId`, `Command`, typed engine targets, string façade), pulled `EngineSnapshot`, `WorkerPool`, typed error enums with `AppError`, and `Services`/`MidiHost`. It clears no item below by itself; the items build on it.
- **Todo 19:** stem separation is built but unproven on real audio. `om-stems` has `OrtInference` (ONNX Runtime, feature `onnx`, tensor names and shapes checked at load), `HtDemucsPipeline` (segmenting, resampling, overlap-add), `StemService` (worker, cache, library index) and the app side `StemsHost` (model install, service created on first submit). The real htdemucs export ran on 2026-10-28 (STFT-outside variant; `tools/export_htdemucs.py`) and the Rust pipeline reproduces Demucs' own output on a reference segment (`crates/om-stems/tests/htdemucs_reference.rs`, the model test is `#[ignore]`d). Nothing has been verified by ear, and no hosted manifest or model file ships. Open: host the model, a real-track run, then a listening check. L
- **Todo 37:** done at engine level (Wave 7 S5): `StemMixer` and stem-mode decks, tested without audio. Stem transitions (39) are done and tested on the engine; not heard. L
- **Todo 30:** done at logic and view level (Wave 7 S6/S7): stem pads in the deck card, cached stems auto-load (`stems.autoLoad`), Load/Stems off, MIDI stem targets. Not checked by ear; mock device only. Transitions are Todo 39. L
- **Todo 38:** done at logic and view level (Wave 7 A1-A6): the Automix settings page (Todo 29), the main-window bar, the `--demo-automix` scene and the A6 safety tests. Not verified by ear (mock device only). A Skip button shows on the bar while a transition runs. Tracks the queue skips as not analysed are now submitted to the prep queue (once per track, up to 3 submissions, then `last_error`) and retried on later picks; not heard by ear. L
- **Todo 40:** 8-pad sampler: the engine (P1, P3: `om-engine::sampler`, `sampler.*` ids, snapshot, `SamplerAssign`), the sample bank (P2: `<data>/samples/sampler.json`, off-thread import, start-up apply), the pad view-model and sampler strip (P4: `om-ui::sampler_binding`, `om-ui-gpui::sampler_view`) and the soak and UI gate scenarios (P5) are built, at logic and view level. Not heard. The staged screenshots were checked by eye (`docs/screenshots/REVIEW.md`); a live display has not been checked. Open: the user's listening and by-eye check; a per-pad mode and gain on MIDI; learn for pad settings. M
- **Library (Phase L, L1-L4): done.** Play history (`play_history`, a play at 30 s of playing time), hotcue save-back and restore on load, cover art stored per track (`artwork`, front cover up to 512 KiB), and free-text search over an FTS5 index (`tracks_fts`, migration 7; LIKE only when FTS5 is missing). Cover art is shown in the deck card header (loaded on a worker, 16-entry cache, PNG and JPEG only, placeholder otherwise); there is no track detail view, the cache is not invalidated on rescan, and it has not been seen on a display. A library scan also enqueues new unanalysed tracks and changed tracks with a finished prep row for analysis.
- **Todo 25:** LookAndFeel partial: `om_ui::style::LookAndFeel` derives the control styles from `ThemeTokens` and the gpui knobs, faders, buttons, meters, tooltips and drop tint draw from them. `ScrollBarStyle`, `TableStyle` and `ToggleStyle` exist; the library table, its display-only scroll thumb and the automix Off|On switch draw from them. The platter switch, toggle pills, MIDI toggles, settings page and debug column scrolling are still on the old paths, and nothing is checked on a live display. S
- **Todo 26a:** partial: the debug column loads a typed path per deck (`TrackLoader::load_path`) and plays or pauses each deck through `DeckBinding` (tests: `track_loader`, `debug_panel::play_toggles_between_zero_and_one`). Faders, EQ and meters are not widgets on it (the raw parameter list shows their values as text). It is still removed once Wave 5 replaces it.

### (B) Quality and verification debt

- **Todos 25-29, 31:** the staged screenshots were checked by eye (`docs/screenshots/REVIEW.md`, rendered by WSLg software GL), but no live window on a real display has been checked, at 900x600 or otherwise. M, needs a native display run.
- **Todo 7:** cpal has never opened a real ALSA or PipeWire device (the device test skips on WSL). S, needs hardware.
- **Todo 7 / 13:** headphone-cue second stream failed on WSLg ("Invalid argument"), cause not isolated. M, verify on native Linux with two devices.
- **Todo 32:** no real MIDI device has been used; the ALSA backend is only smoke-tested. S, needs hardware.
- **Wave 1 CI:** the check matrix is Linux only; the macOS and Windows legs are not in `rust.yml` until the porting phase. S-M
- **Todos 13 and 31:** the 10-minute soaks (`#[ignore]` in `om-audio` and `openmixer`) are never run in CI. A weekly scheduled `soak` job in the rust.yml workflow now runs both; it has not run yet (no scheduled run is in the workflow history), so the result is unknown. S
- **Todos 25-29:** the deferred GPUI paint and layout tests (see "Not yet covered by tests" in `docs/PROGRESS.md`); there is no `#[gpui::test]` in the tree. M
- **Wave 1 CI:** the `check` job now builds and lints the GPUI crate with the rest of the workspace; it has passed on hosted (clean) `ubuntu-latest` runners in PR runs since the merge (for example run 40398196325, 2026-10-26, all jobs green). Only `pull_request` runs exist; nothing scheduled has run. S

### (C) Smaller gaps

- **Todo 10a:** plugin dynamic loading: built over a versioned C ABI (`libloading`, `PluginVTable`, `openmixer_plugin_entry`, `PluginLoader::load`; ADR in `docs/spikes/DECISIONS.md`). Not tried with a third-party plugin. Plugins must catch their own panics (a panic across `extern "C"` aborts); the vtable has no prepare, sample-rate or parameters yet. M
- **Todo 35:** HID layer: not started. L
- **Todo 34:** plug-and-play controller maps: `fixtures/controllers` holds test fixtures only. M (needs hardware to verify)
- **Todo 8b:** FFmpeg decode backend (optional): the `ffmpeg` cargo feature in `om-engine` (off by default) runs the `ffmpeg` binary (override `OPENMIXER_FFMPEG`) for files symphonia reports unsupported, wired only into `Sample::load` (sampler). Decks, stems and analysis have no fallback; the real-ffmpeg tests never ran (ffmpeg is not installed in the dev environment). M
- **Todo 28:** playlist editing UI: new, rename, delete (second press), add by drop on a sidebar playlist and remove tracks are built (`LibraryBrowser`, sidebar buttons); not checked by eye. Also built: reorder (drag a track onto a row of an open playlist, only while unsorted and unsearched), New folder (top level) and nesting by dragging a playlist or folder onto a folder, with a cycle guard pinned by test. The gpui drag and drop is compile-checked only, not seen on a display. A track added twice to one playlist is handled per entry (`dragging_a_duplicate_row_moves_that_entry` in `crates/om-ui/tests/library_browser.rs`). S

Escape on the settings overlay is done (`settings_view.rs`, the `escape` key handler); it is not a gap, only the by-eye check is open.

### (D) Maintainer verification (cannot be checked from the sandbox)

- Export `htdemucs.onnx` with `tools/export_htdemucs.py` (done 2026-10-28: the in-graph STFT does not export with current PyTorch, so the script exports the STFT-outside variant, inputs `mix` and `z`, outputs `stems` and `zout`, and the Rust pipeline matches Demucs on a reference segment). Still yours: host the file and its manifest, and listen to the result.
- Listening: stem seams and clicks, mute and solo, bass swap, key-lock with stems; sampler latency, clicks, loop latch and gate.
- An Automix transition on real decks: phase, EQ and bass swap, echo-out, and how cancel feels with real MIDI fader jitter. Decide whether Automix stays enabled across restarts.
- The GPUI window by eye at 900x600 and smaller, in both themes, on a real display (the E1 glow and the E3 row tint are guesses from software-render screenshots).
- The `onnx` CI job is green on hosted runners (run 40398196325, 2026-10-26); it is still `continue-on-error` in `rust.yml`, so decide whether to make it blocking.
- The macOS and Windows porting phase (root causes in `docs/spikes/DECISIONS.md`).

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
  - Example plugin: `examples/plugins/gain-plugin`. Plugins are in-process; shared-library loading over a C ABI is built (see Todo 10a under (C) below, and `docs/spikes/DECISIONS.md`)
  - No signing/allow-list gatekeeping (see CONTRIBUTING "Extensibility")

- [x] **Todo 32:** MIDI input + mapping engine (pulled forward from Wave 6)
  - MIDI input -> JSON map file -> mixer/deck parameters; controller feedback (LEDs) from engine state
  - Device layer: an ALSA sequencer backend on Linux (the `alsa` crate cpal already links, so midir does not join the build) and a mock backend for tests. macOS and Windows get a midir backend later, behind `system_backend` (see `docs/spikes/DECISIONS.md`)
  - Mapping editor UI stays in Wave 6 (Todo 36)
  - No test or CI job has exercised a real MIDI device. The ALSA backend is smoke-tested only for sequencer enumeration and skips without `/dev/snd/seq`. macOS and Windows have no backend (a stub returns a device error).

- [x] **Todo 33:** Controller trace harness (pulled forward from Wave 6)
  - Replay recorded MIDI streams headlessly to test mappings

- [ ] **Todo 26a:** Minimal debug panel (partial) (pulled forward from Wave 5)
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

- [ ] **Todo 13:** Engine soak test (10-minute headless mix; partial: the `#[ignore]` test exists and a weekly CI job is defined, but no 10-minute run has been recorded)
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
  - Open: the real htdemucs model has not been exported, loaded or listened to (the exporter and the I/O contract are unverified against Meta's code), the hosted manifest (none is shipped: the app reads `models/htdemucs.manifest.json` beside the library), the stems page has only been checked on a staged screenshot (`docs/screenshots/REVIEW.md`), and the ort CPU share on real hardware

## Wave 4: Library & Data

- [x] **Todo 20:** Library schema + DAO layer
  - SQLite schema with migrations and transactions (`rusqlite`)
  - DAOs for tracks, cues, playlists
  - Play history: `play_history` table; a play is recorded when a deck has played its track for 30 s

- [x] **Todo 21:** Metadata ingest + watch folders
  - Metadata reader registry; RIFF INFO tags and duration
  - Tags for MP3/FLAC/M4A via `lofty`; front cover stored in `artwork` (up to 512 KiB, no re-encoding)
  - Recursive ingest and watch folders
  - Open: polling watch folders only; cover art shows in the deck card header only

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
  - A finished library scan queues every track with no BPM that was never queued (`PrepQueue::enqueue_unanalysed`), so new files are analysed without a click

## Wave 5: UI

- [x] **Todo 25a:** Theme/skin loader
  - JSON themes with built-in, user and plugin sources
  - Live theme switching; built-in themes embedded in the binary

- [ ] **Todo 25:** Design tokens + LookAndFeel
  - Original visual identity (no copied layouts or artwork)
  - Partial: `om_ui::style` derives control styles from the tokens (buttons, knobs, faders, tabs, tooltips, text fields, meters, panels, separators); the gpui controls use them, and a grep test blocks raw colour literals in views. Contrast is checked headlessly in both built-in themes
  - Partly done: the library table, its scroll thumb and the automix switch are restyled (`ScrollBarStyle`, `TableStyle`, `ToggleStyle`). Not yet: the platter switch, toggle pills, MIDI toggles, the settings page and menus; nothing has been checked on a live display

- [ ] **Todo 26:** Main window + deck widgets
  - Waveforms, transport, hotcues, loops
  - Partial: deck cards drawn and wired; the waveforms are the analysed peaks the library loader holds (blank until the track is analysed); the header BPM and key come from the loaded track and its grid. Checked on staged screenshots (`docs/screenshots/REVIEW.md`), not on a live display; the main cue point is missing

- [ ] **Todo 27:** Mixer widgets + metering
  - Knobs, faders, meters
  - Partial: channel strips, master, crossfader, cue and FX page drawn with drag; checked on staged screenshots (`docs/screenshots/REVIEW.md`), not on a live display

- [x] **Todo 28:** Library browser + app wiring
  - Tree, track list, search
  - Verified headlessly (model and glue tests, `--smoke`); the browser is in the staged screenshots, checked by eye (`docs/screenshots/REVIEW.md`), not on a live display.
  - Done: the logic is in `om-ui::library`; the window draws the browser (sidebar tree, search box on the query syntax, paged sortable table with the Camelot tint, row selection, "Load to deck" buttons, double-click and Enter into the suggested deck, row drag onto a deck card, which loads it). The binary opens the library from the preferences and scans watch folders; analysis of loaded tracks runs on background workers and the table and deck refresh when results arrive. `--smoke` (gui) ingests three generated tracks, loads one into deck A and asserts the deck and the 3 table rows. The settings library page drives the library (Todo 29)

- [ ] **Todo 29:** Automix panel + settings (the Automix tab and the main-window bar are in; the rest of this Todo is open)
  - Audio/MIDI settings, preferences
  - Verified headlessly (model and glue tests, `--smoke`); the settings pages are in the staged screenshots, checked by eye (`docs/screenshots/REVIEW.md`), not on a live display.
  - Done: versioned preferences, applier, `AppComposition`, audio/MIDI/library/plugins/appearance page models, `--smoke`, the settings overlay (Settings button in the header; Escape closes it; a text field at the foot of the card for paths), audio device state restore on start-up (`openmixer::audio_settings`). The MIDI page lists the inputs, toggles them, loads a map file by typed path, and starts and cancels MIDI learn with a status line. The library page adds and removes watch folders, sets the rescan interval and scan-on-start, rescans, and imports iTunes or Rekordbox XML. The Plugins page lists the loaded plugins, loads and unloads the built-ins (the gain example), and reloads the folder; plugin themes appear in Appearance. `--mock-device --smoke` prints the MIDI and plugin status lines. The overlay's Audio, MIDI, Library, Appearance and Recording pages were checked on screenshots; the Plugins tab was not captured
  - `--settings[=Tab]` start flag, live scan-interval change and the MIDI input list refreshing about once a second while the MIDI page shows are done
  - Also open: dynamic plugin loading (open decision); the GPUI crate is built and linted by the `check` job in `.github/workflows/rust.yml`
  - Automix settings page and bar: done at logic and view level (Todo 38); the rest of the Todo 29 list stays open

- [x] **Todo 30:** Stem pads UI (logic and deck-card view done, not verified by ear; the mock device plays nothing)
  - Per-stem mute/volume

- [x] **Todo 31:** UI verification gate (logic layer; window-driven pass open)
  - Scripted 10-minute mix driven through the UI
  - Done at the logic layer: `crates/openmixer/tests/ui_gate.rs` (8 s `#[test]`, 10-minute `#[ignore]`) drives the composition over a paced mock device with the om-ui transport and binding models. Not done: a window-driven pass (needs a display)
  - `--headless --smoke` opens no window (checked by `crates/openmixer/tests/smoke.rs`, with and without the `gui` feature). The window-opening `--smoke` (no `--headless`) is not run in CI.

## Wave 6: Hardware I/O

- Todos 32/33 moved to Wave 2
- [ ] **Todo 34:** Plug-and-play controller maps (`fixtures/controllers` holds only test fixtures)
- [ ] **Todo 35:** HID layer
- [x] **Todo 36:** Mapping editor UI (logic and view level: edit, undo, conflicts, learn, save and import in the Mapping tab; not verified with hardware)

## Wave 7: Intelligence

- [x] **Todo 37:** Stem-aware mixing DSP (engine level, Wave 7 S5: `om-dsp` `StemMixer`, stem-mode decks, `stem.*` ids and `Command::LoadStems`; pads UI is Todo 30, transitions Todo 39)
- [x] **Todo 38:** Automix transition engine: planner, controller, queue, loader host, settings page and bar done and tested without audio (Wave 7 A1-A6); not verified by ear
- [x] **Todo 39:** Stem transitions: `om-automix` `stem_fade` (bass swap, vocals out, drums-only bridge, filter sweep), the `StemFadeRunner` driven by the controller, and the `BassSwap` and `StemBridge` styles in the planner (fall back to `Crossfade` without stems). The Automix settings page picks the style. Tested end to end on the real `DjEngine` with stem-mode decks on the mock device (`openmixer/tests/automix_stems.rs`: bass swap, stem bridge, fallback); not heard
- [x] **Todo 40:** 8-pad sampler: engine, ids, snapshot and command (P1/P3); sample bank (P2); pad view-model and strip (P4); soak and UI gate scenarios (P5). Done at logic and view level; not heard; checked by eye on staged screenshots only (`docs/screenshots/REVIEW.md`)

## Wave 8: Pro Surface

- [x] **Todo 41:** Mix recording: master-output tap, `rtrb` ring, writer thread, streaming WAV (16/24-bit, float32), crash-safe header, auto split, low-disk checks, `record.start|stop|toggle` ids, `record.*` state reads, Recording prefs and settings page logic, header button view-model. Done at logic and view level: the header Record button (`om-ui-gpui::record_button`, `om_ui::record_bar`), the Recording settings tab and the `r` shortcut are drawn, and `--demo-record` has screenshots. Unverified by ear (mock device only; no real device or listening check yet)
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

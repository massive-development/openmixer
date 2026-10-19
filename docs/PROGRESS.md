# OpenMixer Progress & Parity Tracker

Where the project stands against the goal: a clean-room, djay-Pro-class DJ application, being rewritten in Rust with GPUI.
`TODO.md` is the task list; this page explains **what each task entails, what exists today, and what is still missing**. The "Entails" column is the behavioural spec each Rust task must meet.

C++ baseline: The C++ implementation was removed in this commit range. It is preserved at the `cpp-final` tag (commit c6d2747), and the MIDI learn and feedback C++ is on `origin/wave6-midi-learn-feedback`.

Legend: ✅ done · 🟡 partial / in progress · ⬜ not started

_Last updated: Rust port at parity with the C++ baseline (Waves 1-6); Linux first; Windows/macOS porting phase and Waves 7-9 next. Update this file in the same PR that changes a task's status._

## At a glance

![OpenMixer progress heatmap](progress-heatmap.svg)

_Regenerate with `python3 tools/progress_heatmap.py` after editing `docs/progress.json`. Weighting: done = 1, partial = 0.5, in progress = 0.25._

| Wave | Area | Status |
|------|------|--------|
| 1 | Foundation | ✅ Repo, Cargo workspace, CI, test harness |
| 2 | Audio engine | 🟡 (debug panel 26a partial; 8b FFmpeg optional and open; 10-minute soak not in CI) |
| 3 | Analysis | 🟡 (stems ONNX backend, 19, open) |
| 4 | Library | 🟡 (crates, play history, artwork and FTS open) |
| 5 | UI | 🟡 (LookAndFeel 25 and stem pads 30 open; decks, mixer and settings partial; not checked by eye) |
| 6 | Hardware | 🟡 Parity (32 and 33 moved to Wave 2); 34-36 were never in the C++ baseline and are open |
| 7 | Intelligence | ⬜ Not in the C++ baseline |
| 8 | Pro surface | ⬜ Not in the C++ baseline |
| 9 | Release | ⬜ Not in the C++ baseline |

**Rust port status:** Rust port at parity with the C++ baseline (Waves 1-6); Linux first; Windows/macOS porting phase and Waves 7-9 next. See `docs/spikes/DECISIONS.md`.

**What you can do today:** build the Rust workspace (`cargo build`), run the tests (`cargo test --workspace`), check startup with `cargo run -p openmixer -- --smoke`, and with the `gui` feature open the GPUI window (`cargo run -p openmixer --features gui -- --mock-device`). Audio, analysis, library, MIDI and the UI are implemented; the wave sections say what is partial.

---

## Wave 1: Foundation ✅

| Task | Entails | Status |
|------|---------|--------|
| Repo, governance, legal | AGPLv3 licence, clean-room ADRs (ADR 0004 records the Rust + GPUI stack), contributing guide and code of conduct, asset provenance record, licence check | ✅ |
| Build system | Cargo workspace (resolver 2, shared lints that forbid `unsafe` outside the FFI crate), `rust-toolchain.toml`, `cargo deny` licence policy | ✅ |
| CI | `.github/workflows/rust.yml`: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --workspace` and `cargo deny` on Linux, macOS and Windows | ✅ green on Linux, macOS and Windows, plus `cargo deny` |
| Test harness | `cargo test`; `openmixer --smoke` mode (prints `smoke ok`, exit 0) checked by an integration test | ✅ |

**Missing:** packaging.

---

## Wave 2 — Audio engine 🟡

The Done and Missing cells below describe the Rust code (`crates/om-*`, `openmixer`) and agree with `TODO.md`. The C++ baseline is only the parity reference.

Goal: reliable, low-latency playback of two decks through a mixer, controllable by MIDI, testable headlessly.

| Todo | Entails | Done | Missing |
|------|---------|------|---------|
| 7 Audio device layer | Device abstraction over `cpal` with latency/xrun instrumentation and a headless mock device for CI | `AudioDevice` trait (`om-audio`) with a cpal backend (xrun counters, latency monitor in `om-engine`) and a mock device | Not yet run on a real ALSA or PipeWire device (the WSL device test skips); WASAPI and CoreAudio backends (porting phase) |
| 8 Deck engine | Streaming decode (`symphonia`), lock-free buffer, play/pause/seek/rate | Streaming decode with symphonia (`om-engine` deck source), SPSC ring to the audio callback, play, pause, seek, rate, loop prefetch slots | FFmpeg decode backend (Todo 8b, optional, open) |
| 10 Mixer | Channel gain, 3-band EQ, filter, crossfader (curves), PFL/cue bus, master + headphone out | Channel trim, 3-band EQ, sweep filter (`om-dsp`, `om-engine::mixer`), crossfader with linear and constant-power curves, PFL cue bus, master and headphone outputs | — |
| 10a Plugin API | In-process plugin API (`om-plugins`): plugin and host traits; registry with the id, prefix and collision rules and unload order; stage catalog over the engine insert chains; MIDI maps, parameter targets and themes installed through the composition; plugins settings page model; example plugin (`examples/plugins/gain-plugin`) | All of the above, in-process | Shared-library loading (a stub; open decision, see `docs/spikes/DECISIONS.md`) |
| 32 MIDI input + mapping | `midir` input → JSON map → engine parameters | Event model, JSON map with typed feedback rules, mapper with hot reload, learn, LED feedback, input and output routing over a backend trait; ALSA sequencer backend on Linux and a mock backend; wired into the composition (`om-midi`, `openmixer::midi`). Linux does not use `midir` (see TODO 32) | midir backend for macOS and Windows (behind `system_backend`); the MIDI settings page toggles inputs, loads a map by typed path and starts or cancels learn; no hot-plug refresh while the page is open |
| 33 Controller trace harness | Replay recorded MIDI streams headlessly | Trace read, write and replay through the mapper (`om-midi` trace module) | — |
| 26a Debug panel | Two decks, faders, EQ, meters on plain widgets | `--debug-panel` (with `gui`) shows device, latency, xruns, CPU and the parameter list | Load and play per deck (waits on the library wiring); removed once the Wave 5 UI replaces it |
| 13a Short soak test | 30–60 s headless mix in CI, asserting zero xruns, no steady-state underruns, no NaN/Inf/overflow | Short soak in `om-audio` (`tests/soak.rs`) with the `OutputChecker` decorator counting NaN, Inf and overflow, and an xrun budget | — |
| 9 Key-lock time-stretch | Tempo change without pitch change. Decided: Rubber Band R3 via FFI in `om-stretch`, behind a `Stretcher` trait; no pure-Rust candidate passed the stereo CPU bar (see `docs/spikes/DECISIONS.md`) | `Stretcher` trait; Rubber Band R3 through `rubberband-sys` (the only `unsafe` in `om-stretch`); key-lock in deck playback; `GPL-2.0-or-later` allowed in `deny.toml` | — |
| 12 Sync & quantize | Beat-phase alignment, quantized cue/loop, slip mode, gapless deck loops, 8 hotcues. Control ids `deck.N.sync`, `sync.master`, `deck.N.loop.quantized.<beats>`, `deck.N.slip` | Sync, quantizer and slip in the engine; gapless loops; hotcues; control ids as listed; transport buttons for sync, master, quantize and slip; analysed grids drive sync through `DjEngine::set_grid` | Saving engine hotcues back to the library is not confirmed in code (the cue DAO serves import) |
| 11 FX rack | 8 effects (echo, reverb, flanger, phaser, crush, gate, roll, tape stop) with wet/dry and beat-sync; one rack per mixer channel and one on master (`fx.ch<N>.*`, `fx.master.*`); no allocation on the audio thread, buffer clears amortised | `Effect` trait with `FxUnit` wrapper (enable, smoothed wet/dry, tail), `FxRack` per channel and master, name factory, built-in presets, beat-synced times; FX panel widget in `om-ui` | Beat-phase lock to the grid; FX panel not checked by eye |
| 13 Full soak test | 10-minute headless mix with a scripted DJ (loops, hotcues, key-lock toggles, master FX, seeks); glitch and xrun budget (zero xruns, no audio-thread allocation) | 10-minute run as an `#[ignore]` test in `om-audio` (`tests/soak.rs`) | Not in CI (10 minutes); paced mock device, not real hardware |

**Parity gap vs. pro DJ apps in this wave:** 4-deck support, recording-quality resampling, FX beat-phase lock.

---

## Wave 3 — Analysis 🟡

| Todo | Entails | Done | Missing |
|------|---------|------|---------|
| 14 Analysis service + cache | Background job runner with progress, cancel and wait; versioned analyzer registry; SQLite result cache keyed by file content hash | `AnalysisService` and `AnalysisJob` (submit, progress, cancel, wait; `Drop` cancels and joins); `AnalyzerRegistry` with interface versions; `SqliteResultCache` keyed by SHA-256 content hash, analyser id and version | — |
| 15 BPM + beat grid | Tempo estimation, beat grid with downbeat, manual correction (tempo markers); accuracy target: click tracks within 0.05 BPM and 3 ms | Tempo estimation and downbeat grid; editable `TempoMarker` segments; `AnalysedBeatGrid` implements `BeatGrid` and drives sync and quantize; tests check the 0.05 BPM and 3 ms targets on click tracks | Variable-tempo detection; other time signatures |
| 16 Key detection | Chroma-based key with Camelot mapping and harmonic compatibility; synthetic cadences in 8 keys detected | Chroma key estimation, Camelot mapping, harmonic compatibility | Tuning-offset estimation (assumes A440); key changes within a track |
| 17 Waveform model | Multi-resolution overview and zoom data; low/mid/high band data for colouring; coarser levels rebuilt on load | `WaveformData` with levels and band data (`om-analysis`); drawn in the deck view from analysed peaks (blank until a track is analysed) | Stereo and per-stem waveforms; progressive results while analysing; not checked by eye |
| 18 Structure features | Onset picking, energy curve, section boundaries with intro/main/breakdown/outro labels, mix-in/out points (feeds Automix) | Onset picking, energy, segmenter with intro, main, breakdown and outro labels, mix-in and mix-out points | Bar-aligned boundaries; timbre and repetition features; real-music evaluation |
| 19 Stems precompute | Partial (`om-stems`): separator trait, mock separator, JSON model manifest, licence-gated download of `file://`, `http://` and `https://` URLs (rustls), SHA-256 and size verified, licence digest remembered | `StemSeparator` trait and `MockSeparator`; JSON manifest; `Downloader` with licence gate, SHA-256 and size checks, licence digest remembered | ONNX Runtime backend (`ort`, not in the build); model export and hosted manifest; chunked overlap-add inference; stem storage keyed by content hash and separator version; UI licence dialog |

---

## Wave 4 — Library 🟡

| Todo | Entails | Done | Missing |
|------|---------|------|---------|
| 20 Schema + DAO | Tracks, playlists, crates, cues (hotcues and saved loops in frames), play history; forward-only migrations; WAL and foreign keys | `rusqlite` schema with forward-only migrations, WAL and foreign keys; DAOs for tracks, cues, playlists, watch folders and the prep queue; import runs in one transaction; the app opens the library from preferences | Crates and tags; a play-history table (only a `play_count` column); FTS index (Todo 23) |
| 21 Metadata ingest | Tag reading (ID3/Vorbis/MP4 and RIFF INFO), duration, artwork, recursive scan with content-hash change detection, missing-file flags, watch folders with background rescan | `MetadataReaderRegistry` with the built-in `lofty` reader (MP3, FLAC, M4A and others); duration; recursive scan with content-hash checks; watch folders on their own thread and connection, polling rescan | Artwork (read from tags, not stored: no artwork table); move detection by content hash; file-system change notifications (polling only) |
| 22 Import | iTunes and Rekordbox XML: ratings, play counts, playlists and folders, cues and loops from Rekordbox; single transaction with rollback; merge by path | iTunes and Rekordbox XML importers behind the `LibraryImporter` registry; neutral import model; one-transaction merge by path with rollback; Rekordbox cues imported; file URL decoding | Traktor and Serato importers; smart-playlist rules; artwork; path remapping when the library moved; Camelot from Rekordbox tonality (not confirmed in the importer) |
| 23 Search + filter | Text and field queries (`artist:foo bpm:120-128 key:~8A added:7d -genre:techno sort:-bpm limit:50`) compiled to bound parameters; registry of searchable fields that plugins can extend | Query parser with sort and limit; field registry; compiler to parameter-bound SQL; LIKE contains-match with escaped wildcards | FTS (LIKE is used); saved smart filters and smart crates; ranking |
| 24 Prep queue | Analyse-on-import: persisted order and priority, prioritise/move/requeue/cancel, crash recovery, bounded in-flight analysis, results (BPM, key) written back to the library | Persisted queue with order, priority and urgent enqueue; bounded in-flight analysis through the analysis service; BPM and key written back; the app timer pumps the queue | Auto-enqueue after ingest (tracks are queued when loaded); waveform and structure results not stored in the library; progress events |

---

## Wave 5 — UI 🟡

| Todo | Entails | Done | Missing |
|------|---------|------|---------|
| 25 Design tokens + LookAndFeel | Original visual identity (no copied layouts or artwork); runtime tokens for colour, spacing and type; a widget set covering knobs, faders, buttons, menus, text fields, toggles, tabs, scroll bars, table headers and tooltips, in dark and light themes; dB meters; translucent window option | `ThemeTokens` (colour, spacing, shape, type scale, widget metrics, translucency) in `om-ui::theme`; built-in dark theme and a light "daylight" theme | LookAndFeel that styles the widgets from tokens (knobs, faders, menus, tabs, scroll bars, tooltips); translucent window option |
| 25a Theme/skin loader | JSON themes validated on load (missing keys inherit the dark theme; bad values reject the file); built-in, user-directory and plugin sources with precedence user > plugin > built-in; live switching; `OPENMIXER_THEME` override | All of the above; built-in themes embedded in the binary | Layouts; file-watching of the user theme directory |
| 26 Main window + decks | Layout from data (two decks flank a mixer slot, library below; a 4-deck row optional); deck header (title, artist, BPM, key, elapsed/remaining); overview and zoomed waveforms with seek, grid lines, loop and cue markers; transport (play, cue, sync, master, key-lock, quantize, slip); pitch fader with ±8/16/50% ranges; 8 hotcue pads; loop size, halve/double, exit and reloop | Data-driven `main_layout`; deck cards in GPUI with header (title, BPM and key from the loaded track and its grid), waveforms, transport buttons, pitch fader, hotcue pads and loop controls wired through the control surface | A real main-cue point (Cue is "stop and return to start"); overview seek is not in the gate; 4-deck row; not checked by eye |
| 27 Mixer widgets + metering | Channel strips (trim, filter, 3-band EQ with kill, PFL, FX shortcut, L/Mid/R assign, fader, stereo peak meter with hold and clip); crossfader with Linear/Power curves; master and cue sections; FX panel per rack with unit select, enable, mix and knobs | Channel strips, master, crossfader (linear and constant-power), cue and FX panel drawn in GPUI with drag; stereo peak meter ballistics in `om-ui` | RMS meters; FX tabs hiding the strips; not verified by eye |
| 28 Library browser + wiring | Sidebar tree (all tracks, playlists, folders, watch folders, prep queue); search box with the query syntax and debounce; virtualised, paged, sortable table with a harmonic-compatibility tint against the playing deck; Load buttons, double-click, drag onto a deck; import via file chooser; track loader that fills waveform and grid from cache or from the prep queue; `OPENMIXER_LIBRARY_DB` override | Sidebar tree, search box on the query syntax, paged sortable table with the Camelot tint, row selection, Load buttons, double-click and drag onto a deck; track loader fills waveform and grid and refreshes when analysis lands; `--smoke` (gui) ingests generated tracks and loads one | Playlist editing; prep queue reorder and remove; auto-enqueue after a scan; import by file chooser (the settings page takes a typed path); window only launched and inspected, not checked by eye |
| 29 Automix panel + settings | Settings overlay with tabs for audio (driver, device, rate, buffer, master and headphone outputs, live latency/xrun/CPU), MIDI (per-input toggles, map file, learn), library (watch folders, rescan, scan interval, XML import), appearance (theme picker, plugin themes), plugins (folder, loaded list, reload, built-in load and unload); versioned `preferences.json` applied at start-up and on change | Versioned preferences, applier and `AppComposition`; audio, MIDI, library, plugins and appearance pages; overlay in the header (Escape closes); audio device restored at start-up; `--mock-device --smoke` prints the MIDI and plugin status lines | Automix panel (Wave 7, needs Todo 38); `--settings` start flag; live scan-interval change; MIDI hot-plug refresh while open; dynamic plugin loading (open decision); overlay not checked by eye |
| 30 Stem pads UI | Per-stem mute and volume | — | All; waits on Wave 7 stem playback |
| 31 UI verification gate | Scripted DJ operates the full window only through widgets (search and load, transport, crossfader, EQ, hotcues, loops, FX, theme switch) on a paced mock device; after each action the widget and engine must agree; xrun, underrun, NaN and overflow budgets; 10-minute opt-in run; `--smoke` opens the window and exits 0 (skipped without a display) | `crates/openmixer/tests/ui_gate.rs`: 8 s test plus a 10-minute `#[ignore]` run, driving the same composition as the binary through the widget and binding models; `--smoke` with `gui` | Window-driven pass (needs a display); overview seek, settings and drag-and-drop not in the scenario; stem pads not covered |

---

## Wave 6 — Hardware 🟡 (parity; 34-36 not in the C++ baseline)

| Todo | Entails | Done | Missing |
|------|---------|------|---------|
| 34 Controller maps | Plug-and-play maps for popular controllers | — | All (`fixtures/controllers` holds test fixtures only) |
| 35 HID layer | Non-MIDI controllers | — | All |
| 36 Mapping editor UI | In-app learn and edit | Learn start and cancel on the MIDI settings page | In-app editing of maps |

## Waves 7–9 ⬜ (not in the C++ baseline)

| Wave | Contents | Status |
|------|----------|--------|
| 7 | Stem-aware mixing, Automix, stem transitions, 8-pad sampler | ⬜ |
| 8 | Mix recording, broadcasting, DVS (timecode vinyl), video mixing | ⬜ |
| 9 | Legal audit, trademark check, packaging, docs, v1.0.0 | ⬜ |

## Deferred C++ test ports

Tests from the C++ suite that are not ported yet, each with the Todo that owns it. A line is removed when its port lands.

- `MeterBallisticsTest.DefaultsComeFromTheTheme` (`tests/unit/MeterBallisticsTest.cpp`): deferred to Todo 25 (design tokens + LookAndFeel). The meter defaults are read from the theme, which belongs to the widget layer. <!-- cpp-history -->
- `MeterBallisticsTest.MeterBarFollowsUpdatesPaintsAndClearsClipOnClick` (`tests/unit/MeterBallisticsTest.cpp`): deferred to Todo 27 (mixer widgets + metering). It paints the bar and handles the clip click, so it needs the widget. <!-- cpp-history -->
- `MeterBallisticsTest.StereoMeterFeedsEachSideSeparately` (`tests/unit/MeterBallisticsTest.cpp`): deferred to Todo 27 (mixer widgets + metering). It exercises the stereo meter widget. <!-- cpp-history -->
- `LibraryBrowserTest.SearchBoxTextIsVerticallyCentred` (`tests/unit/LibraryBrowserTest.cpp`): deferred to Todo 28 (library browser window). GPUI paint and layout; covered at model level only (the query text and the status line in `om-ui`). <!-- cpp-history -->
- `LibraryBrowserTest.BrowserAndTableRenderWithGlassTheme` (`tests/unit/LibraryBrowserTest.cpp`): deferred to Todo 28 (library browser window). GPUI paint in the glass theme; covered at model level only. <!-- cpp-history -->
- `LibraryBrowserTest.EveryToolbarButtonLabelFitsAndNothingOverlaps` (`tests/unit/LibraryBrowserTest.cpp`): deferred to Todo 28 (library browser window). GPUI layout of the toolbar; covered at model level only. <!-- cpp-history -->
- `ThemeRegistryTest.SwitchingUpdatesLookAndFeelColours` (`tests/unit/ThemeRegistryTest.cpp`): deferred to Todo 25 (design tokens + LookAndFeel). Needs the look-and-feel that paints the colours; covered at model level only (the registry's selection and listeners). <!-- cpp-history -->
- `ThemeRegistryTest.ThemeSwitchReachesLiveComponents` (`tests/unit/ThemeRegistryTest.cpp`): deferred to Todo 25 (design tokens + LookAndFeel). Checks live GPUI components after a switch; covered at model level only. <!-- cpp-history -->
- `SettingsPanelTest.ShowsOnePageAtATime` (`tests/unit/SettingsPanelTest.cpp`): deferred to Todo 29 (settings overlay). GPUI widget test of the page switcher; covered at model level only. <!-- cpp-history -->
- `SettingsPanelTest.ReflectAllPicksUpPreferencesChangedElsewhere` (`tests/unit/SettingsPanelTest.cpp`): deferred to Todo 29 (settings overlay). GPUI widget test of reflecting a preferences change; covered at model level only. <!-- cpp-history -->
- `SettingsPanelTest.CloseButtonEscapeAndClickOutsideDismiss` (`tests/unit/SettingsPanelTest.cpp`): deferred to Todo 29 (settings overlay). GPUI input handling of the overlay; covered at model level only. <!-- cpp-history -->
- `SettingsPanelTest.CardFitsTheWindowAndShrinksInSmallWindows` (`tests/unit/SettingsPanelTest.cpp`): deferred to Todo 29 (settings overlay). GPUI layout at window sizes; covered at model level only. <!-- cpp-history -->
- `SettingsPanelTest.EveryTabLaysOutInsideTheCardWithoutOverlap` (`tests/unit/SettingsPanelTest.cpp`): deferred to Todo 29 (settings overlay). GPUI layout of every tab; covered at model level only. <!-- cpp-history -->
- `SettingsPanelTest.RendersEveryTabInBothThemes` (`tests/unit/SettingsPanelTest.cpp`): deferred to Todo 29 (settings overlay). GPUI paint of every tab in both themes; covered at model level only. <!-- cpp-history -->

---

## Known out-of-scope / non-goals

- Spotify or other streaming-service integration (terms of service prohibit DJ mixing).
- Mobile and web apps.
- Real-time stem separation (stems are precomputed offline).
- Webview-based UI (no webview, Electron or Tauri).

## How to help

1. Pick a ⬜ or 🟡 row, open an issue naming the Todo number.
2. Read `CONTRIBUTING.md` and `docs/adr/0001-clean-room-doctrine.md` before writing code.
3. Include tests; engine code must not allocate or lock in audio callbacks.

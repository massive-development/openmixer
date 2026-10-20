# OpenMixer Progress Tracker

Where the project stands against the goal: a clean-room, djay-Pro-class DJ application, built in Rust with GPUI.
`TODO.md` is the task list; this page explains **what each task entails, what exists today, and what is still missing**. The "Entails" column is the behavioural spec each Rust task must meet.

Legend: ✅ done · 🟡 partial / in progress · ⬜ not started

_Last updated: 2026-10-20. Update this file in the same PR that changes a task's status._

## At a glance

![OpenMixer progress heatmap](progress-heatmap.svg)

_Regenerate with `python3 tools/progress_heatmap.py` after editing `docs/progress.json`. Weighting: done = 1, partial = 0.5, in progress = 0.25._

| Wave | Area | Status |
|------|------|--------|
| 1 | Foundation | ✅ Repo, Cargo workspace, CI, test harness |
| 2 | Audio engine | 🟡 (debug panel 26a partial; 8b FFmpeg optional and open; 10-minute soak not in CI) |
| 3 | Analysis | 🟡 (htdemucs model export and app wiring, 19, open) |
| 4 | Library | 🟡 (crates open) |
| 5 | UI | 🟡 (LookAndFeel 25 partial; stem pads 30 done at logic and view level, not checked by ear; decks, mixer and settings partial; not checked by eye) |
| 6 | Hardware | 🟡 (32 and 33 moved to Wave 2; 34-36 open) |
| 7 | Intelligence | ⬜ |
| 8 | Pro surface | ⬜ |
| 9 | Release | ⬜ |

**Current state:** Works, verified headlessly on Linux: the audio engine (two decks, mixer, FX racks, sync, key-lock) on cpal or a mock device; analysis (BPM, key, waveform, structure); the library (ingest, import, search, prep queue); MIDI (ALSA sequencer on Linux, mapping and feedback); in-process plugins; the GPUI app on Linux. Partial: the UI (decks, mixer, settings, debug panel), stems (the cache, the service and the settings and browser UI run on the mock separator; the htdemucs model is not installed or listened to yet), the hardware wave. The GPUI window has never been viewed by eye. Windows and macOS support is later. **Next:** the Before-Wave-7 list in `TODO.md`, then Wave 7. See `docs/spikes/DECISIONS.md`.

**What you can do today:** build the Rust workspace (`cargo build`), run the tests (`cargo test --workspace`), check startup with `cargo run -p openmixer -- --smoke`, and with the `gui` feature open the GPUI window (`cargo run -p openmixer --features gui -- --mock-device`). Audio, analysis, library, MIDI and the UI are implemented; the wave sections say what is partial.

---

## Wave 1: Foundation ✅

| Task | Entails | Status |
|------|---------|--------|
| Repo, governance, legal | AGPLv3 licence, clean-room ADRs (ADR 0004 records the Rust + GPUI stack), contributing guide and code of conduct, asset provenance record, licence check | ✅ |
| Build system | Cargo workspace (resolver 2, shared lints that forbid `unsafe` outside the FFI crate), `rust-toolchain.toml`, `cargo deny` licence policy | ✅ |
| CI | `.github/workflows/rust.yml`: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --workspace` and `cargo deny` on Linux, macOS and Windows | ✅ Linux green (required). macOS and Windows legs are non-blocking (`continue-on-error`): macOS `soak_short` exceeds its xrun budget on shared runners; the Windows `file_url` test fix is unconfirmed. See `docs/spikes/DECISIONS.md`, 'CI status by platform'. |
| Test harness | `cargo test`; `openmixer --smoke` mode (prints `smoke ok`, exit 0) checked by an integration test | ✅ |

**Missing:** packaging.

---

## Wave 2 — Audio engine 🟡

The Done and Missing cells below describe the Rust code (`crates/om-*`, `openmixer`) and agree with `TODO.md`.

Goal: reliable, low-latency playback of two decks through a mixer, controllable by MIDI, testable headlessly.

| Todo | Entails | Done | Missing |
|------|---------|------|---------|
| 7 Audio device layer | Device abstraction over `cpal` with latency/xrun instrumentation and a headless mock device for CI | `AudioDevice` trait (`om-audio`) with a cpal backend (xrun counters, latency monitor in `om-engine`) and a mock device | Not yet run on a real ALSA or PipeWire device (the WSL device test skips); WASAPI and CoreAudio backends (Windows and macOS support, later) |
| 8 Deck engine | Streaming decode (`symphonia`), lock-free buffer, play/pause/seek/rate | Streaming decode with symphonia (`om-engine` deck source), SPSC ring to the audio callback, play, pause, seek, rate, loop prefetch slots | FFmpeg decode backend (Todo 8b, optional, open) |
| 10 Mixer | Channel gain, 3-band EQ, filter, crossfader (curves), PFL/cue bus, master + headphone out | Channel trim, 3-band EQ, sweep filter (`om-dsp`, `om-engine::mixer`), crossfader with linear and constant-power curves, PFL cue bus, master and headphone outputs | — |
| 10a Plugin API | In-process plugin API (`om-plugins`): plugin and host traits; registry with the id, prefix and collision rules and unload order; stage catalog over the engine insert chains; MIDI maps, parameter targets and themes installed through the composition; plugins settings page model; example plugin (`examples/plugins/gain-plugin`) | All of the above, in-process | Shared-library loading (a stub; open decision, see `docs/spikes/DECISIONS.md`) |
| 32 MIDI input + mapping | `midir` input → JSON map → engine parameters | Event model, JSON map with typed feedback rules, mapper with hot reload, learn, LED feedback, input and output routing over a backend trait; ALSA sequencer backend on Linux and a mock backend; wired into the composition (`om-midi`, `openmixer::midi`). Linux does not use `midir` (see TODO 32) | midir backend for macOS and Windows (behind `system_backend`); the MIDI settings page toggles inputs, loads a map by typed path and starts or cancels learn; no hot-plug refresh while the page is open |
| 33 Controller trace harness | Replay recorded MIDI streams headlessly | Trace read, write and replay through the mapper (`om-midi` trace module) | — |
| 26a Debug panel | Two decks, faders, EQ, meters on plain widgets | `--debug-panel` (with `gui`) shows device, latency, xruns, CPU and the parameter list; per-deck path load (`TrackLoader::load_path`, refused on a playing deck unless forced) and play/pause through `DeckBinding` | Faders, EQ and meters on the panel; removed once the Wave 5 UI replaces it |
| 13a Short soak test | 30–60 s headless mix in CI, asserting zero xruns, no steady-state underruns, no NaN/Inf/overflow | Short soak in `om-audio` (`tests/soak.rs`) with the `OutputChecker` decorator counting NaN, Inf and overflow, and an xrun budget | — |
| 9 Key-lock time-stretch | Tempo change without pitch change. Decided: Rubber Band R3 via FFI in `om-stretch`, behind a `Stretcher` trait; no pure-Rust candidate passed the stereo CPU bar (see `docs/spikes/DECISIONS.md`) | `Stretcher` trait; Rubber Band R3 through `rubberband-sys` (the only `unsafe` in `om-stretch`); key-lock in deck playback; `GPL-2.0-or-later` allowed in `deny.toml` | — |
| 12 Sync & quantize | Beat-phase alignment, quantized cue/loop, slip mode, gapless deck loops, 8 hotcues. Control ids `deck.N.sync`, `sync.master`, `deck.N.loop.quantized.<beats>`, `deck.N.slip` | Sync, quantizer and slip in the engine; gapless loops; hotcues; control ids as listed; transport buttons for sync, master, quantize and slip; analysed grids drive sync through `DjEngine::set_grid` | Hotcue edits are saved per track and restored on load (`CueSaver`) |
| 11 FX rack | 8 effects (echo, reverb, flanger, phaser, crush, gate, roll, tape stop) with wet/dry and beat-sync; one rack per mixer channel and one on master (`fx.ch<N>.*`, `fx.master.*`); no allocation on the audio thread, buffer clears amortised | `Effect` trait with `FxUnit` wrapper (enable, smoothed wet/dry, tail), `FxRack` per channel and master, name factory, built-in presets, beat-synced times; FX panel widget in `om-ui` | Beat-phase lock to the grid; FX panel not checked by eye |
| 13 Full soak test | 10-minute headless mix with a scripted DJ (loops, hotcues, key-lock toggles, master FX, seeks); glitch and xrun budget (zero xruns, no audio-thread allocation) | 10-minute run as an `#[ignore]` test in `om-audio` (`tests/soak.rs`) | Not in CI (10 minutes); paced mock device, not real hardware |

**Gap vs. pro DJ apps in this wave:** 4-deck support, recording-quality resampling, FX beat-phase lock.

---

## Wave 3 — Analysis 🟡

| Todo | Entails | Done | Missing |
|------|---------|------|---------|
| 14 Analysis service + cache | Background job runner with progress, cancel and wait; versioned analyzer registry; SQLite result cache keyed by file content hash | `AnalysisService` and `AnalysisJob` (submit, progress, cancel, wait; `Drop` cancels and joins); `AnalyzerRegistry` with interface versions; `SqliteResultCache` keyed by SHA-256 content hash, analyser id and version | — |
| 15 BPM + beat grid | Tempo estimation, beat grid with downbeat, manual correction (tempo markers); accuracy target: click tracks within 0.05 BPM and 3 ms | Tempo estimation and downbeat grid; editable `TempoMarker` segments; `AnalysedBeatGrid` implements `BeatGrid` and drives sync and quantize; tests check the 0.05 BPM and 3 ms targets on click tracks | Variable-tempo detection; other time signatures |
| 16 Key detection | Chroma-based key with Camelot mapping and harmonic compatibility; synthetic cadences in 8 keys detected | Chroma key estimation, Camelot mapping, harmonic compatibility | Tuning-offset estimation (assumes A440); key changes within a track |
| 17 Waveform model | Multi-resolution overview and zoom data; low/mid/high band data for colouring; coarser levels rebuilt on load | `WaveformData` with levels and band data (`om-analysis`); drawn in the deck view from analysed peaks (blank until a track is analysed) | Stereo and per-stem waveforms; progressive results while analysing; not checked by eye |
| 18 Structure features | Onset picking, energy curve, section boundaries with intro/main/breakdown/outro labels, mix-in/out points (feeds Automix) | Onset picking, energy, segmenter with intro, main, breakdown and outro labels, mix-in and mix-out points | Bar-aligned boundaries; timbre and repetition features; real-music evaluation |
| 19 Stems precompute | Partial (`om-stems`): streamed separator trait, mock separator, htdemucs pipeline, ONNX backend (feature `onnx`), JSON model manifest, licence-gated download of `file://`, `http://` and `https://` URLs (rustls), SHA-256 and size verified, licence digest remembered | Stereo `StemSet`, streamed `StemSeparator` v2 and `MockSeparator`; `StemService` on `WorkerPool` (licence and installed-model gate, cancel, cap, forget) and the on-disk stem cache with migration 8 (S2, S3); model installer with the licence answer gating the download, and the stems UI (settings licence section, Prepare stems, Stems column, auto-prepare, cache clear) (S4); htdemucs pipeline (resampling, 7.8 s segments, overlap-add, normalisation, STFT/iSTFT) tested with fake networks; `OrtInference` behind the off-by-default `onnx` feature (`ort` 2.0.0-rc.13) with a toy ONNX model test; JSON manifest with a model contract; `Downloader` with licence gate, SHA-256 and size checks, licence digest remembered | The real htdemucs model: export script (`tools/export_htdemucs.py`) and I/O contract unverified, nothing listened to; real model unverified (export, load, listening); hosted manifest not shipped; stems UI unseen in the window; ONNX CI job unproven on a clean runner |

---

## Wave 4 — Library 🟡

| Todo | Entails | Done | Missing |
|------|---------|------|---------|
| 20 Schema + DAO | Tracks, playlists, crates, cues (hotcues and saved loops in frames), play history; forward-only migrations; WAL and foreign keys | `rusqlite` schema with forward-only migrations, WAL and foreign keys; DAOs for tracks, cues, playlists, watch folders and the prep queue; play history (`play_history`, one row per play recorded at 30 s of playing time); import runs in one transaction; the app opens the library from preferences | Crates and tags |
| 21 Metadata ingest | Tag reading (ID3/Vorbis/MP4 and RIFF INFO), duration, artwork, recursive scan with content-hash change detection, missing-file flags, watch folders with background rescan | `MetadataReaderRegistry` with the built-in `lofty` reader (MP3, FLAC, M4A and others); front cover stored in `artwork` (up to 512 KiB); duration; recursive scan with content-hash checks; watch folders on their own thread and connection, polling rescan | Move detection by content hash; file-system change notifications (polling only) |
| 22 Import | iTunes and Rekordbox XML: ratings, play counts, playlists and folders, cues and loops from Rekordbox; single transaction with rollback; merge by path | iTunes and Rekordbox XML importers behind the `LibraryImporter` registry; neutral import model; one-transaction merge by path with rollback; Rekordbox cues imported; file URL decoding | Traktor and Serato importers; smart-playlist rules; artwork; path remapping when the library moved; Camelot from Rekordbox tonality (not confirmed in the importer) |
| 23 Search + filter | Text and field queries (`artist:foo bpm:120-128 key:~8A added:7d -genre:techno sort:-bpm limit:50`) compiled to bound parameters; registry of searchable fields that plugins can extend | Query parser with sort and limit; field registry; compiler to parameter-bound SQL; free text is a substring match on the text columns (LIKE, so `ove` finds `Love`) OR-ed with an FTS5 token-prefix match over `tracks_fts` (migration 7, diacritics folded, so `emile` finds `Émile`; user text quoted so it is never query syntax); negation covers both; LIKE only when FTS5 is missing; field filters keep LIKE contains-match with escaped wildcards; a no-FTS5 build refuses to open a database with the FTS5 triggers (`Fts5Required`) | Saved smart filters and smart crates; ranking (no relevance sort yet) |
| 24 Prep queue | Analyse-on-import: persisted order and priority, prioritise/move/requeue/cancel, crash recovery, bounded in-flight analysis, results (BPM, key) written back to the library | Persisted queue with order, priority and urgent enqueue; bounded in-flight analysis through the analysis service; BPM and key written back; the app timer pumps the queue | Auto-enqueue after ingest (tracks are queued when loaded); waveform and structure results not stored in the library; progress events |

---

## Wave 5 — UI 🟡

| Todo | Entails | Done | Missing |
|------|---------|------|---------|
| 25 Design tokens + LookAndFeel | Original visual identity (no copied layouts or artwork); runtime tokens for colour, spacing and type; a widget set covering knobs, faders, buttons, menus, text fields, toggles, tabs, scroll bars, table headers and tooltips, in dark and light themes; dB meters; translucent window option | `ThemeTokens` in `om-ui::theme`; built-in dark and light "daylight" themes; `om_ui::style::LookAndFeel` derives every control style from the tokens; gpui knobs, faders, buttons, meters, tooltips and drop tint draw from it; headless contrast (3.0 control, 4.5 body) and live-switch tests in both themes; grep test against raw colour literals in views | Scroll bars, table headers, toggles and menus not restyled; window not checked by eye; translucent window option |
| 25a Theme/skin loader | JSON themes validated on load (missing keys inherit the dark theme; bad values reject the file); built-in, user-directory and plugin sources with precedence user > plugin > built-in; live switching; `OPENMIXER_THEME` override | All of the above; built-in themes embedded in the binary | Layouts; file-watching of the user theme directory |
| 26 Main window + decks | Layout from data (two decks flank a mixer slot, library below; a 4-deck row optional); deck header (title, artist, BPM, key, elapsed/remaining); overview and zoomed waveforms with seek, grid lines, loop and cue markers; transport (play, cue, sync, master, key-lock, quantize, slip); pitch fader with ±8/16/50% ranges; 8 hotcue pads; loop size, halve/double, exit and reloop | Data-driven `main_layout`; deck cards in GPUI with header (title, BPM and key from the loaded track and its grid), waveforms, transport buttons, pitch fader, hotcue pads and loop controls wired through the control surface | A real main-cue point (Cue is "stop and return to start"); overview seek is not in the gate; 4-deck row; not checked by eye |
| 27 Mixer widgets + metering | Channel strips (trim, filter, 3-band EQ with kill, PFL, FX shortcut, L/Mid/R assign, fader, stereo peak meter with hold and clip); crossfader with Linear/Power curves; master and cue sections; FX panel per rack with unit select, enable, mix and knobs | Channel strips, master, crossfader (linear and constant-power), cue and FX panel drawn in GPUI with drag; stereo peak meter ballistics in `om-ui` | RMS meters; FX tabs hiding the strips; not verified by eye |
| 28 Library browser + wiring | Sidebar tree (all tracks, playlists, folders, watch folders, prep queue); search box with the query syntax and debounce; virtualised, paged, sortable table with a harmonic-compatibility tint against the playing deck; Load buttons, double-click, drag onto a deck; import via file chooser; track loader that fills waveform and grid from cache or from the prep queue; `OPENMIXER_LIBRARY_DB` override | Sidebar tree, search box on the query syntax, paged sortable table with the Camelot tint, row selection, Load buttons, double-click and drag onto a deck; track loader fills waveform and grid and refreshes when analysis lands; `--smoke` (gui) ingests generated tracks and loads one | Playlist editing; prep queue reorder and remove; auto-enqueue after a scan; import by file chooser (the settings page takes a typed path); window only launched and inspected, not checked by eye |
| 29 Automix panel + settings | Settings overlay with tabs for audio (driver, device, rate, buffer, master and headphone outputs, live latency/xrun/CPU), MIDI (per-input toggles, map file, learn), library (watch folders, rescan, scan interval, XML import), appearance (theme picker, plugin themes), plugins (folder, loaded list, reload, built-in load and unload); versioned `preferences.json` applied at start-up and on change | Versioned preferences, applier and `AppComposition`; audio, MIDI, library, plugins and appearance pages; overlay in the header (Escape closes); audio device restored at start-up; `--mock-device --smoke` prints the MIDI and plugin status lines | Automix panel (Wave 7, needs Todo 38); `--settings` start flag; live scan-interval change; MIDI hot-plug refresh while open; dynamic plugin loading (open decision); overlay not checked by eye |
| 30 Stem pads UI | Per-stem mute, volume, filter, solo and load in the deck card | `om-ui` `StemBinding` (solo with restore, all-on, activity), `TrackLoader` auto-load of cached stems (`stems.autoLoad`, default on) and Load/Stems off toggle, `stem.<d>.<s>.*` MIDI targets and fixture; GPUI stem pads row with `--mock-stems` screenshot | Logic and view are tested; not verified by ear (no audio played, mock device only) |
| 31 UI verification gate | Scripted DJ operates the full window only through widgets (search and load, transport, crossfader, EQ, hotcues, loops, FX, theme switch) on a paced mock device; after each action the widget and engine must agree; xrun, underrun, NaN and overflow budgets; 10-minute opt-in run; `--smoke` opens the window and exits 0 (skipped without a display) | `crates/openmixer/tests/ui_gate.rs`: 8 s test plus a 10-minute `#[ignore]` run, driving the same composition as the binary through the widget and binding models; `--smoke` with `gui` | Window-driven pass (needs a display); overview seek, settings and drag-and-drop not in the scenario; stem pads not covered |

---

## Wave 6 — Hardware 🟡

| Todo | Entails | Done | Missing |
|------|---------|------|---------|
| 34 Controller maps | Plug-and-play maps for popular controllers | — | All (`fixtures/controllers` holds test fixtures only) |
| 35 HID layer | Non-MIDI controllers | — | All |
| 36 Mapping editor UI | In-app learn and edit | Learn start and cancel on the MIDI settings page | In-app editing of maps |

## Waves 7–9 🟡

Before Wave 7, the Foundation (typed control surface, pulled engine snapshot, typed errors, job queue) is done. The Before-Wave-7 (A) list in `TODO.md` is what remains.

| Wave | Contents | Status |
|------|----------|--------|
| 7 | Stem-aware mixing, Automix, stem transitions, 8-pad sampler | 🟡 (stem playback and `StemMixer` in the engine, S5; sampler engine and control surface, P1/P3, not heard; the rest not started) |
| 8 | Mix recording, broadcasting, DVS (timecode vinyl), video mixing | ⬜ |
| 9 | Legal audit, trademark check, packaging, docs, v1.0.0 | ⬜ |

### Wave 7 P1/P3: sampler engine and control surface (Todo 40, engine level)

`om-engine::sampler` has 8 pads of 4 voices each, preallocated. Samples decode off the audio thread (`Sample::load`, capped at 60 s / 64 MiB) and reach the audio side over an `rtrb` command ring; replaced samples return on a second ring and are freed on the control side. It mixes into the master bus before the master gain and inserts. Control: `SamplerControl`, `sampler.pad<N>.trigger|gate|volume|pitch` ids, `StateId::Sampler` reads, `EngineSnapshot.sampler`, `Command::SamplerAssign` through `DjEngine::command_sink`, and `fixtures/controllers/sampler_map.json`. Verified headlessly (render tests, an `assert_no_alloc` run with decks and all pads, a MIDI map test); not listened to. Open: sample storage (P2), the pad UI (P4) and a soak scenario (P5).

Cost (release build, `cargo test -p om-engine --release sampler_cost -- --ignored --nocapture`): per 256-frame block at 44.1 kHz, 12 microseconds with 8 voices, 47 microseconds with 32 voices, 69 microseconds with 32 voices at a fractional pitch (the block budget is 5.8 ms).

### Wave 7 S5: stem-aware deck playback (Todo 37, engine level)

| Piece | Behaviour | Tests | Missing |
|-------|-----------|-------|---------|
| `om-dsp` `StemMixer` / `StemMixerControl` | Four stems (L/R each) fold to stereo per source frame: gain 0 to 1.5 and mute through one-pole smoothers (10 ms, 8 ms), per-stem `SweepFilter` (-1 low-pass, 0 bit-exact bypass, +1 high-pass); control side is atomics, `Send + Sync` | Unity equals the plain sum (1e-6), mute fades without a click, bypass is bit-exact, gain step is monotonic, no subnormals | Filter position is applied per block, not smoothed |
| `Deck::load_stems(dir)` / `clear_stems()` / `stem_control()` | `dir/{drums,bass,vocals,other}.wav` (16-bit or float stereo, same rate, length within 16 frames) are decoded in lockstep into the one frame ring, 8 floats per frame; loop prefetch slots are 8 floats wide; the 8-to-2 mix happens at source-frame pull, before interpolation and key-lock, so a stem deck still has one interpolator and one stretcher. Seek, loop, hotcue, slip, quantize, sync, rate and key-lock are frame-index logic and are untouched. Errors (`DeckError`) leave the deck as it was. `load` of a new track leaves stem mode and resets gains, mutes and filters | Unity stems equal plain render (1e-4), mute drums equals a mix without drums, seek/loop/hotcue land on the same frames, rate moving every block, key-lock (finite, same level), underrun, error cases, `clear_stems` resumes at the same position, PCM16 stems | Switching mode costs a seek-sized gap; a stem file that ends early reads as silence; mute and filter changes are heard after the key-lock look-ahead (up to about 12 ms), not instantly |
| Engine surface | `stem.<d>.<s>.volume\|mute\|filter` typed target (volume is the linear gain, clamped 0 to 1.5; mute from 0.5; filter -1 to 1), `StateId::Stem` reads, `EngineSnapshot.stems`, `DjEngine::register_stem_set` + `command_sink` (`Command::LoadStems`, other commands go to a fallback sink or are rejected) | Router, snapshot, command sink and full-engine render tests | The app's router sink still rejects `LoadStems`; nothing wires a prepared set into a deck yet |
| Real-time guards | `assert_no_alloc` over a full `DjEngine::process`: deck A in stem mode with a loop, key-lock on both decks, FX on, every stem parameter moving each block, 200 blocks; stem soak variant (`soak_short_with_stems`: stem mutes, volumes and filters moving for 30 s, 0 xruns, 0 underruns) | Both pass | Not run on a real device |

Cost of the mix (release build, `cargo test -p om-dsp --release stem_mix_cost -- --ignored --nocapture`): 2.9 microseconds per 256-frame block with all filters bypassed, 4.3 microseconds with four active sweep filters (under 0.1% of the 5.3 ms a 256-frame block lasts at 48 kHz). Four-file decoding is on the decoder thread; the soak showed no underruns. The ring is allocated at eight floats per frame for every deck (2 MiB at the default size) and the loop prefetch slots at eight floats too (4 MiB per deck); plain mode still buffers the same number of frames as before.

### Soak xrun budget: one retry (review item 9)

`soak_short` and `soak_short_with_stems` (`crates/om-audio/tests/soak.rs`) allow the xrun budget (5 callbacks over the buffer's time budget) to be exceeded once: an over-budget run is repeated, and the test fails only when the repeat is over budget too. Shared runners stall for reasons the engine does not control, so one clean run out of two is the bar; two over-budget runs in a row still fail. Non-finite samples, overflow and steady-state underruns keep zero tolerance on both runs. Each run prints its xrun count and worst callback time, so a retry is visible in the log. The 10-minute soak keeps the single-run check.

## Not yet covered by tests

GPUI paint and layout tests that do not exist yet. Each is covered at model level only (the logic behind the widget is tested in `om-ui`). A line is removed when its test lands.

- Todo 25 (LookAndFeel): the gpui controls' paint is not checked by eye; a theme switch repaints live components (the style layer is tested, the paint is not).
- Todo 27 (mixer widgets): the meter bar paints, and clicking the clip indicator clears it; the stereo meter feeds each side separately.
- Todo 28 (library browser): the search box text is vertically centred; the browser and table render in the glass theme; every toolbar button label fits and nothing overlaps.
- Todo 29 (settings overlay): one page at a time; reflects preferences changed elsewhere; close button, Escape and click-outside dismiss; the card fits the window and shrinks in small windows; every tab lays out without overlap and renders in both themes.

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

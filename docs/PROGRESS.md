# OpenMixer Progress & Parity Tracker

Where the project stands against the goal: a clean-room, djay-Pro-class DJ application.
`TODO.md` is the task list; this page explains **what each task actually entails, what exists today, and what is still missing** — use it to find where to contribute.

Legend: ✅ done · 🟡 partial / in progress · ⬜ not started

_Last updated: after Wave 2 Todos 7–8. Update this file in the same PR that changes a task's status._

## At a glance

![OpenMixer progress heatmap](docs/progress-heatmap.svg)

_Regenerate with `python3 tools/progress_heatmap.py` after editing `docs/progress.json`. Weighting: done = 1, partial = 0.5, in progress = 0.25._

| Wave | Area | Status |
|------|------|--------|
| 1 | Foundation | ✅ |
| 2 | Audio engine | 🟡 device, deck (loops, hotcues, key-lock), mixer, FX rack, MIDI mapper + trace harness, debug panel done; sync/FX wiring remain |
| 3 | Analysis | 🟡 service + SQLite cache, BPM/beat grid, key, waveform, structure done; stems partial (no inference backend yet) |
| 4 | Library | 🟡 schema + DAOs, watch-folder ingest, iTunes/Rekordbox import, search, prep queue done; TagLib tags (ID3/Vorbis/MP4), artwork, crates/tags and play history missing |
| 5 | UI | 🟡 window, decks, mixer + FX, library browser, settings, theming and the UI gate done; stem pads (Todo 30, needs Wave 7 stem playback), Automix panel and several polish items remain |
| 6 | Hardware | ⬜ (basic MIDI pulled into Wave 2) |
| 7 | Intelligence | ⬜ |
| 8 | Pro surface | ⬜ |
| 9 | Release | ⬜ |

**What you can do today:** build and run the app: load files into two decks, play, mix with EQ/filter/crossfader/PFL, and drive it from a MIDI controller via a JSON map; run the unit tests.

---

## Wave 1 — Foundation ✅

| Task | Entails | Status |
|------|---------|--------|
| Repo, governance, legal | AGPLv3, clean-room ADRs, contributing/CoC, asset provenance CSV, `tools/license_check.py` | ✅ |
| Build system | CMake + JUCE 8.0.4 via FetchContent, Linux/macOS/Windows settings, presets | ✅ (Linux verified; macOS/Windows untested) |
| CI | `.github/workflows/ci.yml` | 🟡 present; not yet confirmed green on all platforms |
| Test harness | `juce::UnitTest` runner, `ctest` integration, `--smoke` app mode | ✅ |

**Missing:** verified macOS and Windows builds; packaging.

---

## Wave 2 — Audio engine 🟡

Goal: reliable, low-latency playback of two decks through a mixer, controllable by MIDI, testable headlessly.

| Todo | Entails | Done | Missing |
|------|---------|------|---------|
| 7 Audio device layer | Device abstraction, latency/xrun instrumentation, headless mock for CI | ✅ `AudioDevice`, `LatencyMonitor`, `JuceAudioDevice`, `MockAudioDevice` | Real-hardware test of `JuceAudioDevice`; device selection/settings UI |
| 8 Deck engine | Streaming decode, lock-free buffer, play/pause/seek/rate | ✅ `Deck`, `RingBuffer`; JUCE decoders (WAV/AIFF/FLAC/OGG, MP3 where JUCE supports it) | FFmpeg backend (8b) for wide codec support; scratching |
| 10 Mixer | Channel gain, 3-band EQ, filter, crossfader (curves), PFL/cue bus, master + headphone out | ✅ | Headphone routing to a second physical device; EQ/filter smoothing |
| 10a Plugin API | Versioned plugin interfaces, runtime loader, registry; hooks for audio pre/post-processing, FX, MIDI maps | ✅ `src/plugins/` (`openmixer_plugins`): `PluginApi`, `PluginLoader`, `PluginRegistry`, `StageCatalog`; example plugin | Per-deck insert hook (decks use channel inserts); app/UI wiring and plugin-folder settings; plugin-provided analysis/importers/themes (later waves) |
| 32 MIDI input + mapping | `MidiInput` → JSON map → engine parameters | ✅ | Learn mode; feedback/LED output (Wave 6) |
| 33 Controller trace harness | Replay recorded MIDI streams headlessly | ✅ | Real controller recordings |
| 26a Debug panel | Two decks, faders, EQ, meters on plain JUCE widgets | ✅ (not visually verified: no display in CI box) | Removed in Todo 28 (main window + library browser replace it) |
| 13a Short soak test | 30–60 s headless mix in CI | ✅ `tools/soak_mix.cpp` (`openmixer_soak`, CTest `soak_short`, label `soak`): 2 looping decks, moving controls, asserts 0 xruns, 0 steady-state underruns, no NaN/Inf/overflow | Runs on a paced mock device, not real hardware; not yet in CI (workflow owned elsewhere) |
| 9 Key-lock time-stretch | Rubber Band realtime, tempo change without pitch change | ✅ Rubber Band 4.0.0 (R3 engine, realtime) via FetchContent single-file build (`openmixer_rubberband`, -O2 even in Debug); `TimeStretch` wrapper; `Deck::setKeylock` / id `keylock`; pitch held under rate change and device/file rate mismatch; tests measure pitch by zero crossings | Toggling key-lock off skips ahead by the stretcher latency (~tens of ms); end-of-track tail (latency) not flushed; per-deck CPU not yet profiled (Todo 13 soak); R2/R3 engine choice not user-selectable |
| 12 Sync & quantize | Beat-phase alignment, quantized cue/loop, slip mode | 🟡 `BeatGrid` interface + `FixedTempoGrid`, `Sync` (tempo match + beat/bar phase nudge, `ParameterTarget`), `Quantizer` (snap, quantized loop bounds), `SlipTracker`; tested with mock grids; `Deck` gapless loops (sample-accurate wrap into a decoder-prefetched `LoopPrefetch` slot, phase-kept resize, exit/reloop) and 8 hotcues (`HotcueBank`, ids `hotcueN`, `hotcueN.clear`, `loop.exit`, `loop.reloop`) | Wired into `DjEngine` (`DeckControl`, router `deck.<N>`; `Sync` also at `sync`): `deck.N.sync`, `deck.N.sync.master`, `deck.N.loop.quantized.<beats>` (e.g. `…quantized.0.5`), `deck.N.slip`; other deck ids pass through to `Deck`. `DjEngine::setGrid(deck, grid)` is the plug-in point for analysed grids; `DjEngine::updateControl(dt)` (30 Hz timer in the app) runs `Sync::apply`, FX tempo and slip. Open: feeding analysed grids into decks on load (Wave 3 pipeline), UI buttons, slip is manual (not auto-started by loops), hotcue persistence (Wave 4 library) |
| 11 FX rack | 8 effects (delay, reverb, flanger, …) with wet/dry and beat-sync | ✅ `src/engine/fx/` (`openmixer_fx`): echo, reverb, flanger, phaser, crush, gate, roll, tape stop; `FxUnit` (enable, smoothed mix, tails, idle) and `FxRack` as `ProcessStage` inserts; `FxFactory` registry; 8 original presets; ids `bpm`, `fxN.enable/mix/<param>`; `FxTest`. Wired into `DjEngine` (`FxRouting`): one rack per mixer channel (insert slot 0) and one on master, router prefix `fx`: `fx.master.fxN.enable\|mix\|<param>`, `fx.ch<N>.fxN.…`, `fx.master.bpm`, `fx.ch<N>.bpm` (e.g. `fx.ch1.fx2.enable`); rack tempo follows the deck grid x rate (master: sync leader, else first deck with a grid) via `DjEngine::updateControl`. A unit whose tail ends wipes its stale buffers in `Wipe::kSlice` (512) frames per 64-sample chunk, idle or engaged (`Effect::beginReset/clearSome`, `fx/Wipe.h`): slots not yet wiped read as zero, so re-enabling mid-wipe is safe and a callback clears at most 512 frames per 64-sample chunk per unit, i.e. (blockSize/64)*512 (Echo, Flanger, Reverb all sliced; Roll and TapeStop reset only counters and never read past what they recorded, so they are O(1)); `EngineWiringTest` | Not yet in the UI; LFO/gate phase free-runs from engage (no beat-phase lock to the grid yet); MIDI value scaling for natural-unit params |
| 13 Full soak test | 10-minute headless mix, glitch/xrun budget | ✅ `soak_10min` (opt-in: `-DOPENMIXER_LONG_SOAK=ON`, `ctest -L soak_long`). A scripted DJ (`tools/soak/Scenario.h`) moves mixer controls and rates and drives loops, hotcues, key-lock toggles, master FX and seeks. Previous run: 112,508 callbacks, 1 xrun, avg CPU 17.9%, max callback 7.7 ms | Not in CI yet (10 min); paced mock device, not real hardware; after the sliced-wipe fix (Todo 11) a fresh run (linux-debug, 2026-10-17): 112,504 callbacks, 0 xruns (budget 5), 0 steady-state underruns, 0 non-finite, 0 overflow, avg CPU 12.7%, max callback 2.99 ms (was 1 xrun, 7.7 ms, 17.9%). The scenario now also flips channel racks via `fx.ch<N>.*` ids |

**Parity gap vs. pro DJ apps in this wave:** sync wiring, FX UI/wiring, 4-deck support, recording-quality resampling.

---

## Wave 3 — Analysis 🟡

| Todo | Entails | Missing |
|------|---------|---------|
| 14 Analysis service + cache ✅ | Background job runner, SQLite result cache keyed by file hash | Done: `openmixer_analysis` with `AnalysisService` (worker pool, FIFO, per-job progress/cancel/wait, listener), `AnalyzerRegistry` (versioned `Analyzer` extension point), `SqliteResultCache` (SQLite 3.46 amalgamation via FetchContent; key = SHA-256 of file bytes + analyser id + version), `FileClipSource`/`MemoryClipSource`. Missing: app wiring (cache path in user data dir), priority queue / prep-queue integration (Todo 24) |
| 15 BPM + beat grid ✅ | Tempo estimation, grid with downbeat, manual correction data model | Done: `BpmEstimator` (log spectral-flux onsets -> autocorrelation tempo -> tempo/phase grid search -> least-squares onset fit; downbeat from 30-200 Hz band; octave folding into 70-180 BPM), `BeatGridModel` (tempo markers, shift/setBpm/setDownbeatNear/insertTempoChange, JSON, manual flag), `AnalysedBeatGrid` adapter to engine `BeatGrid`, `BpmAnalyzer` (id `bpm`). Click tracks: BPM within 0.05, beats within 3 ms. Missing: automatic variable-tempo detection (manual markers only), time signatures other than the configured beats-per-bar, accuracy benchmark on real music, wiring grids into decks/UI |
| 16 Key detection ✅ | Chroma-based key, Camelot mapping | Done: `Chromagram` (8192-pt STFT, 100-5000 Hz bins folded to 12 pitch classes, per-frame normalised), `KeyEstimator` (Pearson correlation with 24 rotations of project-designed major/minor templates, confidence = margin), `MusicalKey` (names, Camelot code both ways, relative key, harmonic compatibility), `KeyAnalyzer` (id `key`). Synthetic cadences in 8 keys (incl. with drums + noise) detected. Missing: tuning-offset estimation (assumes A440), key changes within a track, real-music benchmark |
| 17 Waveform model ✅ | Multi-resolution overview/zoom data, colour/band data | Done: `WaveformBuilder` (24 dB/oct biquad crossovers at 250 Hz / 4 kHz via `openmixer_dsp`; 128-sample base points with min/max + low/mid/high RMS), `WaveformData` (x4 levels down to ~2048-point overview, `levelFor(samplesPerPixel)`, 5 bytes/point base64 JSON, coarser levels rebuilt on load), `WaveformAnalyzer` (id `waveform`). Missing: stereo/per-stem waveforms, rendering (Wave 5), progressive (partial) results while analysing |
| 18 Structure features ✅ | Onset/energy/section detection (feeds Automix) | Done: `OnsetPicker` (adaptive-threshold peak picking on the shared onset envelope), `Segmenter` (0.5 s loudness curve over a 40 dB range; boundaries where the 8 s mean before/after differs most, min 8 s apart; labels intro/main/breakdown/outro relative to the loudest section), `StructureAnalyzer` (id `structure`, onsets + energy + sections + mix-in/out). Missing: bar-aligned boundaries (snap to the beat grid), timbre/repetition features (chorus vs verse), real-music evaluation |
| 19 Stem precompute 🟡 | Demucs ONNX, weights downloaded on first use with license shown | Done: `openmixer_stems` with versioned `StemSeparator` extension point + `StemSet`, `ModelManifest` (JSON: id, url, sha256, size, licence name/text), `WeightManager` (licence prompt before any download, acceptance remembered per licence text, `.part` download, size + SHA-256 check, verified marker; injectable `Downloader`, `UrlDownloader`). Missing: ONNX Runtime dependency (prebuilt per platform, MIT) behind a CMake option; Demucs (MIT) model export to ONNX and a hosted manifest; chunked overlap-add inference backend; stem storage (rendered stems keyed by content hash + separator version) and an `Analyzer`/prep-queue hook; UI licence dialog |

## Wave 4 — Library 🟡

| Todo | Entails | Missing |
|------|---------|---------|
| 20 Schema + DAO ✅ | Tracks, playlists, crates, history | Done: `openmixer_library` (`src/library/`): `Database` (RAII SQLite, WAL, foreign keys, read-only mode), `Statement`, nesting `Transaction` (savepoints), forward-only `Migrations` (`schema_version`), `TrackDao` (CRUD, upsert by path, analysis/rating/play writers), `CueDao` (hotcues + saved loops in engine frames), `PlaylistDao` (folders, ordered entries, move/remove with dense positions, cycle check), `Library` facade; `LibraryDatabaseTest`, `LibraryDaoTest`. Missing: crates/tags, play history table, FTS index (Todo 23), app wiring (DB path in user data dir), hotcue load/save hooks into `Deck` |
| 21 Metadata ingest 🟡 | Tag reading, artwork, watch folders | Done: `MetadataReader` extension point + `MetadataReaderRegistry` (later registration wins per extension), `JuceMetadataReader` (duration for every JUCE-decodable format, RIFF INFO tags for WAV, file-name title fallback), `Ingestor` (recursive scan, SHA-256 via `analysis::hashFileContent`, mtime+size skip, upsert keeps rating/cues, files that vanish are flagged missing and restored if they return; unreadable folder or failed read never flags anything), migration v2 (`track_files`, `watch_folders`), `WatchFolders` (persisted list, background rescan thread with injectable clock, manual trigger, own DB connection); `LibraryIngestTest`, `LibraryWatchFoldersTest`. Missing: TagLib reader (ID3/Vorbis/MP4 tags for MP3/FLAC/M4A; JUCE exposes none of them), artwork extraction, move detection by content hash, file-system change notifications (polling only), app wiring |
| 22 Import ✅ | iTunes / Rekordbox XML | Done: `LibraryImporter` + `LibraryImporterRegistry` (recognises by XML root, later registration wins), `ItunesImporter` (plist tracks, ratings 0-100 -> stars, play counts, dates, playlists/folders; remote and location-less tracks skipped), `RekordboxImporter` (COLLECTION/TRACK, TEMPO, POSITION_MARK cues/loops, PLAYLISTS NODE tree, KeyType 0/1), `fileUrlToPath` (percent-decoding, localhost, drive letters, UNC), `ImportWriter` (one transaction, rollback on any DB error, dedupe by path, merge policy in `ImportMerge.h`, playlists merge by name), `importLibraryFile`, `ImportReport` (added/merged/skipped/errors); `TrackDao::setPlayStats`; `LibraryImportTest`, fixtures in `tests/data/import`. Missing: Traktor/Serato importers, smart-playlist rules, artwork, path remapping when the library moved between machines, Camelot derivation from Rekordbox Tonality, no schema change (memory cues stored in slots 8+) |
| 23 Search/filter ✅ | Text, BPM/key range, smart filters | Done: `SearchFieldRegistry` (built-in title/artist/album/genre/comment/path/year/bpm/rating/plays/duration/added/played/key/playlist; plugins `add()` fields), `parseQuery` (`artist:foo bpm:120-128 key:~8A added:7d -genre:techno sort:-bpm limit:50`), `compileSearch` (bound parameters only), `SearchEngine` (search/count with `juce::Result*` status), migration v3 indexes; `LibrarySearchTest`. Missing: FTS5 (not enabled in the amalgamation build; LIKE contains-match used), saved smart filters / smart crates, ranking |
| 24 Prep queue ✅ | Analyse-on-import pipeline | Done: `PrepQueueDao` (migration v4 `prep_queue`: user order, priority, status, error; `prioritise` jumps ahead, `move` reorders, requeue, crash recovery resets Running), `PrepQueue` (submits at most `maxInFlight` jobs to `AnalysisService`, ordering stays editable until submit; finished jobs go through a mutex-guarded inbox and `pump()` applies them on the Library-owning thread; cancel/cancelAll/remove; observer on the owner thread; bpm + key/camelot written back via `applyPrepResults`, atonal/unusable results leave stored values), `LibraryPrepQueueTest`. Missing: auto-enqueue after ingest, waveform/structure results are cached by the service but not stored in the library, per-tick progress events (UI polls `list()`), app wiring (timer calling `pump()`) |

## Wave 5 — UI 🟡

| Todo | Entails | Missing |
|------|---------|---------|
| 25 Design tokens + LookAndFeel ✅ | Original visual identity (no copied layouts/artwork) | Done: runtime tokens (`ui/ThemeTokens.h` data, `ui/Theme.{h,cpp}` accessors `theme::textPrimary()`, `accentFor(i)`, `gap()`, type scale, `theme::setCurrent` + `ThemeListener`; compiled-in dark fallback), `GlassLookAndFeel` (ring knobs, capped faders, pill buttons, frosted title bar, plus ComboBox, PopupMenu, TextEditor, ToggleButton switch, TabbedButtonBar, ScrollBar, TableHeader, tooltip in `GlassWidgets.cpp`; follows theme switches and repaints all windows), dB meters, `--translucent` window; `GlassLookAndFeelTest` snapshots every widget in both themes |
| 25a Theme/skin loader 🟡 | User themes, colours and layouts loaded via the plugin API | Done: `ThemeJson` (parse + validate, missing keys inherit dark, bad value rejects the file), `ThemeRegistry` (built-in JSON embedded via BinaryData: "Midnight Glass" dark + "Daylight Frost" light; user dir `<userApplicationData>/OpenMixer/themes/*.json`; plugin themes via `PluginHost::registerTheme` + `PluginRegistry::getThemes()`; precedence user > plugin > built-in; live `select`), `OPENMIXER_THEME=<id>` env in the app. Theme picker UI, persisted choice and plugin theme forwarding are done (Wave 5 settings). Missing: layouts (arrive with the Wave 5 window/widget registries), file-watching of the user dir |
| 26 Main window + decks 🟡 | Waveforms, transport, hotcues, loops | Done: `MainView` (layout from `LayoutSpec`: 2 decks flank a `mixer` slot, `library` slot below; JSON via `OPENMIXER_LAYOUT=<file>`; 4 decks = extra row), `DeckWidget` with header (title/artist/BPM/key+Camelot/elapsed/remaining), band-coloured overview (click/drag seeks via `deck.N.seek`), zoomed waveform (grid lines, centre playhead, loop + cues, wheel zoom), transport (play/cue/sync/master/key-lock/quantize/slip), pitch fader with +-8/16/50% range, 8 hotcue pads (Shift/Alt/Ctrl+click clears), quantized loop sizes + halve/double/exit/reloop; ~display-rate refresh via `VBlankAttachment`; new engine id `deck.N.quantize` (`DeckControl::isQuantize`). Waveform/metadata/grid are fed by `TrackLoader` (Todo 28). Missing: Cue is "stop and return to start" (no main-cue point in the engine), not visually verified in a real session with audio |
| 27 Mixer widgets 🟡 | Knobs, faders, meters | Done: `MixerPanel` fills the `mixer` slot (Mixer/FX pages). Strips: trim, filter, 3-band EQ + kill, PFL, FX shortcut, L/Mid/R assign, fader, stereo peak meter with hold + clip lamp (click clears); crossfader + Linear/Power curve (`mixer.crossfader.curve`); master fader + meters; cue mix/level; `FxPanel` per rack (master/ch) with unit select, enable, mix, knobs built from `FxParamSpec`, beat readout. Ids: `mixer.ch<N>.*`, `mixer.crossfader[.curve]`, `mixer.master`, `mixer.cue.*`, `fx.<master|ch<N>>.fx<U>.*`. Meter ballistics/clip/hold tokens in the theme `mixer` section. Missing: RMS meters, FX tabs hide the strips, sync-leader BPM entry, Mid label stands for "Thru" |
| 28 Library browser 🟡 | Tree, track list, search | Done: `LibraryServices` (message-thread Library, analysis service + sqlite result cache, `PrepQueue` pumped by a 100 ms Timer, `WatchFolders` own thread; shutdown in reverse), `LibraryBrowser` in the `library` slot: search box (query syntax, 120 ms debounce), sidebar tree (All tracks / playlists + folders / watch folders / prep queue), virtualised + paged sortable table (title, artist, BPM, key, time, rating, genre, added; harmonic-compatibility tint against the playing deck), Load 1..N buttons, double-click/Enter, drag onto a deck, Prep button, iTunes/Rekordbox XML import via file chooser. `TrackLoader` loads the file, pushes metadata, and waveform + `AnalysedBeatGrid` from the result cache, or queues the track urgently and fills the deck when the prep queue reports it done. `OPENMIXER_LIBRARY_DB` overrides the database path. Missing: playlist editing, prep queue reorder/remove, auto-enqueue after a scan |
| 29 Automix panel + settings 🟡 | Audio/MIDI settings, preferences | Done: `Preferences` (versioned JSON at `<user data>/OpenMixer/preferences.json`, per-field defaults, newer schema refused, corrupt file kept as `.bad`; `OPENMIXER_PREFS` overrides), `PreferencesApplier` (pushes only changed sections in order plugins -> theme -> audio -> MIDI through `PreferencesTargets`; app glue `AppPreferencesTargets`), `SettingsPanel` overlay (gear button in the mixer tab row, Escape/Close/click-outside dismiss; `--settings` or `OPENMIXER_SETTINGS=1` opens it at start) with tabs Audio (driver, device, rate, buffer, master + headphone output pair via `OutputRouter`, live latency/xruns/CPU from the app's `LatencyMonitor`), MIDI (per-input toggles via `MidiInputRouter::enable`, map file), Library (watch folders add/remove/rescan through `LibraryServices`, scan interval + scan-on-start read at start-up), Appearance (`ThemeRegistry` picker, live), Plugins (folder, loaded list, reload). Plugins now load at start-up from the plugin folder and their `registerTheme` JSON is forwarded to the `ThemeRegistry`. Missing: Automix panel (Wave 7, needs Todo 38), live scan-interval change, per-input MIDI hot-plug refresh while the panel is open |
| 30 Stem pads UI | Per-stem mute/volume | All: waits on Wave 7 stem playback (the engine has no per-stem output yet) |
| 31 UI verification gate ✅ | 10-minute mix test through the UI | Done: `tools/ui_gate.cpp` (`openmixer_ui_gate`) builds the main window (decks, mixer + FX tab, library browser over a file library, track loader; `tools/ui_gate/UiRig.h` shares `AppComposition` with `MainComponent`) on a paced `MockAudioDevice` and a scripted DJ (`UiScenario.h`) operates it only through widgets: library search + Load buttons, Play/Sync/Master/Cue/key-lock buttons, crossfader, EQ/filter/fader/pitch sliders, hotcue pads, loop size/exit buttons, FX scope/unit/enable/mix, theme switch with snapshot painting. After each action it asserts the widget and engine agree (0 mismatches), plus xrun/underrun/NaN/overflow budgets, library errors and leak-detector output. CTest `ui_gate_short` (label `ui_gate`, 40 s) and opt-in `ui_gate_10min` (`-DOPENMIXER_LONG_SOAK=ON`, label `uigate_long`); `app_smoke` (`openmixer --smoke` opens the window ~0.5 s, exit 0; exit 77 without a display = skipped). 10-minute run (linux-debug, 2026-10-18, final HEAD): 113,441 callbacks, 0 xruns (budget 5), 0 steady-state underruns, 0 non-finite, 0 overflow, avg CPU 13.7%, max callback 2.13 ms; 11,767 crossfader moves, 163 hotcue presses, 129 loops, 402 FX changes, 151 searches, 3 library loads, 2 theme switches, 121 snapshots, 0 UI/engine mismatches, 0 library errors, no leaks. Missing: real display and audio device (headless, widgets driven by calls not synthetic mouse events); overview-waveform seek, settings panel and drag-and-drop load are not in the scenario; stem pads (Todo 30) not covered; `ui_gate_short` and `app_smoke` run in the default ctest on CI like `soak_short` (timing-based, so they could flake on a slow runner) |

## Wave 6 — Hardware ⬜

| Todo | Entails | Missing |
|------|---------|---------|
| 34 Controller maps | Plug-and-play maps for popular controllers | All |
| 35 HID layer | Non-MIDI controllers | All |
| 36 Mapping editor UI | In-app learn/edit | All |

## Waves 7–9 ⬜

| Wave | Contents | Status |
|------|----------|--------|
| 7 | Stem-aware mixing, Automix, stem transitions, 8-pad sampler | ⬜ |
| 8 | Mix recording, broadcasting, DVS (timecode vinyl), video mixing | ⬜ |
| 9 | Legal audit, trademark check, packaging, docs, v1.0.0 | ⬜ |

---

## Known out-of-scope / non-goals

- Spotify or other streaming-service integration (terms of service prohibit DJ mixing).
- Mobile and web apps.
- Real-time stem separation (stems are precomputed offline).

## How to help

1. Pick a ⬜ or 🟡 row, open an issue naming the Todo number.
2. Read `CONTRIBUTING.md` and `docs/adr/0001-clean-room-doctrine.md` before writing code.
3. Include tests; engine code must not allocate or lock in audio callbacks.

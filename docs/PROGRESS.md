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
| 4 | Library | 🟡 schema + DAOs (Todo 20), metadata ingest + watch folders (Todo 21) done; import, search, prep queue remain |
| 5 | UI | ⬜ (window shell only) |
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
| 26a Debug panel | Two decks, faders, EQ, meters on plain JUCE widgets | ✅ (not visually verified: no display in CI box) | Replaced by real UI in Wave 5 |
| 13a Short soak test | 30–60 s headless mix in CI | ✅ `tools/soak_mix.cpp` (`openmixer_soak`, CTest `soak_short`, label `soak`): 2 looping decks, moving controls, asserts 0 xruns, 0 steady-state underruns, no NaN/Inf/overflow | Runs on a paced mock device, not real hardware; not yet in CI (workflow owned elsewhere) |
| 9 Key-lock time-stretch | Rubber Band realtime, tempo change without pitch change | ✅ Rubber Band 4.0.0 (R3 engine, realtime) via FetchContent single-file build (`openmixer_rubberband`, -O2 even in Debug); `TimeStretch` wrapper; `Deck::setKeylock` / id `keylock`; pitch held under rate change and device/file rate mismatch; tests measure pitch by zero crossings | Toggling key-lock off skips ahead by the stretcher latency (~tens of ms); end-of-track tail (latency) not flushed; per-deck CPU not yet profiled (Todo 13 soak); R2/R3 engine choice not user-selectable |
| 12 Sync & quantize | Beat-phase alignment, quantized cue/loop, slip mode | 🟡 `BeatGrid` interface + `FixedTempoGrid`, `Sync` (tempo match + beat/bar phase nudge, `ParameterTarget`), `Quantizer` (snap, quantized loop bounds), `SlipTracker`; tested with mock grids; `Deck` gapless loops (sample-accurate wrap into a decoder-prefetched `LoopPrefetch` slot, phase-kept resize, exit/reloop) and 8 hotcues (`HotcueBank`, ids `hotcueN`, `hotcueN.clear`, `loop.exit`, `loop.reloop`) | Real grids (Wave 3); wiring `Sync::apply`, `Quantizer` loops and `SlipTracker` into the app/UI and MIDI maps; hotcue persistence (Wave 4 library) |
| 11 FX rack | 8 effects (delay, reverb, flanger, …) with wet/dry and beat-sync | ✅ `src/engine/fx/` (`openmixer_fx`): echo, reverb, flanger, phaser, crush, gate, roll, tape stop; `FxUnit` (enable, smoothed mix, tails, idle) and `FxRack` as `ProcessStage` inserts; `FxFactory` registry; 8 original presets; ids `bpm`, `fxN.enable/mix/<param>`; `FxTest` | Not yet wired into `DjEngine`/`ParameterRouter` or the UI; LFO/gate phase free-runs from engage (no beat-phase lock to the grid yet); MIDI value scaling for natural-unit params |
| 13 Full soak test | 10-minute headless mix, glitch/xrun budget | ✅ `soak_10min` (opt-in: `-DOPENMIXER_LONG_SOAK=ON`, `ctest -L soak_long`). A scripted DJ (`tools/soak/Scenario.h`) moves mixer controls and rates and drives loops, hotcues, key-lock toggles, master FX and seeks. Last run (linux-debug): 112,508 callbacks, 1 xrun (budget 5), 0 steady-state underruns, 0 non-finite, 0 overflow, avg CPU 17.9%, max callback 7.7 ms | Not in CI yet (10 min); paced mock device, not real hardware; the one slow callback is likely `FxUnit` clearing its delay buffer on the audio thread when a tail ends |

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
| 21 Metadata ingest ✅ | Tag reading, artwork, watch folders | Done: `MetadataReader` extension point + `MetadataReaderRegistry` (later registration wins per extension), `JuceMetadataReader` (duration for every JUCE-decodable format, RIFF INFO tags for WAV, file-name title fallback), `Ingestor` (recursive scan, SHA-256 via `analysis::hashFileContent`, mtime+size skip, upsert keeps rating/cues, files that vanish are flagged missing and restored if they return; unreadable folder or failed read never flags anything), migration v2 (`track_files`, `watch_folders`), `WatchFolders` (persisted list, background rescan thread with injectable clock, manual trigger, own DB connection); `LibraryIngestTest`, `LibraryWatchFoldersTest`. Missing: TagLib reader (ID3/Vorbis/MP4 tags for MP3/FLAC/M4A; JUCE exposes none of them), artwork extraction, move detection by content hash, file-system change notifications (polling only), app wiring |
| 22 Import | iTunes / Rekordbox XML | All |
| 23 Search/filter ✅ | Text, BPM/key range, smart filters | Done: `SearchFieldRegistry` (built-in title/artist/album/genre/comment/path/year/bpm/rating/plays/duration/added/played/key/playlist; plugins `add()` fields), `parseQuery` (`artist:foo bpm:120-128 key:~8A added:7d -genre:techno sort:-bpm limit:50`), `compileSearch` (bound parameters only), `SearchEngine` (search/count with `juce::Result*` status), migration v3 indexes; `LibrarySearchTest`. Missing: FTS5 (not enabled in the amalgamation build; LIKE contains-match used), saved smart filters / smart crates, ranking |
| 24 Prep queue | Analyse-on-import pipeline | All |

## Wave 5 — UI ⬜

| Todo | Entails | Missing |
|------|---------|---------|
| 25 Design tokens + LookAndFeel 🟡 | Original visual identity (no copied layouts/artwork) | Done: `ui/Theme.h` (translucent glass surfaces, per-deck accents, spacing/type), `GlassLookAndFeel` (ring knobs with value arc from the reset point, capped faders with scale, pill/segmented buttons, frosted title bar), dB meters with peak hold, aligned channel-strip grid, `--translucent` / `OPENMIXER_TRANSLUCENT=1` see-through window. Missing: runtime theme loading (25a), combo/popup/text-editor styling |
| 25a Theme/skin loader | User themes, colours and layouts loaded via the plugin API | All |
| 26 Main window + decks | Waveforms, transport, hotcues, loops | All (a placeholder window exists) |
| 27 Mixer widgets | Knobs, faders, meters | All |
| 28 Library browser | Tree, track list, search | All |
| 29 Automix panel + settings | Audio/MIDI settings, preferences | All |
| 30 Stem pads UI | Per-stem mute/volume | All |
| 31 UI verification gate | 10-minute mix test through the UI | All |

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

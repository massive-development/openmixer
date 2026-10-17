# OpenMixer TODO

## Wave 1: Foundation (COMPLETE)

- [x] Repository initialization
- [x] Build system (CMake + JUCE 8)
- [x] CI/CD pipeline
- [x] Governance documents
- [x] Legal framework
- [x] Test harness

## Wave 2: Audio Engine (vertical slice: make sound, control it, test it)

Reordered so audio can be heard and driven (mixer + MIDI + debug panel) before heavy dependencies.

- [x] **Todo 7:** Audio device layer + latency instrumentation + mock device
  - `src/engine/AudioDevice.{h,cpp}`
  - Mock device for CI testing
  - Platform-specific backends via JUCE

- [x] **Todo 8:** Deck engine (streaming decode + transport)
  - `src/engine/Deck.{h,cpp}`
  - Lock-free ring buffer
  - JUCE AudioFormatManager decode first; FFmpeg backend deferred (see Todo 8b)

- [x] **Todo 10:** Mixer (channels, EQ, filter, crossfader)
  - `src/engine/Mixer.{h,cpp}`
  - 3-band EQ
  - PFL cue bus

- [x] **Todo 10a:** Plugin API + registry (extension points)
  - `src/plugins/` library: versioned interfaces, runtime loader, registry
  - Audio pre/post-processing stages (per deck/channel/master), FX, MIDI map loading
  - No signing/allow-list gatekeeping (see CONTRIBUTING "Extensibility")

- [x] **Todo 32:** MIDI input + mapping engine (pulled forward from Wave 6)
  - `src/controllers/MidiMapper.{h,cpp}`
  - `juce::MidiInput` -> JSON map file -> mixer/deck parameters
  - Mapping editor UI stays in Wave 6 (Todo 36)

- [x] **Todo 33:** Controller trace harness (pulled forward from Wave 6)
  - Replay recorded MIDI streams headlessly to test mappings

- [x] **Todo 26a:** Minimal debug panel (pulled forward from Wave 5)
  - 2 decks: load/play, faders, EQ knobs, level meters
  - Plain JUCE components, no design tokens; replaced by Wave 5 UI

- [x] **Todo 13a:** Short soak test in CI (30-60s headless mix)
  - `tools/soak_mix.cpp`
  - Full 10-minute run remains the Wave 2 gate (Todo 13)

- [x] **Todo 9:** Key-lock time-stretch via Rubber Band
  - `src/engine/TimeStretch.{h,cpp}`
  - Realtime mode
  - Golden audio tests

- [~] **Todo 12:** Beat-sync + quantized cue/loop/slip (partial: grid interface, sync, quantizer, slip, gapless Deck loop playback + hotcues done; real grids Wave 3, app/UI wiring open)
  - `src/engine/Sync.{h,cpp}`
  - Beat phase alignment

- [x] **Todo 11:** FX rack (8 essential effects)
  - `src/engine/fx/` (`openmixer_fx`): echo, reverb, flanger, phaser, crush, gate, roll, tape stop
  - `FxUnit`/`FxRack` as `ProcessStage` inserts; `fxN.*` parameter ids; beat-synced via `setTempo`/`BeatGrid`
  - Follow-up: wire a rack into `DjEngine` + `ParameterRouter` (`fx` prefix), beat-phase lock, FX UI

- [ ] **Todo 8b:** FFmpeg decode backend (optional, after the above)

- [x] **Todo 13:** Engine soak test (10-minute headless mix)
  - `tools/soak_mix.cpp`
  - Performance verification

**Gate:** All Wave 2 todos must pass before Wave 3

## Wave 3: Analysis Pipeline

- [x] **Todo 14:** Analysis service skeleton + SQLite cache
  - `src/analysis/AnalysisService.{h,cpp}`, `JobRunner`, `AnalyzerRegistry`, `SqliteResultCache`, `ClipSource`

- [x] **Todo 15:** BPM + beat-grid detection
  - `src/analysis/BpmEstimator.{h,cpp}`, `TempoSearch`, `OnsetEnvelope`, `Stft`, `BeatGridModel`, `AnalysedBeatGrid` (engine adapter)

- [x] **Todo 16:** Musical key detection + Camelot mapping
  - `src/analysis/KeyEstimator.{h,cpp}`, `Chromagram`, `MusicalKey` (Camelot)

- [x] **Todo 17:** Waveform data model
  - `src/analysis/WaveformBuilder.{h,cpp}`, `WaveformData`

- [x] **Todo 18:** Onset/energy/structure features
  - `src/analysis/StructureAnalyzer.{h,cpp}`, `OnsetPicker`, `Segmenter`

- [~] **Todo 19:** Stems precompute (Demucs ONNX) (partial: separator interface, model manifest, licence-gated verified download done; ONNX Runtime backend open)
  - `src/stems/StemSeparator.{h,cpp}`
  - `src/stems/WeightManager.{h,cpp}`, `ModelManifest`

## Wave 4: Library & Data

- [ ] **Todo 20:** Library schema + DAO layer
- [ ] **Todo 21:** Metadata ingest + watch folders
- [ ] **Todo 22:** Import (iTunes/Rekordbox XML)
- [ ] **Todo 23:** Search + filter engine
- [ ] **Todo 24:** Track prep queue

## Wave 5: UI

- [ ] **Todo 25a:** Theme/skin loader (user-supplied themes, colours, layouts via plugin API)
- [~] **Todo 25:** Design tokens + LookAndFeel (partial: `ui/Theme.h` tokens, `GlassLookAndFeel`, glass restyle of the debug console + optional `--translucent` window; missing: token loading for Todo 25a, widgets beyond knobs/faders/buttons)
- [ ] **Todo 26:** Main window + deck widgets
- [ ] **Todo 27:** Mixer widgets + metering
- [ ] **Todo 28:** Library browser
- [ ] **Todo 29:** Automix panel + settings
- [ ] **Todo 30:** Stem pads UI
- [ ] **Todo 31:** UI verification gate

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

**Next Work:** Wave 2 Todos 7, 8, then 10 (Mixer), 32/33 (MIDI), 26a (debug panel)

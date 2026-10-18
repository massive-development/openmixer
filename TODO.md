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

- [~] **Todo 12:** Beat-sync + quantized cue/loop/slip (partial: grid interface, sync, quantizer, slip, gapless Deck loop playback + hotcues done; ids wired through `DeckControl`: `deck.N.sync`, `sync.master`, `loop.quantized.<beats>`, `slip`; real grids Wave 3, UI open)
  - `src/engine/Sync.{h,cpp}`
  - Beat phase alignment

- [x] **Todo 11:** FX rack (8 essential effects)
  - `src/engine/fx/` (`openmixer_fx`): echo, reverb, flanger, phaser, crush, gate, roll, tape stop
  - `FxUnit`/`FxRack` as `ProcessStage` inserts; `fxN.*` parameter ids; beat-synced via `setTempo`/`BeatGrid`
  - Wired into `DjEngine` via `FxRouting` (`fx.master.*`, `fx.ch<N>.*`); idle delay-line clear amortised off the audio thread
  - Follow-up: beat-phase lock, FX UI

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

- [x] **Todo 20:** Library schema + DAO layer
  - `src/library/`: `Database`/`Statement`/`Transaction`, `Migrations`, `TrackDao`, `CueDao`, `PlaylistDao`, `Library`
- [~] **Todo 21:** Metadata ingest + watch folders (partial: RIFF INFO tags, duration, watch folders done; TagLib tags for MP3/FLAC/M4A and artwork open)
  - `src/library/`: `MetadataReader` registry + `JuceMetadataReader`, `Ingestor`, `TrackFileDao`, `WatchFolderDao`, `WatchFolders`
- [x] **Todo 22:** Import (iTunes/Rekordbox XML)
  - `src/library/`: `LibraryImporter` registry (extension point), `ItunesImporter`, `RekordboxImporter`, `ImportWriter` (one transaction, merge by path), `fileUrlToPath`
- [x] **Todo 23:** Search + filter engine
  - `src/library/`: `SearchFieldRegistry` (extension point), `parseQuery`, `compileSearch` (parameterised SQL), `SearchEngine`; migration v3 indexes. LIKE-based (SQLite build has no FTS5)
- [x] **Todo 24:** Track prep queue
  - `src/library/`: `PrepQueueDao` (persisted order/priority/status), `PrepQueue` (bounded in-flight submits to `AnalysisService`, `pump()` on the owner thread writes bpm/key back via `applyPrepResults`), migration v4

## Wave 5: UI

- [~] **Todo 25a:** Theme/skin loader (done: JSON themes, `ThemeRegistry` with built-in/user/plugin sources and live switching, `PluginHost::registerTheme`; missing: layouts, theme picker UI + persisted choice)
  - `src/ui/`: `ThemeTokens`, `Theme` (runtime accessors), `ThemeJson`, `ThemeRegistry`; `resources/themes/*.json` embedded via `juce_add_binary_data`
- [x] **Todo 25:** Design tokens + LookAndFeel
  - `GlassLookAndFeel` covers knobs, faders, buttons, ComboBox, PopupMenu, TextEditor, ToggleButton, TabbedButtonBar, ScrollBar, TableHeader, tooltip (`GlassWidgets.cpp`)
- [~] **Todo 26:** Main window + deck widgets (done: window, widgets and tests; missing: a real main-cue point; the library feed landed with Todo 28)
  - `src/ui/`: `MainView` + `MainLayout` (data-driven rows/cells/slots), `DeckWidget` = `TrackHeader`, `OverviewWaveform`, `ZoomWaveform`, `TransportPanel`, `PitchFader`, `HotcuePads`, `LoopControls`; `DeckBinding` routes ids
- [~] **Todo 27:** Mixer widgets + metering (done: strips, EQ + kill, crossfader + curve, master, cue, FX panel, stereo peak meters with hold + clip; missing: RMS meters, FX assign beyond the per-channel rack shortcut)
  - `src/ui/`: `MixerPanel` (mixer slot; Mixer/FX pages), `ChannelStripWidget`, `MasterSection`, `CrossfaderSection`, `CueSection`, `FxPanel`, `LabeledKnob`, `MeterBar`/`StereoMeter` over `MeterBallistics`; `MixerBinding` routes `mixer.*` / `fx.*` ids; engine: `crossfader.curve` id, stereo channel peaks, state getters
- [~] **Todo 28:** Library browser + app wiring (done: sidebar tree, paged sortable table with Camelot compatibility tint, search box on the query syntax, load buttons/double-click/drag to decks, prep queue view + enqueue, iTunes/Rekordbox import, `LibraryServices` composition (library, analysis cache, prep queue pumped on a timer, watch folders), `TrackLoader` feeds waveform + beat grid + metadata to decks; missing: playlist editing, prep queue reorder/remove UI, auto-enqueue after scans)
  - `src/ui/`: `LibraryServices`, `TrackLoader`, `LibraryBrowser`, `TrackTableModel` over `TrackSource` (`SearchTrackSource`, `PrepQueueSource`), `SidebarModel` + `SidebarTree`, `TrackDrag.h` (`DeckWidget` is a drop target); theme `library` section; `DebugPanel`/`DeckStrip` and `OPENMIXER_DEBUG_PANEL` removed (the browser loads tracks); `OPENMIXER_LIBRARY_DB=<file>` selects another library
- [~] **Todo 29:** Automix panel + settings (done: settings overlay with audio device/rate/buffer/master+headphone output pairs + latency/xrun/CPU, MIDI inputs + map file, library watch folders + rescan, theme picker, plugin folder; versioned `preferences.json` applied at start and on change; plugins loaded at start with their themes forwarded; missing: Automix panel (Wave 7, needs Todo 38), live scan-interval change)
  - `src/ui/`: `Preferences`, `PreferencesApplier` (+ `PreferencesTargets`), `SettingsPanel` over `SettingsPage` tabs (`AudioSettingsPage` + `AudioSettingsModel`/`JuceAudioSettingsModel`, `MidiSettingsPage`, `LibrarySettingsPage`, `AppearanceSettingsPage`, `PluginsSettingsPage`, `PathRow`); `src/engine/OutputRouter`; `src/app/AppPreferencesTargets`; theme `settings` section; `--settings` / `OPENMIXER_SETTINGS=1` opens the panel, `OPENMIXER_PREFS=<file>` selects another preferences file
- [ ] **Todo 30:** Stem pads UI (open: waits on Wave 7 stem playback)
- [x] **Todo 31:** UI verification gate
  - `tools/ui_gate.cpp` + `tools/ui_gate/{UiRig,UiScenario}.h` (`openmixer_ui_gate`): full main window over a paced mock device, scripted DJ operating the widgets; CTest `ui_gate_short` (label `ui_gate`), `ui_gate_10min` (opt-in with `-DOPENMIXER_LONG_SOAK=ON`, label `uigate_long`); `app_smoke` (`openmixer --smoke`, label `smoke`, skipped without a display)

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

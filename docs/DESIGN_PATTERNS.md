# Design Patterns in Use

Registry for agents and contributors. Check here before starting a large implementation; add an entry when you introduce or adopt a pattern.

## Entry template

- **Pattern:** name
- **Where:** files/targets
- **Why:** problem it solves (one line)
- **Reuse when:** situations that should use it
- **Notes:** constraints (threading, ownership)

## Current patterns

### Interface + mock (Strategy / Adapter)
- **Where:** `src/engine/AudioDevice.h`, `JuceAudioDevice.h`, `MockAudioDevice.h`
- **Why:** swap hardware backends and run headless in CI.
- **Reuse when:** wrapping any external I/O (MIDI input, file/network, HID) so tests need no hardware.
- **Notes:** callers depend on the abstract interface only.

### Lock-free SPSC ring buffer
- **Where:** `src/engine/RingBuffer.h`
- **Why:** pass audio between a worker thread and the audio callback without locks or allocation.
- **Reuse when:** any realtime thread needs data from a non-realtime producer (decode, analysis results, MIDI events).
- **Notes:** exactly one producer and one consumer thread.

### Epoch handshake (producer-owned reset)
- **Where:** `src/engine/Deck.h`, `DeckDecoder.cpp`
- **Why:** seek/load without the producer resetting a ring the consumer is reading.
- **Reuse when:** a realtime consumer must discard queued data after a state change.

### Callback interface (Observer-style)
- **Where:** `AudioCallback` in `src/engine/AudioDevice.h`; `Deck` implements it.
- **Why:** engine units plug into any device, real or mock.
- **Reuse when:** adding mixer/FX stages that process audio blocks.

### Lock-free metrics monitor
- **Where:** `src/engine/LatencyMonitor.h`
- **Why:** expose realtime stats (latency, xruns) to UI/tests without locking the audio thread.
- **Reuse when:** adding any realtime telemetry.

### Static library per area
- **Where:** `openmixer_engine` in `CMakeLists.txt`
- **Why:** shared, independently testable modules.
- **Reuse when:** every new `src/<area>/` (controllers, analysis, library, ui).

### Parameter target (Command-style control surface)
- **Where:** `src/core/ParameterTarget.h`; implemented by `Mixer` (`ch0.fader`, `crossfader`, ...) and `Deck` (`play`, `rate`, `seek`).
- **Why:** MIDI, UI and tests drive the engine through one string-id API instead of bespoke setters.
- **Reuse when:** adding any user-controllable engine unit; route by id prefix, return false for unknown ids.
- **Trade-off:** string parsing is off the audio thread (control side only); typed setters remain for hot paths.

### Versioned process stage + stage chain (extension point)
- **Where:** `src/engine/ProcessStage.h`, `StageChain.{h,cpp}`; `Mixer::setChannelInsert` / `setMasterInsert`.
- **Why:** users/plugins can insert, replace or remove audio processors at runtime without core changes.
- **Reuse when:** any per-deck/channel/master processing (FX rack, time-stretch, user plugins).
- **Notes:** fixed slots of atomic non-owning pointers; `kInterfaceVersion` is checked on insert; replacing/removing waits for two audio passes (max 50 ms) so the old stage can then be destroyed. Caller owns the stage.

### Small units composed by a coordinator
- **Where:** `ThreeBandEq`, `SweepFilter`, `Crossfader`, `PeakMeter`, `ChannelStrip` composed by `Mixer`; shared filter maths in `src/dsp` (`openmixer_dsp`).
- **Why:** one responsibility per file; DSP is reusable by FX and tests without the mixer.

### Data-driven map + trace replay harness
- **Where:** `src/controllers/MidiMap.{h,cpp}` (JSON schema in header), `MidiMapper`, `MidiTrace` (`replayTrace`), `RecordingTarget`.
- **Why:** controller mappings are user JSON, not code; mappings are tested by replaying recorded MIDI with no hardware.
- **Reuse when:** any user-editable config or hardware-driven feature (HID, OSC): parse into a plain struct, apply via `ParameterTarget`, test with recorded input.

### Prefix registry (ParameterRouter)
- **Where:** `src/controllers/ParameterRouter.{h,cpp}`
- **Why:** MIDI maps address `mixer.*`, `deck.N.*` or any user-registered prefix; plugins add targets without touching core. Longest prefix wins.
- **Reuse when:** exposing a new controllable unit; register it, do not extend a switch.
- **Notes:** the app registers `mixer`, `fx` (`FxRouting`), `sync` and `deck.<N>` (`DeckControl`, a decorator that handles sync/loop.quantized/slip and forwards the rest to `Deck`). Engine units cannot link `controllers`, so `DjEngine` exposes targets and the app does the registering.

### Composition root for the signal path (DjEngine)
- **Where:** `src/engine/DjEngine.{h,cpp}`
- **Why:** one `AudioCallback` owns decks + mixer, so devices, tests and the UI share the same wiring; the UI links `openmixer_ui -> openmixer_engine` one-way.
- **Reuse when:** adding units to the signal path (FX, sampler); extend DjEngine instead of wiring in the app.
- **Notes:** FX racks, `Sync` and `DeckControl` live here; keep `DjEngine` thin by giving each its own file (`FxRouting`, `DeckControl`). `updateControl(dt)` is the one control-timer entry.

### Grid interface with mock implementations (BeatGrid)
- **Where:** `src/engine/BeatGrid.h`, consumed by `Sync` and `Quantizer`; `FixedTempoGrid` is the built-in implementation, tests use a mock.
- **Why:** sync/quantize can be built and tested before Wave 3 analysis exists; analysis plugs in by implementing `BeatGrid`.
- **Reuse when:** a feature depends on data another wave produces: define the minimal read-only interface first, test with a mock.
- **Notes:** `Sync::step` is a pure function of deck states (control thread, ~20-50 Hz); only `apply` touches `Deck::setRate`. Grids are shared immutably (`shared_ptr<const BeatGrid>`).

### Checking decorator for headless runs (OutputChecker)
- **Where:** `tools/soak/OutputChecker.h`, used by `tools/soak_mix.cpp`.
- **Why:** wrap any `AudioCallback` to count NaN/Inf/overflow samples on the audio thread without changing the engine.
- **Reuse when:** long-running or fuzz-style runs (soak, golden-audio, plugin stress) need output assertions.
- **Notes:** atomics only, no allocation in `process`.

### Plugin registry / extension points (openmixer_plugins)
- **Where:** `src/plugins/` (`PluginApi.h` public API, `PluginLoader`, `PluginRegistry`, `StageCatalog`); example in `examples/plugins/GainPlugin.cpp`.
- **Why:** users add audio stages (pre/post-processing, FX), MIDI maps and parameter targets without touching core and without any signing or allow-list.
- **Reuse when:** a new feature should be replaceable: add a `register...` method to `PluginHost` (bump `kPluginApiVersion` only on breaking changes), never a hardcoded path.
- **How to extend:** build a shared library exporting `OPENMIXER_DEFINE_PLUGIN(MyPlugin)`; `onLoad(PluginHost&)` registers stages (`registerStage`), MIDI maps (JSON strings, validated) and `ParameterTarget`s (appear under the bound `ParameterRouter` prefix). The app calls `PluginRegistry::loadDirectory`/`loadLibrary`, then `attachStage` to a channel (0..N) or master (`StageCatalog::kMasterChannel`) slot; `unload` detaches everything the plugin registered first.
- **Version note:** a plugin that calls a newly appended `PluginHost` hook (e.g. `registerTheme`) needs a host at least as new; hooks are appended with a safe default, so `kPluginApiVersion` stays 1 and older plugins keep loading.
- **Notes:** control thread only; stages run on the audio thread via the existing `StageChain` (no allocation/locks in `process`). C++ types cross the boundary, so plugins must match the host's compiler and JUCE version (trade-off: small ergonomic API over a C ABI). Stage ids, map names and prefixes are unique; a second registration fails rather than overriding. Mixers/routers passed in must outlive the registry. Dependencies: plugins -> engine, controllers -> core. On Windows the loader suppresses the OS "Bad Image" dialog (`SEM_FAILCRITICALERRORS`) so a broken DLL returns an error instead of blocking headless runs.

### Runtime theme tokens + theme registry (extension point)
- **Where:** `src/ui/ThemeTokens.h` (data), `Theme.{h,cpp}` (current theme + accessors), `ThemeJson`, `ThemeRegistry`, `GlassLookAndFeel`; built-in themes in `resources/themes/*.json` (embedded with `juce_add_binary_data`).
- **Why:** widgets never hard-code colours/sizes; they read `theme::textPrimary()`, `theme::accentFor (i)`, `theme::gap()`, `theme::typeBody()` etc., so a theme (built-in, user JSON or plugin) can restyle everything live.
- **Reuse when:** adding any widget. Paint with token accessors (read at paint time, so switches are free). Anything cached (fonts, per-deck accents, label colours) goes in an `applyTheme()` called from the constructor and `lookAndFeelChanged()`; `GlassLookAndFeel` calls `sendLookAndFeelChange()` on every top-level window after a switch, which re-runs both. Non-Component holders can implement `theme::ThemeListener`.
- **How to extend:** drop a `*.json` in `<userApplicationData>/OpenMixer/themes` (schema in `ThemeJson.h`; missing keys inherit the dark theme; an invalid value rejects that file only), call `ThemeRegistry::add/addJson` from code, or ship JSON from a plugin with `PluginHost::registerTheme` and forward `PluginRegistry::getThemes()` to `ThemeRegistry::addJson (json, Source::Plugin, owner)` (`removeOwner (pluginId)` on unload). Precedence per id: User > Plugin > BuiltIn, latest add wins within a source. New token = field in `ThemeTokens`, a row in the key tables in `ThemeJson.cpp`, an accessor in `Theme.h`, and a value in both built-in JSON files.
- **Notes:** message thread only (the current theme is a plain global, not atomic). `ThemeTokens::builtinDark()` is the compiled-in fallback and must stay equal to `openmixer-dark.json` (tested). Use one `ThemeRegistry` per app; the active theme falls back to `openmixer.dark` if it is removed. Dependency direction: plugins do not depend on ui, so the app forwards plugin themes.

### Prefetched jump slot (hazard-slot double buffer)
- **Where:** `src/engine/LoopPrefetch.{h,cpp}`, used by `Deck` (`DeckPlayback.cpp`) and `DeckDecoder`.
- **Why:** the audio thread jumps (loop wrap) without waiting for the decoder to refill the ring.
- **Reuse when:** any realtime jump target that can be predicted (hotcues, saved loops, sampler pads).
- **Notes:** two slots; decoder unpublishes, then fills the slot the audio thread is not holding; audio publishes `inUse` then re-checks `published` (seq_cst). The audio thread also asks the decoder to move the ring to the slot end (`ringReq*`, tagged with the control epoch so stale requests are dropped).

### Seqlock snapshot for small multi-field state
- **Where:** `src/engine/LoopState.h`.
- **Why:** audio and decoder threads need a consistent {start, end, enabled} without locks.
- **Reuse when:** a few fields change rarely from control threads and must be read together in realtime.
- **Notes:** writers serialise on a mutex (control side only); readers retry a bounded number of times and keep their previous snapshot on contention.

### Effect template method + wrapping unit (FX rack)
- **Where:** `src/engine/fx/` (`openmixer_fx`): `Effect` (non-virtual `prepare`/`process` call `prepareEffect`/`render`), `FxUnit`, `FxRack`.
- **Why:** each algorithm only renders a fully wet signal; enable ramps, smoothed wet/dry, tempo smoothing, tail ring-out and idle bypass live once in `FxUnit`, so new effects stay small.
- **Reuse when:** adding an effect or any per-block processor that needs the same enable/mix/tail behaviour; subclass `Effect`, declare params as `FxParamSpec`s, call `tickParams()` per sample.
- **Notes:** params are atomics written on the control thread and smoothed per sample (`smoothSeconds` 0 for step controls). `FxUnit` processes in fixed 64-sample chunks with member scratch arrays (no allocation); buffers are sized in `prepare` for `kMinBpm`. Tail effects (`hasTail`) stop receiving input on disable and go idle after `tailSilenceWindow` samples below -100 dBFS; others fade out over 10 ms. Idle units return immediately (bit-exact dry). `FxUnit` and `FxRack` are both `ProcessStage`s, so either inserts via `Mixer::setChannelInsert`/`setMasterInsert`.

### Name registry for effects (FxFactory)
- **Where:** `src/engine/fx/FxFactory.{h,cpp}`; consumed by `FxRack (layout, factory)`.
- **Why:** rack layouts are lists of names, so users/plugins add effects (`registerEffect`) without touching core; built-ins can't be silently overridden.
- **Reuse when:** any user-extensible family of processors (sampler voices, analysis stages): register creators by name, build from data.

### Data-driven presets
- **Where:** `src/engine/fx/FxPresets.{h,cpp}` (`builtInPresets`, `applyPreset`).
- **Why:** presets are plain data applied through the same parameter API, so they can later load from JSON; all values are original to OpenMixer.
- **Reuse when:** shipping starting points for any parameterised unit (EQ curves, mapping defaults).

### Background job service (worker pool + shared job handle)
- **Where:** `src/analysis/AnalysisService.{h,cpp}`, `AnalysisJob`, `JobRunner`
- **Why:** run slow work (decode + analysis) off the UI/audio threads with progress, cancellation and waiting, without callers owning threads.
- **Reuse when:** any long-running background task (library scan, stem precompute, export).
- **Notes:** `submit()` returns `shared_ptr<AnalysisJob>` (shared by caller and worker); cancellation is a polled atomic flag; listeners run on the worker thread after the final status; the destructor cancels everything and joins. The job body is a free function (`runAnalysisJob`) so it is testable without threads.

### Content-addressed result cache
- **Where:** `src/analysis/ResultCache.h` (interface), `SqliteResultCache.{h,cpp}`, hashing in `ClipSource.cpp`
- **Why:** analysis is expensive; results key on SHA-256 of the audio bytes + producer id + producer version, so renames/moves keep results and bumping a version invalidates old rows.
- **Reuse when:** caching anything derived from file content (waveforms, stems, artwork thumbnails).
- **Notes:** SQLite opened `FULLMUTEX` and statements serialised by a mutex; tests use `":memory:"`. Payloads are JSON strings owned by each analyser.

### Estimator + analyser wrapper + engine adapter
- **Where:** `src/analysis/BpmEstimator.{h,cpp}` (`BpmEstimator` pure, `BpmAnalyzer` JSON wrapper), `BeatGridModel` (editable value type), `AnalysedBeatGrid.h` (implements engine `BeatGrid`).
- **Why:** algorithms stay testable without the service; the cached payload is plain JSON; the engine sees an immutable snapshot through its own interface, so analysis never links the engine.
- **Reuse when:** every analysis feature (key, waveform, structure, stems): pure estimator -> `Analyzer` wrapper with `toJson`/`fromJson` -> adapter for the consumer's interface.
- **Notes:** `analysis` includes only header-only `engine/BeatGrid.h`; if more engine interfaces are needed, move them to `src/core` instead of linking `openmixer_engine`.

### Pyramid of levels (multi-resolution data)
- **Where:** `src/analysis/WaveformData.{h,cpp}`
- **Why:** zoomed and overview views read pre-reduced data instead of scanning samples per paint.
- **Reuse when:** any per-time data drawn at many zooms (energy, stems, spectrogram).
- **Notes:** only the finest level is persisted; coarser levels are derived on load. Merge rules must be associative (min/max, RMS via mean of squares).

### Licence-gated, verified download
- **Where:** `src/stems/ModelManifest.{h,cpp}`, `WeightManager.{h,cpp}` (`Downloader` interface, `UrlDownloader`)
- **Why:** third-party weights/assets are fetched only after the user accepts their licence and only installed if the SHA-256 matches the manifest.
- **Reuse when:** any large optional asset not shipped in the repo (models, sample packs, skins from a catalogue).
- **Notes:** blocking, call off the UI thread; manifests are JSON so users can point at mirrors; tests use `file://` URLs, no network.

### Analyzer registry (extension point)
- **Where:** `src/analysis/Analyzer.h`, `AnalyzerRegistry.{h,cpp}`
- **Why:** built-in and plugin analysers are looked up by id; `kInterfaceVersion` is checked on `add()`.
- **Reuse when:** adding any analysis feature; register an `Analyzer`, do not special-case it in the service.
- **Notes:** `analyze()` is `const` and may run concurrently; registry hands out `shared_ptr<const Analyzer>` so removal during a running job is safe.

### Third-party DSP behind a pimpl wrapper
- **Where:** `src/engine/TimeStretch.{h,cpp}` (Rubber Band), `cmake/RubberBand.cmake`.
- **Why:** keeps the dependency's headers out of engine headers and states the RT contract in one place.
- **Reuse when:** wrapping any external DSP/codec library (FFmpeg, ONNX Runtime).
- **Notes:** `prepare()` allocates (control thread); all other calls are audio-thread, allocation-free. Vendored code builds as its own target with its warnings off and -O2 in Debug.


### Forward-only migration list (extension point)
- **Where:** `src/library/Migrations.{h,cpp}`, `Library::open`.
- **Why:** the schema evolves by appending numbered `Migration`s; each runs in its own transaction and is recorded in `schema_version`.
- **Reuse when:** any later subsystem (search index, history, plugin tables) needs library tables; append a `Migration` with the next version and pass the list to `Library::open`. Never edit a shipped migration.
- **Notes:** a database newer than the list is refused (no downgrades). Versions must be strictly increasing.

### DAO per entity over an RAII database wrapper
- **Where:** `src/library/Database.{h,cpp}`, `Statement`, `TrackDao`, `CueDao`, `PlaylistDao`, `OrderedList`.
- **Why:** keeps SQL out of callers; `Transaction` nests via savepoints so DAO calls compose and roll back together.
- **Reuse when:** adding a table; add a `*Dao` that borrows `Database&` and returns plain value records.
- **Notes:** one writer thread; the UI reads through its own `OpenMode::ReadOnly` connection (WAL). `OrderedList` keeps dense `position` columns for any ordered child set.

### Metadata reader registry (extension point)
- **Where:** `src/library/MetadataReader.{h,cpp}`, `JuceMetadataReader`, `Ingestor`.
- **Why:** tag formats vary; `Ingestor` asks the registry for the reader owning a file's extension, so new readers (TagLib, plugin-supplied) need no ingestor change.
- **Reuse when:** adding a tag source or a new supported file type; implement `MetadataReader` and `registry.add()` it. Later registrations win per extension.
- **Notes:** the registry decides which files are scanned at all. Ingest state (mtime, size, missing) lives in its own `track_files` table so `TrackRecord` stays a pure metadata value.

### Background rescan with injectable clock (WatchFolders)
- **Where:** `src/library/WatchFolders.{h,cpp}`.
- **Why:** a worker thread owns its own DB connection and re-evaluates `clock()` on a short poll; tests drive time with a fake clock plus `poke()`/`triggerScan()`/`waitForScans()` instead of sleeping.
- **Reuse when:** any periodic background task over the library (import refresh, analysis queue feeding).
- **Notes:** one connection per thread (WAL + busy timeout) rather than locking a shared one; the destructor aborts a scan between files.

### Search field registry + compile-to-SQL (extension point)
- **Where:** `src/library/Search{Fields,Parser,Compiler,Terms,Engine}.{h,cpp}`.
- **Why:** a query string parses to generic `FilterTerm`s; each registered `SearchField` (column + kind, or a Custom compile hook) turns its term into SQL text made of code-constant names and `?` placeholders, with user values only in the bound args.
- **Reuse when:** adding a filterable/sortable attribute (e.g. a plugin tag); `engine.registry().add(field)`. Custom hooks must never splice user text into `SqlFragment::sql`.
- **Notes:** reads report failure via the trailing `juce::Result*` (a bad field/value fails rather than returning "no results"). Text match is LIKE-contains with escaped wildcards (`=value` for exact); the SQLite build has no FTS5, so a future FTS index would be a new migration plus a Text-field compile swap.

### Library importer registry + parse/merge split (extension point)
- **Where:** `src/library/LibraryImporter.{h,cpp}`, `ItunesImporter`, `RekordboxImporter`, `ImportModel.h`, `ImportWriter`, `ImportMerge`, `ImportCues`, `LibraryImport`.
- **Why:** importers only turn a foreign XML document into the neutral `ImportedLibrary` (no database code, recognised by XML root since both formats are `.xml`); one `ImportWriter` merges any importer's output in a single `Transaction`, so every format gets the same dedupe-by-path and rollback behaviour.
- **Reuse when:** adding another library format (`registry.add(...)`, later registration wins) or another bulk write that must be all-or-nothing and re-runnable.
- **Notes:** merge policy (local data wins, imports fill gaps and default rating/play count) is documented in `ImportMerge.h`; re-import is idempotent; a failed lookup aborts the import rather than counting as "new".

### Owner-thread pump over a background service (PrepQueue)
- **Where:** `src/library/PrepQueue.{h,cpp}`, `PrepQueueDao`, `PrepResults`.
- **Why:** analysis workers must not touch the Library's sqlite connection, so their listener only copies the final job snapshot into a mutex-guarded inbox; the thread that owns the connection calls `pump()` to apply results, update rows, fire the observer and submit more. At most `maxInFlight` jobs sit in the service, so priority and order stay editable (the service itself is plain FIFO).
- **Reuse when:** a persisted queue feeds `AnalysisService` or another worker pool and results must land in the database (stem precompute, export).
- **Notes:** the inbox is a `shared_ptr` captured by listeners so late callbacks after destruction are safe; progress is read live from the job on `list()` rather than written to the DB; running rows are reset to queued on start. Library depends on analysis (same tier) for this.

### Data-driven main layout + named slots (extension point)
- **Where:** `src/ui/MainLayout.{h,cpp}` (pure geometry + JSON), `MainView.{h,cpp}`.
- **Why:** the window is `LayoutSpec` rows of cells, each a deck index or a named slot; `computeLayout` turns it into rectangles with no Components, so more decks or a different arrangement is a data change (`OPENMIXER_LAYOUT=<file.json>`).
- **Reuse when:** adding a panel (mixer, library, a plugin's view): name a slot in the spec and call `MainView::setSlotContent (name, component)`; empty slots show a `SlotPlaceholder`.
- **Notes:** decks the engine lacks are skipped. Slot content is owned by the view.

### Mixer widgets: binding + descriptor-built FX controls (MixerBinding, FxPanel)
- **Where:** `src/ui/MixerBinding.h`, `MixerPanel`, `ChannelStripWidget`, `FxPanel`, `MeterBallistics`.
- **Why:** same router-id rule as decks (`mixer.<id>`, `fx.<scope>.fx<U>.<param>`); widgets mirror engine getters in `refresh (dt)` without echoing. `FxPanel` builds one `LabeledKnob` per `FxParamSpec`, so a plugin-registered effect in a rack gets controls with no UI code. `MeterBallistics` is drawing-free (peak, release, hold, clip latch, dt-based) so it is tested with synthetic levels; all metrics and thresholds are theme tokens (`mixer` JSON section).
- **Reuse when:** adding a control surface for another engine object, or another meter.

### Library composition: services unit, row sources, load callbacks (LibraryServices, TrackSource, TrackLoader)
- **Where:** `src/ui/LibraryServices.{h,cpp}`, `TrackSource`, `TrackTableModel`, `SidebarModel`, `LibraryBrowser`, `TrackLoader`, `TrackDrag.h`.
- **Why:** one place owns start-up/shutdown order (Library writer -> result cache -> analysis service -> prep queue -> watch folders; destroyed in reverse) and the thread rules: the message thread owns the Library connection and the PrepQueue (a Timer calls `pump()`), `WatchFolders` has its own thread + connections and only bumps an atomic counter that `pump()` turns into `libraryChanged()`. Widgets never open databases; they read `LibraryServices` and react to its `Listener`.
- **Reuse when:** a view needs track rows (implement `TrackSource`: `count()` + `fetch (offset, limit)`; the table pages and caches them), a new sidebar node (add a `SidebarNode::Kind` and its `scopeQuery()`), or another "load" gesture (call `TrackLoader::load`; the browser only emits `onLoadRequested (deck, track)`, drops arrive as `trackdrag::describe/parse` strings on `DeckWidget::onTrackDropped`).
- **Notes:** `LibraryServices::Options::analyzers` swaps the analyser registry (tests, plugins); `":memory:"` runs without watch folders. Preferences: add/remove folders with `getWatchFolders()->add/remove` then `notifyLibraryChanged()`. Library depends on analysis, ui depends on both; never the reverse. `OPENMIXER_LIBRARY_DB` overrides the path (screenshots/tests).

### Widgets write through router ids only (DeckBinding)
- **Where:** `src/ui/DeckBinding.h` and the deck components (`TransportPanel`, `PitchFader`, `HotcuePads`, `LoopControls`, `OverviewWaveform`).
- **Why:** every control calls `binding.send ("<id>", value)` which becomes `deck.<N>.<id>` on the app's `ParameterRouter`, the same path MIDI maps use; reads (position, loop, hotcues, sync state) are the engine's existing atomic getters on the message thread. Toggles send the opposite of the engine state and mirror it back in `refresh()`.
- **Reuse when:** any new control surface; tests pass a recording `ParameterTarget` that forwards to a real router to assert both id/value and engine effect.
- **Notes:** the frame clock is a `juce::VBlankAttachment` in `DeckWidget`; each component's `refresh()` repaints only what changed (playhead strip, cue markers, or the zoom view when position/loop/cues moved).

### Preferences model + applier + settings pages (Preferences, PreferencesApplier, SettingsPanel)
- **Where:** `src/ui/Preferences.{h,cpp}`, `PreferencesApplier`, `SettingsPanel` + `SettingsPage` and the `*SettingsPage` tabs, `AudioSettingsModel`; glue in `src/app/AppPreferencesTargets`; `src/engine/OutputRouter`.
- **Why:** one plain, versioned JSON model (JSON over `juce::PropertiesFile` because the data is nested/versioned and hand-editable, like theme and layout files); subsystems sit behind `PreferencesTargets` so the applier is tested with mocks and pushes only the sections that changed (plugins -> theme -> audio -> MIDI); pages edit the model and call `changed`, the app saves and applies.
- **Reuse when:** adding a setting (field in `Preferences` with a default, read/write in `Preferences.cpp`, a `PreferencesTargets` method if a subsystem must react) or a settings tab (derive `SettingsPage`, `SettingsPanel::addPage (title, page)`; a plugin can add one the same way). Targets must be idempotent.
- **Notes:** the audio page talks to an `AudioSettingsModel` (real: `JuceAudioSettingsModel` over the `AudioDeviceManager`; tests: a fake, no device opened). All device outputs (up to 8) are opened and `OutputRouter` sends master/cue to chosen stereo pairs (audio-thread safe, atomics). Library scan interval/scan-on-start are read when `LibraryServices` is created (restart to change); watch folders live in the library DB, not in the preferences.

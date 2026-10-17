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

### Composition root for the signal path (DjEngine)
- **Where:** `src/engine/DjEngine.{h,cpp}`
- **Why:** one `AudioCallback` owns decks + mixer, so devices, tests and the UI share the same wiring; the UI links `openmixer_ui -> openmixer_engine` one-way.
- **Reuse when:** adding units to the signal path (FX, sampler); extend DjEngine instead of wiring in the app.

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
- **Notes:** control thread only; stages run on the audio thread via the existing `StageChain` (no allocation/locks in `process`). C++ types cross the boundary, so plugins must match the host's compiler and JUCE version (trade-off: small ergonomic API over a C ABI). Stage ids, map names and prefixes are unique; a second registration fails rather than overriding. Mixers/routers passed in must outlive the registry. Dependencies: plugins -> engine, controllers -> core. On Windows the loader suppresses the OS "Bad Image" dialog (`SEM_FAILCRITICALERRORS`) so a broken DLL returns an error instead of blocking headless runs.

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

### Analyzer registry (extension point)
- **Where:** `src/analysis/Analyzer.h`, `AnalyzerRegistry.{h,cpp}`
- **Why:** built-in and plugin analysers are looked up by id; `kInterfaceVersion` is checked on `add()`.
- **Reuse when:** adding any analysis feature; register an `Analyzer`, do not special-case it in the service.
- **Notes:** `analyze()` is `const` and may run concurrently; registry hands out `shared_ptr<const Analyzer>` so removal during a running job is safe.

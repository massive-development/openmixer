# Design Patterns in Use

Registry for agents and contributors. Check here before starting a large implementation; add an entry when you introduce or adopt a pattern.

Most entries below describe the Rust design. Where a pattern comes from the C++ tree, the entry says so and names its file in the frozen baseline (`cpp-final`). Entries marked **(planned)** were written as design before the Rust code existed, and the marker may lag the code (check the crate); **(deferred)** patterns wait for a later phase. Patterns that the Rust port replaces are listed under "Replaced in the Rust port" with what replaces them.

## Entry template

- **Pattern:** name
- **Where:** crate/module (and `cpp-final` path if ported from the C++ tree)
- **Why:** problem it solves (one line)
- **Reuse when:** situations that should use it
- **Notes:** constraints (threading, ownership)

## Current patterns

### Trait + mock implementation (Strategy / Adapter)
- **Where:** `om-audio` (planned: device trait with a `cpal` backend and a mock device)
- **Why:** swap hardware backends and run headless in CI.
- **Reuse when:** wrapping any external I/O (MIDI input, file/network, HID) so tests need no hardware.
- **Notes:** callers depend on the trait only. The C++ tree used an abstract device interface with a framework-backed class and a mock class.

```rust
pub trait AudioDevice {
    fn open(&mut self, cfg: StreamConfig) -> Result<(), AudioError>;
    fn close(&mut self);
}
```

### Lock-free SPSC ring buffer (`rtrb`)
- **Where:** `om-engine` (planned; the C++ version was `RingBuffer.h` at `cpp-final`)
- **Why:** pass audio between a worker thread and the audio callback without locks or allocation.
- **Reuse when:** any realtime thread needs data from a non-realtime producer (decode, analysis results, MIDI events).
- **Notes:** `rtrb::RingBuffer::new(capacity)` returns a `(Producer, Consumer)` pair. Exactly one producer and one consumer thread, and the pair must be created before the audio thread starts.

### Epoch handshake (producer-owned reset)
- **Where:** `om-engine` deck streaming (planned; the C++ version was `Deck.h` and the `DeckDecoder` source at `cpp-final`)
- **Why:** seek/load without the producer resetting a ring the consumer is reading.
- **Reuse when:** a realtime consumer must discard queued data after a state change.
- **Notes:** every ring item and seek request carries a `u64` epoch. The consumer drops items whose epoch is older than the current one, so the producer never touches the ring's read side.

### Callback trait (Observer-style)
- **Where:** `om-engine` (planned: the processing trait the output callback calls; the C++ version was `AudioCallback` at `cpp-final`)
- **Why:** engine units plug into any device, real or mock.
- **Reuse when:** adding mixer/FX stages that process audio blocks.
- **Notes:** use generics on the hot path; `Box<dyn Trait>` only at the composition root, where it is set up once.

```rust
pub trait AudioProcessor: Send {
    fn process(&mut self, out: &mut [f32]);
}
```

### Lock-free metrics monitor (atomics)
- **Where:** `om-audio` latency monitor (planned; the C++ version was `LatencyMonitor.h` at `cpp-final`)
- **Why:** expose realtime stats (latency, xruns) to UI/tests without locking the audio thread.
- **Reuse when:** adding any realtime telemetry.
- **Notes:** `AtomicU32`/`AtomicU64` counters written with `Relaxed` on the audio thread and read by the UI thread. Use `Acquire`/`Release` only if a counter must order other data.

### Crate per area (static library per area)
- **Where:** each `crates/om-<area>` in the Cargo workspace (`Cargo.toml` `members`).
- **Why:** shared, independently testable modules.
- **Reuse when:** every new area (analysis, library, midi, ui).
- **Notes:** dependencies point toward `om-core` (see `agents.md`). Library crates never depend on the `openmixer` binary.

### Command enum (control surface)
- **Where:** `om-core` (planned: typed command enum and snapshot types)
- **Why:** MIDI, UI and tests drive the engine through one typed API; the control thread sends commands and the engine applies them on the audio thread.
- **Reuse when:** adding any user-controllable engine unit: add a variant, handle it in the engine, and send it through the command channel.
- **Notes:** commands cross the control/audio boundary by value through an `rtrb` ring. Parsing text ids (for example MIDI map targets such as `ch0.fader`) into the enum happens on the control thread and returns `Err` for unknown ids (planned). The C++ tree used string ids for every call; the Rust port keeps strings only at the mapping boundary.

### Small units composed by a coordinator (struct composition)
- **Where:** `om-engine` (planned): the channel strip, EQ, crossfader and meter types composed by the mixer; shared filter maths in `om-dsp`.
- **Why:** one responsibility per module; DSP is reusable by FX and tests without the mixer.
- **Reuse when:** a unit is made of smaller units with their own state. Store them as fields and give each a `process` method.

### Data-driven map + trace replay harness (planned)
- **Where:** `om-midi` (planned: mapping as serde-loaded data, a trace replayer and a recording target; the C++ version was `MidiMap.{h,cpp}` and `MidiTrace` at `cpp-final`)
- **Why:** controller mappings are user files, not code; mappings are tested by replaying recorded MIDI with no hardware.
- **Reuse when:** any user-editable config or hardware-driven feature (HID, OSC): parse into a plain struct, apply through the command enum, test with recorded input.

### Grid trait with mock implementations (BeatGrid)
- **Where:** `om-core` (planned: the read-only `BeatGrid` trait). The C++ version lived in the engine header at `cpp-final`.
- **Why:** sync/quantize can be built and tested before analysis exists; analysis plugs in by implementing `BeatGrid`. Keeping the trait in `om-core` lets `om-analysis` implement it without depending on `om-engine`.
- **Reuse when:** a feature depends on data another phase produces: define the minimal read-only trait first, test with a mock.
- **Notes:** `Sync::step` is a pure function of deck states (control thread, ~20-50 Hz); only `apply` sends a rate command. Grids are shared immutably as `Arc<dyn BeatGrid + Send + Sync>`.

### Checking decorator for headless runs (OutputChecker)
- **Where:** `crates/om-audio/tests/soak/` (the C++ version was `tools/soak/OutputChecker.h` at `cpp-final`)
- **Why:** wrap any `AudioProcessor` to count NaN/Inf/overflow samples on the audio thread without changing the engine.
- **Reuse when:** long-running or fuzz-style runs (soak, golden-audio, plugin stress) need output assertions.
- **Notes:** a generic wrapper struct `Checked<P: AudioProcessor>` with atomic counters; no allocation in `process`.

### Plugin registry / extension points (deferred)
- **Where:** `om-plugins` (deferred to its wave). The C++ version was `src/plugins` at `cpp-final`.
- **Why:** users add audio stages, MIDI maps and parameter targets without touching core and without any signing or allow-list.
- **Reuse when:** a new feature should be replaceable: add a registration method to the owning registry, never a hardcoded path.
- **Notes:** the design picks CLAP-first if plugins are needed. Loading a dynamic library needs `unsafe`, so it belongs in the dedicated FFI crate, not in `om-plugins`. Until then, keep extension points as traits and registries inside their crates.

### Theme tokens + theme registry (extension point, planned)
- **Where:** `om-ui` (planned: a serde-loaded theme struct of design tokens and a registry). The C++ version was `ThemeTokens.h` and `ThemeRegistry` at `cpp-final`.
- **Why:** widgets never hard-code colours or sizes; they read tokens, so a theme (built-in, user file or plugin) can restyle everything live.
- **Reuse when:** adding any widget. Read tokens at render time, so a theme switch needs no rebuild.
- **Notes:** UI thread only. Precedence per id: User > Plugin > BuiltIn; a missing key inherits the built-in dark theme, and an invalid value rejects that file only. The built-in theme is compiled in and tested to match its shipped file.

### Prefetched jump slot (two-slot handoff, planned)
- **Where:** `om-engine` loop prefetch (planned; the C++ version was `LoopPrefetch.{h,cpp}` at `cpp-final`)
- **Why:** the audio thread jumps (loop wrap) without waiting for the decoder to refill the ring.
- **Reuse when:** any realtime jump target that can be predicted (hotcues, saved loops, sampler pads).
- **Notes:** two preallocated slots with atomic state. The decoder fills the slot the audio thread is not holding; the audio thread marks a slot in use and re-checks the published index with `SeqCst`. Ring-move requests carry the epoch so stale requests are dropped.

### Snapshot publication with `arc-swap` (replaces the seqlock)
- **Where:** `om-core` snapshot types, published from `om-engine` (planned; the C++ version was the seqlock in `LoopState.h` at `cpp-final`)
- **Why:** audio and decoder threads need a consistent {start, end, enabled} without locks.
- **Reuse when:** a few fields change rarely from control threads and must be read together in realtime or by the UI.
- **Notes:** the writer builds a new immutable snapshot and calls `ArcSwap::store`; readers call `load()` and keep the guard for the block. No retry loop is needed. Confirm the reader path does not allocate with `assert_no_alloc` in tests.

### Effect template method + generic wrapper (FX rack, planned)
- **Where:** `om-dsp` (the `Effect` trait) and `om-engine` (the `FxUnit<E: Effect>` wrapper and `FxRack`). The C++ version was `src/engine/fx/` at `cpp-final`.
- **Why:** each algorithm only renders a fully wet signal; enable ramps, smoothed wet/dry, tempo smoothing, tail ring-out and idle bypass live once in the wrapper, so new effects stay small.
- **Reuse when:** adding an effect or any per-block processor that needs the same enable/mix/tail behaviour: implement `Effect` and declare its parameters as data.
- **Notes:** the trait's provided methods (`prepare`, `process`) call the required `render`. This is the Rust form of the C++ non-virtual template method. The wrapper is generic, so dispatch is static on the hot path. Buffers are sized in `prepare` (off the audio thread) and processed in fixed chunks with no allocation.

### Name registry for effects (planned)
- **Where:** `om-engine` FX factory (planned; the C++ version was `FxFactory` at `cpp-final`)
- **Why:** rack layouts are lists of names, so effects are added by registering a constructor under a name without touching core; built-ins cannot be silently overridden.
- **Reuse when:** any user-extensible family of processors (sampler voices, analysis stages): register constructors by name, build from data.
- **Notes:** `HashMap<&'static str, fn() -> Box<dyn Effect>>` or similar, built on the control thread. Construction never happens on the audio thread.

### Data-driven presets (planned)
- **Where:** `om-engine` FX presets (planned; the C++ version was `FxPresets` at `cpp-final`)
- **Why:** presets are plain data applied through the same command path, so they can load from files.
- **Reuse when:** shipping starting points for any parameterised unit (EQ curves, mapping defaults).
- **Notes:** `const` tables for built-in presets; serde-loaded files for user presets.

### Background job service (worker threads + shared job handle, planned)
- **Where:** `om-analysis` job runner (planned; the C++ version was `AnalysisService`, `AnalysisJob` and `JobRunner` at `cpp-final`)
- **Why:** run slow work (decode + analysis) off the UI/audio threads with progress, cancellation and waiting, without callers owning threads.
- **Reuse when:** any long-running background task (library scan, stem precompute, export).
- **Notes:** `submit()` returns `Arc<AnalysisJob>`, shared by caller and worker. Cancellation is an `AtomicBool` polled by the job. Listeners run on the worker after the final status. `Drop` on the service cancels pending jobs and joins the worker threads. The job body is a plain function, so it is testable without threads.

### Content-addressed result cache (planned)
- **Where:** `om-analysis` cache (planned; the C++ version was `ResultCache` and `SqliteResultCache` at `cpp-final`)
- **Why:** analysis is expensive; results key on the SHA-256 of the audio bytes plus producer id and version, so renames and moves keep results and bumping a version invalidates old rows.
- **Reuse when:** caching anything derived from file content (waveforms, stems, artwork thumbnails).
- **Notes:** the cache is a trait with a `rusqlite` implementation. Tests use `Connection::open_in_memory()`. Payloads are JSON strings owned by each analyser.

### Estimator + analyser wrapper + engine adapter (planned)
- **Where:** `om-analysis` (pure estimator functions, an `Analyzer` wrapper with `to_json`/`from_json`, and an adapter that implements `om-core::BeatGrid`)
- **Why:** algorithms stay testable without the service; the cached payload is plain data; the engine sees an immutable snapshot through its trait, so analysis never links the engine.
- **Reuse when:** every analysis feature (key, waveform, structure, stems): pure estimator, then the wrapper, then an adapter for the consumer's trait.

### Pyramid of levels (multi-resolution data, planned)
- **Where:** `om-analysis` waveform data (planned; the C++ version was `WaveformData` at `cpp-final`)
- **Why:** zoomed and overview views read pre-reduced data instead of scanning samples per paint.
- **Reuse when:** any per-time data drawn at many zooms (energy, stems, spectrogram).
- **Notes:** only the finest level is persisted; coarser levels are derived on load. Merge rules must be associative (min/max, RMS via mean of squares).

### Licence-gated, verified download (planned)
- **Where:** `om-stems` (planned; the C++ version was `ModelManifest`, `WeightManager` and `Downloader` at `cpp-final`)
- **Why:** third-party weights/assets are fetched only after the user accepts their licence and only installed if the SHA-256 matches the manifest.
- **Reuse when:** any large optional asset not shipped in the repo (models, sample packs, skins from a catalogue).
- **Notes:** a `Downloader` trait with a URL implementation. Run it off the UI thread. Manifests are JSON so users can point at mirrors. Tests use `file://` URLs, no network.

### Analyzer registry (extension point, planned)
- **Where:** `om-analysis` (planned; the C++ version was `Analyzer.h` and `AnalyzerRegistry` at `cpp-final`)
- **Why:** built-in and plugin analysers are looked up by id.
- **Reuse when:** adding any analysis feature; register an analyser, do not special-case it in the service.
- **Notes:** `analyze(&self)` may run concurrently. The registry hands out `Arc<dyn Analyzer + Send + Sync>`, so removal during a running job is safe.

### Safe wrapper over a native library (pimpl replacement, planned)
- **Where:** `om-stretch` (Rubber Band R3 via FFI, behind a `Stretcher` trait; see `docs/spikes/DECISIONS.md`) (the C++ version was `TimeStretch.{h,cpp}` at `cpp-final`)
- **Why:** keeps the dependency's raw types out of the engine's public API and states the real-time contract in one place.
- **Reuse when:** wrapping any external DSP or codec library (ONNX Runtime via `ort` is the Rust path for stems).
- **Notes:** the struct owns the native handle and frees it in `Drop`; a private module replaces the pimpl. `prepare()` allocates (control thread); other calls are audio-thread and allocation-free. Only the wrapper's module may contain `unsafe`, each block with a `// SAFETY:` line.

### Forward-only migration list (planned)
- **Where:** `om-library` migrations (planned; the C++ version was `Migrations.{h,cpp}` at `cpp-final`)
- **Why:** the schema evolves by appending numbered migrations; each runs in its own transaction and is recorded in `schema_version`.
- **Reuse when:** any later subsystem needs library tables (search index, history, plugin tables): append a migration with the next version.
- **Notes:** the Rust version starts a fresh database (no migration from the C++ schema, per the design). A database newer than the list is refused. Never edit a shipped migration.

### DAO per entity over an RAII transaction (planned)
- **Where:** `om-library` (planned; the C++ version was `Database`, `Statement`, `TrackDao` and `CueDao` at `cpp-final`)
- **Why:** keeps SQL out of callers; `rusqlite::Transaction` rolls back on drop, and `savepoint()` nests, so DAO calls compose and roll back together.
- **Reuse when:** adding a table: add a DAO that borrows `&Connection` (or `&mut Transaction`) and returns plain value records.
- **Notes:** one writer thread owns the write connection; the UI opens its own read-only connection (WAL). Dense `position` columns keep ordered child sets.

### Metadata reader registry (extension point, planned)
- **Where:** `om-library` ingest (planned; the C++ version was `MetadataReader` and `Ingestor` at `cpp-final`; tags via `lofty`)
- **Why:** tag formats vary; ingest asks the registry for the reader owning a file's extension, so new readers need no ingest change.
- **Reuse when:** adding a tag source or a new supported file type: implement the `MetadataReader` trait and register it. Later registrations win per extension.
- **Notes:** the registry decides which files are scanned at all. Ingest state (mtime, size, missing) lives in its own table, so the track record stays a pure value.

### Background rescan with injectable clock (planned)
- **Where:** `om-library` watch folders (planned; the C++ version was `WatchFolders` at `cpp-final`)
- **Why:** a worker thread owns its own database connection and re-checks the folders on a short poll; tests drive time with a fake clock instead of sleeping.
- **Reuse when:** any periodic background task over the library (import refresh, analysis queue feeding).
- **Notes:** a `Clock` trait with a system and a fake implementation. One connection per thread (WAL + busy timeout). `Drop` on the watcher signals the worker to stop between files and joins it.

### Search field registry + compile-to-SQL (extension point, planned)
- **Where:** `om-library` search (planned; the C++ version was `Search{Fields,Parser,Compiler,Terms,Engine}` at `cpp-final`; `proptest` covers the parser)
- **Why:** a query string parses to generic filter terms; each registered field turns its term into SQL text made of code-constant names and `?` placeholders, with user values only in the bound parameters.
- **Reuse when:** adding a filterable or sortable attribute: register a field with the search engine.
- **Notes:** functions return `Result<SqlFragment, SearchError>` (thiserror), so a bad field or value fails rather than returning "no results". Custom compile functions must never splice user text into the SQL string. Text match is LIKE-contains with escaped wildcards.

### Importer registry + parse/merge split (extension point, planned)
- **Where:** `om-library` import (planned; the C++ version was `LibraryImporter`, `ItunesImporter`, `RekordboxImporter` and `ImportWriter` at `cpp-final`)
- **Why:** importers only turn a foreign XML document into a neutral `ImportedLibrary` (no database code); one writer merges any importer's output in a single transaction, so every format gets the same dedupe and rollback behaviour.
- **Reuse when:** adding another library format (register an importer) or another bulk write that must be all-or-nothing and re-runnable.
- **Notes:** merge policy (local data wins, imports fill gaps) is documented in the merge module. Re-import is idempotent. A failed lookup aborts the import rather than counting as "new".

### Owner-thread pump over a background service (PrepQueue, planned)
- **Where:** `om-library` prep queue (planned; the C++ version was `PrepQueue` at `cpp-final`)
- **Why:** analysis workers must not touch the database connection, so their listeners only copy the final job snapshot into a mutex-guarded inbox (or send it over a channel); the thread that owns the connection calls `pump()` to apply results, update rows, notify observers and submit more.
- **Reuse when:** a persisted queue feeds the analysis service or another worker pool and results must land in the database.
- **Notes:** at most `max_in_flight` jobs sit in the service, so priority and order stay editable. Progress is read from the job on `list()` rather than written to the database. `rusqlite::Connection` is `Send` but not shared, so only the owner thread uses it.

### Data-driven main layout + named slots (extension point, planned)
- **Where:** `om-ui` layout (planned; the C++ version was `MainLayout` at `cpp-final`)
- **Why:** the window is a list of rows of cells, each a deck index or a named slot; `compute_layout` turns it into rectangles without any UI objects, so a different arrangement is a data change.
- **Reuse when:** adding a panel (mixer, library, a plugin view): name a slot in the spec and fill it through the layout API. Empty slots show a placeholder.
- **Notes:** the layout function is pure and unit-tested. Decks the engine lacks are skipped.

### Mixer widgets: binding + descriptor-built FX controls (planned)
- **Where:** `om-ui` mixer views (planned; the C++ version was `MixerBinding`, `MixerPanel` and `FxPanel` at `cpp-final`)
- **Why:** widgets read snapshots and send commands; they never write engine state directly. FX controls are built from parameter descriptors, so a new effect gets controls with no UI code.
- **Reuse when:** adding a control surface for another engine object, or another meter.
- **Notes:** the meter ballistics are a plain struct (peak, release, hold, clip latch, dt-based) with no drawing code, so tests use synthetic levels. Thresholds and colours are theme tokens.

### Library services composition (planned)
- **Where:** `om-ui` (planned; the C++ version was `LibraryServices`, `TrackSource` and `TrackLoader` at `cpp-final`)
- **Why:** one place owns start-up and shutdown order (library writer, result cache, analysis service, prep queue, watch folders). Rust drops struct fields in declaration order (first declared, first dropped), so declare the fields in reverse start-up order to tear down in reverse. Local variables are the opposite (see the composition root entry).
- **Reuse when:** a view needs track rows (implement a row-source trait with `count()` and `fetch(offset, limit)`), a new sidebar node, or another load gesture (send a load command).
- **Notes:** the UI thread owns the library connection and the prep queue (a timer calls `pump()`). Watch folders have their own thread and connection and only bump an atomic counter. Widgets never open databases.

### Widgets send commands only (replaces DeckBinding)
- **Where:** `om-ui` deck views (planned; the C++ version was `DeckBinding` and the deck components at `cpp-final`)
- **Why:** every control sends a typed command through the command enum, the same path MIDI mappings use; reads come from published snapshots.
- **Reuse when:** any new control surface. Tests send commands through a recording sink and assert both the command and the engine's resulting state.
- **Notes:** toggles send the opposite of the engine state and mirror it back when the snapshot changes.

### Preferences model + applier (planned)
- **Where:** `openmixer` binary and `om-core` settings types (planned; the C++ version was `Preferences`, `PreferencesApplier` and `SettingsPanel` at `cpp-final`)
- **Why:** one plain, versioned, serde-loaded model; subsystems sit behind a targets trait so the applier is tested with mocks and pushes only the sections that changed.
- **Reuse when:** adding a setting (a field with a default in the model, plus a target method if a subsystem must react) or a settings tab.
- **Notes:** target methods must be idempotent. The audio section talks to a settings-model trait; tests use a fake that opens no device. Watch folders live in the library database, not in the preferences.

### Device-free composition (planned)
- **Where:** `crates/openmixer` (the composition root; the C++ version was `AppComposition` at `cpp-final`)
- **Why:** engine, command routing, MIDI mapper, preferences, library and views are wired once, so the UI gate tests the same wiring the app ships.
- **Reuse when:** adding any window-level wiring: put it in the composition function; the real binary and the test rig only swap in the audio device.
- **Notes:** the composition root is the only place that holds concrete types and `Box<dyn ...>` objects. Local variables in the composition function drop in reverse declaration order, so declare objects that others reference before them; they then drop after those references (struct fields work the other way, see Library services composition).

### UI gate: rig + scripted operator (planned)
- **Where:** `crates/openmixer/tests/ui_gate.rs` (the C++ version was the `ui_gate` source and `UiRig.h` at `cpp-final`)
- **Why:** the soak drives the engine directly; the gate proves the wiring by using the widgets' own paths and then asserting the widget and the engine agree.
- **Reuse when:** a new panel needs end-to-end coverage: add its actions to the scripted scenario and count them, so the gate fails if an area is never exercised.
- **Notes:** the gate uses the same composition root as the binary, so window wiring is never duplicated in the rig. Labels: `ui_gate` (short, default), `uigate_long` (10 min, planned), `smoke` (`openmixer --smoke`).

## Replaced in the Rust port

These C++ patterns have no direct Rust copy. What replaces each one is listed.

- **Abstract base classes and virtual dispatch:** traits and generics. Use `dyn` only where a runtime choice is needed (composition root, registries).
- **`std::unique_ptr` ownership and non-copyable macros:** `Box`, `Vec` and `Arc`; types are move-only unless they derive `Clone`.
- **RAII wrappers around handles:** `Drop` impls on the wrapping struct (for native handles, transactions and threads).
- **`std::atomic` with explicit memory orders:** `std::sync::atomic` types with the same orderings, used in the same few places.
- **Templates:** generics with trait bounds.
- **Seqlock and retry-on-contention snapshots:** `arc-swap` immutable snapshots.
- **String-id `ParameterRouter` and prefix registry:** the typed command enum (see "Command enum"). String ids are parsed only at the MIDI mapping boundary.
- **Pimpl:** a private module inside the crate.
- **Framework result types and logger:** `Result<T, E>` with `thiserror` enums, and `tracing` for logging.
- **Leak detector macros:** no equivalent needed; ownership makes leaks rare and `assert_no_alloc` covers the audio thread.
- **Message-thread-only checks and `VBlankAttachment`:** the GPUI frame loop; state that is UI-only lives on the UI thread by construction.

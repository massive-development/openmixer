# Design Patterns in Use

Registry for agents and contributors. Check here before starting a large implementation; add an entry when you introduce or adopt a pattern.

Entries describe the Rust design. Entries marked **(planned)** were written as design before the Rust code existed, and the marker may lag the code (check the crate); **(deferred)** patterns wait for later work. Idioms that replace common object-oriented patterns are listed under "Idioms and their Rust equivalents".

## Entry template

- **Pattern:** name
- **Where:** crate/module
- **Why:** problem it solves (one line)
- **Reuse when:** situations that should use it
- **Notes:** constraints (threading, ownership)

## Current patterns

### Trait + mock implementation (Strategy / Adapter)
- **Where:** `om-audio` (`AudioDevice` trait with a `cpal` backend and a mock device; MIDI uses `MidiBackend` in `om-midi`)
- **Why:** swap hardware backends and run headless in CI.
- **Reuse when:** wrapping any external I/O (MIDI input, file/network, HID) so tests need no hardware.
- **Notes:** callers depend on the trait only.

```rust
pub trait AudioDevice: Send {
    fn start(&mut self, callback: Box<dyn AudioCallback>) -> Result<(), AudioError>;
    fn stop(&mut self) -> Option<Box<dyn AudioCallback>>;
}
```

### Lock-free SPSC ring buffer (`rtrb`)
- **Where:** `om-core` (`spsc`, a wrapper over `rtrb::RingBuffer`) and `om-engine` (`stage.rs`, the insert command and retired-stage rings)
- **Why:** pass audio between a worker thread and the audio callback without locks or allocation.
- **Reuse when:** any realtime thread needs data from a non-realtime producer (decode, analysis results, MIDI events).
- **Notes:** `rtrb::RingBuffer::new(capacity)` returns a `(Producer, Consumer)` pair. Exactly one producer and one consumer thread, and the pair must be created before the audio thread starts.

### Epoch handshake (producer-owned reset)
- **Where:** `om-engine` deck streaming (`deck/shared.rs`, `deck/decoder.rs`, `deck/playback.rs`)
- **Why:** seek/load without the producer resetting a ring the consumer is reading.
- **Reuse when:** a realtime consumer must discard queued data after a state change.
- **Notes:** control bumps `request_epoch` and the decoder copies it to `ack_epoch` once it has reset its reader (both `AtomicU32`). The audio side reads both in `playback.rs` and does not consume ring data until they match; the stale buffered data is then skipped on the consumer side, so the producer never touches the ring's read side.

### Callback trait (Observer-style)
- **Where:** `om-engine` (`AudioCallback` in `callback.rs`, the block processor a device drives; `ProcessStage` in `stage.rs` for insert stages)
- **Why:** engine units plug into any device, real or mock.
- **Reuse when:** adding mixer/FX stages that process audio blocks.
- **Notes:** use generics on the hot path; `Box<dyn Trait>` only at the composition root, where it is set up once. Insert stages implement `ProcessStage` and are swapped in through the command ring.

```rust
pub trait AudioCallback: Send {
    fn prepare(&mut self, _sample_rate: f64, _max_block_size: usize) {}
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]);
}
```

### Lock-free metrics monitor (atomics)
- **Where:** `om-engine` latency monitor (`latency.rs`, `LatencyMonitor`) and the cpal xrun counters in `om-audio`
- **Why:** expose realtime stats (latency, xruns) to UI/tests without locking the audio thread.
- **Reuse when:** adding any realtime telemetry.
- **Notes:** `AtomicU32`/`AtomicU64` counters written with `Relaxed` on the audio thread and read by the UI thread. Use `Acquire`/`Release` only if a counter must order other data.

### Crate per area (static library per area)
- **Where:** each `crates/om-<area>` in the Cargo workspace (`Cargo.toml` `members`).
- **Why:** shared, independently testable modules.
- **Reuse when:** every new area (analysis, library, midi, ui).
- **Notes:** dependencies point toward `om-core` (see `agents.md`). Library crates never depend on the `openmixer` binary.

### Command enum (control surface)
- **Where:** `om-core` (planned: typed command enum and snapshot types; today the control surface is the string-id `ParameterTarget` trait and `ParameterRouter`, and the insert ring carries a private `Command` in `om-engine/src/stage.rs`)
- **Why:** MIDI, UI and tests drive the engine through one typed API; the control thread sends commands and the engine applies them on the audio thread.
- **Reuse when:** adding any user-controllable engine unit: add a variant, handle it in the engine, and send it through the command channel.
- **Notes:** planned. Commands would cross the control/audio boundary by value through an `rtrb` ring, and parsing text ids (for example MIDI map targets such as `ch0.fader`) would happen on the control thread, returning `Err` for unknown ids. Until then, `ParameterTarget::set_parameter` returns `false` for unknown ids and widgets and MIDI both use that path.

### Small units composed by a coordinator (struct composition)
- **Where:** `om-engine` (`mixer/channel.rs` and `mixer/control.rs`: the channel strip composed from `ThreeBandEq`, `SweepFilter`, inserts and `Crossfader` gains); shared filter maths in `om-dsp`.
- **Why:** one responsibility per module; DSP is reusable by FX and tests without the mixer.
- **Reuse when:** a unit is made of smaller units with their own state. Store them as fields and give each a `process` method.

### Data-driven map + trace replay harness
- **Where:** `om-midi` (`map.rs` JSON mapping, `mapper.rs`, `trace.rs` replay and `writer.rs` recording)
- **Why:** controller mappings are user files, not code; mappings are tested by replaying recorded MIDI with no hardware.
- **Reuse when:** any user-editable config or hardware-driven feature (HID, OSC): parse into a plain struct, apply through the command enum, test with recorded input.

### Grid trait with mock implementations (BeatGrid)
- **Where:** `om-engine` (`grid.rs`: the read-only `BeatGrid` trait and the `FixedTempoGrid` constant-tempo grid; `om-analysis` implements it for `AnalysedBeatGrid` in `beat_grid.rs`). - **Why:** sync/quantize can be built and tested before analysis exists; analysis plugs in by implementing `BeatGrid`. Keeping the trait in `om-core` lets `om-analysis` implement it without depending on `om-engine`.
- **Reuse when:** a feature depends on data another phase produces: define the minimal read-only trait first, test with a mock.
- **Notes:** `Sync::step` is a pure function of deck states (control thread, ~20-50 Hz); only `apply` sends a rate command. Grids are shared immutably as `Arc<dyn BeatGrid>` (the trait requires `Send + Sync`). `om-analysis` depends on `om-engine` for this trait, so the trait has to stay in the engine crate.

### Checking decorator for headless runs (OutputChecker)
- **Where:** `crates/om-audio/tests/soak/checker.rs` (`OutputChecker<C: AudioCallback>`)
- **Why:** wrap any `AudioProcessor` to count NaN/Inf/overflow samples on the audio thread without changing the engine.
- **Reuse when:** long-running or fuzz-style runs (soak, golden-audio, plugin stress) need output assertions.
- **Notes:** a generic wrapper struct `OutputChecker<C: AudioCallback>` with atomic counters; no allocation in `process`.

### Plugin registry / extension points (in-process; dynamic loading deferred)
- **Where:** `om-plugins` (`api.rs` plugin and host traits, `registry.rs`, `catalog.rs`, `loader.rs` as a stub). - **Why:** users add audio stages, MIDI maps and parameter targets without touching core and without any signing or allow-list.
- **Reuse when:** a new feature should be replaceable: add a registration method to the owning registry, never a hardcoded path.
- **Notes:** the design picks CLAP-first if plugins are needed. Loading a dynamic library needs `unsafe`, so it belongs in the dedicated FFI crate, not in `om-plugins`. Until then the registry runs in-process only, and extension points stay as traits and registries inside their crates.

### Theme tokens + theme registry (extension point)
- **Where:** `om-ui` `theme/` (`tokens.rs` `ThemeTokens`, `json.rs` loader, `registry.rs` `ThemeRegistry`, `builtin.rs`).
- **Why:** widgets never hard-code colours or sizes; they read tokens, so a theme (built-in, user file or plugin) can restyle everything live.
- **Reuse when:** adding any widget. Read tokens at render time, so a theme switch needs no rebuild.
- **Notes:** UI thread only. Precedence per id: User > Plugin > BuiltIn; a missing key inherits the built-in dark theme, and an invalid value rejects that file only. The built-in theme is compiled in and tested to match its shipped file.

### Prefetched jump slot (two-slot handoff)
- **Where:** `om-engine` loop prefetch (`deck/prefetch.rs`)
- **Why:** the audio thread jumps (loop wrap) without waiting for the decoder to refill the ring.
- **Reuse when:** any realtime jump target that can be predicted (hotcues, saved loops, sampler pads).
- **Notes:** two preallocated slots with atomic state. The decoder fills the slot the audio thread is not holding; the audio thread marks a slot in use and re-checks the published index with `SeqCst`. Ring-move requests carry the epoch so stale requests are dropped.

### Snapshot publication with `arc-swap` (replaces the seqlock)
- **Where:** planned for `om-core` snapshot types published from `om-engine`. Not yet adopted: `om-engine/src/deck/loop_state.rs` still uses a seqlock, and `arc-swap` is not in `Cargo.lock`.
- **Why:** audio and decoder threads need a consistent {start, end, enabled} without locks.
- **Reuse when:** a few fields change rarely from control threads and must be read together in realtime or by the UI.
- **Notes:** the writer builds a new immutable snapshot and calls `ArcSwap::store`; readers call `load()` and keep the guard for the block. No retry loop is needed. Confirm the reader path does not allocate with `assert_no_alloc` in tests.

### Effect template method + generic wrapper (FX rack)
- **Where:** `om-engine` `fx/` (the `Effect` trait in `effect.rs`, the `FxUnit` wrapper in `unit.rs`, `FxRack` in `rack.rs`). - **Why:** each algorithm only renders a fully wet signal; enable ramps, smoothed wet/dry, tempo smoothing, tail ring-out and idle bypass live once in the wrapper, so new effects stay small.
- **Reuse when:** adding an effect or any per-block processor that needs the same enable/mix/tail behaviour: implement `Effect` and declare its parameters as data.
- **Notes:** the trait's provided methods (`prepare`, `process`) call the required `render`. The wrapper is generic, so dispatch is static on the hot path. Buffers are sized in `prepare` (off the audio thread) and processed in fixed chunks with no allocation.

### Name registry for effects
- **Where:** `om-engine` FX factory (`fx/factory.rs`, `FxFactory`)
- **Why:** rack layouts are lists of names, so effects are added by registering a constructor under a name without touching core; built-ins cannot be silently overridden.
- **Reuse when:** any user-extensible family of processors (sampler voices, analysis stages): register constructors by name, build from data.
- **Notes:** a `BTreeMap<String, Creator>` where `Creator` is a boxed closure returning `Box<dyn Effect>`; the eight built-ins are registered first. Built on the control thread; construction never happens on the audio thread.

### Data-driven presets
- **Where:** `om-engine` FX presets (`fx/presets.rs`: `builtin_presets`, `find_preset`, `apply_preset`)
- **Why:** presets are plain data applied through the same command path, so they can load from files.
- **Reuse when:** shipping starting points for any parameterised unit (EQ curves, mapping defaults).
- **Notes:** built-in presets are a static table. Loading user preset files from disk is not built yet.

### Generic worker pool (FIFO job queue + job handle)
- **Where:** `om-core` (`jobs.rs` `WorkerPool<J>`, `JobHandle`, `JobCtx`); `om-analysis` `AnalysisService` is built on it.
- **Why:** one place for the queue, the N worker threads, progress, cancel, wait, panic containment and join-on-drop, so a feature supplies only a job type and a handler.
- **Reuse when:** a set of independent jobs runs in the background, served FIFO by a fixed number of threads, and callers want status, progress, cancel or a completion callback. Do not use it for timers or periodic scans (WatchFolders stays a dedicated thread) or for work that is already persisted and paced by its own owner (PrepQueue feeds the pool-backed `AnalysisService`).
- **Notes:** `new(workers, name, handler)`; `submit(job)` returns a `JobHandle` (id, status Queued/Running/Done/Failed/Cancelled, progress 0..1, `cancel()`, `wait(timeout)`). The handler gets a `&JobCtx` and must poll `is_cancelled()`; a running job is never killed. A panic fails only its job (`Failed` with the message) and the worker carries on. `submit_with_listener` runs a panic-guarded callback on the worker once the job is final. `cancel_all()`, `queued_len()`, and `Drop` (cancel queued and running, then join) complete the API. Waiting is condvar-based; no polling loops in the pool.

### Background job service (worker threads + shared job handle)
- **Where:** `om-analysis` (`service.rs` `AnalysisService`, `job.rs` `AnalysisJob`, `job_runner.rs` the job body)
- **Why:** run slow work (decode + analysis) off the UI/audio threads with progress, cancellation and waiting, without callers owning threads.
- **Reuse when:** any long-running background task (library scan, stem precompute, export).
- **Notes:** runs on the generic `WorkerPool` (see above); status, progress and cancellation come from its `JobHandle`, and `AnalysisJob` adds the results. `submit()` returns `Arc<AnalysisJob>`. Listeners run on the worker after the final status. `Drop` on the service cancels pending jobs and joins the worker threads. The job body is a plain function, so it is testable without threads.

### Content-addressed result cache
- **Where:** `om-analysis` cache (`result_cache.rs` `ResultCache` trait, `sqlite_cache.rs` `SqliteResultCache`)
- **Why:** analysis is expensive; results key on the SHA-256 of the audio bytes plus producer id and version, so renames and moves keep results and bumping a version invalidates old rows.
- **Reuse when:** caching anything derived from file content (waveforms, stems, artwork thumbnails).
- **Notes:** the cache is a trait with a `rusqlite` implementation. Tests use `Connection::open_in_memory()`. Payloads are JSON strings owned by each analyser.

### Estimator + analyser wrapper + engine adapter
- **Where:** `om-analysis` (pure estimator modules such as `bpm`, `key` and `structure`, the `Analyzer` implementations, and `AnalysedBeatGrid` in `beat_grid.rs`, which implements `om-engine`'s `BeatGrid`)
- **Why:** algorithms stay testable without the service; the cached payload is plain data; the engine sees an immutable snapshot through its trait, so analysis never links the engine.
- **Reuse when:** every analysis feature (key, waveform, structure, stems): pure estimator, then the wrapper, then an adapter for the consumer's trait.

### Pyramid of levels (multi-resolution data)
- **Where:** `om-analysis` waveform data (`waveform_data.rs`, `WaveformData` with levels built by `merge_points`)
- **Why:** zoomed and overview views read pre-reduced data instead of scanning samples per paint.
- **Reuse when:** any per-time data drawn at many zooms (energy, stems, spectrogram).
- **Notes:** coarser levels are built by merging the finer points: min and max, and RMS as the square root of the mean of squares. Keep merge rules associative.

### Licence-gated, verified download
- **Where:** `om-stems` (`manifest.rs` model manifest, `weights.rs` `Downloader` trait with the URL implementation)
- **Why:** third-party weights/assets are fetched only after the user accepts their licence and only installed if the SHA-256 matches the manifest.
- **Reuse when:** any large optional asset not shipped in the repo (models, sample packs, skins from a catalogue).
- **Notes:** a `Downloader` trait with a URL implementation. Run it off the UI thread. Manifests are JSON so users can point at mirrors. Tests use `file://` URLs, no network.

### Analyzer registry (extension point)
- **Where:** `om-analysis` (`analyzer.rs` `Analyzer` trait, `registry.rs` `AnalyzerRegistry`)
- **Why:** built-in and plugin analysers are looked up by id.
- **Reuse when:** adding any analysis feature; register an analyser, do not special-case it in the service.
- **Notes:** `analyze(&self)` may run concurrently. The registry hands out `Arc<dyn Analyzer + Send + Sync>`, so removal during a running job is safe.

### Safe wrapper over a native library (pimpl replacement)
- **Where:** `om-stretch` (`rubberband.rs` `RubberBandStretcher` behind the `Stretcher` trait, FFI calls in `ffi.rs`; see `docs/spikes/DECISIONS.md`)
- **Why:** keeps the dependency's raw types out of the engine's public API and states the real-time contract in one place.
- **Reuse when:** wrapping any external DSP or codec library (ONNX Runtime via `ort` is the Rust path for stems).
- **Notes:** the struct owns the native handle and frees it in `Drop`; a private module replaces the pimpl. `prepare()` allocates (control thread); other calls are audio-thread and allocation-free. Only the wrapper's module may contain `unsafe`, each block with a `// SAFETY:` line.

### Forward-only migration list
- **Where:** `om-library` migrations (`migrations.rs`)
- **Why:** the schema evolves by appending numbered migrations; each runs in its own transaction and is recorded in `schema_version`.
- **Reuse when:** any later subsystem needs library tables (search index, history, plugin tables): append a migration with the next version.
- **Notes:** the Rust version starts a fresh database (no migration from older schemas). A database newer than the list is refused. Never edit a shipped migration.

### DAO per entity over an RAII transaction
- **Where:** `om-library` (`track_dao.rs` `TrackDao<'a>`, `cue_dao.rs` `CueDao<'a>`, `playlist_dao.rs`)
- **Why:** keeps SQL out of callers; `rusqlite::Transaction` rolls back on drop, and `savepoint()` nests, so DAO calls compose and roll back together.
- **Reuse when:** adding a table: add a DAO that borrows `&Connection` (or `&mut Transaction`) and returns plain value records.
- **Notes:** one writer thread owns the write connection; the UI opens its own read-only connection (WAL). Dense `position` columns keep ordered child sets.

### Metadata reader registry (extension point)
- **Where:** `om-library` ingest (`metadata.rs` `MetadataReader` trait and `MetadataReaderRegistry`, with the built-in `lofty` reader)
- **Why:** tag formats vary; ingest asks the registry for the reader owning a file's extension, so new readers need no ingest change.
- **Reuse when:** adding a tag source or a new supported file type: implement the `MetadataReader` trait and register it. Later registrations win per extension.
- **Notes:** the registry decides which files are scanned at all. Ingest state (mtime, size, missing) lives in its own table, so the track record stays a pure value.

### Background rescan with injectable clock
- **Where:** `om-library` watch folders (`watch_folders.rs` `WatchFolders`)
- **Why:** a worker thread owns its own database connection and re-checks the folders on a short poll; tests drive time with a fake clock instead of sleeping.
- **Reuse when:** any periodic background task over the library (import refresh, analysis queue feeding).
- **Notes:** `Clock` is a type alias for a closure returning the time in seconds (`Arc<dyn Fn() -> i64>`). The default is the system clock; tests inject a fixed one. The worker thread owns its own connection (WAL). `Drop` on the watcher signals the worker to stop and joins it.

### Search field registry + compile-to-SQL (extension point)
- **Where:** `om-library` search (`search_fields.rs` `SearchFieldRegistry`, `search_compiler.rs`, `search_parser.rs`; `proptest` covers the parser)
- **Why:** a query string parses to generic filter terms; each registered field turns its term into SQL text made of code-constant names and `?` placeholders, with user values only in the bound parameters.
- **Reuse when:** adding a filterable or sortable attribute: register a field with the search engine.
- **Notes:** functions return `Result<SqlFragment, SearchError>` (thiserror), so a bad field or value fails rather than returning "no results". Custom compile functions must never splice user text into the SQL string. Text match is LIKE-contains with escaped wildcards.

### Importer registry + parse/merge split (extension point)
- **Where:** `om-library` import (`library_importer.rs` `LibraryImporterRegistry`, `itunes_importer.rs`, `rekordbox_importer.rs`, `import_merge.rs`, `import_writer.rs`)
- **Why:** importers only turn a foreign XML document into a neutral `ImportedLibrary` (no database code); one writer merges any importer's output in a single transaction, so every format gets the same dedupe and rollback behaviour.
- **Reuse when:** adding another library format (register an importer) or another bulk write that must be all-or-nothing and re-runnable.
- **Notes:** merge policy (local data wins, imports fill gaps) is documented in the merge module. Re-import is idempotent. A failed lookup aborts the import rather than counting as "new".

### Owner-thread pump over a background service (PrepQueue)
- **Where:** `om-library` prep queue (`prep_queue.rs` `PrepQueue` with `pump()`, `prep_queue_dao.rs`)
- **Why:** analysis workers must not touch the database connection, so their listeners only copy the final job snapshot into a mutex-guarded inbox (or send it over a channel); the thread that owns the connection calls `pump()` to apply results, update rows, notify observers and submit more.
- **Reuse when:** a persisted queue feeds the analysis service or another worker pool and results must land in the database.
- **Notes:** at most `max_in_flight` jobs sit in the service, so priority and order stay editable. Progress is read from the job on `list()` rather than written to the database. `rusqlite::Connection` is `Send` but not shared, so only the owner thread uses it.

### Data-driven main layout + named slots (extension point)
- **Where:** `om-ui` layout (`main_layout.rs`, `compute_layout`)
- **Why:** the window is a list of rows of cells, each a deck index or a named slot; `compute_layout` turns it into rectangles without any UI objects, so a different arrangement is a data change.
- **Reuse when:** adding a panel (mixer, library, a plugin view): name a slot in the spec and fill it through the layout API. Empty slots show a placeholder.
- **Notes:** the layout function is pure and unit-tested. Decks the engine lacks are skipped.

### Mixer widgets: binding + descriptor-built FX controls
- **Where:** `om-ui` (`binding.rs`, `widgets/mixer.rs`, `widgets/fx_panel.rs`, `meter.rs`); drawn by `om-ui-gpui`
- **Why:** widgets read snapshots and send commands; they never write engine state directly. FX controls are built from parameter descriptors, so a new effect gets controls with no UI code.
- **Reuse when:** adding a control surface for another engine object, or another meter.
- **Notes:** the meter ballistics are a plain struct (peak, release, hold, clip latch, dt-based) with no drawing code, so tests use synthetic levels. Thresholds and colours are theme tokens.

### Library services composition
- **Where:** `om-ui` `library/` (`services.rs` `LibraryServices`, `track_source.rs`, `track_loader.rs`)
- **Why:** one place owns start-up and shutdown order (library writer, result cache, analysis service, prep queue, watch folders). Rust drops struct fields in declaration order (first declared, first dropped), so declare the fields in reverse start-up order to tear down in reverse. Local variables are the opposite (see the composition root entry).
- **Reuse when:** a view needs track rows (implement a row-source trait with `count()` and `fetch(offset, limit)`), a new sidebar node, or another load gesture (send a load command).
- **Notes:** the UI thread owns the library connection and the prep queue (a timer calls `pump()`). Watch folders have their own thread and connection and only bump an atomic counter. Widgets never open databases.

### Widgets send commands only (replaces DeckBinding)
- **Where:** `om-ui` `widgets/` and `binding.rs`; drawn by `om-ui-gpui`
- **Why:** every control sends a parameter id through the control surface (`ParameterTarget`), the same path MIDI mappings use; reads come from published snapshots.
- **Reuse when:** any new control surface. Tests send commands through a recording sink and assert both the command and the engine's resulting state.
- **Notes:** toggles send the opposite of the engine state and mirror it back when the snapshot changes.

### Preferences model + applier
- **Where:** `om-ui` (`preferences.rs` model, `preferences_applier.rs` applier, `settings.rs` pages) and `openmixer` (`app_targets.rs` implements the targets)
- **Why:** one plain, versioned, serde-loaded model; subsystems sit behind a targets trait so the applier is tested with mocks and pushes only the sections that changed.
- **Reuse when:** adding a setting (a field with a default in the model, plus a target method if a subsystem must react) or a settings tab.
- **Notes:** target methods must be idempotent. The audio section talks to a settings-model trait; tests use a fake that opens no device. Watch folders live in the library database, not in the preferences.

### Device-free composition
- **Where:** `crates/openmixer` (`composition.rs`, `AppComposition`, the composition root)
- **Why:** engine, command routing, MIDI mapper, preferences, library and views are wired once, so the UI gate tests the same wiring the app ships.
- **Reuse when:** adding any window-level wiring: put it in the composition function; the real binary and the test rig only swap in the audio device.
- **Notes:** the composition root is the only place that holds concrete types and `Box<dyn ...>` objects. Local variables in the composition function drop in reverse declaration order, so declare objects that others reference before them; they then drop after those references (struct fields work the other way, see Library services composition).

### UI gate: rig + scripted operator
- **Where:** `crates/openmixer/tests/ui_gate.rs`
- **Why:** the soak drives the engine directly; the gate proves the wiring by using the widgets' own paths and then asserting the widget and the engine agree.
- **Reuse when:** a new panel needs end-to-end coverage: add its actions to the scripted scenario and count them, so the gate fails if an area is never exercised.
- **Notes:** the gate uses the same composition root as the binary, so window wiring is never duplicated in the rig. Runs: `cargo test -p openmixer --test ui_gate` (about 8 s), `-- --ignored` (10 minutes), and `openmixer --smoke` for the window.

## Idioms and their Rust equivalents

What the code uses in place of common object-oriented patterns.

- **Abstract base classes and virtual dispatch:** traits and generics. Use `dyn` only where a runtime choice is needed (composition root, registries).
- **`std::unique_ptr` ownership and non-copyable macros:** `Box`, `Vec` and `Arc`; types are move-only unless they derive `Clone`.
- **RAII wrappers around handles:** `Drop` impls on the wrapping struct (for native handles, transactions and threads).
- **`std::atomic` with explicit memory orders:** `std::sync::atomic` types with the same orderings, used in the same few places.
- **Templates:** generics with trait bounds.
- **Seqlock and retry-on-contention snapshots:** planned `arc-swap` immutable snapshots. `LoopState` still uses a seqlock; see "Snapshot publication".
- **String-id `ParameterRouter` and prefix registry:** still in use. Control ids are strings on the `ParameterTarget` surface, and the typed command enum (see "Command enum") is planned, not built.
- **Pimpl:** a private module inside the crate.
- **Framework result types and logger:** `Result<T, E>` with `thiserror` enums, and `tracing` for logging.
- **Leak detector macros:** no equivalent needed; ownership makes leaks rare and `assert_no_alloc` covers the audio thread.
- **Message-thread-only checks and `VBlankAttachment`:** the GPUI frame loop; state that is UI-only lives on the UI thread by construction.

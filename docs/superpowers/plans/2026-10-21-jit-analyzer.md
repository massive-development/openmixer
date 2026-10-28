# Just-in-Time Analyzer Implementation Plan

> **Status (checked 2026-10-27): not built.** No step is ticked, and `git log` shows the plan was added once (commit 33c0e50) with no follow-up. None of its new files or crates exist (`om-net`, `om-analysis/src/incremental.rs`, `stream_job.rs`, `om-engine/src/deck/stream.rs`), and `StreamSource`, `HttpSpool` and `IncrementalAnalyzer` appear nowhere in `crates/`. It was not superseded by other work either: the open need is `TODO.md` item E2a (progressive waveform). The "synchronous today" list below was accurate on 2026-10-21 and has not been re-checked.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Load and play a track the moment its first bytes are readable, and analyse it in one streaming pass that follows the data (local file, growing download, HTTP), so waveform, BPM and key appear as the data arrives and our code never adds a wait.

**Architecture:** One pull-based decode pass feeds *incremental analysers* (`push` blocks, `snapshot` partials, `finish`). The pass reads through a `StreamSource` (a symphonia `MediaSource` that may not have all its bytes yet) and hashes the same bytes as it reads, so there is no separate hash pass and no whole-clip buffer. The deck uses the same source and plays what is available, reporting `buffering` instead of failing. Partial results go to the UI through a cheap polled handle, not events.

**Tech Stack:** Rust, symphonia (already used by `SourceReader`), `ureq` + rustls (already in the lockfile via `om-stems`), SQLite result cache.

**Spec:** `TODO.md` item E2a (Progressive waveform), `docs/PRODUCT.md` (smoothness budget), `docs/superpowers/specs/2026-10-18-rust-rewrite-design.md`.

## What is synchronous today (checked 2026-10-21)

The deck does *not* wait for analysis: `TrackLoader::load_track` (`om-ui/src/library/track_loader.rs:254`) loads the deck, then queues the track at priority. The waits that remain:

1. `Deck::load` (`om-engine/src/deck.rs:144`) runs `SourceReader::open` on the control (UI) thread. When the container has no frame count (`n_frames` is `None`, e.g. some VBR MP3s) `open` calls `count_frames`, a full decode of the file, on that thread. This is the real UI hitch.
2. `SourceReader::open` takes a `&Path` and `File`; it cannot read a file that is still downloading or a URL.
3. `run_analysis_job` (`om-analysis/src/job_runner.rs`) hashes the whole file (0-5%), decodes the whole file into one `Vec<f32>` (`FileClipSource::load`), then runs each analyser on the whole clip; every result lands at the end. The cache is keyed by the full-content hash, so nothing can be looked up or published before the last byte.
4. `Analyzer::analyze(&AudioClip) -> Option<String>` is whole-clip only.

## Global Constraints

- No `unsafe` outside FFI crates; no allocation or locks on the audio thread (`agents.md`). All new work runs on control, decoder or worker threads.
- Tests play no audio (mock device only).
- WSL: `nice -n 19`, `CARGO_BUILD_JOBS=3`, one cargo process at a time.
- Existing `Analyzer` trait, `ClipSource` and cached JSON formats stay readable: plugins and cached results from `waveform` v1, `bpm`, `key`, `structure` keep working. Bump an analyser's `version()` only if its final output changes.
- `docs/PRODUCT.md` frame budget applies: publishing partial data must not add per-frame work in the strips beyond reading one handle.
- One commit per task.

## Review Focus

1. A file whose header has no frame count (VBR MP3 without Xing): loading must return at once; length is an estimate that is corrected when known.
2. A download that stalls or ends early (truncated file): deck shows `buffering` then a failed-source state; analysis finishes `Failed`, nothing partial is written to the cache as if complete.
3. A seek past the downloaded bytes: the deck outputs silence and `buffering`, resumes when bytes arrive; no audio-thread block.
4. Loading track B while track A's analysis is mid-stream: A's pass is cancelled, its partial result is not shown on B or cached as complete.
5. A track with a cached full result: no decode pass at all; the waveform is complete on the first frame after load.

## File Structure

| File | Responsibility |
|---|---|
| `om-engine/src/deck/stream.rs` (new) | `StreamSource`: symphonia `MediaSource` over a local file, a growing file or a spool file; availability, `Pending` signalling, byte tee |
| `om-engine/src/deck/source.rs` (modify) | `SourceReader::open_stream`; estimated length; no `count_frames` on the control thread |
| `om-engine/src/deck.rs`, `deck/shared.rs` (modify) | non-blocking `Deck::load_async`, `buffering` and `length_estimated` flags |
| `om-analysis/src/incremental.rs` (new) | `IncrementalAnalyzer` trait, `BatchAdapter` for old analysers, `Partial` |
| `om-analysis/src/stream_job.rs` (new) | single-pass runner (decode + tee hash + fan-out to incremental analysers), throttle, cancel |
| `om-analysis/src/live.rs` (new) | `LiveAnalysis`: polled handle holding the latest partial waveform, provisional BPM/key, state |
| `om-analysis/src/waveform.rs`, `bpm.rs`, `key.rs`, `structure.rs` (modify) | incremental implementations |
| `om-analysis/src/service.rs`, `job_runner.rs` (modify) | `submit_stream`; cache lookup by fingerprint before decoding |
| `om-library/src/prep_queue.rs`, `om-ui/src/library/track_loader.rs`, `services.rs` (modify) | use the stream path; expose `live(deck)` |
| `om-ui-gpui/src/deck_view.rs` (modify) | strips draw partial data and the placeholder |
| `om-net/` (new crate) | `HttpSpool`: range/streamed download into a spool file |

---

### Task 1: `StreamSource` (readable-while-growing media source)

**Files:**
- Create: `crates/om-engine/src/deck/stream.rs`
- Modify: `crates/om-engine/src/deck.rs` (module declaration, re-export)
- Test: `crates/om-engine/src/deck/tests/stream.rs`

**Interfaces:**
- Produces:
  ```rust
  pub trait Availability: Send + Sync {
      fn available(&self) -> u64;          // bytes readable from the start
      fn total(&self) -> Option<u64>;      // None until the length is known
      fn failed(&self) -> bool;            // download aborted
      fn complete(&self) -> bool;
  }
  pub struct StreamSource { /* file + Arc<dyn Availability> + optional tee */ }
  impl StreamSource {
      pub fn local(path: &Path) -> io::Result<Self>;                       // fully available
      pub fn growing(path: &Path, avail: Arc<dyn Availability>) -> io::Result<Self>;
      pub fn with_tee(self, tee: Arc<Mutex<Sha256>>) -> Self;             // hashes bytes as read in order
  }
  impl Read + Seek + symphonia::core::io::MediaSource for StreamSource
  ```
  `Read` past `available()` returns `ErrorKind::WouldBlock` (never blocks); `byte_len()` is `total()`; `is_seekable()` is true.

- [ ] **Step 1: Write failing tests** in `tests/stream.rs`:
  - `local_reads_whole_file_and_reports_len`: bytes read == file bytes; `byte_len() == Some(len)`.
  - `growing_returns_would_block_past_available`: file of 100 bytes, `available() = 40`; reading 100 bytes yields 40 then `Err(WouldBlock)`; after `available` is raised to 100 the rest reads.
  - `tee_hash_equals_sha256_of_file_after_sequential_read`.
  - `tee_ignores_rereads_after_seek_back` (hash covers each byte once, in file order; seeks back and re-reads do not change it).
- [ ] **Step 2: Run** `cargo test -p om-engine stream` → FAIL (module missing).
- [ ] **Step 3: Implement** `StreamSource`. The tee advances only when the read position equals the next un-hashed offset; any read below that offset is not hashed again; a forward seek past the un-hashed offset marks the tee `incomplete` (a seeking pass cannot produce a full hash; the runner then hashes in a trailing pass over the file only if it is complete, see Task 5).
- [ ] **Step 4: Run** the tests → PASS.
- [ ] **Step 5: Commit** `feat(engine): StreamSource, a media source that may still be growing`.

### Task 2: Non-blocking deck load and estimated length

**Files:**
- Modify: `crates/om-engine/src/deck/source.rs`, `crates/om-engine/src/deck.rs`, `crates/om-engine/src/deck/shared.rs`, `crates/om-engine/src/deck/decoder.rs`
- Test: `crates/om-engine/src/deck/tests.rs`

**Interfaces:**
- Consumes: `StreamSource` (Task 1).
- Produces:
  ```rust
  impl SourceReader { pub fn open_stream(src: StreamSource, ext_hint: Option<&str>) -> Result<Self, SourceError>; }
  // TrackInfo gains: pub length_estimated: bool
  impl Deck {
      pub fn load_async(&self, src: StreamSource, ext_hint: Option<String>) -> LoadTicket;
      pub fn is_buffering(&self) -> bool;          // decoder is waiting for bytes
      pub fn length_is_estimated(&self) -> bool;
  }
  pub struct LoadTicket { /* poll: Pending | Ready | Failed(SourceError) */ }
  ```
  `Deck::load(&Path)` stays and wraps `load_async` + wait, so existing callers and tests keep working.

- [ ] **Step 1: Failing tests:**
  - `open_without_frame_count_does_not_decode_whole_file`: a generated MP3/WAV-stripped fixture with no `n_frames`; `open_stream` returns in under 50 ms of CPU for a 10-minute fixture (assert decoded packet count ≤ 8 via a counting decoder hook) and `info().length_estimated == true`, length within 5 % of truth.
  - `decoder_reports_buffering_on_would_block_and_resumes`: growing source stops at 30 % → `is_buffering()` becomes true within 200 ms and the audio callback still returns silence; raise `available` → `is_buffering()` false and frames after the gap are the file's.
  - `length_estimate_is_corrected_when_stream_ends`: `length_is_estimated()` flips to false and `length_frames` equals the true count.
  - `truncated_download_fails_the_deck_not_hangs`: `Availability::failed() == true` → ticket/deck reports a source error and `is_buffering()` is false.
- [ ] **Step 2: Run** `cargo test -p om-engine deck::tests` → FAIL.
- [ ] **Step 3: Implement.** `open_stream` probes with what is available (retry the probe on `WouldBlock` up to a short deadline on the decoder thread, never the control thread). When `n_frames` is `None`, estimate from `total bytes / average packet bytes` of the first packets and set `length_estimated`; the decoder thread corrects the length when it reaches the end. `Deck::load_async` does the open on the decoder thread and publishes `loaded` when the probe succeeds. The decoder treats `WouldBlock` as an underrun: set `buffering`, sleep 5 ms, retry; the audio thread already outputs silence for unfilled frames.
- [ ] **Step 4: Run** → PASS; also `cargo test -p om-engine` (existing deck tests unchanged).
- [ ] **Step 5: Commit** `feat(engine): load decks without blocking the control thread; play while buffering`.

### Task 3: `IncrementalAnalyzer` and the batch adapter

**Files:**
- Create: `crates/om-analysis/src/incremental.rs`
- Modify: `crates/om-analysis/src/lib.rs`, `registry.rs` (register incremental analysers beside batch ones)
- Test: `crates/om-analysis/src/incremental/tests.rs`

**Interfaces:**
- Produces:
  ```rust
  pub trait IncrementalAnalyzer: Send {
      fn id(&self) -> &str;
      fn version(&self) -> i32;
      fn start(&mut self, sample_rate: f64, total_frames: Option<u64>);
      fn push(&mut self, block: &[f32], ctx: &dyn AnalysisContext);        // mono, in order
      fn snapshot(&self) -> Option<Partial>;                               // cheap; None if nothing new
      fn finish(self: Box<Self>, ctx: &dyn AnalysisContext) -> Option<String>; // final JSON payload
  }
  pub enum Partial { Waveform(Arc<WaveformData>), Tempo { bpm: f64, confidence: f32 }, Key { camelot: String, confidence: f32 } }
  pub struct BatchAdapter(Arc<dyn Analyzer>);   // buffers pushes into an AudioClip; finish() calls analyze()
  ```
  Registry: `AnalyzerRegistry::find_incremental(id) -> Option<Box<dyn IncrementalAnalyzer>>`, falling back to `BatchAdapter` for any registered `Analyzer`.

- [ ] **Step 1: Failing tests:** `batch_adapter_output_equals_analyze_on_whole_clip` (feed a 5 s generated clip in 4096-sample blocks; payload byte-equal to `Analyzer::analyze`); `adapter_snapshot_is_none`; `registry_falls_back_to_adapter_for_unknown_incremental`.
- [ ] **Step 2: Run** `cargo test -p om-analysis incremental` → FAIL.
- [ ] **Step 3: Implement** the trait, `Partial`, `BatchAdapter`, registry lookup.
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Commit** `feat(analysis): incremental analyser interface with a batch adapter`.

### Task 4: Streaming waveform (E2a core)

**Files:**
- Modify: `crates/om-analysis/src/waveform.rs`, `waveform_data.rs`
- Test: `crates/om-analysis/src/waveform/tests.rs`

**Interfaces:**
- Consumes: `IncrementalAnalyzer`, `Partial` (Task 3).
- Produces: `WaveformStream: IncrementalAnalyzer` (id `"waveform"`, same `version()` 1); `WaveformData::with_progress(self, filled_frames: i64) -> WaveformData` and `WaveformData::filled_frames(&self) -> i64` (a complete result reports `filled_frames == num_samples`). `WaveformData` keeps its JSON format: partials are never serialised.

- [ ] **Step 1: Failing tests:**
  - `streamed_waveform_final_equals_batch_build`: any block size (1, 777, 4096, 65536) → `finish()` JSON identical to `WaveformAnalyzer::analyze` on the same clip.
  - `partial_covers_only_pushed_frames`: after pushing half the clip `snapshot()` is `Waveform` with `filled_frames` == pushed count, and its points equal the batch points over that range.
  - `snapshot_is_none_when_nothing_new_since_last`.
  - `unknown_total_frames_still_builds` (`start(sr, None)`).
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement** `WaveformStream` reusing `WaveformBuilder`'s band splitter and `reduce_block`; keep a carry buffer for a partial point; recompute upper levels only for the tail since the last snapshot.
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Commit** `feat(analysis): waveform builds incrementally and publishes partials`.

### Task 5: Single-pass stream job (hash tee, fan-out, throttle, cancel)

**Files:**
- Create: `crates/om-analysis/src/stream_job.rs`, `crates/om-analysis/src/live.rs`
- Modify: `crates/om-analysis/src/service.rs`, `job_runner.rs`, `job.rs`
- Test: `crates/om-analysis/src/stream_job/tests.rs`

**Interfaces:**
- Consumes: `StreamSource`, `IncrementalAnalyzer`, `WaveformStream`.
- Produces:
  ```rust
  pub struct LiveAnalysis { /* Mutex<Arc<LiveState>> */ }
  pub struct LiveState {
      pub phase: LivePhase,                 // Waiting | Running | Complete | Failed(String) | Cancelled
      pub decoded_frames: u64, pub total_frames: Option<u64>,
      pub waveform: Option<Arc<WaveformData>>,
      pub bpm: Option<(f64, f32)>, pub key: Option<(String, f32)>,   // provisional
  }
  impl LiveAnalysis { pub fn latest(&self) -> Arc<LiveState> }       // one lock, one Arc clone
  impl AnalysisService {
      pub fn submit_stream(&self, src: Box<dyn StreamClipSource>, analyzer_ids: Vec<String>,
                           fingerprint: Option<String>, listener: Option<Listener>)
          -> (Arc<AnalysisJob>, Arc<LiveAnalysis>);
  }
  ```
  `StreamClipSource` yields `(StreamSource, ext_hint)`; the existing `submit` and `ClipSource` stay.
  Behaviour: if `fingerprint` hits the cache for every requested analyser, no decode runs and `LiveState` is `Complete` with the cached waveform. Otherwise one pass: read blocks (64k frames), push to all analysers, publish `LiveState` at most every 100 ms, finish all analysers at end, store each payload under the *tee* hash (full content hash). If the tee reports `incomplete` (seek) or the source `failed()`, store nothing and set `Failed`. The pass runs at lowest thread priority and sleeps if a block was produced faster than 4x real time while any deck is playing (`Throttle` hook the service is given), so analysis never competes with audio.

- [ ] **Step 1: Failing tests:**
  - `one_pass_results_equal_batch_job_results` for waveform + bpm + key on a generated 20 s clip (payload equality, same cache entries).
  - `cache_hit_runs_no_decode_and_is_complete_immediately` (counting source asserts zero reads).
  - `partials_are_published_while_running`: growing source released in 10 steps; `latest().waveform.filled_frames` increases monotonically and is `< num_samples` before the last step.
  - `publish_rate_is_capped`: ≤ 12 distinct states per second on a fast source.
  - `truncated_source_fails_and_caches_nothing`.
  - `cancel_stops_within_one_block_and_caches_nothing`.
  - `hash_pass_is_gone`: counting source shows each byte read once for a local file.
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement** `run_stream_job` beside `run_analysis_job` (leave the latter for `submit`); throttle via a `Fn() -> bool` "deck busy" hook injected at `AnalysisService::new`.
- [ ] **Step 4: Run** `cargo test -p om-analysis` → PASS.
- [ ] **Step 5: Commit** `feat(analysis): single-pass streaming analysis job with live partials`.

### Task 6: Streaming BPM and key (provisional, then final)

**Files:**
- Modify: `crates/om-analysis/src/bpm.rs`, `tempo.rs`, `beat_grid.rs`, `key.rs`, `chromagram.rs`, `structure.rs`
- Test: `crates/om-analysis/src/bpm/tests.rs`, `key/tests.rs`

**Interfaces:**
- Produces: `BpmStream`, `KeyStream` (`IncrementalAnalyzer`, ids `"bpm"`, `"key"`, versions unchanged); `structure` stays on `BatchAdapter` (needs the whole track) and runs at `finish`.
  Provisional rule: no tempo before 20 s of audio; then a `Partial::Tempo` every 5 s. The engine grid is applied only when two successive estimates agree within 0.1 BPM and cover ≥ 45 s, or at `finish`; before that the deck shows the provisional BPM marked `~` and sync is disabled.

- [ ] **Step 1: Failing tests:**
  - `bpm_stream_final_equals_batch` and `key_stream_final_equals_batch` (golden clips already in `test_signals.rs`).
  - `no_tempo_partial_before_20_seconds`.
  - `provisional_tempo_within_1_bpm_of_final_on_120_bpm_click_track_after_30_s`.
  - `lock_in_requires_two_agreeing_estimates` (a tempo that flips between 70 and 140 never locks).
  - `key_partial_is_none_until_10_seconds_of_chroma`.
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement** by accumulating the onset-strength envelope and chroma sums incrementally (both already computed frame by frame); the estimators run on the accumulated envelope at each snapshot; `finish` runs the existing full estimator so the final payload is byte-identical.
- [ ] **Step 4: Run** → PASS; `cargo test -p om-analysis` green.
- [ ] **Step 5: Commit** `feat(analysis): tempo and key estimate while the track decodes`.

### Task 7: Wire the loader and prep queue to the stream path

**Files:**
- Modify: `crates/om-ui/src/library/track_loader.rs`, `services.rs`, `crates/om-library/src/prep_queue.rs`
- Test: `crates/om-ui/tests/library_loader.rs` (existing harness in `tests/library_common`)

**Interfaces:**
- Consumes: `Deck::load_async`, `submit_stream`, `LiveAnalysis`.
- Produces: `TrackLoader::live(&self, deck: usize) -> Option<Arc<LiveState>>`; `TrackLoader::load_track` returns before the probe finishes (`LoadError::Decode` is reported later through `track_info(deck).load_error`); the cached-full-result path (`analysis_for`) fills `LiveState` as `Complete`.

- [ ] **Step 1: Failing tests:**
  - `load_returns_without_waiting_for_probe_or_analysis` (slow fake source; `load_track` returns in < 20 ms).
  - `live_state_progresses_to_complete_and_matches_cache`.
  - `loading_b_cancels_a_and_never_shows_a_data_on_b` (Review Focus 4).
  - `cached_track_is_complete_on_first_poll` (Review Focus 5).
  - `failed_decode_is_reported_on_the_deck_info_and_old_grid_restored`.
- [ ] **Step 2: Run** `cargo test -p om-ui library_loader` → FAIL.
- [ ] **Step 3: Implement.** Prep-queue entries for non-loaded tracks (batch scan) keep using `submit`; only deck loads use `submit_stream`. Move the grid/waveform `apply` to react to `Complete`/lock-in instead of `PrepEntryChanged`.
- [ ] **Step 4: Run** → PASS; `cargo test -p om-ui -p openmixer` green.
- [ ] **Step 5: Commit** `feat(ui): deck loads stream; analysis follows the data`.

### Task 8: Strips draw partial data

**Files:**
- Modify: `crates/om-ui/src/waveform.rs`, `crates/om-ui/src/widgets/deck.rs`, `crates/om-ui-gpui/src/deck_view.rs`
- Test: `crates/om-ui/tests/main_layout.rs`; screenshot `tools/screenshots.sh` scene

**Interfaces:**
- Consumes: `LiveState`.
- Produces: `WaveformView::from_live(&LiveState, deck_length_frames: i64) -> WaveformView` with `filled_fraction: f32`; area beyond it is a dim placeholder; provisional BPM shows `~128.0`; a `buffering` badge when `Deck::is_buffering()`.

- [ ] **Step 1: Failing tests:** `partial_waveform_maps_filled_columns_only` (columns past the fill are placeholder), `placeholder_does_not_use_raw_colour_literals` (existing grep test), `provisional_bpm_is_marked`, `buffering_badge_shows_only_while_buffering`.
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement.** Reuse the strip's cached quads: only the filled columns change between polls. Add a `--demo-progressive` scene that releases a fixture in steps for screenshots.
- [ ] **Step 4: Run** → PASS; look at the screenshots at 1280x800 and the minimum window (`docs/PRODUCT.md`: done means looked at).
- [ ] **Step 5: Commit** `feat(ui): waveform fills in while the track loads`.

### Task 9: Downloads (HTTP spool)

**Files:**
- Create: `crates/om-net/` (`Cargo.toml`, `src/lib.rs`, `src/spool.rs`); add to the workspace and `deny.toml` review
- Test: `crates/om-net/tests/spool.rs` (local `TcpListener` server; no external network)

**Interfaces:**
- Produces:
  ```rust
  pub struct HttpSpool { /* spool file + atomic progress */ }
  impl HttpSpool {
      pub fn start(url: &str, spool_dir: &Path, pool: &WorkerPool<..>) -> Result<Self, NetError>;
      pub fn availability(&self) -> Arc<dyn Availability>;     // from Task 1
      pub fn path(&self) -> &Path;
      pub fn fingerprint(&self) -> Option<String>;            // ETag or Content-Length+Last-Modified
      pub fn cancel(&self);
  }
  ```
  Download is sequential into the spool file; a seek beyond `available()` is served by `WouldBlock` (no range requests in this task).

- [ ] **Step 1: Failing tests:** `spool_available_grows_while_server_sends_slowly`; `deck_plays_while_spool_downloads` (integration with Task 2's `growing` source); `server_drop_marks_failed`; `cancel_removes_spool_file`; `fingerprint_uses_etag`.
- [ ] **Step 2: Run** `cargo test -p om-net` → FAIL.
- [ ] **Step 3: Implement** with `ureq` streaming reads, `Availability` over an `AtomicU64`.
- [ ] **Step 4: Run** → PASS; `cargo deny check` still passes.
- [ ] **Step 5: Commit** `feat(net): HTTP spool so a download can be played and analysed as it arrives`.

### Task 10: Measure, soak, document

**Files:**
- Modify: `crates/openmixer/tests/ui_gate.rs`, `TODO.md` (E2a, E7), `docs/PROGRESS.md`, new ADR `docs/adr/0006-jit-analysis.md`
- Test: `crates/openmixer/tests/jit_load.rs`

- [ ] **Step 1: Failing tests:** `load_latency_is_independent_of_file_length` (1 min vs 60 min fixture: `load_track` return time within 2x, both < 20 ms); `ten_minute_mix_with_streamed_loads_has_zero_xruns` (`#[ignore]`, like the existing soaks); `frame_time_budget_holds_while_filling` (frame stats from `om-ui-gpui::frame_stats`).
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement** fixtures/harness only; write the ADR (decision: one pass, tee hash, provisional grid rule) and update TODO E2a/E7.
- [ ] **Step 4: Run** the new tests and `cargo test --workspace` → PASS.
- [ ] **Step 5: Commit** `test(jit): load latency, streamed soak, and docs`.

## Self-review notes

- Spec coverage: E2a (partials, placeholder, BPM/key land later) is Tasks 4-8; "depends on the system, not our delay" is Tasks 1-2 (no blocking load), 5 (no hash pass, no whole-clip buffer, throttled), 9 (network-bound).
- Open decisions for the owner: (a) a new `om-net` crate versus putting `HttpSpool` in `om-stems` beside its downloader; (b) range requests for seeking ahead in a download (left out; seek past the fill shows `buffering`); (c) whether the provisional grid should drive sync before lock-in (plan says no).

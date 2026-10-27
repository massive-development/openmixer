# Known Gaps Implementation Plan

> \*\*For agentic workers:\*\* REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended). Steps use checkbox (`- \[ ]`) syntax for tracking.

**Goal:** Close the known gaps that can be built and verified headlessly: a scheduled soak job, automix and auto-enqueue analysis handling, playlist reorder and folders in the UI, artwork display, restyled scroll bars, tables and toggles, an FFmpeg decode fallback, and C-ABI plugin loading, then one docs sync.

**Architecture:** Each task is an independent workstream with its own file ownership, so agents can run in parallel in separate worktrees. Library, engine and view-model logic is built and tested first; the gpui layer only draws from it. Tasks 7 and 8 sit behind an explicit decision gate.

**Tech Stack:** Rust workspace (`crates/om-\*`, `crates/openmixer`), rusqlite (`om-library`), GPUI (`om-ui-gpui`), symphonia (`om-engine`), GitHub Actions (`.github/workflows/rust.yml`).

**Spec:** `ROADMAP.md` ("Known gaps"), `README.md` ("Known gaps, honestly listed"), `TODO.md` sections (A) to (C), `docs/spikes/DECISIONS.md` ("Plugins: in-process only").

## Global Constraints

* Linux is the required signal. Do not add tests that need audio hardware, a display, or the network.
* Every task ends with `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test -p <touched crates>` green.
* Test-first. Commit per task, message style `feat:`/`fix:`/`docs:`/`ci:`, with the Co-Authored-By trailer.
* Colours and sizes come from `ThemeTokens` via `om\_ui::style::LookAndFeel`; no literals in gpui files (see "Styling a control" in `agents.md`).
* Licences: any new dependency must pass `cargo deny` (project is AGPL-3.0).
* Do not edit `README.md`, `ROADMAP.md`, `TODO.md` or `docs/PROGRESS.md` in tasks 2-9; Task 10 updates them once at the end, so parallel agents never conflict on them.
* Do not touch files owned by another task (see each task's **Files**). Rebase on `feat/ui-redesign` before the final commit.

## Review Focus

* A track that is never analysed (corrupt file, analysis error): automix and auto-enqueue must give up after a bounded number of tries, not wait or re-enqueue forever (Tasks 3, 4).
* Moving a playlist folder into its own descendant must be refused, not corrupt the tree or loop (Task 5).
* Reordering an entry to index 0, to the last index, and to an out-of-range index clamps and keeps positions contiguous (Task 5).
* A track with no artwork, or artwork the image decoder rejects, shows the placeholder and never panics (Task 6).
* A file ffmpeg cannot decode, or a machine with no `ffmpeg` binary, returns a typed error and leaves the symphonia path unchanged (Task 7).
* A plugin library with the wrong ABI version, a missing entry point, or a panicking `on\_load` is skipped with a reason and does not take the host down (Task 8).

\---

### Task 2: Scheduled soak job in CI

**Files:**

* Modify: `.github/workflows/rust.yml`

**Interfaces:**

* Consumes: the `#\[ignore]` soaks `cargo test -p om-audio --test soak -- --ignored --nocapture` (`crates/om-audio/tests/soak.rs:321`) and `cargo test -p openmixer --test ui\_gate -- --ignored` (`crates/openmixer/tests/ui\_gate.rs:287`).
* Produces: a `soak` job on `schedule` (weekly) and `workflow\_dispatch`.
* \[ ] **Step 1:** Add `schedule: - cron: "0 3 \* \* 0"` and `workflow\_dispatch:` to `on:`. Keep existing triggers unchanged.
* \[ ] **Step 2:** Add job `soak` (ubuntu-latest, `timeout-minutes: 40`, `if: github.event\_name != 'pull\_request' \&\& github.event\_name != 'push'`) running both ignored tests, copying the toolchain and cache steps from the `check` job.
* \[ ] **Step 3:** Validate: `python3 -c "import yaml,sys; yaml.safe\_load(open('.github/workflows/rust.yml'))"`; confirm the `check` job's `if`/triggers are untouched with `git diff`. Expected: valid YAML, diff only adds.
* \[ ] **Step 4:** Commit `ci: scheduled 10-minute soaks`.

### Task 3: Automix waits for unanalysed tracks

**Files:**

* Modify: `crates/om-automix/src/queue.rs`, `crates/openmixer/src/automix\_queue.rs`
* Test: `crates/om-automix/src/queue.rs` (unit tests, existing style), `crates/openmixer/tests/automix\_rig/mod.rs`

**Interfaces:**

* Consumes: `select\_next(...) -> Selection` where `Selection.skipped: Vec<(i64, SkipReason)>` and `SkipReason::NotAnalysed`; `PrepQueue` in `crates/om-library/src/prep\_queue.rs`.
* Produces: `Selection.needs\_analysis: Vec<i64>` (track ids skipped as `NotAnalysed`, in candidate order) and `LibraryQueue::pending\_analysis(\&self) -> \&\[i64]`. `LibraryQueue` submits those ids to the prep queue once per id per session and retries them on the next `next\_track` call.
* \[ ] **Step 1:** Write failing test `unanalysed\_candidates\_are\_reported\_for\_analysis` in `queue.rs`: tracks 4 and 6 unanalysed, 5 analysed; assert `sel.needs\_analysis == vec!\[4, 6]` and that `sel.track` is 5.
* \[ ] **Step 2:** Run `cargo test -p om-automix unanalysed\_candidates` ; expect FAIL (field missing).
* \[ ] **Step 3:** Add `needs\_analysis` to `Selection` in `crates/om-automix/src/queue.rs`, filled where `SkipReason::NotAnalysed` is pushed (line \~138). Run the test; expect PASS.
* \[ ] **Step 4:** Failing test in the automix rig, `queue\_submits\_unanalysed\_tracks\_once`: a library where every track is unanalysed; call `next\_track` three times; assert the prep queue holds each id exactly once and `pending\_analysis()` lists them. Then mark one analysed and assert the next call returns it.
* \[ ] **Step 5:** Implement the submit-once set and retry in `LibraryQueue` (`crates/openmixer/src/automix\_queue.rs`). Give up on an id after 3 submissions and add it to `last\_error()` text. Run `cargo test -p openmixer automix`; expect PASS.
* \[ ] **Step 6:** Commit `feat: automix requests analysis for tracks it had to skip`.

### Task 4: Auto-enqueue after a library scan

**Files:**

* Modify: `crates/om-library/src/ingest.rs`, `crates/om-library/src/prep\_queue.rs` (only if a bulk helper is needed)
* Test: `crates/om-library/tests/library\_prep\_queue.rs`

**Interfaces:**

* Consumes: `ingest\_folder(...)` in `crates/om-library/src/ingest.rs:72` and its report; the prep queue API in `prep\_queue.rs` (read it first: use its existing enqueue method, do not add a second path).
* Produces: after a successful `ingest\_folder`, every track the scan inserted or changed and that has no analysis is enqueued, exactly once; `IngestReport` gains `enqueued: usize`.
* \[ ] **Step 1:** Failing test `scan\_enqueues\_new\_unanalysed\_tracks\_once` in `library\_prep\_queue.rs`: ingest a temp folder with 3 generated files (fixture helpers in `om-testing`), assert queue length 3 and `report.enqueued == 3`; ingest again, assert still 3 and `enqueued == 0`.
* \[ ] **Step 2:** Run `cargo test -p om-library --test library\_prep\_queue scan\_enqueues`; expect FAIL.
* \[ ] **Step 3:** Enqueue inside the ingest transaction's success path; skip tracks already queued or analysed. Run the test; expect PASS.
* \[ ] **Step 4:** Add test `scan\_of\_missing\_folder\_enqueues\_nothing` (non-directory path errors, queue empty). Expect PASS.
* \[ ] **Step 5:** Commit `feat: library scan enqueues new tracks for analysis`.

### Task 5: Playlist reorder and nested folders in the UI

**Files:**

* Modify: `crates/om-ui/src/library/browser.rs`, `crates/om-ui/src/sidebar.rs`, `crates/om-ui/src/library/services.rs`, `crates/om-library/src/playlist\_dao.rs` (cycle guard only)
* Modify (draw only, last): the gpui sidebar and library view files under `crates/om-ui-gpui/src/`
* Test: `crates/om-ui/tests/`, `crates/om-library/tests/`

**Interfaces:**

* Consumes: `PlaylistDao::create(name, parent\_id, is\_folder) -> Result<i64>`, `move\_node(id, new\_parent\_id, index)`, `move\_entry(entry\_id, new\_index)`, `entries(playlist\_id)`, `children(parent\_id)` (all in `playlist\_dao.rs`). The DAO already supports folders and reorder; the gap is the view-model and the view.
* Produces: `LibraryBrowser::move\_entry(entry\_id: i64, new\_index: i32)`, `LibraryBrowser::new\_folder(parent: Option<i64>, name: \&str) -> i64`, `LibraryBrowser::move\_playlist(id: i64, new\_parent: Option<i64>, index: i32)`, and `PlaylistDao::move\_node` returning an error when `new\_parent` is `id` or a descendant of `id`.
* \[ ] **Step 1:** Failing DAO test `move\_node\_into\_own\_descendant\_is\_refused` (folder A contains folder B; `move\_node(A, Some(B), 0)` is `Err`; tree unchanged).
* \[ ] **Step 2:** Run `cargo test -p om-library move\_node\_into\_own`; expect FAIL. Implement the guard by walking up `parent\_id` from `new\_parent`; run; expect PASS.
* \[ ] **Step 3:** Failing view-model tests in `crates/om-ui/tests/`: `entry\_moves\_to\_first\_last\_and\_clamped\_index` (positions stay `0..n` contiguous), `new\_folder\_nests\_under\_parent`, `playlist\_moves\_between\_folders`. Run; expect FAIL.
* \[ ] **Step 4:** Implement the three `LibraryBrowser` methods over the DAO; the sidebar tree builder must emit nested folder nodes (extend `sidebar.rs` `SidebarKind` with `Folder` if absent). Run; expect PASS.
* \[ ] **Step 5:** Wire gpui: drag an entry row to reorder, "New folder" sidebar button, drag a playlist onto a folder. Draw only; all decisions through the methods above. Run `cargo build -p om-ui-gpui` and `cargo run -p openmixer -- --smoke --headless`; expect exit 0.
* \[ ] **Step 6:** Commit `feat: playlist reorder and nested folders`.

### Task 6: Show artwork in the UI

**Files:**

* Create: `crates/om-ui/src/artwork\_binding.rs`
* Modify: `crates/om-ui/src/lib.rs`, `crates/om-ui/src/library/services.rs`; gpui: deck card and track detail view under `crates/om-ui-gpui/src/`
* Test: `crates/om-ui/tests/artwork\_binding.rs`

**Interfaces:**

* Consumes: `ArtworkDao::get(track\_id) -> Result<Option<ArtworkRecord>>` (`crates/om-library/src/artwork\_dao.rs:95`; record has `mime` and `data`), `image\_size(\&\[u8]) -> Option<(u32, u32)>`.
* Produces: `ArtworkView` enum `{ None, Image { mime: String, data: Arc<\[u8]>, width: u32, height: u32 } }` and `fn artwork\_for(lib: \&impl ArtworkSource, track\_id: i64) -> ArtworkView`, with a small LRU cache (16 entries) keyed by track id. `ArtworkSource` is a trait with `fn artwork(\&self, track\_id: i64) -> Result<Option<ArtworkRecord>, LibraryError>` so tests use a stub.
* \[ ] **Step 1:** Failing tests in `artwork\_binding.rs`: `missing\_artwork\_is\_none`, `garbage\_bytes\_are\_none\_not\_a\_panic` (data `\[0, 1, 2]`, `image\_size` is None), `valid\_png\_reports\_size`, `cache\_serves\_second\_call\_without\_a\_query` (stub counts calls). Run; expect FAIL.
* \[ ] **Step 2:** Implement `artwork\_binding.rs` as specified. Run; expect PASS.
* \[ ] **Step 3:** Load on a worker, never on the UI thread (follow the `TrackLoader` pattern in `crates/om-ui/src/library/track\_loader.rs`). gpui draws `ArtworkView::Image` with the image element and a token-coloured placeholder for `None`.
* \[ ] **Step 4:** Run `cargo test -p om-ui artwork` and `cargo run -p openmixer -- --smoke --headless`; expect pass. Commit `feat: show cover art`.

### Task 7: Restyle scroll bars, tables and toggles

**Files:**

* Modify: `crates/om-ui/src/style.rs`; gpui scroll, table and toggle drawing under `crates/om-ui-gpui/src/`
* Test: `crates/om-ui/src/style.rs` tests (existing `entries()` contrast tests)

**Interfaces:**

* Consumes: `LookAndFeel::from\_tokens(\&ThemeTokens)`, `LookAndFeel::entries() -> Vec<(\&'static str, ControlStyle)>`, `ControlStyle::contrast\_on`.
* Produces: `ScrollBarStyle { track: ControlStyle, thumb: ControlStyle, thumb\_hover: ControlStyle, width: f32 }`, `TableStyle { header: ControlStyle, row: ControlStyle, row\_alt: ControlStyle, row\_selected: ControlStyle, divider: SeparatorStyle }`, `ToggleStyle { off: ControlStyle, on: ControlStyle, knob: ControlStyle }`, fields `scroll\_bar`, `table`, `toggle` on `LookAndFeel`, all included in `entries()`.
* \[ ] **Step 1:** Failing tests: `every\_theme\_gives\_scroll\_table\_toggle\_readable\_contrast` (each `entries()` item added by this task passes the same contrast floor the existing entries use, in both bundled themes), `thumb\_is\_visible\_against\_track`. Run `cargo test -p om-ui style`; expect FAIL.
* \[ ] **Step 2:** Add the three structs and derive them from tokens in `from\_tokens`; no new token fields unless a theme lacks a needed colour (then add to both `resources/themes/\*` files). Run; expect PASS.
* \[ ] **Step 3:** Replace the default scroll bar, table and toggle paint in gpui with draws from `LookAndFeel`. Add the `old-literals` style test pattern if the repo has one (`labels::channel\_labels` commit) so no colour literal remains in the touched files.
* \[ ] **Step 4:** `cargo build -p om-ui-gpui`, headless smoke, and regenerate no screenshots (by-eye check stays with the maintainer). Commit `feat: restyle scroll bars, tables and toggles`.

### Task 8: FFmpeg decode fallback (Todo 8b)

**Decision (recommended, flag if you disagree):** run the `ffmpeg` executable as a subprocess producing raw `f32le` PCM on stdout, instead of linking `ffmpeg-next`. No link-time dependency, no GPL/LGPL linking question for the AGPL project, no CI system packages.

**Files:**

* Create: `crates/om-engine/src/decode\_ffmpeg.rs`
* Modify: `crates/om-engine/src/lib.rs`, `crates/om-engine/Cargo.toml` (feature `ffmpeg`, off by default, no new crates)
* Test: `crates/om-engine/tests/decode\_ffmpeg.rs`

**Interfaces:**

* Consumes: the existing symphonia decode entry point in `om-engine` (locate it first: `grep -rn "symphonia" crates/om-engine/src`); the file-load path calls it and must try ffmpeg only when it returns an unsupported-format error.
* Produces: `fn decode\_with\_ffmpeg(path: \&Path, target\_rate: u32) -> Result<DecodedAudio, DecodeError>` (use the engine's existing decoded-audio and error types; add `DecodeError::FfmpegMissing` and `DecodeError::FfmpegFailed(String)`), and `fn ffmpeg\_available() -> bool`.
* \[ ] **Step 1:** Failing tests: `ffmpeg\_available\_is\_false\_for\_a\_bogus\_binary\_name` (via an overridable `OPENMIXER\_FFMPEG` env var path), `missing\_binary\_is\_a\_typed\_error`, and an integration test skipped with a printed reason when `ffmpeg\_available()` is false: `decodes\_a\_wav\_to\_the\_same\_frames\_as\_symphonia` (frame count equal, samples within 1e-3). Run `cargo test -p om-engine --features ffmpeg decode\_ffmpeg`; expect FAIL.
* \[ ] **Step 2:** Implement with `std::process::Command` (`-v error -i <path> -f f32le -ac 2 -ar <rate> -`), read stdout fully, map non-zero exit to `FfmpegFailed(stderr)`. Never build a shell string; pass args as separate arguments. Run; expect PASS.
* \[ ] **Step 3:** Hook the fallback into the load path behind `#\[cfg(feature = "ffmpeg")]`; test that an unsupported-extension file falls back, and that with the feature off behaviour is unchanged (`cargo test -p om-engine` without the feature).
* \[ ] **Step 4:** Commit `feat: optional ffmpeg decode fallback`.

### Task 9: Plugin dynamic loading (Todo 10a)

**Gate:** this task starts only after the user confirms the decision. Recommended: a **C ABI** (`extern "C"` entry points and a versioned `#\[repr(C)]` function table) loaded with `libloading`. WASM is heavier (runtime dependency, host-call overhead on the audio path) and source-only leaves the gap open. Step 1 records the decision in `docs/spikes/DECISIONS.md`; the user reviews it before Step 2.

**Files:**

* Create: `crates/om-plugins/src/abi.rs`, `examples/plugins/gain-plugin-cdylib/` (a cdylib exporting the table)
* Modify: `crates/om-plugins/src/loader.rs`, `crates/om-plugins/Cargo.toml` (`libloading`), `docs/spikes/DECISIONS.md`, `deny.toml` if needed
* Test: `crates/om-plugins/tests/dynamic\_load.rs`

**Interfaces:**

* Consumes: `Plugin` and `PluginInfo` in `crates/om-plugins/src/api.rs`, `PluginError::DynamicLoadingUnsupported`, `PluginLoader::load(path) -> Result<Box<dyn Plugin>, PluginError>` and `candidates(dir)`.
* Produces: `pub const PLUGIN\_ABI\_VERSION: u32`; `#\[repr(C)] pub struct PluginVTable { abi\_version: u32, info: extern "C" fn(\*mut c\_char, usize), create: extern "C" fn() -> \*mut c\_void, destroy: extern "C" fn(\*mut c\_void), process: extern "C" fn(\*mut c\_void, \*mut f32, usize) }`; exported symbol `openmixer\_plugin\_entry` returning `\*const PluginVTable`. `PluginError::BadAbi { found: u32, expected: u32 }` and `PluginError::MissingEntry(String)`.
* \[ ] **Step 1:** Write the ADR text into `DECISIONS.md` (C ABI, version field, ownership rules: the host owns the buffer, the plugin never retains pointers past `process`, the library is never unloaded while an instance lives). Stop and ask the user to confirm.
* \[ ] **Step 2:** Failing tests in `dynamic\_load.rs`, building the example cdylib in a temp dir: `loads\_example\_plugin\_and\_processes\_gain`, `wrong\_abi\_version\_is\_bad\_abi`, `library\_without\_entry\_is\_missing\_entry`, `panic\_in\_create\_is\_contained` (wrap each FFI call in `catch\_unwind`). Run; expect FAIL.
* \[ ] **Step 3:** Implement `abi.rs` and `PluginLoader::load` (the only `unsafe` in the crate; add a `// SAFETY:` comment on each block). Keep `SHARED\_LIBRARY\_EXTENSIONS`. Run; expect PASS.
* \[ ] **Step 4:** Plugins page lists loaded vs skipped-with-reason (existing page already shows skipped; change the text only). Update the stub doc comment at the top of `loader.rs`. Commit `feat: load C-ABI plugins from shared libraries`.

### Task 10: Sync docs (last)

Runs after every other task has merged, so each doc line is edited once.

**Files:**
- Modify: `README.md` (Known gaps list), `ROADMAP.md` (Known gaps), `TODO.md` (sections A-C), `docs/PROGRESS.md`

**Interfaces:**
- Consumes: the merged state of tasks 2-9. Which of them landed decides each line; a task that was dropped or gated keeps its gap line.
- Produces: nothing code-level.

- [ ] **Step 1:** Verify each claim in `TODO.md` (A) "Library (Phase L)" against the code: `grep -n "play_history\|artwork\|tracks_fts" crates/om-library/src/migrations.rs` and the hotcue save-back in `crates/om-library/src/cue_dao.rs`. The README currently lists artwork, FTS, play history and hotcue save-back as gaps; they are done.
- [ ] **Step 2:** Remove the gap lines closed by the merged tasks: automix analysis wait (3) and Todo 38 wording, auto-enqueue (4), playlist reorder and folders, Todo 28 (5), artwork in the UI (6), scroll bars, tables, toggles, Todo 25 (7), FFmpeg Todo 8b (8), plugin loading Todo 10a (9).
- [ ] **Step 3:** Make `README.md` and `ROADMAP.md` list the same remaining gaps (they differ today).
- [ ] **Step 4:** Run `python3 tools/progress_heatmap.py --check`. Expected: exit 0.
- [ ] **Step 5:** Commit `docs: update known gaps`.

---

## Execution order

Independent, can run in parallel: Tasks 2, 3, 4, 5, 6, 7, 8. Task 10 (docs) runs last, after everything else has merged. Shared files to watch: Tasks 5, 6 and 7 all touch `crates/om-ui-gpui/src/`, so land their view-model steps first and do the gpui draw steps one at a time. Task 9 waits for the decision gate.


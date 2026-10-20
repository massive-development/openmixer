# Wave 7 Foundation and Features Implementation Plan

> Session handoff and current PR stack: [docs/HANDOFF.md](../../HANDOFF.md)

> REQUIRED SUB-SKILL: superpowers:subagent-driven-development. Constraints (forbid unsafe except FFI crates, RT rules, silent audio in tests, cheap models by default, commit per task, review per phase): see `docs/superpowers/plans/2026-10-18-rust-port-mixer-first.md` (Global Constraints) and `agents.md`. WSL: `nice -n 19`, `CARGO_BUILD_JOBS=3`, one cargo process at a time.

**Goal:** clear the Before-Wave-7 (A) list and build Todos 19, 37, 30/39, 38, 40 on a typed control surface. Spec: `docs/superpowers/specs/2026-10-18-rust-rewrite-design.md` + `TODO.md` Wave 7.
**Order:** F -> (L, S in parallel) -> A, P. Dependencies: F1-F4 before every new control id; L1 before A3; S1-S4 before S5-S7; S5 before A4 (stem transitions, Todo 39); F6 (job queue) before S2 and A2.
**PRs:** one per phase (F may split into F-core and F-cleanup); stack on `chore/test-support` until #67-#71 merge.

## Phase F - Foundation
Verified in code: digit/id parsing is hand-written in `om-engine/src/deck.rs` (`parse_hotcue_id`), `deck_control.rs` (`parse_beats`), `fx_routing.rs` (`find_rack`), `fx/rack.rs` (`parse_unit_id`), `mixer/control.rs`, `sync.rs`, `openmixer/src/engine_state.rs` (`digits`); ids are built by `format!` in `om-ui/src/binding.rs` and `om-ui/src/settings.rs` (`learnable_targets`). `fx/unit.rs` only matches fixed names. `composition.rs` stores MIDI twice (`hooks.midi` and `midi`).
- F1 `om-core`: `ParamId` (parse/Display round-trip, golden table of every current id), `ParamError`. M, sonnet
- F2 `om-core` router: parse once, keep prefix targets and `set_parameter(&str,f64)->bool` as a façade; plugin ids pass as `ParamId::Ext`. S, sonnet
- F3 `om-engine`: migrate deck/deck_control/sync/mixer/fx parsers to `match ParamId`, delete hand parsers. M, sonnet
- F4 `om-ui` binding + `learnable_targets` build ids from `ParamId`. S, haiku
- F5 `EngineSnapshot` (pull-based) in `om-core` types, built by `DjEngine::snapshot()`; `openmixer/engine_state.rs` becomes a snapshot lookup; om-ui reads the snapshot instead of getters (meters keep `LevelSource`). M, sonnet
- F6 `om-core::jobs::WorkerPool<J>` (submit, progress, cancel, join on drop); migrate `AnalysisService` onto it. M, sonnet
- F7 Errors: `thiserror` enums replace `Result<_, String>` in om-library importers/xml, om-ui settings and library, openmixer midi/audio_settings/app_targets/composition/main/library_smoke (~40 sites); `anyhow` only in the binary. M, haiku
- F8 Merge `MidiSink` (app_targets.rs) and `MidiControl` (om-ui settings.rs) into one `&self` `MidiHost`; rename `ServiceHooks` to `Services` with typed `Option<Arc<..>>` fields, drop the duplicate midi field. S, sonnet
- F9 Split `om-ui/src/settings.rs` (~1219 lines, ~513 prod) and `om-ui-gpui/src/settings_view.rs` (1277 prod) into per-page modules. M, haiku
- F10 CI: `gpui-lint` job (fmt, clippy -D warnings, `cargo deny --manifest-path`) for `om-ui-gpui`; keep it outside the workspace. S, haiku
- F11 Update `docs/DESIGN_PATTERNS.md` (Command enum, snapshot, job queue). S, haiku

```rust
// om-core
pub enum ParamId {
    Deck { deck: u8, p: DeckParam },            // play, rate, keylock, seek, hotcue(k)[.clear], loop.*, sync[.master], slip, quantize
    Mixer(MixerParam),                           // ch<N>.trim|fader|filter|pfl|xfader|eq.*, crossfader[.curve], master, cue.*
    Fx { rack: FxRack, unit: Option<u8>, p: FxParam }, // FxParam::{Enable, Mix, Bpm, Effect(String)}
    Sync(SyncParam),
    Stem { deck: u8, stem: u8, p: StemParam },   // stem.<deck>.<0..3>.volume|mute|filter
    Sampler { pad: u8, p: SamplerParam },        // sampler.pad<N>.trigger|volume|gate|pitch
    Automix(AutomixParam),                       // automix.enable|start|cancel|style
    Ext { prefix: String, rest: String },        // plugins
}
impl std::str::FromStr for ParamId { type Err = ParamError; }   // Display gives the canonical id
pub enum Command {                                // Param covers MIDI/UI/tests; the rest carry non-f64 payloads
    Param { id: ParamId, value: f64 },
    LoadStems { deck: u8, set: StemSetRef },
    SamplerAssign { pad: u8, sample: SampleId },
    Automix(AutomixCommand),
}
pub trait CommandSink: Send + Sync { fn send(&self, cmd: Command) -> Result<(), CommandError>; }
// ParameterTarget::set_parameter stays: parse -> Command::Param -> send -> is_ok()

pub struct EngineSnapshot { pub seq: u64, pub decks: Vec<DeckSnapshot>, pub mixer: MixerSnapshot,
    pub stems: Vec<[StemSnapshot; 4]>, pub sampler: SamplerSnapshot, pub automix: AutomixSnapshot }
// DeckSnapshot { loaded, playing, keylock, looping, slip, quantize, sync, sync_master, rate,
//                position_frames, length_frames, hotcues: [Option<i64>; 8] }
impl EngineSnapshot { pub fn read(&self, id: &ParamId) -> Option<f64>; }   // the StateReader backend
```

## Phase L - Library and UI gaps
- L1 `om-library` migration 5: `play_history(id, track_id FK cascade, played_ms, deck)`; `TrackDao::record_play` also inserts; recorded when a deck passes 30 s (hook in `om-ui` track_loader/engine tick). Forward-only, tested on a copy of a v4 DB. M, sonnet
- L2 Hotcue save/load: `CueDao` write on set/clear and load into the deck on track load (nothing loads cues today); `om-ui/library/track_loader.rs`. S, sonnet
- L3 Artwork: migration 6 `artwork(track_id PK, mime, width, height, data BLOB)`, written by ingest from `metadata.rs` `Artwork`; thumbnail only (<=256 px, re-encode skipped: store tag bytes if small). S, haiku
- L4 FTS5: migration 7 `tracks_fts` (external-content, triggers) with rebuild; `search_compiler.rs` uses MATCH for free text, LIKE kept for field filters and as fallback if FTS5 is missing. M, sonnet
- L5 LookAndFeel (Todo 25): `om-ui::theme` style helpers read from tokens; `om-ui-gpui/controls.rs` uses them for knobs, faders, tabs, tooltips. M, sonnet
- L6 Debug panel load/play (26a): load-by-path and play per deck via `DeckBinding`, or retire the panel. S, haiku

## Phase S - Stems (Todos 19, 37, 30, 39)
- S1 `om-stems` `onnx` feature (off by default): `OrtSeparator` over `ort`; fixture toy model (4-way split) for tests; htdemucs STFT/iSTFT done in Rust (`realfft`), 7.8 s segments with 25% overlap-add, input normalised, 44.1 kHz in/out. L, sonnet
- S2 `StemService` on `WorkerPool` (1 worker, ORT threads <= cores/2, cancel, progress); fed by a "Prepare stems" action and an optional pref, never by load. M, sonnet
- S3 Stem cache + schema: `<data>/stems/<content_hash>-<sep_id>-v<n>/{drums,bass,vocals,other}.wav` (16-bit stereo, written at the track's original rate so frame positions match) + `meta.json`; migration 8 `track_stems(track_id, sep_id, version, dir, bytes)`; LRU cap pref. M, sonnet
- S4 Model manifest/licence UI: hosted manifest entry for htdemucs, licence dialog in the settings Library page; `docs/legal/asset-provenance.csv` row. S, haiku
- S5 `om-dsp` `StemMixer` (4 smoothed gains, mute, per-stem sweep filter; allocation-free) + `om-engine` stem mode: decoder reads 4 files in lockstep into a ring of 8 floats per frame; prefetch slots widened; the 8-to-2 mix happens at source-frame pull, before interpolation and key-lock (one stretcher). `assert_no_alloc` render test, soak variant. L, sonnet
- S6 `ParamId::Stem` target + `StemSnapshot`; MIDI map ids; `om-ui` stem widget model. M, sonnet
- S7 `om-ui-gpui` stem pads view (Todo 30) per deck: mute/volume/filter, activity from snapshot. M, sonnet
- S8 Stem transition helpers (Todo 39): `StemFade` ramps (e.g. bass swap, vocals out) driven by the control clock. M, sonnet

```rust
// om-stems: StemSet becomes stereo; mono remains for the mock via from_mono()
pub struct StemBuffer { pub left: Vec<f32>, pub right: Vec<f32> }
pub struct StemSet { pub sample_rate: f64, pub stems: BTreeMap<StemKind, StemBuffer> }
pub trait StemSeparator: Send + Sync {            // INTERFACE_VERSION -> 2
    fn id(&self) -> &str;  fn version(&self) -> i32;
    fn native_rate(&self) -> f64;                 // pipeline resamples in/out
    fn separate_to(&self, input: &mut dyn StereoSource, sink: &mut dyn StemSink,
                   ctx: &dyn AnalysisContext) -> Result<(), StemError>;   // streamed, no whole-track buffers
}
pub trait StemSink { fn write(&mut self, kind: StemKind, left: &[f32], right: &[f32]) -> Result<(), StemError>; }
```

## Phase A - Automix (Todo 38)
- A1 `om-automix` crate (pure, no engine dep): `TransitionPlanner` over BPM/grid/key/structure; styles Crossfade, BassSwap, EchoOut, Cut. M, sonnet
- A2 `AutomixController` (control thread, `WorkerPool` for planning): arms next track, schedules by control clock, emits `Command::Param` steps only. M, sonnet
- A3 Queue source: next track from playlist/search, avoids tracks in `play_history`; key-compatible pick via `is_harmonically_compatible`. S, sonnet
- A4 Stem-aware styles (uses S8). S, sonnet
- A5 UI panel in settings/main layout (enable, style, length, skip, cancel) + `AutomixSnapshot`. M, sonnet
- A6 Safety tests: any manual fader/crossfader/play/seek input cancels the plan and the user's value wins; planner never touches the audio thread; deck underrun leaves automix off. S, sonnet

```rust
pub struct TransitionPlan { pub style: Style, pub start_beat: f64, pub length_beats: f64, pub steps: Vec<Step> }
pub struct Step { pub at_beat: f64, pub id: ParamId, pub from: f64, pub to: f64 }   // linear ramp, applied per tick
pub trait TransitionPlanner { fn plan(&self, out: &DeckInfo, inc: &DeckInfo, style: Style) -> Option<TransitionPlan>; }
pub struct DeckInfo { pub grid: Arc<dyn BeatGrid>, pub camelot: String, pub sections: Vec<Section>, pub mix_out: Option<f64>, pub mix_in: Option<f64>, pub has_stems: bool }
```

## Phase P - Sampler (Todo 40)
- P1 `om-engine` `sampler`: 8 pads x 4 voices, preallocated; samples decoded off-thread into `Arc<[f32]>`, handed over by `rtrb` ring; trigger/stop commands by ring; mixed into master pre-limiter. L, sonnet
- P2 Sample bank storage: `<data>/samples/*.wav` + `sampler.json` (pad -> file, gain, mode one-shot/gate/loop); `Command::SamplerAssign`. S, haiku
- P3 `ParamId::Sampler` target, MIDI ids, `SamplerSnapshot`. S, haiku
- P4 Pad UI (model in `om-ui`, view in `om-ui-gpui`). M, sonnet
- P5 `assert_no_alloc` + soak scenario with pad triggers. S, haiku

```rust
pub struct SamplerControl;   // control side, Send+Sync
impl SamplerControl { pub fn assign(&self, pad: u8, s: Arc<Sample>) -> Result<(), SamplerError>;
  pub fn trigger(&self, pad: u8, velocity: f32); pub fn release(&self, pad: u8); pub fn set_gain(&self, pad: u8, g: f32); }
pub struct SamplerAudio;     // implements ProcessStage; allocation-free
```

## Open Decisions
1. Inference runtime: `ort` (recommended; TODO says so) over tract (op/perf gaps for htdemucs) or candle (needs a model port). Disagree: S1 grows to L+.
2. Model: HTDemucs 4-source, Meta MIT code and weights, ONNX with STFT outside the graph; exported by the user (script in `tools/`) and hosted with a manifest. Licence caveat: training data (MUSDB18-HQ research terms) is not covered by MIT; show it in the licence dialog and record in `docs/legal`. Open-Unmix weights are CC BY-NC-SA: rejected. Fallback: Spleeter 4stems (MIT, lower quality). Disagree: only S1/S4 change.
3. `ort` binaries: `download-binaries` now (build needs network, `onnx` feature off by default, own CI job); `load-dynamic` at packaging (Wave 9). `om-stems` stays `forbid(unsafe)` because ort's API is safe; if raw IoBinding is needed, move to a new `om-stems-ort` crate that lifts the lint.
4. FTS5 vs LIKE: FTS5 for free text with LIKE fallback (bundled SQLite has it). Disagree: drop L4, keep LIKE.
5. Artwork: BLOB table (recommended, one file, thumbnail only). Alternative: files in the data dir with a path column (smaller DB, orphan cleanup needed).
6. Stem cache: 16-bit WAV, 4 files (~210 MB per 5 min track, LRU cap). Alternative: FLAC via `flacenc` (half the size, new dependency and encoder CPU).
7. Stem playback: 8-float ring, mix before interpolation/key-lock (one stretcher). Rejected: four stereo decks (drift, 4x stretch CPU) and mixing in the decoder thread (about a second of control lag).
8. Snapshot: pull-based `DjEngine::snapshot()`, no `arc-swap` (not in `Cargo.lock`). Disagree: push via `ArcSwap` adds a dependency and stale reads in tests; update DESIGN_PATTERNS.
9. Command enum shape: `Param{ParamId,f64}` plus a few payload variants, `set_parameter` stays as the façade. Disagree (pure typed commands only): MIDI/UI/plugins all need rework; larger F.
10. gpui in workspace: stay outside, add `gpui-lint` CI. In-workspace gives one lockfile but every `cargo test --workspace` (WSL, macOS, Windows) builds GPUI.
11. Dynamic plugin loading (10a): stay deferred; Wave 7 uses in-process plugins and `ParamId::Ext`.
12. Automix as its own crate `om-automix` (pure planner, testable without audio) rather than inside `om-engine`.

## Out of scope and user-only
Out: Wave 8 (recording, broadcast, DVS, video), Wave 9 (legal audit, packaging, release), Windows/macOS, HID (35), mapping editor (36), controller maps (34), FFmpeg (8b), dynamic plugins, playlist editing, `--settings` flag, scan-interval/hot-plug polish.
User-only verification: the GPUI window (including stem pads, sampler pads, Automix panel, LookAndFeel) never viewed by eye; real ALSA/PipeWire device and headphone-cue stream; a real MIDI controller; ONNX export and hosting of the model, and listening to separated stems; xrun check on real hardware while a stem job runs; native macOS/Windows runs.

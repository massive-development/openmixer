# Rust Port, Mixer First — Implementation Plan (historical)

> Historical record from the move to Rust (2026-10-18). Not maintained: current status is in `TODO.md` and `docs/PROGRESS.md`.

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development. Lean mode (user directive): the goal is a working mixer, not process. Gate per task = ported tests + `cargo fmt/clippy -D warnings/test --workspace` + CI; one review per phase, not per task.

**Goal:** Reach C++ parity (Wave 1-6 as built in tag `cpp-final`) in Rust, mixer first, Linux first.
**Spec:** `docs/superpowers/specs/2026-10-18-rust-rewrite-design.md`; decisions in `docs/spikes/DECISIONS.md` (gpui =0.2.2, cpal, Rubber Band R3 behind a `Stretcher` trait in `om-stretch`, enable `GPL-2.0-or-later` in deny.toml when it lands).
**Source of truth for behaviour:** C++ in `src/` and its tests in `tests/unit/`, `tools/` (also `git show cpp-final:<path>`). Port behaviour and test cases 1:1 (snake_case names), not structure.

## Global Constraints
- Branch `rust/port`; commit per task; trailer `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`; push per phase.
- `#![forbid(unsafe_code)]` except `om-stretch` (FFI). Audio thread: no allocation/locks/syscalls; `assert_no_alloc` tests for render paths.
- NEVER emit audible sound in tests/agent runs: render to buffers; device streams write zeros; audible opt-in `OPENMIXER_AUDIBLE=1` gain<=0.05 user-run.
- Adopt standard crates directly (cpal, symphonia, rtrb, arc-swap, rusqlite, midir, lofty, realfft, serde/serde_json, thiserror, tracing); no spikes.
- Each task ends: update `TODO.md` checkbox + `docs/progress.json` status, regenerate heatmap (`python3 -I tools/progress_heatmap.py`), `python3 -I tools/check_docs_rust.py` ok.
- Control surface stays a string-id `ParameterTarget` trait (`set_parameter(&self, id:&str, value:f64)->bool`) so MIDI/UI/tests share it.

## Phases (tasks in order; models: haiku = mechanical translation, sonnet = engine/UI judgment)
**Phase A — audible mixer core (headless)**
1. `om-core`+`om-dsp`: ParameterTarget, parameter router, ring buffers (SPSC frame ring), Biquad, ThreeBandEq, SweepFilter, PeakMeter, Crossfader (curves). [sonnet] Tests: RingBufferTest, MeterBallisticsTest, parts of MixerTest.
2. `om-engine` fx: Effect trait, DelayLine, Echo/Reverb/Flanger/Phaser/Crush/Gate/Roll/TapeStop, FxUnit/FxRack/FxFactory/FxPresets. [haiku] Test: FxTest.
3. `om-engine` mixer: ChannelStrip, Mixer, MixerParameters, StageChain/ProcessStage, OutputRouter (master/cue), LatencyMonitor. [sonnet] Tests: MixerTest, OutputRouterTest.
4. `om-engine` deck: Deck (symphonia decode, streaming ring, transport), DeckDecoder, DeckPlayback, DeckControl, LoopState/LoopPrefetch, HotcueBank, Quantizer, SlipTracker, BeatGrid, Sync. [sonnet] Tests: DeckTest, DeckLoopTest, SyncTest, TrackLoaderTest.
5. `om-stretch` (Rubber Band R3 FFI behind `Stretcher` trait; enable GPL entry in deny.toml) + Deck keylock. [sonnet] Test: DeckKeylockTest.
6. `om-audio`: `AudioDevice` trait, MockAudioDevice, cpal device, DjEngine wiring (2 decks, master+cue), engine control timer. [sonnet] Tests: AudioDeviceTest, DjEngineTest, EngineWiringTest, ControlThreadSafetyTest, RingBufferTest.
7. Soak gate: short soak in CI, 10-min `#[ignore]` run. [haiku] Port tools/soak_mix.cpp. **Milestone: headless mixer equals C++ Wave 2.**
**Phase B — minimal playable app (GPUI)**
8. `om-ui`+`openmixer` bin: GPUI window, theme tokens/loader (resources/themes/*.json), deck widgets (transport, waveform strip, hotcues), mixer widgets (strips, EQ, crossfader, meters, FX panel), debug panel. [sonnet] Tests: ThemeJson/ThemeRegistry/ValueFormat/MeterBallistics/DeckWidget/MixerWidget/FxPanel/MainLayout equivalents (logic-level; GPUI TestAppContext where available).
9. App composition + versioned preferences + settings panel + `--smoke` + UI gate (scripted DJ over mock device, short + 10-min ignored). [sonnet] Tests: PreferencesTest, AppPreferencesTargetsTest, SettingsPanelTest, ui_gate.
**Phase C — MIDI**
10. `om-midi`: event, input router (midir), map JSON, mapper, learn, feedback router/rules, trace. [sonnet] Tests: MidiMapperTest, MidiLearn*, MidiFeedback*.
**Phase D — analysis & library**
11. `om-analysis`: Stft, OnsetEnvelope/Picker, TempoSearch, BpmEstimator, BeatGridModel, Chromagram, KeyEstimator, MusicalKey (Camelot), Segmenter/StructureAnalyzer, WaveformBuilder/Data, job runner, SQLite result cache, AnalysisService. [sonnet, split into 2 dispatches: DSP then service] Tests: BpmEstimatorTest, KeyEstimatorTest, StructureAnalyzerTest, WaveformBuilderTest, AnalysisCacheTest, AnalysisServiceTest.
12. `om-library`: schema/migrations, DAOs, ingest + watch folders (lofty), search parser/compiler/engine, iTunes/Rekordbox/plist import, prep queue. [sonnet; haiku for importers/search] Tests: Library*Test.
13. UI: library browser + analysis/waveform wiring + prep queue. [sonnet] Tests: LibraryBrowserTest + library UI utils.
**Phase E — plugins, stems, close-out**
14. `om-plugins` (registry, stage catalog, in-process trait API; dynamic loading deferred, documented) and `om-stems` (separator trait, model manifest, licence-gated verified download). [haiku] Tests: PluginRegistryTest, StemsTest.
15. Parity close-out: fill `docs/parity.md` Rust column by script, remove the C++ tree (src/, tests/, CMake, C++ workflows, C++ tools) in its own commit (tag `cpp-final` preserves it), fix README/agents.md, whole-branch review. [sonnet]

## Review Focus
- Audio-thread allocation/locking in every render path; command/snapshot publication races (ControlThreadSafetyTest).
- Behavioural drift from C++ (fixtures from tests/data, golden numbers copied from C++ tests).
- GPUI API churn pinned to `=0.2.2`; UI logic kept testable without a window.
- Silent-by-default audio in every test.

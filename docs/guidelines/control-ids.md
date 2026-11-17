# Control paths, control ids and demo flags

Control paths, and what is live today:
- **UI (production window):** `DeckBinding::typed` and `MixerBinding::typed` send `Command::Param { id, value }` through the router's `CommandSink`, with no string formatting or parsing. The binding constructors without a sink (`new`) write through the string façade; the widget test rig uses them.
- **Web front end (Tauri window, ADR 0009):** the front end never types an id or a range. It reads the `controls` command once: a map from a stable front-end key (`deck0.play`, `ch1.eq.low`, `mixer.crossfader`) to the canonical id and the range, default, detent, kind and label that `om_ui_bridge::slice_controls` takes from the `om-ui` widget models. It then sends `control(id, value)`; `om_ui_bridge::dispatch_control` rejects a non-finite value, parses and validates the id with `ParamId`, and calls `ParameterRouter::dispatch` once, in the Tauri command handler on the main thread. A new control for the web window is a new key in `om-ui-bridge/src/control.rs` and its shape in `control_spec.rs`. Details: [docs/features/tauri-trial.md](../features/tauri-trial.md).
- **MIDI mappings and plugins:** the string façade, `ParameterTarget::set_parameter(&str, f64)` on the router. It parses the id once there and dispatches typed ids the same way.
- **Typed entry point:** `Command` and `CommandSink` are used by tests, by automix and by the Wave 7 features. The router's sink accepts `Command::Param` and `Command::Automix` (routed to the `automix` target). `LoadStems` and `SamplerAssign` are handled by `DjEngine::command_sink(fallback)` (`EngineCommands`): `DjEngine::register_stem_set(dir)` turns a stem directory into the `StemSetRef` handle the command carries, `DjEngine::register_sample(Arc<Sample>)` / `load_sample(path)` the `SampleId` for `SamplerAssign`; the router's sink still rejects both.
Rules: writers never write engine state directly; readers pull an `EngineSnapshot`; the audio thread takes no locks and never parses an id.

- Demo flags (`--demo-map`, `--demo-library`, `--demo-automix`, `--demo-record`) are listed in `openmixer::flags::DEMO_FLAGS`; `--demo-scene=NAME` takes a value, so `flags::demo_run` matches it by prefix instead. A demo run never writes the user's files: `composition::prefs_save_target` sends settings saves to `<scratch>/preferences.json` when `flags::demo_run` is true, and the recording and map paths also live in the scratch folder. Any new `--demo-*` flag must be added to `DEMO_FLAGS`, and its state must stay in memory or in the scratch folder. Covered by `openmixer/tests/demo_prefs.rs`.

## Plugin control targets

- A plugin registers its target with the router (`register_target`, a string prefix). The router holds its reentrant gate across every target call, and registration and unregistration wait on the same gate. So a slow or blocking `set_parameter` stalls MIDI and UI dispatch for everything else. Do not block, wait on I/O or take a long lock in `set_parameter`; hand the work to a thread or queue and return `true` at once.

## How to add a control id

1. `om-core/src/param_id/mod.rs`: add the variant to the owning enum (`DeckParam`, `MixerParam`, ...) and its line in the grammar doc comment.
2. `om-core/src/param_id/parse.rs`: parse it in the sub-namespace `FromStr`. `Display` in `mod.rs` must print it back exactly. If it carries an index or a bounded value, check it in `ParamId::validate` (`om-core/src/param_id/validate.rs`), which hand-built ids and `dispatch_control` rely on.
3. `om-core/src/param_id/tests.rs`: add the id to `GOLDEN`. Every id must round-trip canonically.
4. The owning target's `set_param` match arm (for example `om-engine/src/mixer/control.rs` or `deck_control.rs`): handle the variant and return `true`. Return `false` for variants it does not own.
5. If the value is readable: add a `StateId` variant (`om-core/src/state_id.rs`), a field on `DeckSnapshot` or `MixerSnapshot` with its read arm (`om-core/src/snapshot.rs`), and fill the field in `DjEngine::snapshot()` (`om-engine/src/dj_engine.rs`).
6. If a UI control or MIDI target uses it: build the id from `ParamId` in `om-ui/src/binding.rs`, and list it in `learnable_targets` (`om-ui/src/settings/midi.rs`) when MIDI learn should reach it.
7. Test with `nice -n 19 env CARGO_BUILD_JOBS=3 cargo test -p om-core -p om-engine`, and add a binding test in `om-ui` when step 6 applies.

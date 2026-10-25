//! Sampler pad view-model: presses and releases as typed commands (one-shot trigger, gate hold),
//! the key map, mode cycling, settings stored through the bank, loads through the bank, and the
//! pad view from the engine's snapshot.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use om_core::param_id::SamplerParam;
use om_core::{Command, CommandError, CommandSink, ParamId, ParameterRouter};
use om_engine::sampler::PadMode;
use om_engine::DjEngine;
use om_testing::fs::TempDir;
use om_testing::wav::write_float_wav;
use om_ui::sample_bank::SampleBank;
use om_ui::sampler_binding::{mode_label, next_mode, pad_for_key, SamplerBinding};

/// Records every command and forwards nothing.
#[derive(Default)]
struct Recorder(Mutex<Vec<Command>>);

impl CommandSink for Recorder {
    fn send(&self, cmd: Command) -> Result<(), CommandError> {
        self.0.lock().unwrap().push(cmd);
        Ok(())
    }
}

impl Recorder {
    fn taken(&self) -> Vec<(ParamId, f64)> {
        self.0
            .lock()
            .unwrap()
            .drain(..)
            .filter_map(|c| match c {
                Command::Param { id, value } => Some((id, value)),
                _ => None,
            })
            .collect()
    }
}

fn rig() -> (
    Arc<DjEngine>,
    Arc<ParameterRouter>,
    Arc<Recorder>,
    SamplerBinding,
) {
    let (engine, _audio) = DjEngine::new(2);
    let engine = Arc::new(engine);
    let router = Arc::new(ParameterRouter::new());
    engine.register(&router);
    let recorder = Arc::new(Recorder::default());
    let binding = SamplerBinding::new(
        Arc::clone(&engine),
        Some(recorder.clone()),
        router.clone(),
        None,
    );
    (engine, router, recorder, binding)
}

fn sampler_id(pad: u8, p: SamplerParam) -> ParamId {
    ParamId::Sampler { pad, p }
}

#[test]
fn keys_one_to_eight_name_pads_and_nothing_else() {
    assert_eq!(pad_for_key("1"), Some(0));
    assert_eq!(pad_for_key("8"), Some(7));
    for key in ["0", "9", "", "10", "a", "enter", "escape"] {
        assert_eq!(pad_for_key(key), None, "{key:?} must not press a pad");
    }
}

#[test]
fn a_one_shot_press_is_a_trigger_with_velocity_and_release_is_a_zero_trigger() {
    let (_engine, _router, recorder, binding) = rig();
    assert!(binding.press(2, 0.75));
    assert!(binding.release(2));
    assert_eq!(
        recorder.taken(),
        [
            (sampler_id(2, SamplerParam::Trigger), 0.75),
            (sampler_id(2, SamplerParam::Trigger), 0.0),
        ]
    );
}

#[test]
fn a_gate_pad_holds_on_press_and_lets_go_on_release() {
    let (engine, _router, recorder, binding) = rig();
    engine.sampler().set_mode(4, PadMode::Gate);
    assert!(binding.press(4, 0.2));
    assert!(binding.release(4));
    assert_eq!(
        recorder.taken(),
        [
            (sampler_id(4, SamplerParam::Gate), 1.0),
            (sampler_id(4, SamplerParam::Gate), 0.0),
        ]
    );
}

#[test]
fn bad_pads_are_refused_and_send_nothing() {
    let (_engine, _router, recorder, binding) = rig();
    assert!(!binding.press(8, 1.0));
    assert!(!binding.release(8));
    assert!(!binding.select(8));
    assert!(recorder.taken().is_empty());
}

#[test]
fn the_string_path_reaches_the_router_when_there_is_no_sink() {
    let (engine, router, _recorder, _binding) = rig();
    let binding = SamplerBinding::new(Arc::clone(&engine), None, router.clone(), None);
    assert!(binding.press(1, 0.5));
    assert!(binding.release(1));
}

#[test]
fn mode_cycles_one_shot_gate_loop_and_back() {
    assert_eq!(next_mode(PadMode::OneShot), PadMode::Gate);
    assert_eq!(next_mode(PadMode::Gate), PadMode::Loop);
    assert_eq!(next_mode(PadMode::Loop), PadMode::OneShot);
    assert_eq!(mode_label(PadMode::Gate), "gate");
    let (_engine, _router, _recorder, binding) = rig();
    assert_eq!(binding.cycle_mode(0).unwrap(), PadMode::Gate);
    assert_eq!(binding.mode(0), Some(PadMode::Gate));
}

#[test]
fn selection_is_clamped_to_the_eight_pads() {
    let (_engine, _router, _recorder, binding) = rig();
    assert_eq!(binding.selected(), 0);
    assert!(binding.select(5));
    assert_eq!(binding.selected(), 5);
    assert!(!binding.select(8));
    assert_eq!(binding.selected(), 5);
}

#[test]
fn the_pad_view_reads_the_snapshot_and_names_loaded_pads() {
    let (engine, _router, _recorder, binding) = rig();
    let dc = Arc::new(om_engine::Sample::from_mono("dc", 44_100.0, &[0.1; 100]).unwrap());
    engine.sampler().assign(3, dc).unwrap();
    engine.sampler().set_gain(3, 0.5);
    let pads = binding.pads();
    assert_eq!(pads.len(), 8);
    assert_eq!(pads[3].key, '4');
    assert!(pads[3].loaded);
    assert_eq!(pads[3].name, "Pad 4", "no bank, so no file name");
    assert_eq!(pads[3].gain, 0.5);
    assert_eq!(pads[0].name, "Empty");
    assert!(!pads[0].loaded && !pads[0].playing);
    assert!(pads[0].selected);
    assert!(binding.caption().starts_with("Empty: drop a sample"));
    binding.select(3);
    assert_eq!(binding.caption(), "Pad 4  One-shot  +0.0 st  50%");
}

#[test]
fn settings_without_a_bank_reach_the_engine_and_a_bank_stores_them() {
    let (engine, _router, _recorder, binding) = rig();
    binding.set_gain(6, 2.5).unwrap();
    binding.set_pitch(6, 30.0).unwrap();
    binding.set_mode(6, PadMode::Loop).unwrap();
    let pad = engine.snapshot().sampler.pads[6];
    assert_eq!(pad.gain, 1.5, "clamped to the pad maximum");
    assert_eq!(pad.pitch, 24.0, "clamped to the engine limit");
    assert_eq!(pad.mode, PadMode::Loop);
}

#[test]
fn a_loaded_file_is_decoded_by_the_bank_and_its_settings_persist() {
    let tmp = TempDir::new("sampler-binding");
    let (engine, _router, _recorder, _b) = rig();
    let bank = Arc::new(SampleBank::open(
        tmp.path().join("samples"),
        Arc::clone(&engine),
        engine.command_sink(None),
    ));
    let binding = SamplerBinding::new(
        Arc::clone(&engine),
        Some(engine.command_sink(None)),
        Arc::new(ParameterRouter::new()),
        Some(Arc::clone(&bank)),
    );
    let wav = tmp.file("kick.wav");
    write_float_wav(&wav, 1, 44_100, &[0.4; 1000]);
    binding.set_path(0, wav.to_str().unwrap());
    let handle = binding.load_typed_path(0).unwrap();
    assert!(handle.wait(Duration::from_secs(10)));
    assert_eq!(
        bank.settle(Duration::from_secs(10)),
        1,
        "one decode installed"
    );
    binding.prune_pending();
    assert!(engine.snapshot().sampler.pads[0].loaded);
    assert_eq!(binding.pads()[0].name, "kick");

    binding.set_mode(0, PadMode::Gate).unwrap();
    let reopened = SampleBank::open(
        tmp.path().join("samples"),
        Arc::clone(&engine),
        engine.command_sink(None),
    );
    assert_eq!(reopened.entry(0).unwrap().mode, PadMode::Gate);
}

#[test]
fn loading_without_a_bank_or_with_an_empty_path_is_refused() {
    let (_engine, _router, _recorder, binding) = rig();
    assert!(binding.load(0, std::path::Path::new("/nope.wav")).is_err());
    assert!(binding.load_typed_path(0).is_err());
    binding.set_path(0, "   ");
    assert!(binding.load_typed_path(0).is_err());
}

#[test]
fn every_mode_has_a_label_and_a_hint_and_loop_explains_the_latch() {
    use om_ui::sampler_binding::{mode_hint, mode_label};
    for mode in [PadMode::OneShot, PadMode::Gate, PadMode::Loop] {
        assert!(!mode_label(mode).is_empty() && !mode_hint(mode).is_empty());
    }
    assert!(mode_hint(PadMode::Loop).contains("again"));
}

fn pid(s: &str) -> ParamId {
    s.parse().unwrap()
}

#[test]
fn pressing_a_second_pad_releases_the_first_held_one() {
    let (engine, _router, recorder, binding) = rig();
    engine.sampler().set_mode(0, PadMode::Gate);
    assert!(binding.press_held(0, 0.9));
    assert_eq!(binding.held(), Some(0));
    assert!(binding.press_held(1, 0.9));
    assert_eq!(binding.held(), Some(1));
    assert_eq!(
        recorder.taken(),
        vec![
            (pid("sampler.pad0.gate"), 1.0),
            (pid("sampler.pad0.gate"), 0.0),
            (pid("sampler.pad1.trigger"), 0.9_f32 as f64),
        ]
    );
    // A bad pad changes nothing.
    assert!(!binding.press_held(99, 1.0));
    assert_eq!(binding.held(), Some(1));
}

#[test]
fn release_held_lets_go_once_and_key_up_only_releases_its_own_pad() {
    let (engine, _router, recorder, binding) = rig();
    engine.sampler().set_mode(2, PadMode::Gate);
    binding.press_held(2, 1.0);
    recorder.taken();
    assert!(!binding.release_held_if(3), "another pad's key coming up");
    assert_eq!(binding.held(), Some(2));
    assert!(binding.release_held_if(2));
    assert_eq!(recorder.taken(), vec![(pid("sampler.pad2.gate"), 0.0)]);
    // Focus lost or mouse up elsewhere with nothing held is a no-op.
    assert_eq!(binding.release_held(), None);
    assert!(recorder.taken().is_empty());
}

#[test]
fn stop_all_forgets_the_held_pad_and_queues_a_stop() {
    let (engine, _router, _recorder, binding) = rig();
    binding.press_held(4, 1.0);
    binding.stop_all();
    assert_eq!(binding.held(), None);
    assert_eq!(engine.sampler().dropped_commands(), 0);
}

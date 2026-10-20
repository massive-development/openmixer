//! Plugin registry cases. Loading the example plugin from a shared library is covered by
//! `examples/plugins/gain-plugin/tests`, and the loader stub is tested here instead.

use om_core::{ParameterRouter, ParameterTarget};
use om_engine::{ChannelInput, Mixer, MixerControl, ProcessStage};
use om_midi::{control_change, MidiMapper};
use om_plugins::{
    InsertAt, InsertPoint, Plugin, PluginError, PluginHost, PluginInfo, PluginRegistry,
    StageFactory, StageKind, API_VERSION,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

const BLOCK: usize = 256;

/// Test rig: a mixer fed a constant signal, measured at the master peak.
struct Rig {
    mixer: Mixer,
    control: Arc<MixerControl>,
    input: Vec<f32>,
    ml: Vec<f32>,
    mr: Vec<f32>,
    cl: Vec<f32>,
    cr: Vec<f32>,
}

impl Rig {
    fn new() -> Self {
        let (mut mixer, control) = Mixer::new(2);
        mixer.prepare(48_000.0, BLOCK);
        Self {
            mixer,
            control: Arc::new(control),
            input: vec![0.5; BLOCK],
            ml: vec![0.0; BLOCK],
            mr: vec![0.0; BLOCK],
            cl: vec![0.0; BLOCK],
            cr: vec![0.0; BLOCK],
        }
    }

    fn point(&self) -> Arc<dyn InsertPoint> {
        self.control.clone()
    }

    fn master_peak(&mut self) -> f32 {
        let inputs = [
            ChannelInput {
                left: Some(&self.input),
                right: Some(&self.input),
            },
            ChannelInput::default(),
        ];
        // Let smoothers and the peak decay settle.
        for _ in 0..400 {
            self.mixer.process(
                &inputs,
                &mut self.ml,
                &mut self.mr,
                &mut self.cl,
                &mut self.cr,
            );
        }
        self.control.master_peak_left()
    }
}

/// Records the last id and value it was handed.
#[derive(Default)]
struct Recorder {
    last: Mutex<(String, f64)>,
}

impl ParameterTarget for Recorder {
    fn set_parameter(&self, id: &str, value: f64) -> bool {
        *self.last.lock().unwrap() = (id.to_owned(), value);
        true
    }
}

struct Mute;

impl ProcessStage for Mute {
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        left.fill(0.0);
        right.fill(0.0);
    }
}

fn mute_factory() -> StageFactory {
    Box::new(|| Box::new(Mute))
}

/// The registration results a test plugin observed in `on_load`, read back by the test.
#[derive(Default)]
struct Outcomes(Mutex<HashMap<&'static str, bool>>);

impl Outcomes {
    fn get(&self, key: &str) -> bool {
        self.0.lock().unwrap()[key]
    }
}

/// In-process plugin so registry behaviour is testable without a shared library.
struct TestPlugin {
    id: String,
    recorder: Arc<Recorder>,
    api: u32,
    outcomes: Arc<Outcomes>,
    unloads: Arc<AtomicUsize>,
}

impl TestPlugin {
    fn new(id: &str, recorder: &Arc<Recorder>, unloads: &Arc<AtomicUsize>) -> Self {
        Self {
            id: id.to_owned(),
            recorder: Arc::clone(recorder),
            api: API_VERSION,
            outcomes: Arc::new(Outcomes::default()),
            unloads: Arc::clone(unloads),
        }
    }

    fn with_api(mut self, api: u32) -> Self {
        self.api = api;
        self
    }
}

impl Plugin for TestPlugin {
    fn api_version(&self) -> u32 {
        self.api
    }

    fn info(&self) -> PluginInfo {
        PluginInfo {
            id: self.id.clone(),
            name: "Test".into(),
            version: "1".into(),
        }
    }

    fn on_load(&mut self, host: &mut dyn PluginHost) {
        let mut map = self.outcomes.0.lock().unwrap();
        map.insert(
            "stageOk",
            host.register_stage("test.mute", StageKind::PreProcess, mute_factory())
                .is_ok(),
        );
        map.insert(
            "duplicateStageOk",
            host.register_stage("test.mute", StageKind::Fx, mute_factory())
                .is_ok(),
        );
        map.insert(
            "badMapOk",
            host.register_midi_map("bad", "{ not json").is_ok(),
        );
        map.insert(
            "mapOk",
            host.register_midi_map(
                "test.map",
                r#"{ "mappings": [ { "type": "cc", "number": 7, "target": "test.x" } ] }"#,
            )
            .is_ok(),
        );
        map.insert(
            "targetOk",
            host.register_parameter_target("test", self.recorder.clone())
                .is_ok(),
        );
    }

    fn on_unload(&mut self) {
        self.unloads.fetch_add(1, Ordering::SeqCst);
    }
}

fn counter() -> Arc<AtomicUsize> {
    Arc::new(AtomicUsize::new(0))
}

#[test]
fn registrations_are_validated_and_listed() {
    let rec = Arc::new(Recorder::default());
    let unloads = counter();
    let mut reg = PluginRegistry::new();
    let plugin = TestPlugin::new("test.a", &rec, &unloads);
    let outcomes = Arc::clone(&plugin.outcomes);
    reg.add_plugin(Box::new(plugin)).expect("loads");

    assert!(outcomes.get("stageOk") && outcomes.get("mapOk") && outcomes.get("targetOk"));
    assert!(!outcomes.get("duplicateStageOk") && !outcomes.get("badMapOk"));
    assert_eq!(reg.stage_ids(None), vec!["test.mute".to_owned()]);
    assert_eq!(reg.midi_map_names(), vec!["test.map".to_owned()]);
    assert!(reg
        .add_plugin(Box::new(TestPlugin::new("test.a", &rec, &unloads)))
        .is_err());
    assert!(reg
        .add_plugin(Box::new(
            TestPlugin::new("test.b", &rec, &unloads).with_api(99)
        ))
        .is_err());
}

#[test]
fn stages_attach_to_channel_and_master_then_detach_on_unload() {
    let rec = Arc::new(Recorder::default());
    let unloads = counter();
    let mut rig = Rig::new();
    let mut reg = PluginRegistry::new(); // declared last: it detaches from the rig on drop
    let baseline = rig.master_peak();
    assert!(baseline > 0.0);

    reg.add_plugin(Box::new(TestPlugin::new("test.a", &rec, &unloads)))
        .expect("loads");
    reg.attach_stage("test.mute", &rig.point(), InsertAt::Master, 0)
        .expect("attaches");
    assert!(rig.master_peak() < baseline * 0.01);

    assert!(reg.unload("test.a"));
    assert!(rig.master_peak() > 0.0);
    assert!(reg.stage_ids(None).is_empty());

    assert!(reg
        .attach_stage("nope", &rig.point(), InsertAt::Master, 0)
        .is_err());
}

#[test]
fn bad_channel_or_slot_is_rejected() {
    let rec = Arc::new(Recorder::default());
    let unloads = counter();
    let rig = Rig::new();
    let mut reg = PluginRegistry::new();
    reg.add_plugin(Box::new(TestPlugin::new("test.a", &rec, &unloads)))
        .expect("loads");
    let point = rig.point();
    assert!(reg
        .attach_stage("test.mute", &point, InsertAt::Channel(5), 0)
        .is_err());
    assert!(reg
        .attach_stage(
            "test.mute",
            &point,
            InsertAt::Channel(0),
            om_engine::MAX_STAGES
        )
        .is_err());
    assert!(reg
        .attach_stage("test.mute", &point, InsertAt::Channel(0), 1)
        .is_ok());
    reg.detach_stage(&point, InsertAt::Channel(0), 1);
}

#[test]
fn parameter_targets_follow_the_bound_router() {
    let router = Arc::new(ParameterRouter::new());
    let rec = Arc::new(Recorder::default());
    let unloads = counter();
    let mut reg = PluginRegistry::new();
    reg.add_plugin(Box::new(TestPlugin::new("test.a", &rec, &unloads)))
        .expect("loads");
    reg.bind_router(Some(Arc::clone(&router)));
    assert!(router.set_parameter("test.knob", 0.25));
    assert_eq!(*rec.last.lock().unwrap(), ("knob".to_owned(), 0.25));

    reg.unload("test.a");
    assert!(!router.set_parameter("test.knob", 1.0));
}

#[test]
fn midi_map_from_a_plugin_drives_the_router() {
    let router = Arc::new(ParameterRouter::new());
    let rec = Arc::new(Recorder::default());
    let unloads = counter();
    let mapper = MidiMapper::new();
    let mut reg = PluginRegistry::new();
    mapper.set_target(Arc::clone(&router) as Arc<dyn ParameterTarget>);
    reg.bind_router(Some(Arc::clone(&router)));
    reg.add_plugin(Box::new(TestPlugin::new("test.a", &rec, &unloads)))
        .expect("loads");
    reg.apply_midi_map("test.map", &mapper).expect("applies");
    mapper.handle_message(&control_change(1, 7, 127));
    assert_eq!(rec.last.lock().unwrap().0, "x");
    assert!(matches!(
        reg.apply_midi_map("missing", &mapper),
        Err(PluginError::UnknownMap(_))
    ));
}

#[test]
fn destruction_unloads_plugins() {
    let rec = Arc::new(Recorder::default());
    let unloads = counter();
    {
        let mut reg = PluginRegistry::new();
        reg.add_plugin(Box::new(TestPlugin::new("test.a", &rec, &unloads)))
            .expect("loads");
    }
    assert_eq!(unloads.load(Ordering::SeqCst), 1);
}

#[test]
fn invalid_libraries_are_rejected() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut reg = PluginRegistry::new();
    assert!(reg
        .load_library(&dir.path().join("does_not_exist.so"))
        .is_err());

    let not_lib = dir.path().join("om_not_a_plugin.so");
    std::fs::write(&not_lib, "nope").expect("write");
    assert!(reg.load_library(&not_lib).is_err());

    // A folder with a shared library in it: the stub refuses it and says so. The count is 1.
    let example = dir.path().join("libgain.so");
    std::fs::write(&example, "placeholder").expect("write");
    let mut errors = Vec::new();
    assert_eq!(reg.load_directory(dir.path(), &mut errors), 0);
    assert_eq!(errors.len(), 2, "one per shared library file found");
    assert!(errors[0].contains("not supported"));
}

/// Tries to register every engine namespace as a parameter prefix and records the refusals.
struct Squatter {
    refused: Arc<Mutex<Vec<String>>>,
}

impl Plugin for Squatter {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            id: "test.squatter".into(),
            name: "Squatter".into(),
            version: "1".into(),
        }
    }

    fn on_load(&mut self, host: &mut dyn PluginHost) {
        let target: Arc<dyn ParameterTarget> = Arc::new(Recorder::default());
        for prefix in [
            "deck", "deck.0", "mixer", "fx", "sync", "stem", "sampler", "automix", "record",
        ] {
            let outcome = host.register_parameter_target(prefix, Arc::clone(&target));
            if let Err(PluginError::Rejected(_)) = outcome {
                self.refused.lock().unwrap().push(prefix.to_owned());
            }
        }
    }
}

#[test]
fn plugins_cannot_register_engine_namespaces() {
    let refused = Arc::new(Mutex::new(Vec::new()));
    let mut reg = PluginRegistry::new();
    reg.add_plugin(Box::new(Squatter {
        refused: Arc::clone(&refused),
    }))
    .expect("loads");
    assert_eq!(
        *refused.lock().unwrap(),
        ["deck", "deck.0", "mixer", "fx", "sync", "stem", "sampler", "automix", "record"]
    );
    let router = Arc::new(ParameterRouter::new());
    reg.bind_router(Some(Arc::clone(&router)));
    assert!(!router.set_parameter("deck.0.play", 1.0));
    assert_eq!(router.unknown_count(), 1);
}

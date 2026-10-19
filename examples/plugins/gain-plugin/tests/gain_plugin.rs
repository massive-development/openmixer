//! Port of `PluginLibraryTest.ExamplePluginLoadsFromASharedLibrary`, in-process: the same checks on
//! the same routing, with the plugin added to the registry instead of loaded from a library.

use gain_plugin::{GainPlugin, MAP_NAME, PLUGIN_ID, STAGE_ID};
use om_core::{ParameterRouter, ParameterTarget};
use om_engine::{ChannelInput, Mixer, MixerControl};
use om_midi::{control_change, MidiMapper};
use om_plugins::{InsertAt, InsertPoint, PluginRegistry};
use std::sync::Arc;

const BLOCK: usize = 256;

fn master_peak(mixer: &mut Mixer, control: &MixerControl) -> f32 {
    let input = vec![0.5_f32; BLOCK];
    let inputs = [
        ChannelInput {
            left: Some(&input),
            right: Some(&input),
        },
        ChannelInput::default(),
    ];
    let mut out = [vec![0.0_f32; BLOCK], vec![0.0; BLOCK], vec![0.0; BLOCK], vec![0.0; BLOCK]];
    for _ in 0..400 {
        let [ml, mr, cl, cr] = &mut out;
        mixer.process(&inputs, ml, mr, cl, cr);
    }
    control.master_peak_left()
}

#[test]
fn example_plugin_loads_in_process() {
    let (mut mixer, control) = Mixer::new(2);
    mixer.prepare(48_000.0, BLOCK);
    let control = Arc::new(control);
    let point: Arc<dyn InsertPoint> = control.clone();
    let router = Arc::new(ParameterRouter::new());
    let mapper = MidiMapper::new();
    let mut reg = PluginRegistry::new();
    mapper.set_target(router.clone());
    reg.bind_router(Some(Arc::clone(&router)));

    reg.add_plugin(Box::new(GainPlugin::new())).expect("loads");
    assert!(
        reg.add_plugin(Box::new(GainPlugin::new())).is_err(),
        "same id twice"
    );
    assert_eq!(reg.plugin_ids(), vec![PLUGIN_ID.to_owned()]);
    assert!(reg.stage_ids(None).contains(&STAGE_ID.to_owned()));

    let baseline = master_peak(&mut mixer, &control);
    reg.attach_stage(STAGE_ID, &point, InsertAt::Master, 0)
        .expect("attaches");
    assert!((master_peak(&mut mixer, &control) - baseline).abs() < 1.0e-4);

    reg.apply_midi_map(MAP_NAME, &mapper).expect("applies");
    mapper.handle_message(&control_change(1, 1, 0)); // gain -> 0
    assert!(master_peak(&mut mixer, &control) < baseline * 0.01);

    assert!(router.set_parameter("example.gain", 0.5));
    assert!((master_peak(&mut mixer, &control) - baseline * 0.5).abs() < 1.0e-3);

    assert!(reg.unload(PLUGIN_ID));
    assert!(!router.set_parameter("example.gain", 1.0));
    assert!((master_peak(&mut mixer, &control) - baseline).abs() < 1.0e-4);
}

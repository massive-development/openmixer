#![forbid(unsafe_code)]

//! Example plugin: a gain stage (`example.gain`), a parameter target (`example.gain` = the gain
//! value), a MIDI map (`example.gain-knob`, CC 1 to the gain) and a theme.
//!
//! This is the in-process example: it is linked into the application or loaded by a test with
//! `PluginRegistry::add_plugin(Box::new(GainPlugin::new()))`. The shared-library (C ABI) example is
//! `examples/plugins/gain-plugin-cdylib`.

use om_core::ParameterTarget;
use om_engine::ProcessStage;
use om_plugins::{Plugin, PluginHost, PluginInfo, StageKind};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

pub const PLUGIN_ID: &str = "org.openmixer.example.gain";
pub const STAGE_ID: &str = "example.gain";
pub const THEME_ID: &str = "org.openmixer.example.theme";
pub const MAP_NAME: &str = "example.gain-knob";

/// Gain as `f32` bits, shared by the stage and the parameter target.
type GainBits = Arc<AtomicU32>;

fn gain_of(bits: &GainBits) -> f32 {
    f32::from_bits(bits.load(Ordering::Relaxed))
}

struct GainStage {
    gain: GainBits,
}

impl ProcessStage for GainStage {
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let g = gain_of(&self.gain);
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            *l *= g;
            *r *= g;
        }
    }
}

struct GainTarget {
    gain: GainBits,
}

impl ParameterTarget for GainTarget {
    fn set_parameter(&self, id: &str, value: f64) -> bool {
        if id != "gain" {
            return false;
        }
        self.gain.store((value as f32).to_bits(), Ordering::Relaxed);
        true
    }
}

pub struct GainPlugin {
    gain: GainBits,
}

impl GainPlugin {
    pub fn new() -> Self {
        Self {
            gain: Arc::new(AtomicU32::new(1.0_f32.to_bits())),
        }
    }

    /// The current gain, for tests and for the plugin's own UI.
    pub fn gain(&self) -> f32 {
        gain_of(&self.gain)
    }
}

impl Default for GainPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for GainPlugin {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            id: PLUGIN_ID.into(),
            name: "Example Gain".into(),
            version: "1.0".into(),
        }
    }

    fn on_load(&mut self, host: &mut dyn PluginHost) {
        let bits = Arc::clone(&self.gain);
        // Registration results are not checked: a plugin registered in a fresh host cannot clash.
        let _ = host.register_stage(
            STAGE_ID,
            StageKind::PostProcess,
            Box::new(move || {
                Box::new(GainStage {
                    gain: Arc::clone(&bits),
                })
            }),
        );
        let _ = host.register_parameter_target(
            "example",
            Arc::new(GainTarget {
                gain: Arc::clone(&self.gain),
            }),
        );
        let _ = host.register_theme(&format!(
            r#"{{ "id": "{THEME_ID}", "name": "Example Plugin Theme" }}"#
        ));
        let _ = host.register_midi_map(
            MAP_NAME,
            r#"{ "name": "Example gain", "mappings": [
                { "type": "cc", "number": 1, "target": "example.gain", "min": 0, "max": 1 } ] }"#,
        );
    }
}

//! Original OpenMixer starting points for each built-in effect (data, not code paths).

use super::unit::FxUnit;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FxPreset {
    pub name: &'static str,
    pub effect: &'static str,
    pub mix: f32,
    pub params: &'static [(&'static str, f32)],
}

pub const BUILT_IN_PRESETS: &[FxPreset] = &[
    FxPreset {
        name: "Dotted Throw",
        effect: "echo",
        mix: 0.5,
        params: &[("time", 0.75), ("feedback", 0.55), ("tone", 0.45)],
    },
    FxPreset {
        name: "Big Room",
        effect: "reverb",
        mix: 0.35,
        params: &[("size", 0.85), ("damp", 0.3), ("width", 1.0)],
    },
    FxPreset {
        name: "Slow Jet",
        effect: "flanger",
        mix: 0.8,
        params: &[("time", 16.0), ("depth", 0.9), ("feedback", 0.6)],
    },
    FxPreset {
        name: "Bar Swirl",
        effect: "phaser",
        mix: 0.7,
        params: &[("time", 4.0), ("depth", 0.7), ("feedback", 0.5)],
    },
    FxPreset {
        name: "Lo-Fi Grit",
        effect: "crush",
        mix: 0.6,
        params: &[("bits", 6.0), ("downsample", 6.0)],
    },
    FxPreset {
        name: "Sixteenth Chop",
        effect: "gate",
        mix: 1.0,
        params: &[("time", 0.25), ("duty", 0.5)],
    },
    FxPreset {
        name: "Half Roll",
        effect: "roll",
        mix: 1.0,
        params: &[("time", 0.5)],
    },
    FxPreset {
        name: "Power Down",
        effect: "tapestop",
        mix: 1.0,
        params: &[("time", 2.0)],
    },
];

pub fn builtin_presets() -> &'static [FxPreset] {
    BUILT_IN_PRESETS
}

pub fn find_preset(name: &str) -> Option<&'static FxPreset> {
    BUILT_IN_PRESETS.iter().find(|p| p.name == name)
}

/// Returns `false` if the preset targets a different effect than the unit runs.
pub fn apply_preset(unit: &FxUnit, preset: &FxPreset) -> bool {
    let effect = unit.effect();
    if effect.name() != preset.effect {
        return false;
    }
    unit.set_mix(preset.mix);
    for (name, value) in preset.params {
        if let Some(index) = effect.find_param(name) {
            effect.set_param(index, *value);
        }
    }
    true
}

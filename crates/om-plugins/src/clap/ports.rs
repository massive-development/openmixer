//! Audio ports as data, and the stage descriptor built from them (ADR 0007, "Keeping instruments and
//! sidechain open"). The first release accepts one stereo input and one stereo output; a sidechain
//! or a note port later is a new descriptor value, not a new engine path.

use crate::PluginError;

/// What a port is for, from the plugin's `IS_MAIN` flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortRole {
    Main,
    Sidechain,
}

/// One audio port a plugin declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioPort {
    pub id: u32,
    pub name: String,
    pub channels: u32,
    pub role: PortRole,
}

/// The ports a plugin declares through the `audio-ports` extension.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PortLayout {
    pub inputs: Vec<AudioPort>,
    pub outputs: Vec<AudioPort>,
}

/// What a stage does with its ports. Only `Effect` is built today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageRole {
    Effect,
    /// Reserved: needs the `note-ports` feature and its own design note.
    Instrument,
}

/// A stage's shape: its role and audio ports. The engine only ever sees stereo in and out; the
/// descriptor is what the host checks before it builds the stage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageDescriptor {
    pub role: StageRole,
    pub inputs: Vec<AudioPort>,
    pub outputs: Vec<AudioPort>,
}

impl StageDescriptor {
    /// Accepts exactly one stereo input and one stereo output; anything else is refused with the reason.
    pub fn effect_from(layout: &PortLayout, plugin: &str) -> Result<Self, PluginError> {
        let refuse = |reason: String| PluginError::UnsupportedLayout {
            plugin: plugin.to_owned(),
            reason,
        };
        let [input] = layout.inputs.as_slice() else {
            return Err(refuse(format!(
                "{} input ports, the host takes 1 (sidechain is not supported yet)",
                layout.inputs.len()
            )));
        };
        let [output] = layout.outputs.as_slice() else {
            return Err(refuse(format!(
                "{} output ports, the host takes 1",
                layout.outputs.len()
            )));
        };
        for (name, port) in [("input", input), ("output", output)] {
            if port.channels != 2 {
                return Err(refuse(format!(
                    "{name} port has {} channels, the host takes 2",
                    port.channels
                )));
            }
        }
        Ok(Self {
            role: StageRole::Effect,
            inputs: layout.inputs.clone(),
            outputs: layout.outputs.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn port(id: u32, channels: u32) -> AudioPort {
        AudioPort {
            id,
            name: format!("p{id}"),
            channels,
            role: PortRole::Main,
        }
    }

    fn layout(inputs: &[u32], outputs: &[u32]) -> PortLayout {
        let make = |c: &[u32]| c.iter().zip(0..).map(|(&n, id)| port(id, n)).collect();
        PortLayout {
            inputs: make(inputs),
            outputs: make(outputs),
        }
    }

    #[test]
    fn one_stereo_pair_is_an_effect() {
        let d = StageDescriptor::effect_from(&layout(&[2], &[2]), "p").expect("stereo effect");
        assert_eq!(d.role, StageRole::Effect);
        assert_eq!((d.inputs.len(), d.outputs.len()), (1, 1));
    }

    #[test]
    fn other_layouts_are_refused_with_a_reason() {
        for (inputs, outputs, needle) in [
            (&[][..], &[2][..], "0 input"),
            (&[2, 2][..], &[2][..], "2 input"),
            (&[2][..], &[][..], "0 output"),
            (&[1][..], &[2][..], "input port has 1"),
            (&[2][..], &[6][..], "output port has 6"),
        ] {
            let err = StageDescriptor::effect_from(&layout(inputs, outputs), "p")
                .expect_err("refused")
                .to_string();
            assert!(err.contains(needle), "{err}");
        }
    }
}

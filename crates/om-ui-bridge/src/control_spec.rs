//! Each slice control's range, default, kind and label, read from the same `om-ui` widget models
//! the GPUI window builds, so the web front end never types a range by hand. Built on a control
//! thread; the models only read the engine while they are made, and send nothing.

use crate::control::slice_params;
use om_core::param_id::{ChannelParam, DeckParam, EqBand, MixerParam, MAX_CHANNELS};
use om_core::ParamId;
use om_ui::binding::MixerBinding;
use om_ui::control::{Knob, Ranged};
use om_ui::meter::MeterParams;
use om_ui::widgets::channel::ChannelStrip;
use om_ui::widgets::crossfader::CrossfaderSection;
use om_ui::widgets::loops::{SIZES, SIZE_NAMES};
use om_ui::widgets::master::MasterSection;
use om_ui::widgets::pitch::RANGES;
use om_ui::widgets::transport::TransportButton;

/// How a control is pressed or moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlKind {
    /// On or off: the front end sends the opposite of the state the snapshot reports.
    Toggle,
    /// A press: the front end sends 1.
    Trigger,
    /// A value anywhere in `min..=max`.
    Continuous,
}

/// One front-end control: its key, canonical id and the shape of its value, in engine units.
#[derive(Debug, Clone, PartialEq)]
pub struct ControlSpec {
    pub key: String,
    pub id: String,
    pub min: f64,
    pub max: f64,
    /// What a double-click restores.
    pub default: f64,
    /// The value a knob's pointer sits at 12 o'clock for, when its model has one.
    pub detent: Option<f64>,
    pub kind: ControlKind,
    /// The word GPUI prints on the control; empty where it prints none.
    pub label: String,
}

/// The value shape of one control, before its key and id are attached.
#[derive(Debug, Clone, PartialEq)]
struct Shape {
    min: f64,
    max: f64,
    default: f64,
    detent: Option<f64>,
    kind: ControlKind,
    label: String,
}

/// Every slice control for `decks` decks and the mixer's channels, with ranges from the models.
pub fn slice_controls(mixer: &MixerBinding, decks: u8) -> Vec<ControlSpec> {
    let channels = u8::try_from(mixer.num_channels()).unwrap_or(MAX_CHANNELS);
    let models = Models::new(mixer, channels);
    let params = slice_params(decks, channels).into_iter();
    params
        .filter_map(|(key, id)| {
            let shape = models.shape(&id)?;
            Some(spec(key, &id, shape))
        })
        .collect()
}

fn spec(key: String, id: &ParamId, s: Shape) -> ControlSpec {
    ControlSpec {
        key,
        id: id.to_string(),
        min: s.min,
        max: s.max,
        default: s.default,
        detent: s.detent,
        kind: s.kind,
        label: s.label,
    }
}

/// The GPUI models the mixer ranges come from.
struct Models {
    strips: Vec<ChannelStrip>,
    master: MasterSection,
    crossfader: CrossfaderSection,
}

impl Models {
    fn new(mixer: &MixerBinding, channels: u8) -> Self {
        let meter = MeterParams::default();
        let strip = |ch| ChannelStrip::new(mixer.clone(), usize::from(ch), meter);
        Self {
            strips: (0..channels).map(strip).collect(),
            master: MasterSection::new(mixer.clone(), meter),
            crossfader: CrossfaderSection::new(mixer.clone()),
        }
    }

    fn shape(&self, id: &ParamId) -> Option<Shape> {
        match id {
            ParamId::Deck { p, .. } => deck_shape(p),
            ParamId::Mixer(MixerParam::Channel { ch, p }) => self
                .strips
                .get(usize::from(*ch))
                .map(|s| channel_shape(s, p)),
            ParamId::Mixer(MixerParam::Master) => Some(knob(&self.master.fader)),
            ParamId::Mixer(MixerParam::Crossfader) => Some(ranged(&self.crossfader.slider, "")),
            _ => None,
        }
    }
}

fn channel_shape(strip: &ChannelStrip, p: &ChannelParam) -> Shape {
    match p {
        ChannelParam::Trim => knob(&strip.trim),
        ChannelParam::Filter => knob(&strip.filter),
        ChannelParam::Eq(band) => knob(&strip.eq[band_index(*band)]),
        _ => ranged(&strip.fader, ""),
    }
}

// `ChannelStrip::eq` is in engine order: low, mid, high.
fn band_index(band: EqBand) -> usize {
    match band {
        EqBand::Low => 0,
        EqBand::Mid => 1,
        EqBand::High => 2,
    }
}

fn deck_shape(p: &DeckParam) -> Option<Shape> {
    match p {
        DeckParam::Play => Some(toggle(TransportButton::Play)),
        DeckParam::Sync => Some(toggle(TransportButton::Sync)),
        DeckParam::Rate => Some(tempo()),
        DeckParam::Seek => Some(seek()),
        DeckParam::Hotcue { index, .. } => Some(trigger((index + 1).to_string())),
        DeckParam::LoopQuantized(beats) => Some(trigger(loop_name(*beats))),
        DeckParam::LoopExit => Some(trigger(String::new())),
        _ => None,
    }
}

fn loop_name(beats: f64) -> String {
    let at = SIZES.iter().position(|s| (s - beats).abs() < f64::EPSILON);
    at.map(|i| SIZE_NAMES[i].to_owned()).unwrap_or_default()
}

/// The tempo fader on its narrowest range, as `PitchFader::new` starts (rate 1.0 in the middle).
fn tempo() -> Shape {
    let range = RANGES[0];
    Shape {
        min: 1.0 - range,
        max: 1.0 + range,
        default: 1.0,
        detent: None,
        kind: ControlKind::Continuous,
        label: String::new(),
    }
}

/// `om_engine::deck::Deck::set_deck_param`: seek takes 0..1 of the track's length.
fn seek() -> Shape {
    Shape {
        min: 0.0,
        max: 1.0,
        default: 0.0,
        detent: None,
        kind: ControlKind::Continuous,
        label: String::new(),
    }
}

fn knob(k: &Knob) -> Shape {
    Shape {
        detent: k.detent,
        ..ranged(&k.range, &k.label)
    }
}

fn ranged(r: &Ranged, label: &str) -> Shape {
    Shape {
        min: r.min,
        max: r.max,
        default: r.default,
        detent: None,
        kind: ControlKind::Continuous,
        label: label.to_owned(),
    }
}

// The engine reads a press as a value of 0.5 or more.
fn toggle(button: TransportButton) -> Shape {
    Shape {
        kind: ControlKind::Toggle,
        label: button.label().to_owned(),
        ..trigger(String::new())
    }
}

fn trigger(label: String) -> Shape {
    Shape {
        min: 0.0,
        max: 1.0,
        default: 0.0,
        detent: None,
        kind: ControlKind::Trigger,
        label,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slice_control_ids;
    use om_core::ParameterRouter;
    use om_engine::DjEngine;
    use std::sync::Arc;

    fn controls() -> Vec<ControlSpec> {
        let engine = Arc::new(DjEngine::new(2).0);
        let mixer = MixerBinding::typed(engine, Arc::new(ParameterRouter::new()));
        slice_controls(&mixer, 2)
    }

    fn find(key: &str) -> ControlSpec {
        let all = controls();
        all.into_iter().find(|c| c.key == key).unwrap()
    }

    #[test]
    fn every_slice_key_has_a_shape_and_the_same_id() {
        let specs = controls();
        let ids = slice_control_ids(2, 2);
        assert_eq!(specs.len(), ids.len());
        for (spec, (key, id)) in specs.iter().zip(&ids) {
            assert_eq!((&spec.key, &spec.id), (key, id));
            assert!(
                spec.min <= spec.default && spec.default <= spec.max,
                "{key}"
            );
        }
    }

    #[test]
    fn mixer_ranges_are_the_gpui_models() {
        let trim = find("ch0.trim");
        assert_eq!((trim.min, trim.max, trim.default), (0.0, 4.0, 1.0));
        assert_eq!((trim.detent, trim.label.as_str()), (Some(1.0), "GAIN"));
        let low = find("ch1.eq.low");
        assert!(low.min < 0.0 && low.max > 0.0);
        assert_eq!(
            (low.default, low.detent, low.label.as_str()),
            (0.0, Some(0.0), "LOW")
        );
        let fader = find("ch0.fader");
        assert_eq!((fader.min, fader.max, fader.default), (0.0, 1.0, 1.0));
        let master = find("mixer.master");
        assert_eq!(
            (master.max, master.default, master.detent),
            (2.0, 1.0, Some(1.0))
        );
        assert_eq!(find("mixer.crossfader").default, 0.5);
    }

    #[test]
    fn deck_controls_have_their_kinds() {
        assert_eq!(find("deck0.play").kind, ControlKind::Toggle);
        assert_eq!(find("deck0.play").label, "Play");
        assert_eq!(find("deck1.sync").kind, ControlKind::Toggle);
        let tempo = find("deck0.tempo");
        assert!((tempo.min - 0.92).abs() < 1e-9 && (tempo.max - 1.08).abs() < 1e-9);
        assert_eq!((tempo.default, tempo.detent), (1.0, None));
        assert_eq!(find("deck0.hotcue7").label, "8");
        assert_eq!(find("deck0.hotcue7.clear").kind, ControlKind::Trigger);
        assert_eq!(find("deck1.loop.0.25").label, "1/4");
        assert_eq!(find("deck1.loop.32").label, "32");
        assert_eq!(find("deck0.seek").kind, ControlKind::Continuous);
    }
}

//! Each slice control's range, default, kind and label, read from the same `om-ui` widget models
//! the GPUI window builds, so the web front end never types a range by hand. Built on a control
//! thread; the models only read the engine while they are made, and send nothing.

use crate::control::{composite_controls, slice_params, Composite};
use crate::control_law::{sampled_law, snap_spec, tick_specs, SnapSpec, TickSpec, TravelLaw};
use om_core::param_id::{ChannelParam, DeckParam, EqBand, MixerParam, MAX_CHANNELS};
use om_core::ParamId;
use om_ui::binding::MixerBinding;
use om_ui::control::{Knob, Ranged};
use om_ui::fader_law::FaderLaw;
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
    /// How travel maps to the value: the taper GPUI's fader uses, else linear.
    pub law: TravelLaw,
    /// The dB marks GPUI draws beside the control; empty where it draws none.
    pub ticks: Vec<TickSpec>,
    /// The detent pull GPUI applies while the control is dragged, if any.
    pub snap: Option<SnapSpec>,
    /// A composite key's writes, in order, as (front-end key, value); its `id` is then empty.
    /// Empty for every single control.
    pub writes: Vec<(String, f64)>,
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
    law: TravelLaw,
    ticks: Vec<TickSpec>,
    snap: Option<SnapSpec>,
}

/// Every slice control for `decks` decks and the mixer's channels, with ranges from the models,
/// then the composite keys (`deck<d>.cue`).
pub fn slice_controls(mixer: &MixerBinding, decks: u8) -> Vec<ControlSpec> {
    let channels = u8::try_from(mixer.num_channels()).unwrap_or(MAX_CHANNELS);
    let models = Models::new(mixer, channels);
    let params = slice_params(decks, channels).into_iter();
    let single = params.filter_map(|(key, id)| {
        let shape = models.shape(&id)?;
        Some(spec(key, id.to_string(), shape))
    });
    let composite = composite_controls(decks).into_iter().map(composite_spec);
    single.chain(composite).collect()
}

/// A press with no id of its own: the front end sends its writes in order.
fn composite_spec(c: Composite) -> ControlSpec {
    ControlSpec {
        writes: c.writes,
        ..spec(c.key, String::new(), trigger(c.label.to_owned()))
    }
}

fn spec(key: String, id: String, s: Shape) -> ControlSpec {
    ControlSpec {
        key,
        id,
        min: s.min,
        max: s.max,
        default: s.default,
        detent: s.detent,
        kind: s.kind,
        label: s.label,
        law: s.law,
        ticks: s.ticks,
        snap: s.snap,
        writes: Vec::new(),
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
            ParamId::Mixer(MixerParam::Master) => {
                Some(tapered(knob(&self.master.fader), FaderLaw::Master))
            }
            ParamId::Mixer(MixerParam::Crossfader) => Some(ranged(&self.crossfader.slider, "")),
            _ => None,
        }
    }
}

// GPUI's knob drag pulls trim, filter and EQ to their detents (`drag.rs`, `knob_of`), and tapers
// the channel and master faders (`drag.rs`, `fader_law`); every other control is plain.
fn channel_shape(strip: &ChannelStrip, p: &ChannelParam) -> Shape {
    match p {
        ChannelParam::Trim => snapping(&strip.trim),
        ChannelParam::Filter => snapping(&strip.filter),
        ChannelParam::Eq(band) => snapping(&strip.eq[band_index(*band)]),
        _ => tapered(ranged(&strip.fader, ""), FaderLaw::Channel),
    }
}

/// `shape` with `law`'s taper and the dB marks GPUI draws for it.
fn tapered(shape: Shape, law: FaderLaw) -> Shape {
    Shape {
        law: sampled_law(law),
        ticks: tick_specs(law),
        ..shape
    }
}

fn snapping(k: &Knob) -> Shape {
    Shape {
        snap: snap_spec(k),
        ..knob(k)
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
        ..plain()
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
        ..plain()
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
        ..plain()
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
        ..plain()
    }
}

// The law, marks and pull of a control GPUI draws without any.
fn plain() -> Shape {
    Shape {
        min: 0.0,
        max: 1.0,
        default: 0.0,
        detent: None,
        kind: ControlKind::Continuous,
        label: String::new(),
        law: TravelLaw::Linear,
        ticks: Vec::new(),
        snap: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slice_control_ids;
    use om_core::{ParameterRouter, ParameterTarget};
    use om_engine::DjEngine;
    use om_ui::binding::DeckBinding;
    use om_ui::widgets::transport::TransportPanel;
    use std::sync::{Arc, Mutex};

    fn mixer() -> MixerBinding {
        let engine = Arc::new(DjEngine::new(2).0);
        MixerBinding::typed(engine, Arc::new(ParameterRouter::new()))
    }

    fn controls() -> Vec<ControlSpec> {
        slice_controls(&mixer(), 2)
    }

    fn find(key: &str) -> ControlSpec {
        let all = controls();
        all.into_iter().find(|c| c.key == key).unwrap()
    }

    #[test]
    fn every_slice_key_has_a_shape_and_the_same_id() {
        let specs = controls();
        let ids = slice_control_ids(2, 2);
        // The composite keys (one per deck) follow the single ones.
        assert_eq!(specs.len(), ids.len() + 2);
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
    fn the_tapered_faders_carry_the_gpui_law_and_marks() {
        let fader = find("ch0.fader");
        assert_eq!(fader.law, sampled_law(FaderLaw::Channel));
        assert_eq!(fader.ticks, tick_specs(FaderLaw::Channel));
        assert_eq!(fader.snap, None);
        assert_eq!(fader.ticks[0].label, "0");
        let master = find("mixer.master");
        assert_eq!(master.law, sampled_law(FaderLaw::Master));
        assert_eq!(master.ticks, tick_specs(FaderLaw::Master));
        // GPUI drags the master as a fader: its unity detent marks the scale but never pulls.
        assert_eq!(master.snap, None);
        let TravelLaw::Knots(knots) = &master.law else {
            panic!("the master is tapered")
        };
        assert_eq!(knots[75], (0.75, 1.0), "unity at three quarters");
    }

    #[test]
    fn the_knobs_carry_the_gpui_detent_pull() {
        let strip = ChannelStrip::new(mixer(), 0, MeterParams::default());
        for (key, k) in [
            ("ch0.eq.low", &strip.eq[0]),
            ("ch0.filter", &strip.filter),
            ("ch0.trim", &strip.trim),
        ] {
            let spec = find(key);
            assert_eq!(spec.snap, snap_spec(k), "{key}");
            assert_eq!(
                (spec.law.clone(), spec.ticks.len()),
                (TravelLaw::Linear, 0),
                "{key}"
            );
        }
        assert_eq!(
            find("ch0.trim").snap,
            Some(SnapSpec {
                at: 1.0,
                width: 0.12
            })
        );
        assert_eq!(
            find("ch0.filter").snap,
            Some(SnapSpec {
                at: 0.0,
                width: 0.06
            })
        );
        // 3% of the EQ's -48..+6 dB, either side of 0 dB.
        let low = find("ch0.eq.low").snap.unwrap();
        assert!(low.at == 0.0 && (low.width - 1.62).abs() < 1e-12);
    }

    #[test]
    fn tempo_and_crossfader_are_plain() {
        for key in ["deck0.tempo", "mixer.crossfader"] {
            let spec = find(key);
            assert_eq!(spec.law, TravelLaw::Linear, "{key}");
            assert!(spec.ticks.is_empty() && spec.snap.is_none(), "{key}");
        }
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

    /// Records the string writes of a GPUI widget model.
    struct Strings(Arc<Mutex<Vec<(String, f64)>>>);

    impl ParameterTarget for Strings {
        fn set_parameter(&self, id: &str, value: f64) -> bool {
            self.0.lock().unwrap().push((id.to_owned(), value));
            true
        }
    }

    /// What GPUI's Cue button sends for `deck`, as ids and values, in order.
    fn gpui_cue(deck: usize) -> Vec<(String, f64)> {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let engine = Arc::new(DjEngine::new(2).0);
        let binding = DeckBinding::new(engine, Arc::new(Strings(Arc::clone(&seen))), deck);
        TransportPanel::new(binding).press(TransportButton::Cue);
        let writes = seen.lock().unwrap().clone();
        writes
    }

    #[test]
    fn cue_writes_what_the_gpui_cue_button_sends_in_its_order() {
        for deck in 0..2 {
            let cue = find(&format!("deck{deck}.cue"));
            assert_eq!(cue.kind, ControlKind::Trigger);
            assert_eq!((cue.label.as_str(), cue.id.as_str()), ("Cue", ""));
            let sent: Vec<(String, f64)> =
                cue.writes.iter().map(|(k, v)| (find(k).id, *v)).collect();
            assert_eq!(sent, gpui_cue(deck), "deck {deck}");
        }
    }

    #[test]
    fn only_composite_controls_carry_writes() {
        let all = controls();
        let composite: Vec<&str> = all
            .iter()
            .filter(|c| !c.writes.is_empty())
            .map(|c| c.key.as_str())
            .collect();
        assert_eq!(composite, ["deck0.cue", "deck1.cue"]);
    }
}

//! A control's travel law, scale marks and detent pull as plain data, built from the same
//! `om-ui` definitions the GPUI window applies, so the web front end interpolates and never
//! types a curve by hand.

use om_ui::control::Knob;
use om_ui::fader_law::{marks, FaderLaw};

/// Intervals the tapered laws are sampled at: every 1% of travel. GPUI's laws are linear in dB
/// between their knots, so the front end's piecewise-linear gain is within 0.01 dB of them.
pub const LAW_SAMPLES: u32 = 100;

/// How a control's travel `0..=1` maps to its value in engine units.
#[derive(Debug, Clone, PartialEq)]
pub enum TravelLaw {
    /// Proportional to travel (a knob's detent, when it has one, still takes the middle).
    Linear,
    /// Piecewise linear through `(position, value)` points, positions rising from 0 to 1.
    Knots(Vec<(f64, f64)>),
}

/// One scale mark beside a fader: where it sits, the text GPUI prints there (empty for an
/// unlabelled mark), and whether it is a long mark and the 0 dB mark.
#[derive(Debug, Clone, PartialEq)]
pub struct TickSpec {
    pub pos: f64,
    pub label: String,
    pub major: bool,
    pub unity: bool,
}

/// A dragged knob's detent pull: a value within `width` of `at` (inclusive) becomes `at`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnapSpec {
    pub at: f64,
    pub width: f64,
}

/// `law` sampled at every [`LAW_SAMPLES`]th of travel, its own knots included.
pub fn sampled_law(law: FaderLaw) -> TravelLaw {
    let knot = |i: u32| {
        let pos = i as f32 / LAW_SAMPLES as f32;
        (
            f64::from(i) / f64::from(LAW_SAMPLES),
            f64::from(law.gain_at(pos)),
        )
    };
    TravelLaw::Knots((0..=LAW_SAMPLES).map(knot).collect())
}

/// Every dB mark GPUI may draw for `law`, loudest first; the label only on the marks GPUI labels.
/// GPUI also drops marks closer than 12 px on a short fader; the front end has the travel to do so.
pub fn tick_specs(law: FaderLaw) -> Vec<TickSpec> {
    let spec = |t: om_ui::fader_law::Tick| TickSpec {
        pos: f64::from(t.pos),
        label: if t.major {
            t.label.to_owned()
        } else {
            String::new()
        },
        major: t.major,
        unity: t.unity,
    };
    marks(law).into_iter().map(spec).collect()
}

/// The pull `om_ui::control::snap_with_bypass` applies to `knob` without Shift, if it has a detent.
pub fn snap_spec(knob: &Knob) -> Option<SnapSpec> {
    let width = knob.detent_width();
    knob.detent.map(|at| SnapSpec { at, width })
}

#[cfg(test)]
mod tests {
    use super::*;
    use om_ui::control::{snap_with_bypass, Readout};

    fn knots(law: FaderLaw) -> Vec<(f64, f64)> {
        match sampled_law(law) {
            TravelLaw::Knots(k) => k,
            TravelLaw::Linear => panic!("a tapered law samples to knots"),
        }
    }

    /// The front end's interpolation (`range.ts`), for checking the sampling error here.
    fn interpolate(knots: &[(f64, f64)], pos: f64) -> f64 {
        let i = knots
            .partition_point(|k| k.0 < pos)
            .clamp(1, knots.len() - 1);
        let ((p0, v0), (p1, v1)) = (knots[i - 1], knots[i]);
        v0 + (v1 - v0) * (pos - p0) / (p1 - p0)
    }

    #[test]
    fn sampled_laws_rise_from_silence_and_include_their_knees() {
        for law in [FaderLaw::Channel, FaderLaw::Master] {
            let k = knots(law);
            assert_eq!(k.len(), 101);
            assert_eq!(k[0], (0.0, 0.0));
            assert!(k.windows(2).all(|w| w[1].0 > w[0].0 && w[1].1 >= w[0].1));
            assert_eq!(k[100].1, f64::from(law.gain_at(1.0)));
        }
        assert_eq!(
            knots(FaderLaw::Channel)[20].1,
            f64::from(FaderLaw::Channel.gain_at(0.2))
        );
        assert_eq!(
            knots(FaderLaw::Master)[75].1,
            f64::from(FaderLaw::Master.gain_at(0.75))
        );
    }

    #[test]
    fn interpolating_the_samples_stays_within_a_hundredth_of_a_db() {
        for law in [FaderLaw::Channel, FaderLaw::Master] {
            let k = knots(law);
            for i in 1..=2000 {
                let pos = f64::from(i) / 2000.0;
                let want = f64::from(law.gain_at(pos as f32));
                let error_db = 20.0 * (interpolate(&k, pos) / want).log10();
                assert!(error_db.abs() < 0.01, "{law:?} at {pos}: {error_db} dB");
            }
        }
    }

    #[test]
    fn ticks_are_the_om_ui_marks_labelled_where_gpui_labels_them() {
        let t = tick_specs(FaderLaw::Channel);
        let labels: Vec<&str> = t.iter().map(|k| k.label.as_str()).collect();
        assert_eq!(labels, ["0", "", "-12", "-24", "-48"]);
        assert!(t[0].unity && t[0].major && !t[1].major);
        let marks = marks(FaderLaw::Master);
        let master = tick_specs(FaderLaw::Master);
        assert_eq!(master.len(), marks.len());
        assert!(master
            .iter()
            .zip(&marks)
            .all(|(s, m)| s.pos == f64::from(m.pos)));
    }

    #[test]
    fn the_snap_is_the_knob_detent_pull() {
        let eq = Knob::new("HI", Readout::Decibels, -48.0, 6.0, 0.0).with_detent();
        let snap = snap_spec(&eq).unwrap();
        assert_eq!((snap.at, snap.width), (0.0, eq.detent_width()));
        // Inside the width (edge included) the rule lands on the detent; outside it does not.
        assert_eq!(snap_with_bypass(&eq, snap.at + snap.width, false), snap.at);
        assert_ne!(
            snap_with_bypass(&eq, snap.at + snap.width * 1.01, false),
            snap.at
        );
        assert_eq!(
            snap_spec(&Knob::new("X", Readout::Whole, 0.0, 1.0, 0.5)),
            None
        );
    }
}

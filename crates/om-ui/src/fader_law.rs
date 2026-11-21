//! Fader taper and dB scale marks, toolkit-free so both windows share one definition.
//!
//! The stored parameter stays a LINEAR gain; a [`FaderLaw`] maps it to where the cap sits on screen
//! and back. Nothing here changes what a stored value sounds like. Conventions only (ADR 0001).
//! The GPUI painters (`om-ui-gpui/src/fader_paint.rs`) and the web window's control specs
//! (`om-ui-bridge`) both read it.

/// Ticks closer than this (px along the travel) are dropped, loudest kept. The unity tick is
/// always kept. Same idea as `om_ui::meter::MIN_MARK_SPACING`, with the plan's 12 px.
pub const MIN_TICK_SPACING: f32 = 12.0;

/// Top of a master fader in dB: the stored maximum gain 2.0 (`MAX_GAIN`), exactly +6.02 dB.
const MASTER_TOP_DB: f32 = 6.020_6;
/// Law knots `(position, dB)`, bottom first. Below the first knot the gain ramps linearly from
/// silence; between knots the law is linear in dB. Channel: plan section 2 (`ChannelLaw`).
const CHANNEL_KNOTS: [(f32, f32); 2] = [(0.2, -48.0), (1.0, 0.0)];
/// Master: plan section 2 (`MasterLaw`): +6 at the top, 0 dB at 0.75, -30 dB at 0.25.
const MASTER_KNOTS: [(f32, f32); 3] = [(0.25, -30.0), (0.75, 0.0), (1.0, MASTER_TOP_DB)];

/// Linear gain of a dB value.
pub fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// dB of a linear gain; `-inf` at or below silence.
pub fn gain_to_db(gain: f32) -> f32 {
    if gain <= 0.0 {
        f32::NEG_INFINITY
    } else {
        20.0 * gain.log10()
    }
}

fn gain_from_knots(knots: &[(f32, f32)], pos: f32) -> f32 {
    let pos = pos.clamp(0.0, 1.0);
    let (p0, d0) = knots[0];
    if pos <= p0 {
        return db_to_gain(d0) * pos / p0;
    }
    for pair in knots.windows(2) {
        let ((pa, da), (pb, db)) = (pair[0], pair[1]);
        if pos <= pb {
            return db_to_gain(da + (db - da) * (pos - pa) / (pb - pa));
        }
    }
    db_to_gain(knots[knots.len() - 1].1)
}

fn pos_from_knots(knots: &[(f32, f32)], gain: f32) -> f32 {
    if gain <= 0.0 {
        return 0.0;
    }
    let (p0, d0) = knots[0];
    let g0 = db_to_gain(d0);
    if gain <= g0 {
        return p0 * gain / g0;
    }
    let db = gain_to_db(gain);
    for pair in knots.windows(2) {
        let ((pa, da), (pb, db_b)) = (pair[0], pair[1]);
        if db <= db_b {
            return pa + (pb - pa) * (db - da) / (db_b - da);
        }
    }
    1.0
}

/// How a fader's travel maps to the stored gain. The cap position `pos` is `0..=1` of travel from
/// the bottom; the gain is in stored (linear) units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FaderLaw {
    /// Today's behaviour: gain is proportional to travel, up to `max_gain` (1.0 channel, 2.0
    /// master). The Phase 2 default; the ticks sit at the true dB of the linear travel.
    Linear { max_gain: f32 },
    /// Channel fader (stored 0..1): dB-linear from 0 dB at the top to -48 dB at 0.2, then a
    /// linear gain ramp to silence.
    Channel,
    /// Master fader (stored 0..2): +6 dB at the top, 0 dB at 0.75, -30 dB at 0.25, ramp below.
    Master,
}

impl FaderLaw {
    /// The largest stored gain (the top of travel).
    pub fn max_gain(self) -> f32 {
        match self {
            FaderLaw::Linear { max_gain } => max_gain,
            FaderLaw::Channel => 1.0,
            FaderLaw::Master => 2.0,
        }
    }

    /// Stored gain at cap position `pos`.
    pub fn gain_at(self, pos: f32) -> f32 {
        match self {
            FaderLaw::Linear { max_gain } => pos.clamp(0.0, 1.0) * max_gain,
            FaderLaw::Channel => gain_from_knots(&CHANNEL_KNOTS, pos),
            FaderLaw::Master => gain_from_knots(&MASTER_KNOTS, pos).min(2.0),
        }
    }

    /// Cap position for a stored gain; the inverse of [`FaderLaw::gain_at`], clamped to `0..=1`.
    pub fn pos_of(self, gain: f32) -> f32 {
        match self {
            FaderLaw::Linear { max_gain } if max_gain > 0.0 => (gain / max_gain).clamp(0.0, 1.0),
            FaderLaw::Linear { .. } => 0.0,
            FaderLaw::Channel => pos_from_knots(&CHANNEL_KNOTS, gain).clamp(0.0, 1.0),
            FaderLaw::Master => pos_from_knots(&MASTER_KNOTS, gain).clamp(0.0, 1.0),
        }
    }

    /// dB at cap position `pos`; `-inf` at silence.
    pub fn db_at(self, pos: f32) -> f32 {
        gain_to_db(self.gain_at(pos))
    }

    /// Cap position of a dB value (`-inf` gives 0).
    pub fn pos_at_db(self, db: f32) -> f32 {
        if db == f32::NEG_INFINITY {
            0.0
        } else {
            self.pos_of(db_to_gain(db))
        }
    }
}

/// One scale mark beside a fader.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tick {
    /// Cap position the mark stands for, `0..=1` from the bottom.
    pub pos: f32,
    pub db: f32,
    /// Long mark, and the only kind that carries a label.
    pub major: bool,
    /// The 0 dB mark: always kept, drawn brightest.
    pub unity: bool,
    pub label: &'static str,
}

/// Channel marks (plan Task 1.2, R section 3 numbers; the dB choice is a proposal).
const CHANNEL_TICKS: [(f32, &str); 5] = [
    (0.0, "0"),
    (-6.0, "-6"),
    (-12.0, "-12"),
    (-24.0, "-24"),
    (-48.0, "-48"),
];
/// Master marks (same provenance). Unsourced: which marks are major.
const MASTER_TICKS: [(f32, &str); 6] = [
    (6.0, "+6"),
    (0.0, "0"),
    (-6.0, "-6"),
    (-12.0, "-12"),
    (-24.0, "-24"),
    (-48.0, "-48"),
];

/// Every mark for `law`, loudest first, before any are dropped for spacing. Major marks (0 dB,
/// the top mark and multiples of 12 dB) carry the labels.
pub fn marks(law: FaderLaw) -> Vec<Tick> {
    let table: &[(f32, &'static str)] = match law {
        FaderLaw::Master => &MASTER_TICKS,
        FaderLaw::Channel => &CHANNEL_TICKS,
        FaderLaw::Linear { max_gain } if max_gain > 1.0 => &MASTER_TICKS,
        FaderLaw::Linear { .. } => &CHANNEL_TICKS,
    };
    let mut all: Vec<Tick> = Vec::with_capacity(table.len());
    for (i, &(db, label)) in table.iter().enumerate() {
        if db_to_gain(db) > law.max_gain() * 1.001 {
            continue;
        }
        all.push(Tick {
            pos: law.pos_at_db(db),
            db,
            major: db == 0.0 || db % 12.0 == 0.0 || i == 0,
            unity: db == 0.0,
            label,
        });
    }
    all
}

/// The marks for `law` on a fader whose cap travels `travel_px`, loudest first. A mark closer than
/// [`MIN_TICK_SPACING`] to one already kept is dropped; the unity mark is never dropped.
pub fn ticks(law: FaderLaw, travel_px: f32) -> Vec<Tick> {
    let all = marks(law);
    let mut kept: Vec<Tick> = all.iter().copied().filter(|t| t.unity).collect();
    for tick in all.iter().copied().filter(|t| !t.unity) {
        let clear = kept
            .iter()
            .all(|k| (k.pos - tick.pos).abs() * travel_px >= MIN_TICK_SPACING - 0.01);
        if clear {
            kept.push(tick);
        }
    }
    kept.sort_by(|a, b| b.pos.total_cmp(&a.pos));
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() <= tol
    }

    const LAWS: [FaderLaw; 4] = [
        FaderLaw::Channel,
        FaderLaw::Master,
        FaderLaw::Linear { max_gain: 1.0 },
        FaderLaw::Linear { max_gain: 2.0 },
    ];

    #[test]
    fn channel_law_pos_one_is_unity_and_pos_zero_is_silent() {
        let l = FaderLaw::Channel;
        assert!(approx(l.gain_at(1.0), 1.0, 1e-6));
        assert_eq!(l.gain_at(0.0), 0.0);
        assert!(approx(l.db_at(0.2), -48.0, 1e-3), "the knee is -48 dB");
        assert_eq!(l.db_at(0.0), f32::NEG_INFINITY);
    }

    #[test]
    fn master_law_unity_at_three_quarters_and_plus_six_at_top() {
        let l = FaderLaw::Master;
        assert!(approx(l.gain_at(0.75), 1.0, 1e-5));
        assert!(approx(l.gain_at(1.0), 2.0, 1e-4), "the stored maximum");
        assert!(approx(l.db_at(0.25), -30.0, 1e-3));
        assert_eq!(l.gain_at(0.0), 0.0);
    }

    #[test]
    fn law_round_trips() {
        for law in LAWS {
            for i in 0..=200 {
                let pos = i as f32 / 200.0;
                let back = law.pos_of(law.gain_at(pos));
                assert!(
                    approx(back, pos, 1e-4),
                    "{law:?} pos {pos} came back {back}"
                );
            }
        }
    }

    #[test]
    fn law_is_monotonic_and_continuous_at_the_ramp_join() {
        for law in LAWS {
            let mut last = -1.0;
            for i in 0..=1000 {
                let g = law.gain_at(i as f32 / 1000.0);
                assert!(g >= last, "{law:?} not monotonic at {i}");
                last = g;
            }
        }
        // Either side of each ramp join the gain differs by a hair, not a jump.
        for (law, join) in [(FaderLaw::Channel, 0.2), (FaderLaw::Master, 0.25)] {
            let below = law.gain_at(join - 1e-4);
            let above = law.gain_at(join + 1e-4);
            assert!(above - below < 1e-3, "{law:?}: {below} -> {above}");
        }
    }

    #[test]
    fn linear_law_is_the_identity() {
        let l = FaderLaw::Linear { max_gain: 1.0 };
        for i in 0..=20 {
            let p = i as f32 / 20.0;
            assert!(approx(l.gain_at(p), p, 1e-6));
            assert!(approx(l.pos_of(p), p, 1e-6));
        }
        assert!(approx(
            FaderLaw::Linear { max_gain: 2.0 }.gain_at(0.5),
            1.0,
            1e-6
        ));
    }

    #[test]
    fn gain_to_db_floor() {
        assert_eq!(gain_to_db(0.0), f32::NEG_INFINITY);
        assert_eq!(FaderLaw::Channel.pos_at_db(f32::NEG_INFINITY), 0.0);
    }

    #[test]
    fn channel_ticks_are_at_the_planned_dbs() {
        let t = ticks(FaderLaw::Channel, 1000.0);
        let dbs: Vec<f32> = t.iter().map(|k| k.db).collect();
        assert_eq!(dbs, vec![0.0, -6.0, -12.0, -24.0, -48.0]);
        assert!(approx(t[0].pos, 1.0, 1e-4) && t[0].unity);
        assert!(approx(t[4].pos, 0.2, 1e-3));
    }

    #[test]
    fn master_ticks_are_at_the_planned_dbs() {
        let t = ticks(FaderLaw::Master, 1000.0);
        let dbs: Vec<f32> = t.iter().map(|k| k.db).collect();
        assert_eq!(dbs, vec![6.0, 0.0, -6.0, -12.0, -24.0, -48.0]);
        assert!(approx(t[1].pos, 0.75, 1e-4) && t[1].unity);
    }

    #[test]
    fn ticks_are_dropped_when_closer_than_12px() {
        let tight = ticks(FaderLaw::Channel, 60.0);
        for pair in tight.windows(2) {
            assert!((pair[0].pos - pair[1].pos) * 60.0 >= MIN_TICK_SPACING - 1e-3);
        }
        assert!(tight.len() < 5);
        assert!(tight.iter().any(|k| k.unity), "unity is always kept");
        // The linear travel bunches the quiet marks together at the bottom.
        let linear = ticks(FaderLaw::Linear { max_gain: 1.0 }, 140.0);
        assert!(linear.len() < 5 && linear.iter().any(|k| k.unity));
        // Unity survives even where a neighbour would crowd it out.
        assert!(ticks(FaderLaw::Master, 30.0).iter().any(|k| k.unity));
    }

    #[test]
    fn labels_only_on_major_ticks() {
        let t = ticks(FaderLaw::Master, 1000.0);
        let majors: Vec<&str> = t.iter().filter(|k| k.major).map(|k| k.label).collect();
        assert_eq!(majors, vec!["+6", "0", "-12", "-24", "-48"]);
    }

    #[test]
    fn marks_are_the_ticks_on_a_long_travel() {
        for law in LAWS {
            assert_eq!(marks(law), ticks(law, 1000.0), "{law:?}");
        }
    }

    // Generated from `om-ui-gpui/src/fader_paint.rs` before the law moved here, so the move is
    // checked to the bit: positions outside 0..=1, the ramp, every knot and the top.
    const GAIN_PINS: [(FaderLaw, f32, f32); 44] = [
        (FaderLaw::Channel, -0.1, 0.0),
        (FaderLaw::Channel, 0.0, 0.0),
        (FaderLaw::Channel, 0.1, 0.0019905355),
        (FaderLaw::Channel, 0.2, 0.003981071),
        (FaderLaw::Channel, 0.25, 0.0056234132),
        (FaderLaw::Channel, 0.5, 0.031622775),
        (FaderLaw::Channel, 0.6, 0.063095726),
        (FaderLaw::Channel, 0.75, 0.17782794),
        (FaderLaw::Channel, 0.9, 0.501187),
        (FaderLaw::Channel, 1.0, 1.0),
        (FaderLaw::Channel, 1.2, 1.0),
        (FaderLaw::Master, -0.1, 0.0),
        (FaderLaw::Master, 0.0, 0.0),
        (FaderLaw::Master, 0.1, 0.0126491105),
        (FaderLaw::Master, 0.2, 0.025298221),
        (FaderLaw::Master, 0.25, 0.031622775),
        (FaderLaw::Master, 0.5, 0.17782794),
        (FaderLaw::Master, 0.6, 0.3548135),
        (FaderLaw::Master, 0.75, 1.0),
        (FaderLaw::Master, 0.9, 1.5157164),
        (FaderLaw::Master, 1.0, 1.9999999),
        (FaderLaw::Master, 1.2, 1.9999999),
        (FaderLaw::Linear { max_gain: 1.0 }, -0.1, 0.0),
        (FaderLaw::Linear { max_gain: 1.0 }, 0.0, 0.0),
        (FaderLaw::Linear { max_gain: 1.0 }, 0.1, 0.1),
        (FaderLaw::Linear { max_gain: 1.0 }, 0.2, 0.2),
        (FaderLaw::Linear { max_gain: 1.0 }, 0.25, 0.25),
        (FaderLaw::Linear { max_gain: 1.0 }, 0.5, 0.5),
        (FaderLaw::Linear { max_gain: 1.0 }, 0.6, 0.6),
        (FaderLaw::Linear { max_gain: 1.0 }, 0.75, 0.75),
        (FaderLaw::Linear { max_gain: 1.0 }, 0.9, 0.9),
        (FaderLaw::Linear { max_gain: 1.0 }, 1.0, 1.0),
        (FaderLaw::Linear { max_gain: 1.0 }, 1.2, 1.0),
        (FaderLaw::Linear { max_gain: 2.0 }, -0.1, 0.0),
        (FaderLaw::Linear { max_gain: 2.0 }, 0.0, 0.0),
        (FaderLaw::Linear { max_gain: 2.0 }, 0.1, 0.2),
        (FaderLaw::Linear { max_gain: 2.0 }, 0.2, 0.4),
        (FaderLaw::Linear { max_gain: 2.0 }, 0.25, 0.5),
        (FaderLaw::Linear { max_gain: 2.0 }, 0.5, 1.0),
        (FaderLaw::Linear { max_gain: 2.0 }, 0.6, 1.2),
        (FaderLaw::Linear { max_gain: 2.0 }, 0.75, 1.5),
        (FaderLaw::Linear { max_gain: 2.0 }, 0.9, 1.8),
        (FaderLaw::Linear { max_gain: 2.0 }, 1.0, 2.0),
        (FaderLaw::Linear { max_gain: 2.0 }, 1.2, 2.0),
    ];

    const POS_PINS: [(FaderLaw, f32, f32); 36] = [
        (FaderLaw::Channel, 0.0, 0.0),
        (FaderLaw::Channel, 0.001, 0.050237738),
        (FaderLaw::Channel, 0.01, 0.33333334),
        (FaderLaw::Channel, 0.1, 0.6666667),
        (FaderLaw::Channel, 0.5, 0.8996567),
        (FaderLaw::Channel, 1.0, 1.0),
        (FaderLaw::Channel, 1.5, 1.0),
        (FaderLaw::Channel, 2.0, 1.0),
        (FaderLaw::Channel, 3.0, 1.0),
        (FaderLaw::Master, 0.0, 0.0),
        (FaderLaw::Master, 0.001, 0.007905695),
        (FaderLaw::Master, 0.01, 0.07905694),
        (FaderLaw::Master, 0.1, 0.4166667),
        (FaderLaw::Master, 0.5, 0.64965665),
        (FaderLaw::Master, 1.0, 0.75),
        (FaderLaw::Master, 1.5, 0.8962406),
        (FaderLaw::Master, 2.0, 1.0),
        (FaderLaw::Master, 3.0, 1.0),
        (FaderLaw::Linear { max_gain: 1.0 }, 0.0, 0.0),
        (FaderLaw::Linear { max_gain: 1.0 }, 0.001, 0.001),
        (FaderLaw::Linear { max_gain: 1.0 }, 0.01, 0.01),
        (FaderLaw::Linear { max_gain: 1.0 }, 0.1, 0.1),
        (FaderLaw::Linear { max_gain: 1.0 }, 0.5, 0.5),
        (FaderLaw::Linear { max_gain: 1.0 }, 1.0, 1.0),
        (FaderLaw::Linear { max_gain: 1.0 }, 1.5, 1.0),
        (FaderLaw::Linear { max_gain: 1.0 }, 2.0, 1.0),
        (FaderLaw::Linear { max_gain: 1.0 }, 3.0, 1.0),
        (FaderLaw::Linear { max_gain: 2.0 }, 0.0, 0.0),
        (FaderLaw::Linear { max_gain: 2.0 }, 0.001, 0.0005),
        (FaderLaw::Linear { max_gain: 2.0 }, 0.01, 0.005),
        (FaderLaw::Linear { max_gain: 2.0 }, 0.1, 0.05),
        (FaderLaw::Linear { max_gain: 2.0 }, 0.5, 0.25),
        (FaderLaw::Linear { max_gain: 2.0 }, 1.0, 0.5),
        (FaderLaw::Linear { max_gain: 2.0 }, 1.5, 0.75),
        (FaderLaw::Linear { max_gain: 2.0 }, 2.0, 1.0),
        (FaderLaw::Linear { max_gain: 2.0 }, 3.0, 1.0),
    ];

    const TICK_PINS: [(FaderLaw, f32, f32, bool, bool, &str); 22] = [
        (FaderLaw::Channel, 1.0, 0.0, true, true, "0"),
        (FaderLaw::Channel, 0.90000004, -6.0, false, false, "-6"),
        (FaderLaw::Channel, 0.8, -12.0, true, false, "-12"),
        (FaderLaw::Channel, 0.6, -24.0, true, false, "-24"),
        (FaderLaw::Channel, 0.2, -48.0, true, false, "-48"),
        (FaderLaw::Master, 0.9991446, 6.0, true, false, "+6"),
        (FaderLaw::Master, 0.75, 0.0, true, true, "0"),
        (FaderLaw::Master, 0.65, -6.0, false, false, "-6"),
        (FaderLaw::Master, 0.55, -12.0, true, false, "-12"),
        (FaderLaw::Master, 0.35, -24.0, true, false, "-24"),
        (FaderLaw::Master, 0.031473134, -48.0, true, false, "-48"),
        (
            FaderLaw::Linear { max_gain: 1.0 },
            1.0,
            0.0,
            true,
            true,
            "0",
        ),
        (
            FaderLaw::Linear { max_gain: 1.0 },
            0.5011872,
            -6.0,
            false,
            false,
            "-6",
        ),
        (
            FaderLaw::Linear { max_gain: 1.0 },
            0.25118864,
            -12.0,
            true,
            false,
            "-12",
        ),
        (
            FaderLaw::Linear { max_gain: 1.0 },
            0.063095726,
            -24.0,
            true,
            false,
            "-24",
        ),
        (
            FaderLaw::Linear { max_gain: 1.0 },
            0.003981071,
            -48.0,
            true,
            false,
            "-48",
        ),
        (
            FaderLaw::Linear { max_gain: 2.0 },
            0.9976312,
            6.0,
            true,
            false,
            "+6",
        ),
        (
            FaderLaw::Linear { max_gain: 2.0 },
            0.5,
            0.0,
            true,
            true,
            "0",
        ),
        (
            FaderLaw::Linear { max_gain: 2.0 },
            0.2505936,
            -6.0,
            false,
            false,
            "-6",
        ),
        (
            FaderLaw::Linear { max_gain: 2.0 },
            0.12559432,
            -12.0,
            true,
            false,
            "-12",
        ),
        (
            FaderLaw::Linear { max_gain: 2.0 },
            0.031547863,
            -24.0,
            true,
            false,
            "-24",
        ),
        (
            FaderLaw::Linear { max_gain: 2.0 },
            0.0019905355,
            -48.0,
            true,
            false,
            "-48",
        ),
    ];

    /// The same marks on a 100 px travel, after the 12 px spacing rule.
    const TICK_100_PX_PINS: [(FaderLaw, f32, f32, bool, bool, &str); 18] = [
        (FaderLaw::Channel, 1.0, 0.0, true, true, "0"),
        (FaderLaw::Channel, 0.8, -12.0, true, false, "-12"),
        (FaderLaw::Channel, 0.6, -24.0, true, false, "-24"),
        (FaderLaw::Channel, 0.2, -48.0, true, false, "-48"),
        (FaderLaw::Master, 0.9991446, 6.0, true, false, "+6"),
        (FaderLaw::Master, 0.75, 0.0, true, true, "0"),
        (FaderLaw::Master, 0.55, -12.0, true, false, "-12"),
        (FaderLaw::Master, 0.35, -24.0, true, false, "-24"),
        (FaderLaw::Master, 0.031473134, -48.0, true, false, "-48"),
        (
            FaderLaw::Linear { max_gain: 1.0 },
            1.0,
            0.0,
            true,
            true,
            "0",
        ),
        (
            FaderLaw::Linear { max_gain: 1.0 },
            0.5011872,
            -6.0,
            false,
            false,
            "-6",
        ),
        (
            FaderLaw::Linear { max_gain: 1.0 },
            0.25118864,
            -12.0,
            true,
            false,
            "-12",
        ),
        (
            FaderLaw::Linear { max_gain: 1.0 },
            0.063095726,
            -24.0,
            true,
            false,
            "-24",
        ),
        (
            FaderLaw::Linear { max_gain: 2.0 },
            0.9976312,
            6.0,
            true,
            false,
            "+6",
        ),
        (
            FaderLaw::Linear { max_gain: 2.0 },
            0.5,
            0.0,
            true,
            true,
            "0",
        ),
        (
            FaderLaw::Linear { max_gain: 2.0 },
            0.2505936,
            -6.0,
            false,
            false,
            "-6",
        ),
        (
            FaderLaw::Linear { max_gain: 2.0 },
            0.12559432,
            -12.0,
            true,
            false,
            "-12",
        ),
        (
            FaderLaw::Linear { max_gain: 2.0 },
            0.0019905355,
            -48.0,
            true,
            false,
            "-48",
        ),
    ];

    #[test]
    fn gain_at_is_pinned_to_the_bit() {
        for (law, pos, gain) in GAIN_PINS {
            assert_eq!(law.gain_at(pos).to_bits(), gain.to_bits(), "{law:?} {pos}");
        }
    }

    #[test]
    fn pos_of_is_pinned_to_the_bit() {
        for (law, gain, pos) in POS_PINS {
            assert_eq!(law.pos_of(gain).to_bits(), pos.to_bits(), "{law:?} {gain}");
        }
    }

    #[test]
    fn ticks_are_pinned_to_the_bit() {
        for (travel, pins) in [(1000.0, &TICK_PINS[..]), (100.0, &TICK_100_PX_PINS[..])] {
            for law in LAWS {
                let want: Vec<_> = pins.iter().filter(|p| p.0 == law).collect();
                let got = ticks(law, travel);
                assert_eq!(got.len(), want.len(), "{law:?} at {travel} px");
                for (t, &&(_, pos, db, major, unity, label)) in got.iter().zip(&want) {
                    assert_eq!(t.pos.to_bits(), pos.to_bits(), "{law:?} {label}");
                    assert_eq!((t.db, t.major, t.unity, t.label), (db, major, unity, label));
                }
            }
        }
    }
}

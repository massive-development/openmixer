use super::*;
use om_core::{AutomixSnapshot, MixerSnapshot, SamplerSnapshot, StemSnapshot};
use proptest::prelude::*;
use std::sync::Mutex;

/// Records every command sent to it.
#[derive(Default)]
struct Recorder {
    sent: Mutex<Vec<Command>>,
}

impl Recorder {
    fn commands(&self) -> Vec<Command> {
        self.sent.lock().unwrap().clone()
    }

    fn values_for(&self, id: &ParamId) -> Vec<f64> {
        self.commands()
            .into_iter()
            .filter_map(|c| match c {
                Command::Param { id: i, value } if i == *id => Some(value),
                _ => None,
            })
            .collect()
    }
}

impl CommandSink for Recorder {
    fn send(&self, cmd: Command) -> Result<(), CommandError> {
        self.sent.lock().unwrap().push(cmd);
        Ok(())
    }
}

/// A snapshot with `decks` decks; each deck in `with_stems` has all four stems available.
fn engine(decks: usize, with_stems: &[usize]) -> EngineSnapshot {
    let stems = (0..decks)
        .map(|d| {
            let on = with_stems.contains(&d);
            let s = StemSnapshot {
                available: on,
                ..Default::default()
            };
            [s; 4]
        })
        .collect();
    EngineSnapshot {
        ticks: 0,
        built: 1,
        decks: Vec::new(),
        mixer: MixerSnapshot::default(),
        stems,
        fx: Default::default(),
        sampler: SamplerSnapshot::default(),
        automix: AutomixSnapshot::default(),
        record: Default::default(),
    }
}

fn bass_id(deck: u8) -> ParamId {
    ParamId::Stem {
        deck,
        stem: BASS,
        p: StemParam::Volume,
    }
}

fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn fade(from: f64, to: f64, start: f64, len: f64, curve: Curve) -> StemFade {
    StemFade {
        stem: BASS,
        deck: 0,
        param: StemParam::Volume,
        from,
        to,
        start_beat: start,
        length_beats: len,
        curve,
    }
}

#[test]
fn linear_values_at_quarters() {
    let f = fade(0.0, 1.0, 10.0, 8.0, Curve::Linear);
    let got: Vec<f64> = [0.0, 0.25, 0.5, 0.75, 1.0]
        .iter()
        .map(|t| f.value_at(10.0 + 8.0 * t))
        .collect();
    assert_eq!(got, vec![0.0, 0.25, 0.5, 0.75, 1.0]);
}

#[test]
fn equal_power_values_at_quarters() {
    // Length 4 beats: beats 0, 1, 2, 3, 4 are 0, 25, 50, 75, 100 %.
    let f = fade(1.0, 0.0, 0.0, 4.0, Curve::EqualPower);
    let want = [1.0, 0.75_f64.sqrt(), 0.5_f64.sqrt(), 0.25_f64.sqrt(), 0.0];
    for (i, w) in want.iter().enumerate() {
        assert!(approx(f.value_at(i as f64), *w), "{i}");
    }
}

#[test]
fn exponential_values_at_quarters() {
    let f = fade(0.25, 1.0, 0.0, 4.0, Curve::Exponential);
    let want = [
        0.25,
        0.25 * 2.0_f64.sqrt(),
        0.5,
        0.25 * 2.0_f64.powf(1.5),
        1.0,
    ];
    for (i, w) in want.iter().enumerate() {
        assert!(approx(f.value_at(i as f64), *w), "{i}");
    }
}

#[test]
fn exponential_falls_back_to_quadratic_at_zero() {
    let f = fade(1.0, 0.0, 0.0, 4.0, Curve::Exponential);
    assert!(approx(f.value_at(2.0), 0.75)); // 1 + (0 - 1) * 0.5^2
}

#[test]
fn equal_power_sums_to_one_in_power() {
    let out = fade(1.0, 0.0, 0.0, 16.0, Curve::EqualPower);
    let inc = fade(0.0, 1.0, 0.0, 16.0, Curve::EqualPower);
    for step in 0..=100 {
        let beat = 16.0 * f64::from(step) / 100.0;
        let p = out.value_at(beat).powi(2) + inc.value_at(beat).powi(2);
        assert!(approx(p, 1.0), "step {step}: {p}");
    }
}

#[test]
fn bass_swap_crosses_at_midpoint() {
    let plan = bass_swap(1.0, 0.0, 0, 1, 8.0, 8.0);
    let fades = plan.fades();
    assert_eq!(fades.len(), 2);
    assert!(fades
        .iter()
        .all(|f| f.stem == BASS && f.param == StemParam::Volume));
    let (out, inc) = (fades[0], fades[1]);
    assert_eq!((out.deck, inc.deck), (0, 1));

    // Start: outgoing full, incoming silent. End: the reverse.
    assert_eq!(out.value_at(8.0), 1.0);
    assert_eq!(inc.value_at(8.0), 0.0);
    assert_eq!(out.value_at(16.0), 0.0);
    assert_eq!(inc.value_at(16.0), 1.0);

    // Midpoint: both at the same level, 1 / sqrt(2) each in equal power.
    let mid = 12.0;
    assert!(approx(out.value_at(mid), inc.value_at(mid)));
    assert!(approx(out.value_at(mid), 0.5_f64.sqrt()));
    // Before the midpoint outgoing is louder, after it incoming is.
    assert!(out.value_at(10.0) > inc.value_at(10.0));
    assert!(out.value_at(14.0) < inc.value_at(14.0));
}

#[test]
fn bass_swap_is_linear_in_value_when_asked() {
    // Linear crossing: the same midpoint property holds with a linear ramp.
    let out = fade(1.0, 0.0, 8.0, 8.0, Curve::Linear);
    let inc = fade(0.0, 1.0, 8.0, 8.0, Curve::Linear);
    assert!(approx(out.value_at(12.0), inc.value_at(12.0)));
    assert!(approx(out.value_at(12.0), 0.5));
}

#[test]
fn values_end_exactly_on_to() {
    // Lengths and starts that do not sum exactly in binary floating point.
    for (start, len) in [(1.3_f64, 3.7_f64), (0.1, 0.3), (100.7, 13.9)] {
        for curve in [Curve::Linear, Curve::EqualPower, Curve::Exponential] {
            let f = fade(0.2, 0.9, start, len, curve);
            assert_eq!(f.value_at(f.end_beat()), 0.9, "{curve:?} {start} {len}");
            assert_eq!(f.value_at(f.end_beat() + 50.0), 0.9);
            assert_eq!(f.value_at(start), 0.2);
            assert_eq!(f.value_at(start - 1.0), 0.2);
        }
    }
}

#[test]
fn steps_are_idempotent() {
    let plan = bass_swap(1.0, 0.0, 0, 1, 8.0, 8.0);
    let a = plan.steps_at(8.0..9.0);
    let b = plan.steps_at(8.0..9.0);
    assert_eq!(a, b);
    assert_eq!(a.len(), 2);
    // A range that starts after a fade has ended does not include it.
    assert!(plan.steps_at(20.0..21.0).is_empty());
}

#[test]
fn restore_values_are_first_from_per_id() {
    let plan = drums_only_bridge([false; 4], 0, 4.0, 8.0);
    let restore = plan.restore_values();
    assert_eq!(restore.len(), 3, "one entry per muted stem");
    assert!(restore.iter().all(|(_, v)| *v == 0.0));
    let ids: Vec<ParamId> = restore.iter().map(|(id, _)| id.clone()).collect();
    assert!(ids.contains(&ParamId::Stem {
        deck: 0,
        stem: BASS,
        p: StemParam::Mute
    }));
    assert!(ids.contains(&ParamId::Stem {
        deck: 0,
        stem: VOCALS,
        p: StemParam::Mute
    }));
    assert!(ids.contains(&ParamId::Stem {
        deck: 0,
        stem: OTHER,
        p: StemParam::Mute
    }));
}

#[test]
fn drums_only_bridge_mutes_for_the_window() {
    let plan = drums_only_bridge([false; 4], 0, 4.0, 8.0);
    assert_eq!(plan.fades().len(), 6);
    assert!(plan.fades().iter().all(|f| f.stem != DRUMS));
    let bass_mute = plan
        .fades()
        .iter()
        .filter(|f| f.stem == BASS)
        .collect::<Vec<_>>();
    let mute = bass_mute[0];
    assert_eq!(mute.value_at(3.999), 0.0);
    assert_eq!(mute.value_at(4.0), 1.0);
    assert_eq!(mute.value_at(11.999), 1.0);
    let unmute = bass_mute[1];
    assert_eq!(unmute.value_at(11.999), 1.0);
    assert_eq!(unmute.value_at(12.0), 0.0);
}

#[test]
fn builders_name_the_expected_params() {
    let v = vocals_out(1.0, 2, 0.0, 4.0);
    assert_eq!(v.fades().len(), 1);
    assert_eq!(
        (v.fades()[0].stem, v.fades()[0].param),
        (VOCALS, StemParam::Volume)
    );
    let f = filter_sweep(1, OTHER, 0.0, 4.0, 0.0, -1.0);
    assert_eq!(
        (f.fades()[0].stem, f.fades()[0].param),
        (OTHER, StemParam::Filter)
    );
    assert_eq!(f.fades()[0].value_at(4.0), -1.0);
    assert_eq!(f.end_beat(), Some(4.0));
    assert_eq!(StemFadePlan::default().end_beat(), None);
}

#[test]
fn beat_of_deck_reads_position() {
    let unloaded = DeckSnapshot::default();
    assert_eq!(beat_of_deck(&unloaded, |_| 0.0), None);
    let loaded = DeckSnapshot {
        loaded: true,
        position_frames: 48_000,
        ..Default::default()
    };
    assert_eq!(beat_of_deck(&loaded, |f| f as f64 / 24_000.0), Some(2.0));
}

#[test]
fn cancel_restore_sends_from_values_once() {
    let mut runner = StemFadeRunner::new(bass_swap(1.0, 0.0, 0, 1, 8.0, 8.0));
    let rec = Recorder::default();
    let eng = engine(2, &[0, 1]);
    runner.tick(0.0, &eng, &rec).unwrap();
    runner.tick(12.0, &eng, &rec).unwrap(); // both fades (8 to 16 beats) have started
    let before = rec.commands().len();

    let n = runner.cancel(CancelMode::Restore, &rec).unwrap();
    assert_eq!(n, 2, "one restore per started fade");
    let restored = rec.commands()[before..].to_vec();
    assert!(restored.contains(&Command::Param {
        id: bass_id(0),
        value: 1.0
    }));
    assert!(restored.contains(&Command::Param {
        id: bass_id(1),
        value: 0.0
    }));

    // After a cancel nothing else is sent, and a second cancel does nothing.
    let after = rec.commands().len();
    runner.tick(12.0, &eng, &rec).unwrap();
    assert_eq!(runner.cancel(CancelMode::Restore, &rec).unwrap(), 0);
    assert_eq!(rec.commands().len(), after);
    assert!(runner.is_cancelled());
}

#[test]
fn cancel_leave_sends_nothing() {
    let mut runner = StemFadeRunner::new(bass_swap(1.0, 0.0, 0, 1, 8.0, 8.0));
    let rec = Recorder::default();
    let eng = engine(2, &[0, 1]);
    runner.tick(0.0, &eng, &rec).unwrap();
    let before = rec.commands().len();
    assert_eq!(runner.cancel(CancelMode::Leave, &rec).unwrap(), 0);
    runner.tick(10.0, &eng, &rec).unwrap();
    assert_eq!(rec.commands().len(), before);
}

#[test]
fn simulated_30hz_ticks_land_on_the_end_values() {
    // 120 BPM is 2 beats per second; ticks run at 30 Hz for 12 s.
    let mut runner = StemFadeRunner::new(bass_swap(1.0, 0.0, 0, 1, 8.0, 8.0));
    let rec = Recorder::default();
    let eng = engine(2, &[0, 1]);
    let mut total = TickReport::default();
    for tick in 0..=360 {
        let beat = f64::from(tick) / 30.0 * 2.0;
        let r = runner.tick(beat, &eng, &rec).unwrap();
        total.sent += r.sent;
    }
    // Each fade overlaps about 8 s * 30 ticks = 240 ticks; the bound adds a little slack.
    assert!(total.sent <= 2 * 243, "sent {}", total.sent);
    let out = rec.values_for(&bass_id(0));
    let inc = rec.values_for(&bass_id(1));
    assert_eq!(out.last().copied(), Some(0.0), "outgoing lands on 0");
    assert_eq!(inc.last().copied(), Some(1.0), "incoming lands on 1");
    assert!(out.windows(2).all(|w| w[1] <= w[0]), "outgoing never rises");
    assert!(inc.windows(2).all(|w| w[1] >= w[0]), "incoming never falls");
    assert!(runner.is_finished(16.0));
    assert!(!runner.is_finished(15.0));
}

#[test]
fn skips_a_deck_without_stems_after_the_retry_window_and_reports_once() {
    let mut runner = StemFadeRunner::new(bass_swap(1.0, 0.0, 0, 1, 0.0, 8.0));
    let rec = Recorder::default();
    let eng = engine(2, &[0]); // deck 1 has no stems
                               // Each tick overlaps the window; deck 1 waits RETRY_TICKS ticks before it is given up.
    for tick in 0..RETRY_TICKS {
        let report = runner.tick(f64::from(tick) * 0.01, &eng, &rec).unwrap();
        assert!(report.skipped.is_empty(), "still waiting on tick {tick}");
    }
    let first = runner.tick(0.5, &eng, &rec).unwrap();
    assert_eq!(first.skipped, vec![(1, SkipReason::NoStems)]);
    assert_eq!(first.sent, 1);
    let second = runner.tick(1.0, &eng, &rec).unwrap();
    assert!(second.skipped.is_empty());
    assert!(rec.values_for(&bass_id(1)).is_empty());
    assert!(!rec.values_for(&bass_id(0)).is_empty());
    // A skipped fade does not hold the plan open: once the live fade ends, the plan is finished.
    assert!(runner.is_finished(8.0));
}

#[test]
fn reports_an_invalid_id_and_skips_it() {
    let plan = StemFadePlan::new(vec![StemFade {
        deck: 9,
        ..fade(0.0, 1.0, 0.0, 4.0, Curve::Linear)
    }]);
    let mut runner = StemFadeRunner::new(plan);
    let rec = Recorder::default();
    let r = runner.tick(0.0, &engine(2, &[0, 1]), &rec).unwrap();
    assert_eq!(r.skipped, vec![(0, SkipReason::InvalidId)]);
    assert_eq!(r.sent, 0);
    assert!(rec.commands().is_empty());
}

#[test]
fn backwards_jump_resends_at_the_new_position() {
    let mut runner = StemFadeRunner::new(bass_swap(1.0, 0.0, 0, 1, 8.0, 8.0));
    let rec = Recorder::default();
    let eng = engine(2, &[0, 1]);
    runner.tick(12.0, &eng, &rec).unwrap();
    let mid = rec.values_for(&bass_id(0))[0];
    runner.tick(9.0, &eng, &rec).unwrap();
    let back = rec.values_for(&bass_id(0));
    assert_eq!(back.len(), 2);
    assert!(back[1] > mid, "value follows the jump back");
}

#[test]
fn nan_beat_sends_nothing() {
    let mut runner = StemFadeRunner::new(bass_swap(1.0, 0.0, 0, 1, 0.0, 8.0));
    let rec = Recorder::default();
    let r = runner.tick(f64::NAN, &engine(2, &[0, 1]), &rec).unwrap();
    assert_eq!(r, TickReport::default());
    assert!(rec.commands().is_empty());
}

proptest! {
    #[test]
    fn value_is_bounded_and_monotonic(
        from in -1.0f64..1.5,
        to in -1.0f64..1.5,
        start in 0.0f64..64.0,
        len in 0.01f64..32.0,
        b1 in 0.0f64..100.0,
        b2 in 0.0f64..100.0,
        curve_idx in 0usize..3,
    ) {
        let curve = [Curve::Linear, Curve::EqualPower, Curve::Exponential][curve_idx];
        let f = fade(from, to, start, len, curve);
        let (lo, hi) = if b1 <= b2 { (b1, b2) } else { (b2, b1) };
        let (v_lo, v_hi) = (f.value_at(lo), f.value_at(hi));
        let (min, max) = (from.min(to), from.max(to));
        for v in [v_lo, v_hi] {
            prop_assert!(v >= min && v <= max, "{v} outside [{min}, {max}]");
        }
        if curve != Curve::EqualPower {
            if to >= from {
                prop_assert!(v_hi >= v_lo - 1e-12);
            } else {
                prop_assert!(v_hi <= v_lo + 1e-12);
            }
        }
    }

    #[test]
    fn equal_power_pair_sums_to_one(t in 0.0f64..=1.0, len in 0.5f64..32.0) {
        let out = fade(1.0, 0.0, 0.0, len, Curve::EqualPower);
        let inc = fade(0.0, 1.0, 0.0, len, Curve::EqualPower);
        let beat = t * len;
        let p = out.value_at(beat).powi(2) + inc.value_at(beat).powi(2);
        prop_assert!((p - 1.0).abs() < 1e-9);
    }
}

#[test]
fn a_deck_whose_stems_arrive_within_the_retry_window_gets_its_fade() {
    let mut runner = StemFadeRunner::new(bass_swap(1.0, 0.0, 0, 1, 0.0, 8.0));
    let rec = Recorder::default();
    for tick in 0..3 {
        let eng = engine(2, &[0]);
        assert!(runner
            .tick(f64::from(tick), &eng, &rec)
            .unwrap()
            .skipped
            .is_empty());
    }
    let eng = engine(2, &[0, 1]);
    let report = runner.tick(3.0, &eng, &rec).unwrap();
    assert!(report.skipped.is_empty());
    assert!(
        !rec.values_for(&bass_id(1)).is_empty(),
        "the late deck still fades"
    );
    assert!(!runner.is_finished(3.0));
}

#[test]
fn zero_retry_ticks_skips_on_the_first_tick() {
    let mut runner = StemFadeRunner::new(bass_swap(1.0, 0.0, 0, 1, 0.0, 8.0)).with_retry_ticks(0);
    let rec = Recorder::default();
    let report = runner.tick(0.0, &engine(2, &[0]), &rec).unwrap();
    assert_eq!(report.skipped, vec![(1, SkipReason::NoStems)]);
}

#[test]
fn a_bass_swap_starts_and_restores_from_the_snapshot_gains() {
    let mut eng = engine(2, &[0, 1]);
    eng.stems[0][usize::from(BASS)].gain = 0.5;
    eng.stems[1][usize::from(BASS)].gain = 0.25;
    let plan = bass_swap_from_snapshot(&eng, 0, 1, 8.0, 8.0);
    assert!(
        approx(plan.fades()[0].value_at(8.0), 0.5),
        "no click at the start"
    );
    assert!(
        approx(plan.fades()[1].value_at(8.0), 0.25),
        "no click at the start"
    );
    let restore = plan.restore_values();
    assert!(restore.contains(&(bass_id(0), 0.5)), "{restore:?}");
    assert!(restore.contains(&(bass_id(1), 0.25)), "{restore:?}");
}

#[test]
fn a_vocals_out_restores_the_current_vocal_gain() {
    let mut eng = engine(1, &[0]);
    eng.stems[0][usize::from(VOCALS)].gain = 0.5;
    let plan = vocals_out_from_snapshot(&eng, 0, 0.0, 4.0);
    let vocals = ParamId::Stem {
        deck: 0,
        stem: VOCALS,
        p: StemParam::Volume,
    };
    assert_eq!(plan.restore_values(), vec![(vocals, 0.5)]);
}

#[test]
fn a_drums_only_bridge_restores_the_mutes_the_user_had() {
    let mut eng = engine(1, &[0]);
    eng.stems[0][usize::from(VOCALS)].muted = true;
    let plan = drums_only_bridge_from_snapshot(&eng, 0, 4.0, 8.0);
    let vocals_mute = ParamId::Stem {
        deck: 0,
        stem: VOCALS,
        p: StemParam::Mute,
    };
    let bass_mute = ParamId::Stem {
        deck: 0,
        stem: BASS,
        p: StemParam::Mute,
    };
    let restore = plan.restore_values();
    assert!(restore.contains(&(vocals_mute, 1.0)), "{restore:?}");
    assert!(restore.contains(&(bass_mute, 0.0)), "{restore:?}");
}

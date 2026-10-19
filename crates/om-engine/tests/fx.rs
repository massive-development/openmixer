//! FX rack, beat-synced timing, wet/dry, tails, bounds, ids, presets.
//! Signals are rendered to buffers only; nothing is played.

use om_core::ParameterTarget;
use om_engine::fx::delay_line::DelayLine;
use om_engine::fx::wipe::{Wipe, SLICE};
use om_engine::fx::{
    apply_preset, builtin_presets, find_preset, Effect, FxContext, FxFactory, FxRack, FxUnit,
    GateFx, MAX_BPM, MIN_BPM,
};
use om_engine::ProcessStage;
use om_testing::signals::{noise as lcg_noise, sine_amp};
use std::collections::HashSet;

const SR: f64 = 48000.0;

#[derive(Clone)]
struct Signal {
    l: Vec<f32>,
    r: Vec<f32>,
}

impl Signal {
    fn zeros(n: usize) -> Self {
        Self {
            l: vec![0.0; n],
            r: vec![0.0; n],
        }
    }

    fn len(&self) -> usize {
        self.l.len()
    }
}

fn sine(n: usize, hz: f64, amp: f32) -> Signal {
    let v = sine_amp(hz, amp, SR, n);
    Signal { l: v.clone(), r: v }
}

/// Independent uniform noise on each channel, scaled to `amp`.
fn noise(n: usize, amp: f32) -> Signal {
    let scaled = |seed| -> Vec<f32> { lcg_noise(seed, n).into_iter().map(|s| amp * s).collect() };
    Signal {
        l: scaled(1234),
        r: scaled(1235),
    }
}

fn run<S: ProcessStage>(stage: &mut S, s: &mut Signal, from: usize, to: usize, block: usize) {
    for off in (from..to).step_by(block) {
        let end = (off + block).min(to);
        stage.process(&mut s.l[off..end], &mut s.r[off..end]);
    }
}

fn make_unit(name: &str, enabled: bool, mix: f32) -> FxUnit {
    let unit = FxUnit::new(FxFactory::with_built_ins().create(name).expect("built-in"));
    unit.set_enabled(enabled);
    unit.set_mix(mix);
    unit
}

/// First index of the largest magnitude, like `std::max_element` on `abs`.
fn argmax_abs(v: &[f32]) -> usize {
    let mut best = 0;
    for (i, x) in v.iter().enumerate() {
        if x.abs() > v[best].abs() {
            best = i;
        }
    }
    best
}

fn all_finite(s: &Signal) -> bool {
    s.l.iter().chain(s.r.iter()).all(|x| x.is_finite())
}

fn peak(s: &Signal, from: usize, to: usize) -> f32 {
    let mut p = 0.0_f32;
    for i in from..to {
        p = p.max(s.l[i].abs()).max(s.r[i].abs());
    }
    p
}

/// Echo with no feedback: the only non-zero output is the first repeat of an impulse.
fn first_echo_position(bpm: f64, beats: f64, block: usize) -> usize {
    let mut unit = make_unit("echo", true, 1.0);
    unit.set_tempo(bpm);
    unit.set_parameter("time", beats);
    unit.set_parameter("feedback", 0.0);
    unit.prepare(SR, block);
    let mut s = Signal::zeros(60000);
    s.l[0] = 1.0;
    s.r[0] = 1.0;
    run(&mut unit, &mut s, 0, 60000, block);
    argmax_abs(&s.l)
}

/// gtest `EXPECT_FLOAT_EQ`: equal within 4 ULPs.
fn ulps_f32(a: f32, b: f32) -> i64 {
    fn ordered(x: f32) -> i64 {
        let bits = i64::from(x.to_bits());
        if bits & 0x8000_0000 != 0 {
            -(bits & 0x7fff_ffff)
        } else {
            bits
        }
    }
    (ordered(a) - ordered(b)).abs()
}

fn assert_float_eq(actual: f32, expected: f32) {
    assert!(
        actual == expected || ulps_f32(actual, expected) <= 4,
        "{actual} != {expected}"
    );
}

/// gtest `EXPECT_DOUBLE_EQ`: equal within 4 ULPs.
fn assert_double_eq(actual: f64, expected: f64) {
    fn ordered(x: f64) -> i128 {
        let bits = i128::from(x.to_bits());
        if bits & (1 << 63) != 0 {
            -(bits & ((1 << 63) - 1))
        } else {
            bits
        }
    }
    let ulps = (ordered(actual) - ordered(expected)).abs();
    assert!(actual == expected || ulps <= 4, "{actual} != {expected}");
}

fn make_gate() -> Box<dyn Effect> {
    Box::new(GateFx::default())
}

#[test]
fn fx_factory_registers_eight_unique_built_ins() {
    let names = FxFactory::builtin_names();
    assert_eq!(names.len(), 8);
    assert_eq!(names.iter().collect::<HashSet<_>>().len(), 8);
    let factory = FxFactory::with_built_ins();
    for n in names {
        let fx = factory.create(n).unwrap_or_else(|| panic!("{n}"));
        assert_eq!(fx.name(), *n);
    }
    assert!(factory.create("nope").is_none());
}

#[test]
fn fx_factory_plugins_can_add_but_not_override() {
    let mut factory = FxFactory::with_built_ins();
    assert!(!factory.register_effect("echo", Box::new(make_gate)));
    assert!(factory.register_effect("mygate", Box::new(make_gate)));
    assert!(!factory.register_effect("", Box::new(make_gate)));
    assert!(factory.create("mygate").is_some());
    let rack = FxRack::new(&["mygate", "missing", "echo"], &factory);
    assert_eq!(rack.num_units(), 2);
    assert_eq!(rack.find_unit("echo"), Some(1));
}

#[test]
fn fx_echo_repeat_lands_on_the_beat_grid_at_any_tempo() {
    assert_eq!(first_echo_position(120.0, 1.0, 512), 24000);
    assert_eq!(first_echo_position(90.0, 1.0, 512), 32000);
    assert_eq!(first_echo_position(120.0, 0.5, 512), 12000);
    assert_eq!(first_echo_position(128.0, 0.75, 37), 16875); // odd block size crosses chunks
}

#[test]
fn fx_echo_feedback_repeats_decay() {
    let mut unit = make_unit("echo", true, 1.0);
    unit.set_tempo(120.0);
    unit.set_parameter("time", 0.25); // 6000 samples
    unit.set_parameter("feedback", 0.5);
    unit.prepare(SR, 512);
    let mut s = Signal::zeros(48000);
    s.l[0] = 1.0;
    s.r[0] = 1.0;
    run(&mut unit, &mut s, 0, 48000, 512);
    let first = peak(&s, 5000, 7000);
    let second = peak(&s, 11000, 13000);
    let third = peak(&s, 17000, 19000);
    assert!(first > 0.9);
    assert!(second < first);
    assert!(third < second);
    assert!(third > 0.01);
}

#[test]
fn fx_unit_mix_zero_and_disabled_are_bit_exact_dry() {
    for name in FxFactory::builtin_names() {
        for enabled in [true, false] {
            let mut unit = make_unit(name, enabled, if enabled { 0.0 } else { 1.0 });
            unit.prepare(SR, 512);
            let reference = sine(24000, 220.0, 0.5);
            let mut s = reference.clone();
            run(&mut unit, &mut s, 0, 24000, 512);
            assert_eq!(s.l, reference.l, "{name} enabled={enabled}");
            assert_eq!(s.r, reference.r, "{name} enabled={enabled}");
        }
    }
}

#[test]
fn fx_unit_every_effect_stays_finite_and_bounded_at_extremes() {
    for name in FxFactory::builtin_names() {
        for at_max in [true, false] {
            let mut unit = make_unit(name, true, 1.0);
            unit.set_tempo(if at_max { 250.0 } else { 50.0 });
            {
                let fx = unit.effect();
                for p in 0..fx.num_params() {
                    let spec = fx.param_spec(p);
                    fx.set_param(p, if at_max { spec.max } else { spec.min });
                }
            }
            unit.prepare(SR, 512);
            let mut s = noise(96000, 1.0);
            run(&mut unit, &mut s, 0, 96000, 512);
            assert!(all_finite(&s), "{name}");
            assert!(peak(&s, 0, s.len()) < 4.0, "{name} atMax={at_max}");
        }
    }
}

#[test]
fn fx_unit_every_effect_alters_the_signal_when_fully_wet() {
    for name in FxFactory::builtin_names() {
        let mut unit = make_unit(name, true, 1.0);
        unit.prepare(SR, 512);
        let reference = noise(48000, 0.5);
        let mut s = reference.clone();
        run(&mut unit, &mut s, 0, 48000, 512);
        let diff: f64 =
            s.l.iter()
                .zip(&reference.l)
                .map(|(a, b)| f64::from((a - b).abs()))
                .sum();
        assert!(diff / s.len() as f64 > 0.01, "{name}");
    }
}

#[test]
fn fx_unit_rejects_non_finite_and_clamps_parameters() {
    let unit = make_unit("echo", true, 1.0);
    assert!(!unit.set_parameter("time", f64::NAN));
    assert!(unit.set_parameter("feedback", 5.0));
    let fx = unit.effect();
    assert_float_eq(fx.param_target(fx.find_param("feedback").unwrap()), 0.95);
    unit.set_mix(-1.0);
    assert_float_eq(unit.mix(), 0.0);
    unit.set_tempo(1000.0);
    assert_double_eq(unit.tempo(), MAX_BPM);
    assert_double_eq(MIN_BPM, 50.0);
}

#[test]
fn fx_unit_tail_rings_out_after_disable_then_goes_idle() {
    for name in ["echo", "reverb"] {
        let mut unit = make_unit(name, true, 0.5);
        unit.set_tempo(120.0);
        unit.set_parameter("time", 0.25);
        unit.prepare(SR, 512);
        let mut s = Signal::zeros((SR * 20.0) as usize);
        let burst = noise(4800, 0.5);
        s.l[..4800].copy_from_slice(&burst.l);
        s.r[..4800].copy_from_slice(&burst.r);
        run(&mut unit, &mut s, 0, 4800, 512);
        unit.set_enabled(false);
        let len = s.len();
        run(&mut unit, &mut s, 4800, len, 512);
        assert!(
            peak(&s, 9600, 20000) > 1.0e-3,
            "{name}: tail cut on disable"
        );
        assert!(!unit.is_active(), "{name}: tail never finished");
        let len = s.len();
        assert_eq!(peak(&s, len - 4800, len), 0.0, "{name}");
    }
}

#[test]
fn fx_unit_non_tail_effect_fades_out_and_stops_when_disabled() {
    let mut unit = make_unit("gate", true, 1.0);
    unit.prepare(SR, 512);
    let mut s = sine(48000, 330.0, 0.5);
    let reference = s.clone();
    run(&mut unit, &mut s, 0, 9600, 512);
    unit.set_enabled(false);
    run(&mut unit, &mut s, 9600, 48000, 512);
    assert!(!unit.is_active());
    assert_eq!(s.l[24000..], reference.l[24000..]);
}

#[test]
fn delay_line_clears_in_slices_not_all_at_once() {
    let mut line = DelayLine::default();
    line.allocate(5000);
    for _ in 0..5000 {
        line.push(1.0, 1.0);
    }
    line.begin_clear();
    let mut slices = 0;
    while !line.clear_some(512) {
        slices += 1;
    }
    assert!(
        slices >= 9,
        "a 512-frame slice must not wipe the whole line"
    );
    for d in (1..5000).step_by(97) {
        assert_eq!(line.read(0, d as f32), 0.0);
    }
    assert!(line.clear_some(512), "nothing left once cleared");
}

// Feeds loud audio, starts a sliced reset, then renders silence with no wipe at all: stale slots
// must read as zero.
#[test]
fn effect_stale_buffer_reads_as_zero_while_wipe_pending() {
    for name in ["echo", "flanger", "reverb"] {
        let mut fx = FxFactory::with_built_ins().create(name).expect("built-in");
        fx.prepare(SR, 512);
        let ctx = FxContext {
            sample_rate: SR,
            samples_per_beat: SR * 0.5,
        };
        let mut loud = noise(SR as usize, 0.8);
        for off in (0..loud.len()).step_by(64) {
            fx.process(&mut loud.l[off..off + 64], &mut loud.r[off..off + 64], &ctx);
        }
        fx.begin_reset();
        let mut silence = Signal::zeros(SR as usize);
        for off in (0..silence.len()).step_by(64) {
            fx.process(
                &mut silence.l[off..off + 64],
                &mut silence.r[off..off + 64],
                &ctx,
            );
        }
        assert_eq!(
            peak(&silence, 0, silence.len()),
            0.0,
            "{name}: stale audio read after begin_reset"
        );
    }
}

#[test]
fn effect_wipe_is_bounded_per_call_and_finishes() {
    for name in ["echo", "flanger", "reverb"] {
        let mut fx = FxFactory::with_built_ins().create(name).expect("built-in");
        fx.prepare(SR, 512);
        fx.begin_reset();
        let mut calls = 0;
        while !fx.clear_some(SLICE) && calls < 100_000 {
            calls += 1;
        }
        if name != "flanger" {
            // the flanger line is shorter than one slice
            assert!(calls > 1, "{name}: one slice must not wipe everything");
        }
        assert!(calls < 100_000, "{name}");
    }
    let mut wipe = Wipe::default();
    wipe.begin();
    assert_eq!(wipe.claim(10000, 512), (0, 512));
    assert_eq!(
        wipe.claim(10000, 1 << 30).1,
        10000 - 512,
        "claims are capped by what is left"
    );
}

// Re-engage on the very chunk after is_active() drops, i.e. while the wipe is still pending.
#[test]
fn fx_unit_re_engaging_during_wipe_leaks_nothing_stale() {
    for name in ["flanger", "echo", "reverb"] {
        let mut unit = make_unit(name, true, 1.0);
        if name == "flanger" {
            unit.set_parameter("feedback", 0.9);
        }
        unit.prepare(SR, 512);
        let mut loud = noise(SR as usize, 0.8);
        run(&mut unit, &mut loud, 0, SR as usize, FxUnit::CHUNK);
        unit.set_enabled(false);
        let mut quiet = Signal::zeros(SR as usize * 30);
        let mut off = 0;
        while off < quiet.len() && unit.is_active() {
            let end = off + FxUnit::CHUNK;
            unit.process(&mut quiet.l[off..end], &mut quiet.r[off..end]);
            off += FxUnit::CHUNK;
        }
        assert!(!unit.is_active(), "{name}");
        unit.set_enabled(true); // wipe has had at most one slice
        let mut silence = Signal::zeros(4096);
        run(&mut unit, &mut silence, 0, 4096, FxUnit::CHUNK);
        assert!(
            peak(&silence, 0, silence.len()) < 1.0e-5,
            "{name}: stale audio leaked after early re-engage"
        );
    }
}

#[test]
fn fx_unit_mix_changes_are_smoothed() {
    let mut unit = make_unit("echo", true, 0.0);
    unit.set_tempo(120.0);
    unit.set_parameter("time", 0.51); // 12240 samples: half a 100 Hz cycle past a whole number
    unit.set_parameter("feedback", 0.0);
    unit.prepare(SR, 512);
    let mut s = sine(48000, 100.0, 0.5);
    run(&mut unit, &mut s, 0, 24576, 512);
    unit.set_mix(1.0); // dry and wet are 180 degrees apart here: a hard switch would jump
    run(&mut unit, &mut s, 24576, 48000, 512);
    let mut max_step = 0.0_f32;
    for i in 24000..30000 {
        max_step = max_step.max((s.l[i] - s.l[i - 1]).abs());
    }
    assert!(max_step < 0.02);
}

#[test]
fn fx_unit_no_subnormals_in_decaying_reverb() {
    let mut unit = make_unit("reverb", true, 1.0);
    unit.prepare(SR, 512);
    let mut s = Signal::zeros(SR as usize * 10);
    s.l[0] = 1.0;
    s.r[0] = 1.0;
    run(&mut unit, &mut s, 0, SR as usize * 10, 512);
    assert!(!s.l.iter().any(|x| x.is_subnormal()));
    assert!(!s.r.iter().any(|x| x.is_subnormal()));
}

#[test]
fn fx_roll_repeats_one_beat_division() {
    let mut unit = make_unit("roll", true, 1.0);
    unit.set_tempo(120.0);
    unit.set_parameter("time", 0.25); // 6000 samples
    unit.prepare(SR, 512);
    let mut s = sine(30000, 173.0, 0.5);
    let reference = s.clone();
    run(&mut unit, &mut s, 0, 30000, 512);
    assert_float_eq(s.l[12000 + 3000], reference.l[3000]);
    assert_float_eq(s.l[24000 + 4500], reference.l[4500]);
    assert_float_eq(s.l[3000], reference.l[3000]);
}

#[test]
fn fx_gate_chops_on_beat_divisions() {
    let mut unit = make_unit("gate", true, 1.0);
    unit.set_tempo(120.0);
    unit.set_parameter("time", 0.5); // 12000-sample period, open for the first half
    unit.set_parameter("duty", 0.5);
    unit.prepare(SR, 512);
    let mut s = Signal::zeros(36000);
    s.l.fill(0.5);
    s.r.fill(0.5);
    run(&mut unit, &mut s, 0, 36000, 512);
    assert!((s.l[3000] - 0.5).abs() <= 1.0e-3);
    assert!((s.l[9000] - 0.0).abs() <= 1.0e-3);
    assert!((s.l[15000] - 0.5).abs() <= 1.0e-3);
}

#[test]
fn fx_tape_stop_slows_to_silence_in_the_synced_time() {
    let mut unit = make_unit("tapestop", true, 1.0);
    unit.set_tempo(120.0);
    unit.set_parameter("time", 1.0); // stops within 24000 samples
    unit.prepare(SR, 512);
    let mut s = sine(48000, 440.0, 0.5);
    run(&mut unit, &mut s, 0, 48000, 512);
    assert!(peak(&s, 0, 4800) > 0.4);
    assert_eq!(peak(&s, 24100, s.len()), 0.0);
}

#[test]
fn fx_rack_parameter_ids_route_to_units() {
    let rack = FxRack::default();
    assert_eq!(rack.num_units(), 8);
    assert!(rack.set_parameter("fx0.mix", 0.3));
    assert_float_eq(rack.unit(0).unwrap().mix(), 0.3);
    assert!(rack.set_parameter("fx0.enable", 1.0));
    assert!(rack.unit(0).unwrap().is_enabled());
    assert!(rack.set_parameter("fx0.time", 2.0));
    let echo = rack.unit(0).unwrap().effect();
    assert_float_eq(echo.param_target(echo.find_param("time").unwrap()), 2.0);
    assert!(rack.set_parameter("fx7.time", 4.0));
    for bad in [
        "fx8.mix",
        "fx0.bogus",
        "fxa.mix",
        "fx.mix",
        "fx0",
        "mix",
        "",
    ] {
        assert!(!rack.set_parameter(bad, 0.5), "{bad}");
    }
}

// The grid overload would normally take a FixedTempoGrid; that type lands with the deck
// (BeatGrid) work, so this case passes the grid's BPM directly.
#[test]
fn fx_rack_tempo_from_id_and_grid_bpm() {
    let rack = FxRack::default();
    assert!(rack.set_parameter("bpm", 128.0));
    assert_double_eq(rack.unit(3).unwrap().tempo(), 128.0);
    rack.set_tempo_from_grid_bpm(100.0, 1.08);
    for i in 0..rack.num_units() {
        assert_double_eq(rack.unit(i).unwrap().tempo(), 108.0);
    }
}

#[test]
fn fx_rack_all_units_together_stay_bounded() {
    let mut rack = FxRack::default();
    for i in 0..rack.num_units() {
        rack.unit(i).unwrap().set_enabled(true);
    }
    rack.prepare(SR, 256);
    let mut s = sine(96000, 110.0, 0.9);
    run(&mut rack, &mut s, 0, 96000, 256);
    assert!(all_finite(&s));
    assert!(peak(&s, 0, s.len()) < 4.0);
}

#[test]
fn fx_presets_original_presets_apply_to_matching_effects() {
    let presets = builtin_presets();
    assert_eq!(presets.len(), 8);
    let rack = FxRack::default();
    for preset in presets {
        let idx = rack.find_unit(preset.effect);
        assert!(idx.is_some(), "{}", preset.name);
        let unit = rack.unit(idx.unwrap()).unwrap();
        assert!(apply_preset(unit, preset));
        assert_float_eq(unit.mix(), preset.mix);
    }
    assert!(!apply_preset(
        rack.unit(0).unwrap(),
        find_preset("Half Roll").unwrap()
    ));
    assert!(find_preset("does not exist").is_none());
}

#[test]
fn fx_rack_inserts_into_mixer_master_without_colouring_when_off() {
    use om_engine::{ChannelInput, Mixer};
    const N: usize = 4096;
    const BLOCK: usize = 512;
    let (mut plain, _plain_ctl) = Mixer::new(1);
    let (mut with_fx, fx_ctl) = Mixer::new(1);
    plain.prepare(SR, BLOCK);
    with_fx.prepare(SR, BLOCK);
    assert!(fx_ctl.set_master_insert(0, Some(Box::new(FxRack::default()))));
    let input = sine(N, 440.0, 0.5);
    let (mut a_l, mut a_r) = (vec![0.0_f32; N], vec![0.0_f32; N]);
    let (mut b_l, mut b_r) = (vec![0.0_f32; N], vec![0.0_f32; N]);
    // Cue is not compared; each mixer gets its own buffer.
    let (mut cue_a, mut cue_b) = (vec![0.0_f32; N], vec![0.0_f32; N]);
    for off in (0..N).step_by(BLOCK) {
        let end = off + BLOCK;
        let ch = [ChannelInput {
            left: Some(&input.l[off..end]),
            right: Some(&input.r[off..end]),
        }];
        plain.process(
            &ch,
            &mut a_l[off..end],
            &mut a_r[off..end],
            &mut cue_a[off..end],
            &mut cue_b[off..end],
        );
        with_fx.process(
            &ch,
            &mut b_l[off..end],
            &mut b_r[off..end],
            &mut cue_a[off..end],
            &mut cue_b[off..end],
        );
    }
    assert_eq!(a_l, b_l);
    assert_eq!(a_r, b_r);
}

#[test]
fn control_handle_exposes_param_specs_and_targets() {
    let rack = FxRack::new(&["echo"], &FxFactory::with_built_ins());
    let control = rack.control();
    let unit = control.unit(0).expect("echo unit");
    let params = unit.params();
    let feedback = params.find("feedback").expect("echo has feedback");
    assert_eq!(params.spec(feedback).map(|s| s.name), Some("feedback"));
    assert!(params.set(feedback, 0.8));
    assert!((params.target(feedback).unwrap() - 0.8).abs() < 1e-6);
    assert!(params.spec(params.len()).is_none());
    assert!(params.target(params.len()).is_none());
}

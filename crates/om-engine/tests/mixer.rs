//! EQ, filter, faders, crossfader, cue bus, meters, parameter
//! ids and inserts. Signals are rendered to buffers only; nothing is played.

use om_core::ParameterTarget;
use om_dsp::{CrossfaderAssign, CrossfaderCurve};
use om_engine::{ChannelInput, Mixer, MixerControl, ProcessStage};
use om_testing::level::{gain_to_db, rms_tail};
use om_testing::signals::sine_amp;

const SR: f64 = 48_000.0;
const N: usize = 16_384;
const BLOCK: usize = 512;

struct Rig {
    mixer: Mixer,
    ctl: MixerControl,
    ins: [[Vec<f32>; 2]; 2], // [channel][L/R]
    ml: Vec<f32>,
    mr: Vec<f32>,
    cl: Vec<f32>,
    cr: Vec<f32>,
}

impl Rig {
    fn new() -> Self {
        let (mut mixer, ctl) = Mixer::new(2);
        mixer.prepare(SR, BLOCK);
        Self {
            mixer,
            ctl,
            ins: std::array::from_fn(|_| std::array::from_fn(|_| vec![0.0; N])),
            ml: vec![0.0; N],
            mr: vec![0.0; N],
            cl: vec![0.0; N],
            cr: vec![0.0; N],
        }
    }

    fn set_sine(&mut self, ch: usize, hz: f64, amp: f32) {
        let s = sine_amp(hz, amp, SR, N);
        self.ins[ch][0].copy_from_slice(&s);
        self.ins[ch][1].copy_from_slice(&s);
    }

    /// Block-wise like a device.
    fn run(&mut self) {
        for off in (0..N).step_by(BLOCK) {
            let end = off + BLOCK;
            let inputs = [
                ChannelInput {
                    left: Some(&self.ins[0][0][off..end]),
                    right: Some(&self.ins[0][1][off..end]),
                },
                ChannelInput {
                    left: Some(&self.ins[1][0][off..end]),
                    right: Some(&self.ins[1][1][off..end]),
                },
            ];
            self.mixer.process(
                &inputs,
                &mut self.ml[off..end],
                &mut self.mr[off..end],
                &mut self.cl[off..end],
                &mut self.cr[off..end],
            );
        }
    }

    /// RMS over the second half, after the filters have settled.
    fn rms(v: &[f32]) -> f64 {
        rms_tail(v)
    }
}

/// Gain in dB through master for a 0.5-peak sine at `hz` with the given setup.
fn gain_db(hz: f64, setup: impl FnOnce(&MixerControl)) -> f64 {
    let mut r = Rig::new();
    r.set_sine(0, hz, 0.5);
    setup(&r.ctl);
    r.run();
    gain_to_db(Rig::rms(&r.ml) / (0.5 / 2.0_f64.sqrt()))
}

fn unity() -> f64 {
    0.5 / 2.0_f64.sqrt()
}

#[test]
fn mixer_test_flat_settings_are_unity_gain() {
    for hz in [60.0, 1000.0, 10000.0] {
        let db = gain_db(hz, |_| {});
        assert!(db.abs() <= 0.1, "hz={hz}: {db}");
    }
}

#[test]
fn mixer_test_three_band_eq_cuts_and_boosts_the_intended_band() {
    assert!(gain_db(60.0, |m| m.set_eq(0, 0, -48.0)) < -30.0, "low kill");
    assert!((gain_db(10000.0, |m| m.set_eq(0, 0, -48.0))).abs() <= 0.5);
    assert!(
        gain_db(1000.0, |m| m.set_eq(0, 1, -48.0)) < -30.0,
        "mid kill"
    );
    assert!((gain_db(60.0, |m| m.set_eq(0, 1, -48.0))).abs() <= 5.0);
    assert!(
        gain_db(12000.0, |m| m.set_eq(0, 2, -48.0)) < -30.0,
        "high kill"
    );
    assert!((gain_db(100.0, |m| m.set_eq(0, 2, -48.0))).abs() <= 0.5);
    assert!((gain_db(60.0, |m| m.set_eq(0, 0, 6.0)) - 6.0).abs() <= 1.0);
    assert!((gain_db(1000.0, |m| m.set_eq(0, 1, 6.0)) - 6.0).abs() <= 0.3);
    assert!((gain_db(12000.0, |m| m.set_eq(0, 2, 6.0)) - 6.0).abs() <= 1.0);
}

#[test]
fn mixer_test_sweep_filter_low_pass_high_pass_and_bypass() {
    assert!(
        gain_db(10000.0, |m| m.set_filter(0, -0.8)) < -20.0,
        "LP removes highs"
    );
    assert!(gain_db(40.0, |m| m.set_filter(0, -0.8)).abs() <= 1.0);
    assert!(
        gain_db(100.0, |m| m.set_filter(0, 0.8)) < -20.0,
        "HP removes lows"
    );
    assert!(gain_db(10000.0, |m| m.set_filter(0, 0.8)).abs() <= 1.0);
    assert!(gain_db(100.0, |m| m.set_filter(0, 0.01)).abs() <= 0.01);
}

#[test]
fn mixer_test_channel_fader_and_master_gain_scale_output() {
    assert!((gain_db(1000.0, |m| m.set_fader(0, 0.5)) - -6.02).abs() <= 0.1);
    assert!(gain_db(1000.0, |m| m.set_fader(0, 0.0)) < -100.0);
    assert!((gain_db(1000.0, |m| m.set_master_gain(0.5)) - -6.02).abs() <= 0.1);
    assert!((gain_db(1000.0, |m| m.set_trim(0, 2.0)) - 6.02).abs() <= 0.1);
}

#[test]
fn mixer_test_parameters_are_clamped() {
    let (_mixer, m) = Mixer::new(2);
    m.set_fader(0, 5.0);
    assert_eq!(m.fader(0), 1.0);
    m.set_eq(0, 1, -999.0);
    assert_eq!(m.eq(0, 1), -48.0);
    m.set_filter(1, -3.0);
    assert_eq!(m.filter(1), -1.0);
    m.set_fader(99, 0.25);
    assert_eq!(m.fader(1), 0.25, "index clamps");
}

#[test]
fn mixer_test_crossfader_routes_assigned_channels() {
    for curve in [CrossfaderCurve::Linear, CrossfaderCurve::ConstantPower] {
        let label = if curve == CrossfaderCurve::Linear {
            "Linear"
        } else {
            "ConstantPower"
        };
        let mut r = Rig::new();
        r.set_sine(0, 1000.0, 0.5);
        r.set_sine(1, 3000.0, 0.5);
        r.ctl.set_crossfader_curve(curve);
        r.ctl.set_crossfader_assign(0, CrossfaderAssign::Left);
        r.ctl.set_crossfader_assign(1, CrossfaderAssign::Right);

        r.ctl.set_crossfader(0.0);
        r.run();
        assert!(
            (Rig::rms(&r.ml) - unity()).abs() <= 0.005,
            "{label}: only ch0"
        );

        r.ctl.set_crossfader(1.0);
        r.run();
        assert!(
            (Rig::rms(&r.ml) - unity()).abs() <= 0.005,
            "{label}: only ch1"
        );

        r.ctl.set_crossfader(0.5);
        r.run();
        // Two uncorrelated equal sines: powers sum.
        let g = if curve == CrossfaderCurve::Linear {
            0.5
        } else {
            0.5_f64.sqrt()
        };
        let expected = (2.0 * g * g).sqrt() * 0.5 / 2.0_f64.sqrt();
        assert!((Rig::rms(&r.ml) - expected).abs() <= 0.01, "{label}: mix");
    }
}

#[test]
fn mixer_test_thru_channels_ignore_the_crossfader() {
    let mut r = Rig::new();
    r.set_sine(0, 1000.0, 0.5);
    r.ctl.set_crossfader(1.0);
    r.run();
    assert!((Rig::rms(&r.ml) - unity()).abs() <= 0.005);
}

#[test]
fn mixer_test_pfl_cue_bus_is_pre_fader_and_independent_of_crossfader() {
    let mut r = Rig::new();
    r.set_sine(0, 1000.0, 0.5);
    r.set_sine(1, 3000.0, 0.5);
    r.ctl.set_crossfader_assign(0, CrossfaderAssign::Left);
    r.ctl.set_crossfader(1.0); // ch0 fully cut from master
    r.ctl.set_fader(0, 0.0);
    r.ctl.set_pfl(0, true);
    r.run();
    let u = unity();
    assert!(Rig::rms(&r.cl) > u * 0.98, "cue carries ch0 at full level");
    assert!(Rig::rms(&r.cl) < u * 1.02, "cue carries ch0 at full level");
    assert!(Rig::rms(&r.ml) > u * 0.98, "master carries only ch1");
    assert!(Rig::rms(&r.ml) < u * 1.02, "master carries only ch1");

    r.ctl.set_pfl(0, false);
    r.run();
    assert!(Rig::rms(&r.cl) < 1e-6, "cue is silent without PFL");
}

#[test]
fn mixer_test_cue_mix_blends_master_into_headphones_and_cue_level_scales() {
    let mut r = Rig::new();
    r.set_sine(0, 1000.0, 0.5);
    r.set_sine(1, 3000.0, 0.5);
    r.ctl.set_pfl(0, true);
    r.ctl.set_fader(0, 0.0); // ch0 only in cue
    r.ctl.set_cue_mix(1.0);
    r.run();
    let master_rms = Rig::rms(&r.ml);
    assert!((Rig::rms(&r.cl) - master_rms).abs() <= 1e-4);
    r.ctl.set_cue_mix(0.5);
    r.ctl.set_cue_level(0.5);
    r.run();
    assert!(Rig::rms(&r.cl) > 0.0);
    assert!(Rig::rms(&r.cl) < master_rms, "scaled below master");
}

#[test]
fn mixer_test_meters_track_level_and_decay() {
    let mut r = Rig::new();
    r.set_sine(0, 1000.0, 0.5);
    r.run();
    assert!(r.ctl.channel_peak(0) > 0.1);
    assert!(r.ctl.master_peak_left() > 0.1);
    assert!((r.ctl.channel_peak(1)).abs() <= 1e-6);
    r.set_sine(0, 1000.0, 0.0);
    for _ in 0..6 {
        r.run();
    }
    assert!(r.ctl.channel_peak(0) < 0.01, "peak decays after silence");
}

#[test]
fn mixer_test_channel_meters_are_stereo_and_read_back() {
    let mut r = Rig::new();
    r.set_sine(0, 1000.0, 0.5);
    r.ins[0][1].fill(0.0); // right input silent
    r.run();
    assert!(r.ctl.channel_peak_left(0) > 0.1);
    assert!((r.ctl.channel_peak_right(0)).abs() <= 1e-6);
    assert_eq!(r.ctl.channel_peak(0), r.ctl.channel_peak_left(0));
}

#[test]
fn mixer_test_crossfader_curve_and_state_getters_follow_parameters() {
    let (_mixer, m) = Mixer::new(2);
    assert_eq!(m.crossfader_curve(), CrossfaderCurve::ConstantPower);
    assert!(m.set_parameter("crossfader.curve", 0.0));
    assert_eq!(m.crossfader_curve(), CrossfaderCurve::Linear);
    assert!(m.set_parameter("crossfader.curve", 1.0));
    assert_eq!(m.crossfader_curve(), CrossfaderCurve::ConstantPower);
    m.set_parameter("crossfader", 0.8);
    m.set_parameter("master", 0.7);
    m.set_parameter("cue.level", 0.6);
    m.set_parameter("cue.mix", 0.4);
    m.set_parameter("ch1.trim", 2.0);
    m.set_parameter("ch1.xfader", 2.0);
    assert!((m.crossfader() - 0.8).abs() <= 1e-6);
    assert!((m.master_gain() - 0.7).abs() <= 1e-6);
    assert!((m.cue_level() - 0.6).abs() <= 1e-6);
    assert!((m.cue_mix() - 0.4).abs() <= 1e-6);
    assert!((m.trim(1) - 2.0).abs() <= 1e-6);
    assert_eq!(m.crossfader_assign(1), CrossfaderAssign::Right);
}

#[test]
fn mixer_test_set_parameter_drives_the_same_controls_by_string_id() {
    let (_mixer, m) = Mixer::new(2);
    assert!(m.set_parameter("ch1.fader", 0.25));
    assert_eq!(m.fader(1), 0.25);
    assert!(m.set_parameter("ch0.eq.high", -6.0));
    assert_eq!(m.eq(0, 2), -6.0);
    assert!(m.set_parameter("ch0.pfl", 1.0));
    assert!(m.pfl(0));
    assert!(m.set_parameter("ch0.filter", -0.5));
    assert_eq!(m.filter(0), -0.5);
    assert!(m.set_parameter("crossfader", 0.9));
    assert!(m.set_parameter("master", 1.0));
    assert!(m.set_parameter("cue.level", 1.0));
    assert!(m.set_parameter("cue.mix", 0.5));
    assert!(!m.set_parameter("ch2.fader", 1.0), "channel out of range");
    assert!(!m.set_parameter("ch0.nonsense", 1.0));
    assert!(!m.set_parameter("ch0.eq.sub", 1.0));
    assert!(!m.set_parameter("bogus", 1.0));
}

struct Gain(f32);

impl ProcessStage for Gain {
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            *l *= self.0;
            *r *= self.0;
        }
    }
}

struct Stale;

impl ProcessStage for Stale {
    fn interface_version(&self) -> u32 {
        999
    }
    fn process(&mut self, _: &mut [f32], _: &mut [f32]) {}
}

#[test]
fn mixer_test_channel_and_master_inserts_can_be_added_replaced_and_removed_at_runtime() {
    let mut r = Rig::new();
    r.set_sine(0, 1000.0, 0.5);

    assert!(r.ctl.set_channel_insert(0, 0, Some(Box::new(Gain(0.5)))));
    r.run();
    assert!((Rig::rms(&r.ml) - unity() * 0.5).abs() <= 0.005);

    assert!(
        r.ctl.set_channel_insert(0, 0, Some(Box::new(Gain(0.25)))),
        "replace while running"
    );
    r.run();
    assert!((Rig::rms(&r.ml) - unity() * 0.25).abs() <= 0.005);

    assert!(r.ctl.set_master_insert(0, Some(Box::new(Gain(2.0)))));
    r.run();
    assert!((Rig::rms(&r.ml) - unity() * 0.5).abs() <= 0.005);

    assert!(r.ctl.set_channel_insert(0, 0, None));
    assert!(r.ctl.set_master_insert(0, None));
    r.run();
    assert!((Rig::rms(&r.ml) - unity()).abs() <= 0.005);

    assert!(
        !r.ctl.set_channel_insert(0, 0, Some(Box::new(Stale))),
        "incompatible interface version"
    );
    assert!(
        !r.ctl.set_channel_insert(0, 99, Some(Box::new(Gain(0.5)))),
        "bad slot"
    );
}

#[test]
fn mixer_test_null_inputs_produce_silence_and_oversize_blocks_are_chunked() {
    let mut r = Rig::new();
    let none = [ChannelInput::default(); 2];
    r.mixer.process(
        &none,
        &mut r.ml[..BLOCK],
        &mut r.mr[..BLOCK],
        &mut r.cl[..BLOCK],
        &mut r.cr[..BLOCK],
    );
    assert!(r.ml[..BLOCK].iter().all(|&v| v == 0.0));

    r.set_sine(0, 1000.0, 0.5);
    let inputs = [
        ChannelInput {
            left: Some(&r.ins[0][0][..]),
            right: Some(&r.ins[0][1][..]),
        },
        ChannelInput::default(),
    ];
    // N frames in one call, more than the prepared 512-frame block.
    r.mixer
        .process(&inputs, &mut r.ml, &mut r.mr, &mut r.cl, &mut r.cr);
    assert!((Rig::rms(&r.ml) - unity()).abs() <= 0.005);
}

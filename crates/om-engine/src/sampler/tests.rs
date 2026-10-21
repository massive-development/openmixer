use super::audio::START_RAMP_FRAMES;
use super::*;
use crate::stage::ProcessStage;
use std::sync::Arc;

const RATE: f64 = 48_000.0;

fn setup() -> (SamplerAudio, SamplerControl) {
    let (mut audio, control) = SamplerAudio::new();
    audio.prepare(RATE, 256);
    (audio, control)
}

fn ramp_sample(frames: usize) -> Arc<Sample> {
    let data: Vec<f32> = (0..frames)
        .flat_map(|n| {
            let v = 0.25 + 0.5 * (n % 100) as f32 / 100.0;
            [v, -v]
        })
        .collect();
    Arc::new(Sample::from_interleaved("ramp", RATE, data).expect("sample"))
}

fn dc(frames: usize, value: f32) -> Arc<Sample> {
    Arc::new(Sample::from_mono("dc", RATE, &vec![value; frames]).expect("sample"))
}

fn render(audio: &mut SamplerAudio, frames: usize) -> (Vec<f32>, Vec<f32>) {
    let mut l = vec![0.0; frames];
    let mut r = vec![0.0; frames];
    audio.process(&mut l, &mut r);
    (l, r)
}

fn ramp_gain(i: usize) -> f32 {
    ((i + 1) as f32 / START_RAMP_FRAMES).min(1.0)
}

#[test]
fn trigger_renders_the_sample_exactly_after_the_start_ramp() {
    let (mut audio, control) = setup();
    let sample = ramp_sample(1000);
    control.assign(0, Arc::clone(&sample)).unwrap();
    render(&mut audio, 64); // applies the assign; nothing sounds
    control.trigger(0, 1.0);
    let (l, r) = render(&mut audio, 400);
    for i in 0..400 {
        let want = sample.data()[i * 2] * ramp_gain(i);
        assert!((l[i] - want).abs() < 1e-5, "frame {i}: {} vs {want}", l[i]);
        assert!((r[i] + want).abs() < 1e-5);
        if i >= START_RAMP_FRAMES as usize {
            assert_eq!(l[i], sample.data()[i * 2], "exact at frame {i}");
        }
    }
}

#[test]
fn a_sample_plays_to_its_end_then_the_voice_is_free_and_silent() {
    let (mut audio, control) = setup();
    control.assign(0, dc(100, 0.5)).unwrap();
    control.trigger(0, 1.0);
    let (l, _) = render(&mut audio, 300);
    assert_eq!(l[60], 0.5, "full level before the tail");
    assert!(
        l[99] > 0.0 && l[99] < 0.1,
        "last frame is nearly faded: {}",
        l[99]
    );
    assert!(
        l[68..100].windows(2).all(|w| w[1] < w[0]),
        "a 32 frame tail fade"
    );
    assert!(l[100..].iter().all(|&v| v == 0.0));
    assert_eq!(control.snapshot().voices_active, 0);
    assert!(!control.snapshot().pads[0].playing);
}

#[test]
fn velocity_scales_the_level() {
    let (mut audio, control) = setup();
    control.assign(0, dc(500, 0.8)).unwrap();
    control.trigger(0, 0.25);
    let (l, _) = render(&mut audio, 200);
    assert!((l[150] - 0.2).abs() < 1e-6);
    control.trigger(0, 7.0); // clamps to 1
    control.trigger(0, f32::NAN); // ignored
    let (l, _) = render(&mut audio, 100);
    assert!(l[99] > 0.8 && l[99] < 1.1, "two voices: {}", l[99]);
}

#[test]
fn gain_scales_and_is_ramped_not_stepped() {
    let (mut audio, control) = setup();
    control.assign(0, dc(2000, 1.0)).unwrap();
    control.trigger(0, 1.0);
    render(&mut audio, 200);
    control.set_gain(0, 0.5);
    let (l, _) = render(&mut audio, 200);
    assert!(l[0] > 0.95, "starts from the old gain: {}", l[0]);
    assert!((l[199] - 0.5).abs() < 1e-5);
    assert!(l.windows(2).all(|w| w[1] <= w[0]));
    control.set_gain(0, f32::NAN);
    control.set_gain(0, 99.0);
    assert_eq!(control.snapshot().pads[0].gain, MAX_PAD_GAIN);
}

#[test]
fn pitch_plus_twelve_plays_at_double_speed() {
    let (mut audio, control) = setup();
    // A slow ramp so the cubic interpolation is near exact.
    let data: Vec<f32> = (0..4000).flat_map(|n| [n as f32 / 4000.0; 2]).collect();
    let sample = Arc::new(Sample::from_interleaved("slope", RATE, data).unwrap());
    control.assign(0, sample).unwrap();
    control.set_pitch(0, 12.0);
    control.trigger(0, 1.0);
    let (l, _) = render(&mut audio, 600);
    for (i, &got) in l.iter().enumerate().skip(100) {
        let want = (2 * i) as f32 / 4000.0;
        assert!((got - want).abs() < 1e-4, "frame {i}: {got} vs {want}");
    }
    // And the sample ends after half the frames.
    let (l, _) = render(&mut audio, 2000);
    assert!(l[..1400].iter().any(|&v| v != 0.0));
    assert!(l[1500..].iter().all(|&v| v == 0.0));
}

#[test]
fn a_sample_at_another_rate_plays_at_its_true_speed() {
    let (mut audio, control) = setup();
    let data: Vec<f32> = (0..4000).flat_map(|n| [n as f32 / 8000.0; 2]).collect();
    let sample = Arc::new(Sample::from_interleaved("half", RATE / 2.0, data).unwrap());
    control.assign(0, sample).unwrap();
    control.trigger(0, 1.0);
    let (l, _) = render(&mut audio, 600);
    // Engine runs at twice the sample rate: one source frame per two output frames.
    assert!((l[400] - 200.0 / 8000.0).abs() < 1e-4, "{}", l[400]);
}

#[test]
fn gate_stops_on_release_with_a_fade() {
    let (mut audio, control) = setup();
    control.assign(0, dc(48_000, 1.0)).unwrap();
    control.set_mode(0, PadMode::Gate);
    control.trigger(0, 1.0);
    let (l, _) = render(&mut audio, 1000);
    assert_eq!(l[999], 1.0);
    control.release(0);
    let (l, _) = render(&mut audio, 1000);
    let fade = (0.005 * RATE) as usize;
    assert!(l[0] > 0.9 && l[0] <= 1.0);
    assert!(l.windows(2).all(|w| w[1] <= w[0] + 1e-6), "monotonic fade");
    assert!(
        l[fade + 2..].iter().all(|&v| v == 0.0),
        "silent after the fade"
    );
    assert!(
        l[fade / 2] > 0.2 && l[fade / 2] < 0.8,
        "a real ramp, not a cut"
    );
    assert_eq!(control.snapshot().voices_active, 0);
}

#[test]
fn one_shot_ignores_release() {
    let (mut audio, control) = setup();
    control.assign(0, dc(2000, 1.0)).unwrap();
    control.trigger(0, 1.0);
    render(&mut audio, 100);
    control.release(0);
    let (l, _) = render(&mut audio, 500);
    assert!(l.iter().all(|&v| v == 1.0));
}

#[test]
fn loop_wraps_seamlessly_and_latches_until_a_second_press() {
    let (mut audio, control) = setup();
    let data: Vec<f32> = (0..50).flat_map(|n| [n as f32; 2]).collect();
    let sample = Arc::new(Sample::from_interleaved("saw", RATE, data).unwrap());
    control.assign(0, sample).unwrap();
    control.set_mode(0, PadMode::Loop);
    control.trigger(0, 1.0);
    let (l, _) = render(&mut audio, 500);
    for (i, &got) in l.iter().enumerate().skip(40) {
        assert_eq!(got, (i % 50) as f32, "frame {i}");
    }
    // Release, trigger=0 and a note-off style release do not stop a loop.
    control.release(0);
    let (l, _) = render(&mut audio, 400);
    assert!(l.iter().any(|&v| v > 0.0));
    assert!(control.snapshot().pads[0].playing);
    // The second press fades it out in 5 ms and does not start another voice.
    control.trigger(0, 1.0);
    render(&mut audio, 400);
    let (l, _) = render(&mut audio, 100);
    assert!(l.iter().all(|&v| v == 0.0));
    assert_eq!(control.snapshot().voices_active, 0);
    // A third press starts it again.
    control.trigger(0, 1.0);
    let (l, _) = render(&mut audio, 300);
    assert!(l.iter().any(|&v| v > 0.0));
    assert_eq!(control.snapshot().voices_active, 1);
    // stop_all ends a latched loop too.
    control.stop_all();
    render(&mut audio, 400);
    assert_eq!(control.snapshot().voices_active, 0);
}

#[test]
fn a_gate_override_on_a_loop_pad_is_a_gate_not_a_latch() {
    let (mut audio, control) = setup();
    control.assign(0, dc(48_000, 1.0)).unwrap();
    control.set_mode(0, PadMode::Loop);
    control.trigger_with(0, 1.0, Some(PadMode::Gate));
    render(&mut audio, 300);
    control.release(0);
    render(&mut audio, 400);
    assert_eq!(control.snapshot().voices_active, 0);
}

#[test]
fn loop_with_fractional_pitch_stays_continuous_across_the_seam() {
    let (mut audio, control) = setup();
    let data: Vec<f32> = (0..200)
        .flat_map(|n| {
            let v = (n as f32 / 200.0 * std::f32::consts::TAU).sin();
            [v, v]
        })
        .collect();
    control
        .assign(
            0,
            Arc::new(Sample::from_interleaved("sine", RATE, data).unwrap()),
        )
        .unwrap();
    control.set_mode(0, PadMode::Loop);
    control.set_pitch(0, 3.0);
    control.trigger(0, 1.0);
    let (l, _) = render(&mut audio, 2000);
    let max_step = l[64..]
        .windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .fold(0.0_f32, f32::max);
    assert!(max_step < 0.12, "no jump at the loop point: {max_step}");
}

#[test]
fn the_fifth_trigger_steals_the_oldest_voice() {
    let (mut audio, control) = setup();
    control.assign(0, dc(48_000, 0.1)).unwrap();
    for _ in 0..VOICES_PER_PAD {
        control.trigger(0, 1.0);
    }
    let (l, _) = render(&mut audio, 500);
    assert!((l[499] - 0.4).abs() < 1e-5, "four voices: {}", l[499]);
    assert_eq!(control.snapshot().voices_active, 4);
    control.trigger(0, 1.0);
    let (l, _) = render(&mut audio, 500);
    // Four sound after the stolen one has faded; level is back to four voices, never five.
    assert!((l[499] - 0.4).abs() < 1e-5, "still four: {}", l[499]);
    assert!(l.iter().all(|&v| v <= 0.5 + 1e-5), "never five voices loud");
    assert_eq!(control.snapshot().voices_active, 4);
    assert_eq!(control.dropped_commands(), 0);
}

#[test]
fn pads_are_independent() {
    let (mut audio, control) = setup();
    control.assign(0, dc(48_000, 0.1)).unwrap();
    control.assign(1, dc(48_000, 0.2)).unwrap();
    control.set_gain(1, 0.5);
    for _ in 0..6 {
        control.trigger(0, 1.0);
    }
    control.trigger(1, 1.0);
    render(&mut audio, 600);
    let (l, _) = render(&mut audio, 100);
    assert!((l[99] - (0.4 + 0.1)).abs() < 1e-5, "{}", l[99]);
    let snap = control.snapshot();
    assert!(snap.pads[0].playing && snap.pads[1].playing && !snap.pads[2].playing);
    control.set_mode(1, PadMode::Gate);
    control.release(0);
    control.release(1);
    control.trigger(2, 1.0); // empty pad: nothing
    render(&mut audio, 600);
    // Pad 0 is one-shot and keeps going; pad 1's voice was triggered as one-shot and does too.
    assert_eq!(control.snapshot().voices_active, 5);
}

#[test]
fn stop_all_fades_every_voice_out() {
    let (mut audio, control) = setup();
    control.assign(0, dc(48_000, 0.3)).unwrap();
    control.assign(3, dc(48_000, 0.3)).unwrap();
    control.trigger(0, 1.0);
    control.trigger(3, 1.0);
    render(&mut audio, 500);
    control.stop_all();
    render(&mut audio, 400);
    let (l, _) = render(&mut audio, 100);
    assert!(l.iter().all(|&v| v == 0.0));
    assert_eq!(control.snapshot().voices_active, 0);
}

#[test]
fn assign_replaces_for_new_triggers_without_cutting_the_playing_voice() {
    let (mut audio, control) = setup();
    let first = dc(48_000, 0.5);
    control.assign(0, Arc::clone(&first)).unwrap();
    control.trigger(0, 1.0);
    render(&mut audio, 300);
    control.assign(0, dc(48_000, 0.25)).unwrap();
    let (l, _) = render(&mut audio, 100);
    assert!(
        l.iter().all(|&v| v == 0.5),
        "the playing voice is untouched"
    );
    control.trigger(0, 1.0);
    let (l, _) = render(&mut audio, 100);
    assert!((l[99] - 0.75).abs() < 1e-5, "old voice plus the new sample");
}

#[test]
fn a_replaced_sample_is_freed_on_the_control_side() {
    let (mut audio, control) = setup();
    let first = dc(1000, 0.5);
    control.assign(0, Arc::clone(&first)).unwrap();
    render(&mut audio, 64);
    assert_eq!(Arc::strong_count(&first), 2, "test + pad");
    control.assign(0, dc(1000, 0.1)).unwrap();
    render(&mut audio, 64);
    // The audio side handed it back; only the control side can drop the last reference.
    assert_eq!(Arc::strong_count(&first), 2, "test + the retired ring");
    control.reclaim();
    assert_eq!(Arc::strong_count(&first), 1);

    // A finished voice's reference goes back the same way.
    control.assign(1, Arc::clone(&first)).unwrap();
    control.trigger(1, 1.0);
    render(&mut audio, 2000);
    render(&mut audio, 64);
    control.clear(1).unwrap();
    render(&mut audio, 64);
    control.reclaim();
    assert_eq!(Arc::strong_count(&first), 1);
    assert!(!control.snapshot().pads[1].loaded);
}

#[test]
fn a_full_command_ring_drops_triggers_and_counts_them() {
    let (mut audio, control) = setup();
    for _ in 0..RING_CAPACITY + 10 {
        control.trigger(0, 1.0);
    }
    // The ring holds RING_CAPACITY commands; the rest were counted, not blocked on.
    assert_eq!(control.dropped_commands(), 10);
    assert!(
        control.assign(1, dc(10, 0.1)).is_err(),
        "assign reports a full ring"
    );
    control.release(0);
    assert_eq!(control.dropped_commands(), 11);
    render(&mut audio, 128);
    assert_eq!(control.snapshot().dropped_cmds, 11);
    // The ring drained: commands work again.
    control.stop_all();
    assert_eq!(control.dropped_commands(), 11);
    control.assign(1, dc(10, 0.1)).unwrap();
}

#[test]
fn bad_pads_and_hostile_values_are_ignored() {
    let (mut audio, control) = setup();
    assert!(matches!(
        control.assign(PADS as u8, dc(10, 0.1)),
        Err(SamplerError::BadPad(_))
    ));
    control.trigger(200, 1.0);
    control.release(200);
    control.set_pitch(0, f32::INFINITY);
    control.set_pitch(0, 1.0e9);
    assert_eq!(control.snapshot().pads[0].pitch, MAX_PITCH_SEMITONES);
    control.set_master_gain(f32::NAN);
    control.set_master_gain(50.0);
    render(&mut audio, 64);
    assert_eq!(control.dropped_commands(), 0);
}

#[test]
fn param_ids_trigger_gate_volume_and_pitch() {
    use om_core::{ParamId, ParamTarget};
    let (mut audio, control) = setup();
    let set = |id: &str, v: f64| control.set_param(&id.parse::<ParamId>().unwrap(), v);
    control.assign(0, dc(48_000, 0.5)).unwrap();
    assert!(set("sampler.pad0.volume", 9.0));
    assert_eq!(control.snapshot().pads[0].gain, MAX_PAD_GAIN);
    assert!(set("sampler.pad0.volume", 1.0));
    assert!(set("sampler.pad0.pitch", -99.0));
    assert_eq!(control.snapshot().pads[0].pitch, -12.0);
    assert!(set("sampler.pad0.pitch", 0.0));
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for id in ["trigger", "gate", "volume", "pitch"] {
            assert!(!set(&format!("sampler.pad0.{id}"), bad), "{id} {bad}");
        }
    }
    // Trigger: velocity; 0 releases. The pad is one-shot, so the release changes nothing.
    assert!(set("sampler.pad0.trigger", 0.5));
    let (l, _) = render(&mut audio, 200);
    assert!((l[199] - 0.25).abs() < 1e-6);
    assert!(set("sampler.pad0.trigger", 0.0));
    let (l, _) = render(&mut audio, 200);
    assert!((l[199] - 0.25).abs() < 1e-6);
    // Gate: held while on, fades on off, whatever the pad's mode.
    assert!(set("sampler.pad0.trigger", 1e30));
    render(&mut audio, 100);
    assert!(set("sampler.pad0.gate", 1.0));
    render(&mut audio, 100);
    assert!(set("sampler.pad0.gate", 0.0));
    render(&mut audio, 600);
    let (l, _) = render(&mut audio, 100);
    assert!(
        (l[99] - 0.75).abs() < 1e-5,
        "only the one-shot voices remain: {}",
        l[99]
    );
}

#[test]
fn load_decodes_resamples_and_caps_size() {
    let dir = om_testing::fs::TempDir::new("om_sampler");
    let wav = dir.file("mono.wav");
    let mono: Vec<f32> = (0..22_050)
        .map(|n| (n as f32 / 22_050.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5)
        .collect();
    om_testing::wav::write_float_wav(&wav, 1, 22_050, &mono);
    let sample = Sample::load(&wav, 44_100.0).expect("loads");
    assert_eq!(sample.name(), "mono");
    assert_eq!(sample.frames(), 44_100);
    assert_eq!(sample.sample_rate(), 44_100.0);
    let d = sample.data();
    assert_eq!(d[2000], d[2001], "mono is duplicated to both sides");
    // Check the sine survives the resampling: sample 1000 sits at t = 1000/44100.
    let t = 1000.0_f32 / 44_100.0;
    let want = (t * 440.0 * std::f32::consts::TAU).sin() * 0.5;
    assert!((d[2000] - want).abs() < 2e-3, "{} vs {want}", d[2000]);

    // Unchanged rate: no resampling, exact frames.
    let same = Sample::load(&wav, 22_050.0).expect("loads");
    assert_eq!(same.frames(), 22_050);

    // Too long: refused before decoding it all.
    let long = dir.file("long.wav");
    om_testing::wav::write_float_wav(&long, 1, 8_000, &vec![0.0; 8_000 * 61]);
    assert!(matches!(
        Sample::load(&long, 8_000.0),
        Err(SamplerError::TooLarge)
    ));
    assert!(Sample::load(&dir.file("missing.wav"), 8_000.0).is_err());
    assert!(Sample::load(&wav, f64::NAN).is_err());
}

#[test]
fn from_interleaved_sanitizes_and_validates() {
    let s = Sample::from_interleaved("x", RATE, vec![f32::NAN, 1.0e-30, 0.5, f32::INFINITY, 0.3])
        .unwrap();
    assert_eq!(s.frames(), 2, "odd tail dropped");
    assert_eq!(&s.data()[..], &[0.0, 0.0, 0.5, 0.0]);
    assert!(matches!(
        Sample::from_interleaved("x", RATE, vec![]),
        Err(SamplerError::Empty)
    ));
    assert!(matches!(
        Sample::from_interleaved("x", 0.0, vec![0.0; 2]),
        Err(SamplerError::BadRate(_))
    ));
}

#[test]
fn registry_maps_ids_to_samples() {
    let reg = SampleRegistry::default();
    let s = dc(10, 0.1);
    let id = reg.register(Arc::clone(&s));
    assert!(Arc::ptr_eq(&reg.get(id).unwrap(), &s));
    reg.forget(id);
    assert!(reg.get(id).is_none());
}

/// Cost of a full sampler (every pad has all 4 voices looping, fractional pitch) per 256-frame
/// block. Run with `cargo test -p om-engine --release sampler_cost -- --ignored --nocapture`.
#[test]
#[ignore = "timing measurement, not a correctness test"]
fn sampler_cost() {
    fn per_block_us(voices_per_pad: usize, pitch: f32) -> f64 {
        let (mut audio, control) = setup();
        for pad in 0..PADS as u8 {
            control.assign(pad, ramp_sample(48_000)).unwrap();
            control.set_mode(pad, PadMode::Loop);
            control.set_pitch(pad, pitch);
            for _ in 0..voices_per_pad {
                control.trigger(pad, 0.7);
            }
        }
        let (mut l, mut r) = (vec![0.0_f32; 256], vec![0.0_f32; 256]);
        for _ in 0..8 {
            audio.process(&mut l, &mut r);
        }
        let blocks = 20_000;
        let start = std::time::Instant::now();
        for _ in 0..blocks {
            audio.process(std::hint::black_box(&mut l), &mut r);
            std::hint::black_box((&l, &r));
        }
        start.elapsed().as_secs_f64() * 1e6 / f64::from(blocks)
    }
    println!(
        "1 voice per pad, pitch 0:      {:.2} us / block",
        per_block_us(1, 0.0)
    );
    println!(
        "4 voices per pad, pitch 0:     {:.2} us / block",
        per_block_us(4, 0.0)
    );
    println!(
        "4 voices per pad, pitch +3.3:  {:.2} us / block",
        per_block_us(4, 3.3)
    );
}

fn audible(l: &[f32]) -> bool {
    l.iter().any(|&v| v != 0.0)
}

#[test]
fn two_hundred_one_shots_play_with_no_reclaim_at_all() {
    let (mut audio, control) = setup();
    control.assign(0, dc(50, 0.5)).unwrap();
    render(&mut audio, 64);
    for n in 0..200 {
        control.trigger(0, 1.0);
        let (l, _) = render(&mut audio, 128);
        assert!(audible(&l), "trigger {n} was silent");
    }
    assert_eq!(control.dropped_commands(), 0);
}

#[test]
fn one_shots_survive_a_slow_control_thread() {
    let (mut audio, control) = setup();
    control.assign(0, dc(50, 0.5)).unwrap();
    control.assign(1, dc(50, 0.25)).unwrap();
    render(&mut audio, 64);
    for n in 0..200 {
        control.trigger(0, 1.0);
        control.trigger(1, 1.0);
        let (l, _) = render(&mut audio, 128);
        assert!(audible(&l), "trigger {n} was silent");
        if n % 50 == 49 {
            control.reclaim();
        }
    }
    assert_eq!(control.dropped_commands(), 0);
}

#[test]
fn voices_of_replaced_samples_stay_reusable_when_the_retire_ring_is_full() {
    let (mut audio, control) = setup();
    audio.fill_retire_ring_for_test();
    let old = dc(50, 0.5);
    control.assign(0, Arc::clone(&old)).unwrap();
    for _ in 0..4 {
        control.trigger(0, 1.0);
    }
    render(&mut audio, 16);
    control.assign(0, dc(50, 0.25)).unwrap(); // `old` is now stale: ring full, nowhere to go
    for n in 0..200 {
        control.trigger(0, 1.0);
        let (l, _) = render(&mut audio, 128);
        assert!(audible(&l), "trigger {n} was silent");
    }
    assert_eq!(control.dropped_commands(), 0);
    // Nothing was freed on the audio side: the references are parked, then handed back once the
    // control side reclaims and the audio side retries.
    assert!(Arc::strong_count(&old) > 1);
    control.reclaim();
    render(&mut audio, 16);
    control.reclaim();
    assert_eq!(Arc::strong_count(&old), 1);
}

#[test]
fn a_release_lost_to_a_full_command_ring_still_ends_the_gate() {
    let (mut audio, control) = setup();
    control.assign(0, dc(48_000, 1.0)).unwrap();
    control.set_mode(0, PadMode::Gate);
    control.trigger(0, 1.0);
    let (l, _) = render(&mut audio, 500);
    assert_eq!(l[499], 1.0);
    for _ in 0..RING_CAPACITY {
        control.release(1);
    }
    control.release(0); // the ring is full: this one is lost
    assert_eq!(control.dropped_commands(), 1);
    render(&mut audio, 500);
    let (l, _) = render(&mut audio, 100);
    assert!(
        l.iter().all(|&v| v == 0.0),
        "the held flag released the voice"
    );
    assert_eq!(control.snapshot().voices_active, 0);
}

#[test]
fn trigger_and_release_in_one_block_is_a_short_audible_tap() {
    let (mut audio, control) = setup();
    control.assign(0, dc(48_000, 1.0)).unwrap();
    control.set_mode(0, PadMode::Gate);
    control.trigger(0, 1.0);
    control.release(0);
    let (l, _) = render(&mut audio, 1000);
    let peak = l.iter().copied().fold(0.0_f32, f32::max);
    assert!(peak > 0.99, "the tap reaches full level: {peak}");
    let sounding = l.iter().filter(|&&v| v > 0.0).count();
    assert!(sounding >= 32, "at least a ramp long: {sounding}");
    assert!(l[500..].iter().all(|&v| v == 0.0), "and then silent");
    assert_eq!(control.snapshot().voices_active, 0);
}

fn tone_wav(dir: &om_testing::fs::TempDir, name: &str, frames: usize) -> std::path::PathBuf {
    let path = dir.file(name);
    let mono: Vec<f32> = (0..frames).map(|n| (n as f32 * 0.05).sin() * 0.5).collect();
    om_testing::wav::write_float_wav(&path, 1, 8_000, &mono);
    path
}

#[test]
fn load_rejects_a_file_cut_off_mid_stream() {
    let dir = om_testing::fs::TempDir::new("om_sampler_trunc");
    let whole = tone_wav(&dir, "whole.wav", 8_000);
    let bytes = std::fs::read(&whole).unwrap();
    let cut = dir.file("cut.wav");
    std::fs::write(&cut, &bytes[..44 + (bytes.len() - 44) / 2]).unwrap();
    assert!(Sample::load(&whole, 8_000.0).is_ok());
    let err = Sample::load(&cut, 8_000.0).expect_err("a half file must not load as a short sample");
    assert!(matches!(err, SamplerError::Corrupt(_)), "{err:?}");
}

#[test]
fn an_unknown_length_is_decoded_to_the_end_under_the_cap() {
    use super::sample::decode_all;
    let dir = om_testing::fs::TempDir::new("om_sampler_unknown");
    let path = tone_wav(&dir, "stream.wav", 10_000);
    let open = || crate::deck::SourceReader::open(&path).unwrap();
    // declared 0: what a container without a frame count reports.
    let data = decode_all(&mut open(), 0, 1_000_000).expect("decodes to the end");
    assert_eq!(data.len() / 2, 10_000);
    assert!(matches!(
        decode_all(&mut open(), 0, 5_000),
        Err(SamplerError::TooLarge)
    ));
}

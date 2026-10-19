//! Sanity checks for the helpers themselves.

use crate::fs::TempDir;
use crate::level::{gain_to_db, peak, rms};
use crate::signals::{impulse_train, noise, sine, stereo_sine};
use crate::wait::wait_until;
use crate::wav::{deinterleave, interleave, read_wav, write_float_wav, write_pcm16_wav};
use std::cell::Cell;
use std::time::{Duration, Instant};

#[test]
fn float_wav_round_trips() {
    let dir = TempDir::new("om_testing_float");
    let path = dir.file("float.wav");
    let samples = [0.25_f32, -0.5, 0.75, -1.0, 0.0, 0.125];
    write_float_wav(&path, 2, 44_100, &samples);
    let back = read_wav(&path);
    assert_eq!(back.channels, 2);
    assert_eq!(back.sample_rate, 44_100);
    assert_eq!(back.samples, samples);
}

#[test]
fn pcm16_wav_round_trips_within_one_lsb() {
    let dir = TempDir::new("om_testing_pcm");
    let path = dir.file("pcm.wav");
    let samples = [0_i16, 16_384, -16_384, i16::MAX, i16::MIN];
    write_pcm16_wav(&path, 1, 8_000, &samples);
    let back = read_wav(&path);
    assert_eq!((back.channels, back.sample_rate), (1, 8_000));
    for (got, want) in back.samples.iter().zip(samples) {
        assert!(
            (got * 32_768.0 - f32::from(want)).abs() < 1e-3,
            "{got} vs {want}"
        );
    }
}

#[test]
fn interleave_and_deinterleave_are_inverse() {
    let l = [1.0_f32, 2.0, 3.0];
    let r = [-1.0_f32, -2.0, -3.0];
    let packed = interleave(&[&l[..], &r[..]]);
    assert_eq!(packed, [1.0, -1.0, 2.0, -2.0, 3.0, -3.0]);
    assert_eq!(deinterleave(&packed, 2), vec![l.to_vec(), r.to_vec()]);
}

#[test]
fn sine_has_the_requested_frequency() {
    // 1 kHz at 48 kHz for 48 000 frames is one second: about 1000 rising zero crossings.
    let x = sine(1_000.0, 48_000.0, 48_000);
    let crossings = x.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
    // The last crossing may fall one sample past the end depending on rounding at phase 2*pi.
    assert!((999..=1000).contains(&crossings), "{crossings} crossings");
    assert!((peak(&x) - 1.0).abs() < 1e-3);
    assert!((rms(&x) - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-3);
}

#[test]
fn stereo_sine_repeats_one_channel() {
    let x = stereo_sine(440.0, 0.5, 44_100.0, 100);
    assert_eq!(x.len(), 200);
    for frame in x.chunks_exact(2) {
        assert_eq!(frame[0], frame[1]);
    }
    assert!((peak(&x) - 0.5).abs() < 1e-3);
}

#[test]
fn noise_is_deterministic_and_bounded() {
    let a = noise(7, 1_000);
    assert_eq!(a, noise(7, 1_000));
    assert_ne!(a, noise(8, 1_000));
    assert!(a.iter().all(|&s| (-1.0..1.0).contains(&s)));
    assert!(rms(&a) > 0.4, "noise should not be near silent");
}

#[test]
fn impulse_train_places_clicks_on_the_period() {
    let x = impulse_train(4, 1.0, 10);
    assert_eq!(x, [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]);
    assert!(impulse_train(0, 1.0, 5).iter().all(|&s| s == 0.0));
}

#[test]
fn gain_to_db_maps_unity_and_half() {
    assert!(gain_to_db(1.0).abs() < 1e-12);
    assert!((gain_to_db(0.5) + 6.0206).abs() < 1e-3);
    assert!(gain_to_db(0.0).is_finite());
}

#[test]
fn wait_until_succeeds_when_the_condition_turns_true() {
    let polls = Cell::new(0);
    let ok = wait_until(Duration::from_secs(5), Duration::from_millis(1), || {
        polls.set(polls.get() + 1);
        polls.get() >= 3
    });
    assert!(ok);
    assert_eq!(polls.get(), 3);
}

#[test]
fn wait_until_times_out_and_returns_false() {
    let start = Instant::now();
    let ok = wait_until(Duration::from_millis(50), Duration::from_millis(5), || {
        false
    });
    assert!(!ok);
    let waited = start.elapsed();
    assert!(
        waited >= Duration::from_millis(50),
        "returned early: {waited:?}"
    );
    assert!(waited < Duration::from_secs(5), "overran: {waited:?}");
}

#[test]
fn wait_until_with_zero_timeout_checks_once() {
    let polls = Cell::new(0);
    assert!(!wait_until(Duration::ZERO, Duration::ZERO, || {
        polls.set(polls.get() + 1);
        false
    }));
    assert_eq!(polls.get(), 1);
}

#[test]
fn assert_eventually_passes_on_success() {
    let flag = std::sync::atomic::AtomicBool::new(false);
    std::thread::scope(|s| {
        s.spawn(|| {
            std::thread::sleep(Duration::from_millis(10));
            flag.store(true, std::sync::atomic::Ordering::Relaxed);
        });
        crate::assert_eventually!(Duration::from_secs(5), || flag
            .load(std::sync::atomic::Ordering::Relaxed));
    });
}

#[test]
#[should_panic(expected = "never happens")]
fn assert_eventually_panics_with_message() {
    crate::assert_eventually!(Duration::from_millis(20), || false, "never happens");
}

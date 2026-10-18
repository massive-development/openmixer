//! Port of tests/unit/DeckKeylockTest.cpp, one test per C++ case, same numbers and tolerances.
//! Generated audio only; nothing here opens an audio device.

use super::tests::{pump_until_buffered, write_float_wav, Fixture, RATE};
use super::{Deck, DeckAudio};
use om_core::ParameterTarget;
use std::f64::consts::TAU;
use std::path::{Path, PathBuf};
use std::time::Duration;

const TONE_HZ: f64 = 440.0;
const TONE_FRAMES: usize = 66_150; // 1.5 s
const BLOCK: usize = 256;

/// Writes a stereo 440 Hz tone at half scale: the C++ `writeTone` fixture.
fn write_tone(path: &Path) {
    let mut samples = Vec::with_capacity(TONE_FRAMES * 2);
    for i in 0..TONE_FRAMES {
        let v = (0.5 * (TAU * TONE_HZ * i as f64 / RATE).sin()) as f32;
        samples.push(v);
        samples.push(v);
    }
    write_float_wav(path, 2, &samples);
}

/// Renders `blocks` blocks of 256 frames (left channel), pausing 1 ms per block as the C++ test
/// does so the decoder keeps up. `before_block` runs ahead of each block.
fn render_left(
    deck: &Deck,
    audio: &mut DeckAudio,
    blocks: usize,
    mut before_block: impl FnMut(usize),
) -> Vec<f32> {
    let mut out = Vec::with_capacity(blocks * BLOCK);
    let mut l = [0.0_f32; BLOCK];
    let mut r = [0.0_f32; BLOCK];
    for b in 0..blocks {
        before_block(b);
        audio.render_stereo(&mut l, &mut r);
        out.extend_from_slice(&l);
        std::thread::sleep(Duration::from_millis(1));
    }
    let _ = deck;
    out
}

/// Frequency from rising zero crossings over `[from, to)`.
fn estimate_hz(x: &[f32], from: usize, to: usize) -> f64 {
    let mut crossings = Vec::new();
    for i in (from + 1)..to {
        if x[i - 1] < 0.0 && x[i] >= 0.0 {
            crossings.push((i - 1) as f64 + f64::from(x[i - 1]) / f64::from(x[i - 1] - x[i]));
        }
    }
    if crossings.len() < 2 {
        return 0.0;
    }
    let span = crossings[crossings.len() - 1] - crossings[0];
    RATE * (crossings.len() - 1) as f64 / span
}

fn longest_silent_run(x: &[f32], from: usize) -> usize {
    let (mut run, mut best) = (0_usize, 0_usize);
    for &v in &x[from..] {
        run = if v.abs() < 1e-4 { run + 1 } else { 0 };
        best = best.max(run);
    }
    best
}

/// The C++ `KeylockFixture`: a deck prepared at 44.1 kHz with the tone loaded and buffered.
fn keylock_fixture(fixture: &Fixture) -> (Deck, DeckAudio, PathBuf) {
    let tone = fixture.path("tone.wav");
    write_tone(&tone);
    let (deck, mut audio) = Deck::new(1 << 16);
    audio.prepare(RATE, 256);
    deck.load(&tone).expect("tone loads");
    assert!(pump_until_buffered(&deck, &mut audio, 8192, 0));
    (deck, audio, tone)
}

#[test]
fn keylock_holds_pitch_when_rate_changes() {
    let fixture = Fixture::new();
    let (deck, mut audio, _tone) = keylock_fixture(&fixture);
    deck.set_keylock(true);
    deck.set_rate(1.25);
    deck.play();
    let out = render_left(&deck, &mut audio, 120, |_| {});
    assert!((estimate_hz(&out, 8192, out.len()) - TONE_HZ).abs() < TONE_HZ * 0.01);
    assert_eq!(deck.underrun_count(), 0);
    // Tempo still follows rate: ~1.25 source frames per output frame.
    let advance = deck.position_frames() as f64 / out.len() as f64;
    assert!((advance - 1.25).abs() < 0.1, "advance {advance}");
}

#[test]
fn without_keylock_rate_shifts_pitch() {
    let fixture = Fixture::new();
    let (deck, mut audio, _tone) = keylock_fixture(&fixture);
    deck.set_rate(1.25);
    deck.play();
    let out = render_left(&deck, &mut audio, 60, |_| {});
    assert!((estimate_hz(&out, 1024, out.len()) - TONE_HZ * 1.25).abs() < TONE_HZ * 0.01);
}

#[test]
fn toggling_keylock_while_playing_stays_audible_and_finite() {
    let fixture = Fixture::new();
    let (deck, mut audio, _tone) = keylock_fixture(&fixture);
    deck.set_rate(1.1);
    deck.play();
    let out = render_left(&deck, &mut audio, 150, |block| {
        deck.set_keylock((40..100).contains(&block));
    });
    assert!(out.iter().all(|v| v.is_finite()));
    assert!(
        longest_silent_run(&out, 256) < 64,
        "no audible dropout when switching paths"
    );
    assert_eq!(deck.underrun_count(), 0);
    assert!(deck.set_parameter("keylock", 1.0));
    assert!(deck.is_keylock());
}

#[test]
fn keylock_compensates_a_device_rate_mismatch() {
    let fixture = Fixture::new();
    let tone = fixture.path("tone.wav");
    write_tone(&tone);
    let (fast, mut audio) = Deck::new(1 << 16);
    audio.prepare(48_000.0, 256);
    fast.load(&tone).expect("tone loads");
    assert!(pump_until_buffered(&fast, &mut audio, 8192, 0));
    fast.set_keylock(true);
    fast.play();
    let out = render_left(&fast, &mut audio, 120, |_| {});
    let hz = estimate_hz(&out, 8192, out.len()) * 48_000.0 / RATE;
    assert!((hz - TONE_HZ).abs() < TONE_HZ * 0.01, "pitch {hz} Hz");
}

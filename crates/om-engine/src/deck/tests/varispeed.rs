//! The rubato varispeed path (#241): quality against linear interpolation, the compensated
//! position, loop wraps, path switches, seeks, and two decks at different rates. Fixtures are
//! generated in-test; a ramp has an exact answer under any band-limited interpolator, so it
//! measures the position without trusting the code under test.

use crate::deck::{Deck, DeckAudio, VarispeedQuality};
use crate::quantizer::Loop;
use om_testing::fs::TempDir;
use om_testing::wait::{wait_until, DEFAULT_POLL};
use om_testing::wav::write_float_wav;
use std::time::Duration;

const RATE: f64 = 44_100.0;
const BLOCK: usize = 256;

struct Rig {
    deck: Deck,
    audio: DeckAudio,
    _fixture: TempDir,
}

/// A deck playing a stereo file made of `frames` frames of `sample(n)` (same on both channels).
fn rig_with(frames: usize, quality: VarispeedQuality, sample: impl Fn(usize) -> f32) -> Rig {
    let fixture = TempDir::new("om_engine");
    let wav = fixture.file("fixture.wav");
    let samples: Vec<f32> = (0..frames)
        .flat_map(|n| {
            let v = sample(n);
            [v, v]
        })
        .collect();
    write_float_wav(&wav, 2, RATE as u32, &samples);
    let (deck, mut audio) = Deck::new(1 << 16);
    audio.set_varispeed_quality(quality);
    audio.prepare(RATE, BLOCK);
    deck.load(&wav).expect("fixture loads");
    assert!(
        super::pump_until_buffered(&deck, &mut audio, 4096, 0),
        "decoder did not buffer in time"
    );
    Rig {
        deck,
        audio,
        _fixture: fixture,
    }
}

fn sine_rig(freq: f64, frames: usize) -> Rig {
    rig_with(frames, VarispeedQuality::Sinc, move |n| {
        (0.5 * (std::f64::consts::TAU * freq * n as f64 / RATE).sin()) as f32
    })
}

fn ramp_rig(frames: usize) -> Rig {
    rig_with(frames, VarispeedQuality::Sinc, move |n| {
        n as f32 / frames as f32
    })
}

impl Rig {
    /// Renders one block once the decoder has buffered enough for it.
    fn block(&mut self) -> Vec<f32> {
        assert!(
            wait_until(Duration::from_secs(10), DEFAULT_POLL, || self
                .deck
                .buffered_frames()
                >= BLOCK * 3),
            "decoder stalled"
        );
        let mut l = vec![0.0_f32; BLOCK];
        let mut r = vec![0.0_f32; BLOCK];
        self.audio.render_stereo(&mut l, &mut r);
        l
    }
}

/// Relative residual, in dB, of `got` after a least-squares fit of a sine of `w` rad/sample.
fn residual_db(got: &[f32], w: f64) -> f64 {
    let (mut ss, mut cc, mut sc, mut sy, mut cy, mut yy) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    for (i, &v) in got.iter().enumerate() {
        let (sn, cs) = (w * i as f64).sin_cos();
        let y = f64::from(v);
        ss += sn * sn;
        cc += cs * cs;
        sc += sn * cs;
        sy += sn * y;
        cy += cs * y;
        yy += y * y;
    }
    let det = ss * cc - sc * sc;
    let (a, b) = ((sy * cc - cy * sc) / det, (cy * ss - sy * sc) / det);
    let resid = (yy - (a * sy + b * cy)).max(0.0);
    10.0 * (resid / yy).max(1e-30).log10()
}

#[test]
fn sinc_leaves_a_residual_below_minus_80_db_at_each_ratio() {
    for step in [0.9, 1.0, 1.08] {
        let mut rig = sine_rig(1000.0, 60_000);
        // Unity normally bypasses to the bit-exact linear path; this test wants the filter.
        rig.audio.force_varispeed_at_unity(true);
        rig.deck.set_rate(step);
        rig.deck.play();
        let mut out = Vec::new();
        for _ in 0..80 {
            out.extend(rig.block());
        }
        let settled = &out[2000..];
        let w = std::f64::consts::TAU * 1000.0 * step / RATE;
        let db = residual_db(settled, w);
        assert!(db < -80.0, "step {step}: residual {db:.1} dB");
    }
}

#[test]
fn linear_quality_is_the_old_behaviour_and_is_worse() {
    let mut rig = rig_with(60_000, VarispeedQuality::Linear, |n| {
        (0.5 * (std::f64::consts::TAU * 1000.0 * n as f64 / RATE).sin()) as f32
    });
    rig.deck.set_rate(1.08);
    rig.deck.play();
    let mut out = Vec::new();
    for _ in 0..80 {
        out.extend(rig.block());
    }
    let db = residual_db(&out[2000..], std::f64::consts::TAU * 1000.0 * 1.08 / RATE);
    assert!(db > -80.0, "linear should not reach -80 dB, got {db:.1}");
}

/// Position published after each block against the source frame read off a ramp at the last
/// output sample. The ramp is exact under the filter, so this is the audible source frame.
fn assert_position_tracks(step: f64, seconds: f64) {
    let frames = ((seconds * RATE * step) as usize) + 20_000;
    let mut rig = ramp_rig(frames);
    rig.deck.set_rate(step);
    rig.deck.play();
    let blocks = (seconds * RATE / BLOCK as f64) as usize;
    let mut worst = 0.0_f64;
    for b in 0..blocks {
        let out = rig.block();
        if b < 3 {
            continue;
        }
        let audible = f64::from(*out.last().unwrap()) * frames as f64;
        let published = rig.deck.position_frames() as f64;
        worst = worst.max((audible - published).abs());
    }
    assert!(
        worst < 1.5,
        "step {step}: published position was {worst:.2} source frames from the audible frame"
    );
}

#[test]
fn position_tracks_the_audible_frame_over_ten_seconds() {
    assert_position_tracks(1.08, 10.0);
    assert_position_tracks(0.9, 10.0);
}

/// Largest sample-to-sample step of a sine of amplitude 0.5 read at `step` source frames per
/// output frame.
fn steady_delta(freq: f64, step: f64) -> f32 {
    (0.5 * std::f64::consts::TAU * freq * step / RATE) as f32
}

fn max_delta(samples: &[f32]) -> f32 {
    samples
        .windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .fold(0.0, f32::max)
}

#[test]
fn a_loop_wrap_does_not_click_and_the_position_wraps_with_it() {
    // 441 Hz has a 100-frame period; the loop is 44 periods, so the source is seamless at the wrap.
    let mut rig = sine_rig(441.0, 120_000);
    let (start, end) = (10_000, 14_400);
    assert!(rig.deck.set_loop(Loop { start, end }));
    rig.deck.seek(start + 1);
    assert!(super::pump_until_buffered(
        &rig.deck,
        &mut rig.audio,
        4096,
        start + 1
    ));
    rig.deck.set_rate(1.08);
    rig.deck.play();
    let mut out = Vec::new();
    let mut positions = Vec::new();
    for _ in 0..120 {
        out.extend(rig.block());
        positions.push(rig.deck.position_frames());
    }
    let settled = &out[1000..];
    let limit = steady_delta(441.0, 1.08) * 1.05;
    assert!(
        max_delta(settled) <= limit,
        "click across the wrap: {} > {limit}",
        max_delta(settled)
    );
    // 120 blocks at 1.08 cover 33k source frames: at least seven wraps of the 4400-frame loop.
    let wraps = positions.windows(2).filter(|w| w[1] < w[0]).count();
    assert!(wraps >= 6, "expected several wraps, saw {wraps}");
    for &p in &positions[4..] {
        assert!(
            (start..end).contains(&p),
            "position {p} left the loop {start}..{end}"
        );
    }
}

#[test]
fn switching_between_the_linear_and_rubato_paths_does_not_click() {
    let mut rig = sine_rig(1000.0, 200_000);
    rig.deck.play();
    // Unity (linear, bit-exact), then a pitch move (rubato), then out of range (linear), then back.
    let plan = [(1.0, 20), (1.08, 40), (3.0, 20), (1.08, 20), (1.0, 20)];
    let mut out = Vec::new();
    let mut worst_step = 1.0_f64;
    for (rate, blocks) in plan {
        rig.deck.set_rate(rate);
        worst_step = worst_step.max(rate);
        for _ in 0..blocks {
            out.extend(rig.block());
        }
    }
    let limit = steady_delta(1000.0, worst_step) * 1.5;
    assert!(
        max_delta(&out[64..]) <= limit,
        "click on a path switch: {} > {limit}",
        max_delta(&out[64..])
    );
}

#[test]
fn a_seek_while_playing_has_no_silent_gap_and_starts_at_the_target() {
    // 441 Hz: sample n is 0.5 * sin(2 pi n / 100), so frame 100_025 is the positive peak.
    let mut rig = sine_rig(441.0, 200_000);
    rig.deck.set_rate(1.08);
    rig.deck.play();
    for _ in 0..10 {
        rig.block();
    }
    rig.deck.seek(100_025);
    // Wait for the handshake and the new ring, rendering as a device would.
    // Render as a device would until the new position is up and the ring has refilled; blocks
    // before that are the decoder catching up and are silent by design.
    let mut out = Vec::new();
    let mut after_seek = false;
    for _ in 0..2000 {
        let block = rig.block();
        after_seek |= rig.deck.position_frames() >= 100_025;
        if after_seek {
            out.extend(block);
            if out.iter().any(|&v| v != 0.0) && out.len() >= 4 * BLOCK {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let start = out
        .iter()
        .position(|&v| v != 0.0)
        .expect("audio resumed after the seek");
    let out = &out[start..];
    assert!(
        out[..8].iter().any(|&v| v.abs() > 0.1),
        "the signal must start at the target, not ramp in from silence"
    );
    // No run of exact zeros after the start: a dropped block, not a zero crossing.
    let mut run = 0;
    let mut longest = 0;
    for &v in out {
        run = if v == 0.0 { run + 1 } else { 0 };
        longest = longest.max(run);
    }
    assert!(longest < 16, "{longest} zero samples in a row after a seek");
}

#[test]
fn two_decks_at_different_rates_report_comparable_positions() {
    // Deck A at unity (linear, no delay), deck B at 1.1 (rubato, 64-frame delay). After the same
    // number of output frames their positions must match rate * frames, so sync compares true
    // audible positions rather than input-side ones.
    let mut a = ramp_rig(300_000);
    let mut b = ramp_rig(300_000);
    a.deck.set_rate(1.0);
    b.deck.set_rate(1.1);
    a.deck.play();
    b.deck.play();
    let blocks = 400;
    for _ in 0..blocks {
        a.block();
        b.block();
    }
    let out_frames = (blocks * BLOCK) as f64;
    let pa = a.deck.position_frames() as f64;
    let pb = b.deck.position_frames() as f64;
    assert!((pa - (out_frames - 1.0)).abs() < 2.0, "unity deck at {pa}");
    assert!(
        (pb - (out_frames * 1.1 - 1.1)).abs() < 2.5,
        "rubato deck at {pb}, expected about {}",
        out_frames * 1.1
    );
}

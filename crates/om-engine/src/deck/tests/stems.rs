//! Stem mode: four stems that sum to a known mix are loaded next to the main file, and every
//! behaviour is compared against plain playback of the main file (or of a mix without a stem).

use super::{pump_until_buffered, RATE};
use crate::deck::{Deck, DeckAudio, DeckError};
use crate::quantizer::Loop;
use om_testing::fs::TempDir;
use om_testing::wait::{wait_until, DEFAULT_POLL};
use om_testing::wav::{write_float_wav, write_pcm16_wav};
use std::path::Path;
use std::sync::atomic::Ordering::Relaxed;
use std::time::Duration;

pub(crate) const STEM_FRAMES: usize = 30_000;
const BLOCK: usize = 256;
const NAMES: [&str; 4] = ["drums.wav", "bass.wav", "vocals.wav", "other.wav"];
const AMP: [f32; 4] = [0.20, 0.16, 0.12, 0.10];
const HZ: [f32; 4] = [60.0, 220.0, 880.0, 3_000.0];

/// One stem sample: distinct tone per stem, right channel phase-shifted.
pub(crate) fn stem_sample(stem: usize, channel: usize, n: usize) -> f32 {
    let t = n as f32 / RATE as f32;
    AMP[stem] * (std::f32::consts::TAU * HZ[stem] * t + channel as f32 * 0.7).sin()
}

/// Interleaved stereo samples of one stem.
fn stem_samples(stem: usize, frames: usize) -> Vec<f32> {
    (0..frames)
        .flat_map(|n| [stem_sample(stem, 0, n), stem_sample(stem, 1, n)])
        .collect()
}

/// Mix of the stems that are not in `skip`, summed in stem order (as the mixer does).
fn mix_samples(frames: usize, skip: &[usize]) -> Vec<f32> {
    (0..frames)
        .flat_map(|n| {
            [0, 1].map(|c| {
                (0..4)
                    .filter(|s| !skip.contains(s))
                    .fold(0.0_f32, |acc, s| acc + stem_sample(s, c, n))
            })
        })
        .collect()
}

/// Writes `main.wav` (the full mix) and the four stems into `dir`; stems as float WAV.
pub(crate) fn write_stem_set(dir: &Path, frames: usize) {
    write_float_wav(
        &dir.join("main.wav"),
        2,
        RATE as u32,
        &mix_samples(frames, &[]),
    );
    for (stem, name) in NAMES.iter().enumerate() {
        write_float_wav(&dir.join(name), 2, RATE as u32, &stem_samples(stem, frames));
    }
}

fn fixture() -> TempDir {
    let dir = TempDir::new("om_engine_stems");
    write_stem_set(dir.path(), STEM_FRAMES);
    dir
}

/// A prepared deck with the main file loaded, buffered at frame 0.
fn new_deck(dir: &TempDir) -> (Deck, DeckAudio) {
    let (deck, mut audio) = Deck::new(1 << 16);
    audio.prepare(RATE, BLOCK);
    deck.load(&dir.file("main.wav")).expect("main loads");
    assert!(pump_until_buffered(&deck, &mut audio, BLOCK * 4, 0));
    (deck, audio)
}

fn settle(deck: &Deck, audio: &mut DeckAudio, position: i64) {
    assert!(
        pump_until_buffered(deck, audio, BLOCK * 4, position),
        "deck did not settle at {position}"
    );
}

/// Renders `blocks` device-paced blocks; `before` runs ahead of each.
fn render(
    deck: &Deck,
    audio: &mut DeckAudio,
    blocks: usize,
    mut before: impl FnMut(&Deck, usize),
) -> (Vec<f32>, Vec<f32>) {
    let (mut out_l, mut out_r) = (Vec::new(), Vec::new());
    let mut l = [0.0_f32; BLOCK];
    let mut r = [0.0_f32; BLOCK];
    for b in 0..blocks {
        before(deck, b);
        assert!(
            // A ring parked at end of stream (a loop slot reaching the end of the file) is empty
            // by design: the decoder has nothing left to produce, and the slot feeds playback.
            wait_until(Duration::from_secs(10), DEFAULT_POLL, || deck
                .buffered_frames()
                >= BLOCK * 3
                || (deck.shared.end_of_stream.load(Relaxed)
                    && deck.handshake_settled())),
            "decoder stalled at block {b}: {}",
            stall_state(deck)
        );
        audio.render_stereo(&mut l, &mut r);
        out_l.extend_from_slice(&l);
        out_r.extend_from_slice(&r);
    }
    (out_l, out_r)
}

/// Handshake counters, for the stall message.
fn stall_state(deck: &Deck) -> String {
    let s = &deck.shared;
    format!(
        "eos={} req_epoch={} ack_epoch={} ack_seq={} drained_seq={} written={} read={} stride={}",
        s.end_of_stream.load(Relaxed),
        s.request_epoch.load(Relaxed),
        s.ack_epoch.load(Relaxed),
        s.ack_seq.load(Relaxed),
        s.drained_seq.load(Relaxed),
        s.written_samples.load(Relaxed),
        s.read_samples.load(Relaxed),
        s.ack_stride.load(Relaxed),
    )
}

fn assert_close(a: &[f32], b: &[f32], tol: f32, what: &str) {
    assert_eq!(a.len(), b.len(), "{what}: length");
    for (i, (x, y)) in a.iter().zip(b).enumerate() {
        assert!(x.is_finite() && y.is_finite(), "{what}: non-finite at {i}");
        assert!((x - y).abs() <= tol, "{what}: sample {i}: {x} vs {y}");
    }
}

fn energy(x: &[f32]) -> f32 {
    x.iter().map(|v| v * v).sum::<f32>()
}

#[test]
fn unity_stem_render_matches_the_plain_render() {
    let dir = fixture();
    let (plain, mut plain_audio) = new_deck(&dir);
    plain.play();
    let (pl, pr) = render(&plain, &mut plain_audio, 40, |_, _| {});

    let (deck, mut audio) = new_deck(&dir);
    deck.load_stems(dir.path()).expect("stems load");
    assert!(deck.has_stems());
    settle(&deck, &mut audio, 0);
    deck.play();
    let (sl, sr) = render(&deck, &mut audio, 40, |_, _| {});
    assert!(energy(&sl) > 1.0, "audible");
    assert_close(&sl, &pl, 1e-4, "left");
    assert_close(&sr, &pr, 1e-4, "right");
    assert_eq!(deck.underrun_count(), 0);
}

#[test]
fn pcm16_stems_load_and_play() {
    let dir = fixture();
    for (stem, name) in NAMES.iter().enumerate() {
        let q: Vec<i16> = stem_samples(stem, STEM_FRAMES)
            .iter()
            .map(|v| (v * 32_767.0).round() as i16)
            .collect();
        write_pcm16_wav(&dir.file(name), 2, RATE as u32, &q);
    }
    let (plain, mut plain_audio) = new_deck(&dir);
    plain.play();
    let (pl, _) = render(&plain, &mut plain_audio, 20, |_, _| {});
    let (deck, mut audio) = new_deck(&dir);
    deck.load_stems(dir.path()).expect("pcm16 stems load");
    settle(&deck, &mut audio, 0);
    deck.play();
    let (sl, _) = render(&deck, &mut audio, 20, |_, _| {});
    assert_close(&sl, &pl, 4e-4, "pcm16 left");
}

#[test]
fn muting_drums_equals_a_mix_without_drums() {
    let dir = fixture();
    write_float_wav(
        &dir.file("nodrums.wav"),
        2,
        RATE as u32,
        &mix_samples(STEM_FRAMES, &[0]),
    );
    let (reference, mut ref_audio) = Deck::new(1 << 16);
    ref_audio.prepare(RATE, BLOCK);
    reference.load(&dir.file("nodrums.wav")).unwrap();
    settle(&reference, &mut ref_audio, 0);
    reference.play();
    let (rl, rr) = render(&reference, &mut ref_audio, 30, |_, _| {});

    let (deck, mut audio) = new_deck(&dir);
    deck.stem_control().set_muted(0, true);
    deck.load_stems(dir.path()).unwrap();
    settle(&deck, &mut audio, 0);
    deck.play();
    let (sl, sr) = render(&deck, &mut audio, 30, |_, _| {});
    assert_close(&sl, &rl, 1e-4, "left");
    assert_close(&sr, &rr, 1e-4, "right");
}

#[test]
fn live_mute_fades_instead_of_stepping() {
    let dir = fixture();
    let (deck, mut audio) = new_deck(&dir);
    deck.load_stems(dir.path()).unwrap();
    settle(&deck, &mut audio, 0);
    deck.play();
    let control = deck.stem_control();
    let (l, _) = render(&deck, &mut audio, 24, |_, b| {
        if b == 4 {
            for s in 0..4 {
                control.set_muted(s, true);
            }
        }
    });
    assert!(energy(&l[..BLOCK * 4]) > 1.0);
    // Fully silent a few blocks later, and no sample-to-sample jump beyond the signal's own slope
    // plus the glide.
    assert!(
        l[BLOCK * 16..].iter().all(|v| v.abs() < 1e-4),
        "settles to silence"
    );
    let max_step = l
        .windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .fold(0.0, f32::max);
    assert!(max_step < 0.1, "no click, max step {max_step}");
}

/// Same transport script in either mode; returns everything rendered.
fn transport_script(deck: &Deck, audio: &mut DeckAudio) -> (Vec<f32>, Vec<f32>) {
    let mut out = (Vec::new(), Vec::new());
    let mut append = |chunk: (Vec<f32>, Vec<f32>)| {
        out.0.extend(chunk.0);
        out.1.extend(chunk.1);
    };
    deck.seek(5_000);
    settle(deck, audio, 5_000);
    assert!(deck.set_loop(Loop {
        start: 1_024,
        end: 3_072
    }));
    deck.set_hotcue(0, 9_000);
    deck.seek(1_500);
    settle(deck, audio, 1_500);
    assert!(wait_until(Duration::from_secs(10), DEFAULT_POLL, || deck
        .is_loop_prefetched()));
    deck.play();
    append(render(deck, audio, 30, |_, _| {}));
    deck.pause();
    deck.exit_loop();
    deck.trigger_hotcue(0);
    settle(deck, audio, 9_000);
    deck.play();
    append(render(deck, audio, 20, |_, _| {}));
    out
}

/// Plays into a loop whose prefetch slot runs to the end of the file, so the wrap parks the ring
/// at end of stream, and waits until the audio thread has drained it.
fn park_ring_at_end_of_stream(deck: &Deck, audio: &mut DeckAudio) {
    deck.seek(1_500);
    settle(deck, audio, 1_500);
    assert!(deck.set_loop(Loop {
        start: 1_024,
        end: 3_072
    }));
    assert!(wait_until(Duration::from_secs(10), DEFAULT_POLL, || deck
        .is_loop_prefetched()));
    deck.play();
    let mut l = [0.0_f32; BLOCK];
    let mut r = [0.0_f32; BLOCK];
    // Real-time pacing: the wrap is requested, the decoder repositions the ring to the slot's
    // end (end of file here) and the next render drains it.
    assert!(wait_until(Duration::from_secs(10), DEFAULT_POLL, || {
        audio.render_stereo(&mut l, &mut r);
        deck.handshake_settled() && deck.buffered_frames() == 0 && deck.position_frames() < 3_072
    }));
}

/// A ring parked at end of stream behind a loop slot is empty by design, not a stalled decoder.
#[test]
fn render_does_not_wait_for_a_ring_parked_at_end_of_stream() {
    let dir = fixture();
    let (plain, mut plain_audio) = new_deck(&dir);
    park_ring_at_end_of_stream(&plain, &mut plain_audio);
    let (pl, _) = render(&plain, &mut plain_audio, 4, |_, _| {});
    assert!(energy(&pl) > 0.0, "the loop keeps playing from its slot");

    let (deck, mut audio) = new_deck(&dir);
    deck.load_stems(dir.path()).unwrap();
    settle(&deck, &mut audio, 0);
    park_ring_at_end_of_stream(&deck, &mut audio);
    let (sl, _) = render(&deck, &mut audio, 4, |_, _| {});
    assert!(energy(&sl) > 0.0);
    assert_eq!(deck.underrun_count(), 0);
}

#[test]
fn seek_loop_and_hotcue_land_on_the_same_frames_as_plain_mode() {
    let dir = fixture();
    let (plain, mut plain_audio) = new_deck(&dir);
    let (pl, pr) = transport_script(&plain, &mut plain_audio);

    let (deck, mut audio) = new_deck(&dir);
    deck.load_stems(dir.path()).unwrap();
    settle(&deck, &mut audio, 0);
    let (sl, sr) = transport_script(&deck, &mut audio);
    assert!(energy(&sl) > 1.0);
    assert_close(&sl, &pl, 1e-4, "left");
    assert_close(&sr, &pr, 1e-4, "right");
}

#[test]
fn rate_changes_every_block_match_plain_mode() {
    let dir = fixture();
    let script = |deck: &Deck, audio: &mut DeckAudio| {
        deck.play();
        render(deck, audio, 40, |d, b| {
            d.set_rate(1.0 + 0.2 * (b as f64 * 0.37).sin());
        })
    };
    let (plain, mut plain_audio) = new_deck(&dir);
    let (pl, _) = script(&plain, &mut plain_audio);
    let (deck, mut audio) = new_deck(&dir);
    deck.load_stems(dir.path()).unwrap();
    settle(&deck, &mut audio, 0);
    let (sl, _) = script(&deck, &mut audio);
    assert_close(&sl, &pl, 1e-4, "left");
}

#[test]
fn keylock_with_stems_is_finite_audible_and_matches_plain_level() {
    let dir = fixture();
    let script = |deck: &Deck, audio: &mut DeckAudio| {
        deck.set_keylock(true);
        deck.set_rate(1.05);
        deck.play();
        render(deck, audio, 60, |_, _| {})
    };
    let (plain, mut plain_audio) = new_deck(&dir);
    let (pl, _) = script(&plain, &mut plain_audio);
    let (deck, mut audio) = new_deck(&dir);
    deck.load_stems(dir.path()).unwrap();
    settle(&deck, &mut audio, 0);
    let (sl, sr) = script(&deck, &mut audio);
    assert_eq!(sl.len(), 60 * BLOCK);
    assert!(sl.iter().chain(&sr).all(|v| v.is_finite()), "no NaN");
    let (pe, se) = (energy(&pl[BLOCK * 20..]), energy(&sl[BLOCK * 20..]));
    assert!(se > 1.0, "audible");
    assert!(
        (se / pe - 1.0).abs() < 0.05,
        "same level as plain key-lock: {se} vs {pe}"
    );
    assert_close(&sl, &pl, 1e-3, "key-locked stems vs key-locked plain");
}

#[test]
fn underrun_in_stem_mode_behaves_like_plain_mode() {
    let dir = fixture();
    let (deck, mut audio) = Deck::new(1 << 12);
    audio.prepare(RATE, BLOCK);
    deck.load(&dir.file("main.wav")).unwrap();
    deck.load_stems(dir.path()).unwrap();
    assert!(pump_until_buffered(&deck, &mut audio, 2048, 0));
    deck.set_rate(8.0);
    deck.play();
    let mut l = vec![1.0_f32; 4096];
    let mut r = vec![1.0_f32; 4096];
    audio.render_stereo(&mut l, &mut r);
    assert!(deck.underrun_count() >= 1, "underrun counted");
    assert_eq!(l[4095], 0.0, "tail is silence");
    assert!(l.iter().any(|v| *v != 0.0), "head had audio");
    assert!(deck.is_playing(), "an underrun is not end-of-file");
}

#[test]
fn failed_load_stems_leaves_the_deck_untouched() {
    let dir = fixture();
    let (deck, mut audio) = new_deck(&dir);
    assert!(matches!(
        Deck::new(1 << 12).0.load_stems(dir.path()),
        Err(DeckError::NotLoaded)
    ));

    // Missing file.
    let bad = TempDir::new("om_engine_stems_bad");
    write_stem_set(bad.path(), STEM_FRAMES);
    std::fs::remove_file(bad.file("vocals.wav")).unwrap();
    assert!(matches!(
        deck.load_stems(bad.path()),
        Err(DeckError::Missing(_))
    ));
    // Length mismatch.
    write_float_wav(
        &bad.file("vocals.wav"),
        2,
        RATE as u32,
        &stem_samples(2, STEM_FRAMES - 100),
    );
    assert!(matches!(
        deck.load_stems(bad.path()),
        Err(DeckError::LengthMismatch {
            name: "vocals.wav",
            ..
        })
    ));
    // Rate mismatch.
    write_float_wav(
        &bad.file("vocals.wav"),
        2,
        48_000,
        &stem_samples(2, STEM_FRAMES),
    );
    assert!(matches!(
        deck.load_stems(bad.path()),
        Err(DeckError::RateMismatch { .. })
    ));
    // A few frames off is within tolerance.
    write_float_wav(
        &bad.file("vocals.wav"),
        2,
        RATE as u32,
        &stem_samples(2, STEM_FRAMES - 8),
    );
    assert!(deck.load_stems(bad.path()).is_ok());
    deck.clear_stems();

    assert!(!deck.has_stems());
    assert!(deck.is_loaded());
    assert_eq!(deck.length_frames(), STEM_FRAMES as i64);
    settle(&deck, &mut audio, 0);
    deck.play();
    let (l, _) = render(&deck, &mut audio, 10, |_, _| {});
    let expected = mix_samples(10 * BLOCK, &[]);
    let left: Vec<f32> = expected.chunks_exact(2).map(|f| f[0]).collect();
    assert_close(&l, &left, 1e-4, "plain playback after the failed loads");
}

#[test]
fn clear_stems_continues_from_the_same_position() {
    let dir = fixture();
    let (deck, mut audio) = new_deck(&dir);
    deck.load_stems(dir.path()).unwrap();
    settle(&deck, &mut audio, 0);
    deck.play();
    render(&deck, &mut audio, 10, |_, _| {});
    deck.pause();
    let p = deck.position_frames();
    assert!(p > 2_000);
    deck.clear_stems();
    assert!(!deck.has_stems());
    settle(&deck, &mut audio, p);
    deck.play();
    let (l, _) = render(&deck, &mut audio, 4, |_, _| {});
    let at = |n: usize| (0..4).fold(0.0_f32, |a, s| a + stem_sample(s, 0, n));
    let n = p as usize;
    assert!(
        (l[0] - at(n)).abs() < 1e-4 || (l[0] - at(n + 1)).abs() < 1e-4,
        "resumed at {} (expected frame {n} or {})",
        l[0],
        n + 1
    );
    let reference: Vec<f32> = (0..BLOCK).map(|i| at(n + i)).collect();
    let shifted: Vec<f32> = (0..BLOCK).map(|i| at(n + 1 + i)).collect();
    let (a, b) = (&l[..BLOCK], &l[..BLOCK]);
    assert!(
        a.iter().zip(&reference).all(|(x, y)| (x - y).abs() < 1e-4)
            || b.iter().zip(&shifted).all(|(x, y)| (x - y).abs() < 1e-4),
        "plain playback follows the source from the paused position"
    );
}

#[test]
fn loading_a_new_track_leaves_stem_mode_and_resets_the_mixer() {
    let dir = fixture();
    let (deck, mut audio) = new_deck(&dir);
    deck.load_stems(dir.path()).unwrap();
    let control = deck.stem_control();
    control.set_gain(1, 0.5);
    control.set_muted(2, true);
    control.set_filter(3, 0.4);
    assert!(deck.has_stems());
    deck.load(&dir.file("main.wav")).unwrap();
    assert!(!deck.has_stems());
    assert_eq!(control.gain(1), 1.0);
    assert!(!control.is_muted(2));
    assert_eq!(control.filter(3), 0.0);
    settle(&deck, &mut audio, 0);
    deck.play();
    let (l, _) = render(&deck, &mut audio, 4, |_, _| {});
    let expected: Vec<f32> = (0..4 * BLOCK)
        .map(|n| (0..4).fold(0.0_f32, |a, s| a + stem_sample(s, 0, n)))
        .collect();
    assert_close(&l, &expected, 1e-4, "plain again");
}

/// Stress: the transport script on plain and stem decks while CPU-hog threads starve the decoder.
/// Run with `--ignored`; bounded to about 20 s.
#[test]
#[ignore = "stress"]
fn stress_transport_script_under_cpu_contention() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    let stop = Arc::new(AtomicBool::new(false));
    let hogs: Vec<_> = (0..8)
        .map(|_| {
            let stop = stop.clone();
            std::thread::spawn(move || {
                let mut x = 1_u64;
                while !stop.load(Ordering::Relaxed) {
                    x = std::hint::black_box(x.wrapping_mul(6364136223846793005).wrapping_add(1));
                }
            })
        })
        .collect();
    let dir = fixture();
    let started = std::time::Instant::now();
    let mut runs = 0;
    while started.elapsed() < Duration::from_secs(20) {
        let (deck, mut audio) = new_deck(&dir);
        deck.load_stems(dir.path()).unwrap();
        settle(&deck, &mut audio, 0);
        transport_script(&deck, &mut audio);
        runs += 1;
    }
    stop.store(true, Ordering::Relaxed);
    for h in hogs {
        h.join().unwrap();
    }
    eprintln!("{runs} runs");
}

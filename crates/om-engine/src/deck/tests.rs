//! Deck tests, and the engine-level track loading cases. Fixtures are
//! generated in-test: 32-bit float WAVs whose samples are exact, so the tight tolerances apply.

use super::{Deck, DeckAudio};
use crate::callback::AudioCallback;
use om_testing::wait::{wait_until, DEFAULT_POLL};
use std::time::Duration;

// Scenario suites: key-lock cases and loop cases. They share the fixtures above.
#[cfg(test)]
mod keylock;
#[cfg(test)]
mod loops;
#[cfg(test)]
pub(crate) mod stems;

use om_testing::fs::TempDir;
use om_testing::wav::write_float_wav;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub(crate) const FRAMES: i64 = 20_000;
pub(crate) const RATE: f64 = 44_100.0;

/// Writes a 32-bit float WAV of `FRAMES` frames: left = n / FRAMES, right = -left. Mono files
/// carry only the left ramp.
pub(crate) fn write_ramp(path: &Path, channels: u16) {
    let mut samples = Vec::with_capacity(FRAMES as usize * channels as usize);
    for n in 0..FRAMES {
        let v = n as f32 / FRAMES as f32;
        samples.push(v);
        if channels == 2 {
            samples.push(-v);
        }
    }
    write_float_wav(path, channels, RATE as u32, &samples);
}

/// Renders 64-frame blocks until the deck sits at `expected_pos` with `frames` buffered, or times out.
/// Rendering services the seek handshake while paused.
pub(crate) fn pump_until_buffered(
    deck: &Deck,
    audio: &mut DeckAudio,
    frames: usize,
    expected_pos: i64,
) -> bool {
    let mut l = [0.0_f32; 64];
    let mut r = [0.0_f32; 64];
    wait_until(Duration::from_secs(10), DEFAULT_POLL, || {
        audio.render_stereo(&mut l, &mut r);
        deck.handshake_settled()
            && deck.position_frames() == expected_pos
            && deck.buffered_frames() >= frames
    })
}

fn deck_with_ramp(fixture: &TempDir) -> (Deck, DeckAudio, PathBuf) {
    let wav = fixture.file("ramp.wav");
    write_ramp(&wav, 2);
    let (deck, mut audio) = Deck::new(1 << 16);
    audio.prepare(RATE, 256);
    deck.load(&wav).expect("ramp loads");
    (deck, audio, wav)
}

#[test]
fn load_rejects_non_audio_files() {
    let fixture = TempDir::new("om_engine");
    let junk = fixture.file("junk.wav");
    std::fs::write(&junk, "not audio").unwrap();
    let (deck, _audio) = Deck::new(1 << 16);
    assert!(deck.load(&junk).is_err());
    assert!(!deck.is_loaded());
}

#[test]
fn load_reports_length_and_sample_rate() {
    let fixture = TempDir::new("om_engine");
    let (deck, _audio, _) = deck_with_ramp(&fixture);
    assert!(deck.is_loaded());
    assert_eq!(deck.length_frames(), FRAMES);
    assert_eq!(deck.file_sample_rate(), RATE);
}

#[test]
fn missing_file_fails_without_touching_the_deck() {
    let fixture = TempDir::new("om_engine");
    let (deck, _audio) = Deck::new(1 << 16);
    assert!(deck.load(&fixture.file("gone.wav")).is_err());
    assert!(!deck.is_loaded());
    assert_eq!(deck.length_frames(), 0);
}

#[test]
fn paused_deck_outputs_silence_and_does_not_advance() {
    let fixture = TempDir::new("om_engine");
    let (deck, mut audio, _) = deck_with_ramp(&fixture);
    assert!(
        pump_until_buffered(&deck, &mut audio, 1000, 0),
        "decoder did not buffer in time"
    );
    let mut l = [1.0_f32; 256];
    let mut r = [1.0_f32; 256];
    audio.render_stereo(&mut l, &mut r);
    assert!(l.iter().all(|&v| v == 0.0));
    assert_eq!(deck.position_frames(), 0);
}

#[test]
fn playback_at_rate_one_reproduces_the_source_and_finishes() {
    let fixture = TempDir::new("om_engine");
    let wav = fixture.file("ramp.wav");
    write_ramp(&wav, 2);
    let (deck, mut audio) = Deck::new(1 << 12); // small ring forces many wraparounds
    audio.prepare(RATE, 256);
    deck.load(&wav).unwrap();
    assert!(
        pump_until_buffered(&deck, &mut audio, 512, 0),
        "decoder did not buffer in time"
    );
    deck.play();

    let mut out = Vec::new();
    let mut l = [0.0_f32; 256];
    let mut r = [0.0_f32; 256];
    let ok = wait_until(Duration::from_secs(5), DEFAULT_POLL, || {
        // Pace like a device: only render when a full block is buffered
        // (or everything remaining is already buffered).
        if deck.buffered_frames() < 300
            && deck.position_frames() + (deck.buffered_frames() as i64) < FRAMES - 3
        {
            return false;
        }
        audio.render_stereo(&mut l, &mut r);
        out.extend_from_slice(&l);
        deck.has_finished()
    });
    assert!(ok, "deck should reach the end");
    assert!(!deck.is_playing());

    // Output frame i == source frame i until the final frame (the interpolation anchor, never emitted).
    assert!(out.len() >= FRAMES as usize - 1);
    for (i, &v) in out.iter().take(FRAMES as usize - 1).enumerate() {
        let expected = i as f32 / FRAMES as f32;
        assert!(
            (v - expected).abs() <= 1e-4,
            "mismatch at frame {i}: {v} vs {expected}"
        );
    }
}

#[test]
fn underrun_is_counted_and_outputs_silence_when_decoder_is_starved() {
    let fixture = TempDir::new("om_engine");
    let wav = fixture.file("ramp.wav");
    write_ramp(&wav, 2);
    let (deck, mut audio) = Deck::new(1 << 12);
    audio.prepare(RATE, 256);
    deck.load(&wav).unwrap();
    assert!(
        pump_until_buffered(&deck, &mut audio, 2048, 0),
        "decoder did not buffer in time"
    );
    deck.set_rate(8.0); // consume far faster than the ring can hold
    deck.play();
    let mut l = vec![1.0_f32; 4096];
    let mut r = vec![1.0_f32; 4096];
    audio.render_stereo(&mut l, &mut r);
    assert!(deck.underrun_count() >= 1, "underrun should be counted");
    assert_eq!(l[4095], 0.0, "tail is silence");
    assert_ne!(l[1], 0.0, "head had real audio");
    assert!(deck.is_playing(), "an underrun is not end-of-file");
}

#[test]
fn seek_repositions_playback() {
    let fixture = TempDir::new("om_engine");
    let (deck, mut audio, _) = deck_with_ramp(&fixture);
    assert!(
        pump_until_buffered(&deck, &mut audio, 512, 0),
        "decoder did not buffer in time"
    );
    deck.seek(10_000);
    let mut l = [0.0_f32; 64];
    let mut r = [0.0_f32; 64];
    let synced = wait_until(Duration::from_secs(3), DEFAULT_POLL, || {
        audio.render_stereo(&mut l, &mut r);
        deck.position_frames() == 10_000 && deck.buffered_frames() > 256
    });
    assert!(synced, "seek handshake should complete");
    deck.play();
    assert!(
        wait_until(Duration::from_secs(3), DEFAULT_POLL, || {
            audio.render_stereo(&mut l, &mut r);
            l[0] != 0.0
        }),
        "playback after the seek produced no audio"
    );
    assert!((l[0] - 10_000.0 / FRAMES as f32).abs() <= 1e-4);
    assert!((r[0] + 10_000.0 / FRAMES as f32).abs() <= 1e-4);
    assert!((l[1] - l[0] - 1.0 / FRAMES as f32).abs() <= 1e-6);
}

#[test]
fn rate_half_advances_half_as_fast_and_interpolates() {
    let fixture = TempDir::new("om_engine");
    let (deck, mut audio, _) = deck_with_ramp(&fixture);
    deck.seek(4000);
    deck.set_rate(0.5);
    assert!(
        pump_until_buffered(&deck, &mut audio, 2048, 4000),
        "decoder did not buffer in time"
    );
    deck.play();
    let mut l = vec![0.0_f32; 1000];
    let mut r = vec![0.0_f32; 1000];
    audio.render_stereo(&mut l, &mut r);
    assert!(
        (deck.position_frames() - (4000 + 500)).abs() <= 2,
        "position {}",
        deck.position_frames()
    );
    // Linear ramp => interpolation is exact; slope is half.
    assert!((l[2] - l[0] - 1.0 / FRAMES as f32).abs() <= 1e-6);
}

#[test]
fn mono_files_play_on_both_channels() {
    let fixture = TempDir::new("om_engine");
    let mono = fixture.file("mono.wav");
    write_ramp(&mono, 1);
    let (deck, mut audio) = Deck::new(1 << 16);
    audio.prepare(RATE, 256);
    deck.load(&mono).unwrap();
    assert!(
        pump_until_buffered(&deck, &mut audio, 512, 0),
        "decoder did not buffer in time"
    );
    deck.seek(5000);
    let mut l = [0.0_f32; 64];
    let mut r = [0.0_f32; 64];
    assert!(
        wait_until(Duration::from_secs(3), DEFAULT_POLL, || {
            audio.render_stereo(&mut l, &mut r);
            deck.position_frames() == 5000 && deck.buffered_frames() > 128
        }),
        "seek to 5000 did not settle"
    );
    deck.play();
    audio.render_stereo(&mut l, &mut r);
    assert!((l[0] - r[0]).abs() <= 1e-7);
    assert!(l[0] > 0.2);
}

#[test]
fn deck_renders_through_the_audio_callback_in_device_blocks() {
    let fixture = TempDir::new("om_engine");
    let (deck, mut audio, _) = deck_with_ramp(&fixture);
    audio.prepare(RATE, 256);
    let block = 256;
    let silent = vec![0.0_f32; block];
    let mut left = vec![0.0_f32; block];
    let mut right = vec![0.0_f32; block];
    let mut prepared_blocks = 0;
    let buffered = wait_until(Duration::from_secs(10), DEFAULT_POLL, || {
        let mut outs: [&mut [f32]; 2] = [&mut left, &mut right];
        audio.process(&[&silent], &mut outs);
        prepared_blocks += 1;
        deck.buffered_frames() > 1024
    });
    assert!(buffered, "decoder did not buffer in time");
    deck.play();
    let advanced = wait_until(Duration::from_secs(10), DEFAULT_POLL, || {
        let mut outs: [&mut [f32]; 2] = [&mut left, &mut right];
        audio.process(&[], &mut outs);
        deck.position_frames() > 1000
    });
    assert!(advanced, "playback did not advance past 1000 frames");
    assert!(deck.position_frames() > 1000);
    assert!(prepared_blocks > 0);
}

#[test]
fn hotcue_parameters_trigger_seek_and_clear() {
    use om_core::ParameterTarget;
    let fixture = TempDir::new("om_engine");
    let (deck, mut audio, _) = deck_with_ramp(&fixture);
    assert!(pump_until_buffered(&deck, &mut audio, 512, 0));
    assert!(deck.set_parameter("hotcue2", 1.0)); // empty cue: set at the playhead
    assert_eq!(deck.hotcue(2), Some(0));
    deck.seek(3000);
    assert!(deck.set_parameter("hotcue2", 1.0)); // existing cue: jump back
    assert_eq!(deck.hotcue(2), Some(0));
    assert!(deck.set_parameter("hotcue2.clear", 1.0));
    assert_eq!(deck.hotcue(2), None);
    assert!(!deck.set_parameter("hotcue9", 1.0), "out of range");
    assert!(!deck.set_parameter("hotcue1.bogus", 1.0));
    assert!(!deck.set_parameter("nonsense", 1.0));
}

#[test]
fn a_decoder_fault_silences_the_deck_and_is_reported() {
    let fixture = TempDir::new("om_engine");
    // The hook is inert until armed; once armed, the decoder panics on its next chunk.
    let armed = Arc::new(AtomicBool::new(false));
    let trip = Arc::clone(&armed);
    let wav = fixture.file("ramp.wav");
    write_ramp(&wav, 2);
    // A small ring keeps the decoder from producing the whole ramp before the hook is armed.
    // With the default ring it could finish first, and then no chunk would ever reach the hook.
    let (deck, mut audio) = Deck::with_decoder_hook(1 << 13, move || {
        if trip.load(Ordering::SeqCst) {
            panic!("injected decoder fault");
        }
    });
    audio.prepare(RATE, 256);
    deck.load(&wav).expect("ramp loads");
    assert!(pump_until_buffered(&deck, &mut audio, 512, 0));
    deck.play();
    armed.store(true, Ordering::SeqCst);

    let mut l = [0.0_f32; 256];
    let mut r = [0.0_f32; 256];
    // Rendering frees ring space, which lets the decoder reach its next chunk and fault.
    let faulted = wait_until(Duration::from_secs(5), DEFAULT_POLL, || {
        audio.render_stereo(&mut l, &mut r);
        deck.is_errored()
    });
    assert!(faulted, "the decoder fault should be reported");
    assert!(!deck.is_playing(), "a faulted deck stops playing");
    for _ in 0..4 {
        audio.render_stereo(&mut l, &mut r);
        assert!(
            l.iter().chain(r.iter()).all(|&v| v == 0.0),
            "faulted deck is silent"
        );
    }
}

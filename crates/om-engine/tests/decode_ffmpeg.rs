//! The optional ffmpeg decode fallback. Tests that need a real ffmpeg print a skip reason and
//! pass when none is installed.
#![cfg(feature = "ffmpeg")]

use om_engine::decode_ffmpeg::{decode_with_ffmpeg, ffmpeg_available, DecodeError};
use om_engine::Sample;
use om_testing::wav::{read_wav, write_pcm16_wav};
use std::path::Path;
use std::sync::Mutex;

/// Every test takes this lock: some change `OPENMIXER_FFMPEG`, the rest read it.
static ENV: Mutex<()> = Mutex::new(());

fn lock() -> std::sync::MutexGuard<'static, ()> {
    ENV.lock().unwrap_or_else(|e| e.into_inner())
}

const BOGUS: &str = "definitely-not-a-real-ffmpeg-binary";

fn with_binary<T>(bin: &str, f: impl FnOnce() -> T) -> T {
    let _guard = lock();
    let old = std::env::var_os("OPENMIXER_FFMPEG");
    std::env::set_var("OPENMIXER_FFMPEG", bin);
    let out = f();
    match old {
        Some(v) => std::env::set_var("OPENMIXER_FFMPEG", v),
        None => std::env::remove_var("OPENMIXER_FFMPEG"),
    }
    out
}

/// True when a real ffmpeg can run here; otherwise prints why the test is skipped.
fn real_ffmpeg() -> bool {
    if ffmpeg_available() {
        return true;
    }
    eprintln!("skipped: no ffmpeg executable available (set OPENMIXER_FFMPEG to one)");
    false
}

fn write_tone(path: &Path) {
    let samples: Vec<i16> = (0..4800)
        .flat_map(|i| {
            let v = ((i as f32 * 0.05).sin() * 12000.0) as i16;
            [v, v / 2]
        })
        .collect();
    write_pcm16_wav(path, 2, 48_000, &samples);
}

#[test]
fn ffmpeg_available_is_false_for_a_bogus_binary_name() {
    assert!(!with_binary(BOGUS, ffmpeg_available));
}

#[test]
fn missing_binary_is_a_typed_error() {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("a.wav");
    write_tone(&wav);
    let err = with_binary(BOGUS, || decode_with_ffmpeg(&wav, 48_000)).unwrap_err();
    assert!(matches!(err, DecodeError::FfmpegMissing), "{err:?}");
}

#[test]
fn undecodable_file_is_a_typed_ffmpeg_failure() {
    let _guard = lock();
    if !real_ffmpeg() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let fake = dir.path().join("notes.mp3");
    std::fs::write(&fake, b"this is just text, not audio\n").unwrap();
    let err = decode_with_ffmpeg(&fake, 48_000).unwrap_err();
    assert!(matches!(err, DecodeError::FfmpegFailed(_)), "{err:?}");
}

#[test]
fn decodes_a_wav_to_the_same_frames_as_symphonia() {
    let _guard = lock();
    if !real_ffmpeg() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("tone.wav");
    write_tone(&wav);
    let expected = read_wav(&wav);
    let got = decode_with_ffmpeg(&wav, 48_000).unwrap();
    assert_eq!(got.sample_rate, 48_000);
    assert_eq!(got.samples.len() / 2, expected.samples.len() / 2);
    for (a, b) in got.samples.iter().zip(&expected.samples) {
        assert!((a - b).abs() < 1.0e-3, "{a} vs {b}");
    }
}

/// A stand-in ffmpeg that emits two stereo frames, so the fallback is exercised without ffmpeg.
#[cfg(unix)]
#[test]
fn sample_load_falls_back_when_symphonia_rejects_the_format() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("fake-ffmpeg");
    std::fs::write(
        &script,
        "#!/bin/sh\nprintf '\\000\\000\\200\\077\\000\\000\\000\\000\\000\\000\\000\\000\\000\\000\\200\\077'\n",
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let odd = dir.path().join("track.xyz");
    std::fs::write(&odd, b"not audio").unwrap();
    let sample = with_binary(script.to_str().unwrap(), || Sample::load(&odd, 48_000.0)).unwrap();
    assert_eq!(sample.frames(), 2);
    assert_eq!(sample.sample_rate(), 48_000.0);
    assert_eq!(sample.data()[0], 1.0);
    assert_eq!(sample.data()[3], 1.0);
}

#[test]
fn sample_load_without_a_usable_ffmpeg_keeps_the_symphonia_error() {
    let dir = tempfile::tempdir().unwrap();
    let fake = dir.path().join("notes.xyz");
    std::fs::write(&fake, b"not audio").unwrap();
    let err = with_binary(BOGUS, || Sample::load(&fake, 48_000.0)).unwrap_err();
    assert!(
        err.to_string()
            .starts_with("cannot read sample: unsupported"),
        "{err}"
    );
}

//! The ffmpeg fallback on the deck read path (deck load, stems, `SourceReader`). Fixtures are
//! encoded at test time with the real `ffmpeg`; a missing ffmpeg or encoder skips the case.
#![cfg(feature = "ffmpeg")]

use om_engine::deck::{Deck, SourceError, SourceReader, STEM_FILES};
use om_engine::decode_ffmpeg::ffmpeg_available;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;

static ENV: Mutex<()> = Mutex::new(());

/// Name, file extension and ffmpeg encoder args of formats symphonia (as built here) cannot read.
const FORMATS: [(&str, &str, &[&str]); 3] = [
    ("opus", "opus", &["-c:a", "libopus"]),
    ("alac", "m4a", &["-c:a", "alac"]),
    ("wma", "wma", &["-c:a", "wmav2"]),
];

fn encode(dir: &Path, ext: &str, args: &[&str]) -> Option<PathBuf> {
    if !ffmpeg_available() {
        eprintln!("skipped: no ffmpeg executable available");
        return None;
    }
    let out = dir.join(format!("tone.{ext}"));
    let ok = Command::new("ffmpeg")
        .args(["-nostdin", "-v", "error", "-y", "-f", "lavfi", "-i"])
        .arg("sine=frequency=440:duration=2")
        .args(["-ac", "2", "-ar", "48000"])
        .args(args)
        .arg(&out)
        .stdin(Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if !ok {
        eprintln!("skipped: this ffmpeg cannot encode {ext}");
        return None;
    }
    Some(out)
}

fn with_binary<T>(bin: &str, f: impl FnOnce() -> T) -> T {
    let old = std::env::var_os("OPENMIXER_FFMPEG");
    std::env::set_var("OPENMIXER_FFMPEG", bin);
    let out = f();
    match old {
        Some(v) => std::env::set_var("OPENMIXER_FFMPEG", v),
        None => std::env::remove_var("OPENMIXER_FFMPEG"),
    }
    out
}

fn peak(frames: &[[f32; 2]]) -> f32 {
    frames.iter().flatten().fold(0.0, |m, v| m.max(v.abs()))
}

#[test]
fn deck_load_reads_formats_symphonia_rejects() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    for (name, ext, args) in FORMATS {
        let dir = tempfile::tempdir().unwrap();
        let Some(file) = encode(dir.path(), ext, args) else {
            continue;
        };
        let without = with_binary("no-such-ffmpeg-binary", || SourceReader::open(&file));
        assert!(
            matches!(without, Err(SourceError::Unsupported(_))),
            "{name}: symphonia should reject it, got {:?}",
            without.err()
        );
        let mut reader = SourceReader::open(&file).unwrap_or_else(|e| panic!("{name}: {e}"));
        let info = reader.info();
        assert_eq!(info.sample_rate, 48_000.0, "{name}");
        assert!(
            info.length_frames > 90_000,
            "{name}: {}",
            info.length_frames
        );
        let mut block = vec![[0.0; 2]; 4096];
        assert_eq!(reader.read_stereo(10_000, &mut block), 4096, "{name}");
        assert!(peak(&block) > 0.05, "{name}: silent");
        let mut tail = vec![[1.0; 2]; 64];
        let got = reader.read_stereo(info.length_frames - 10, &mut tail);
        assert_eq!(got, 10, "{name}");
        assert_eq!(tail[10], [0.0, 0.0], "{name}: past the end is silence");

        let (deck, _audio) = Deck::new(1 << 12);
        deck.load(&file).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(deck.is_loaded());
        assert_eq!(deck.length_frames(), info.length_frames, "{name}");
        assert_eq!(deck.file_sample_rate(), 48_000.0, "{name}");
    }
}

#[test]
fn unusable_ffmpeg_leaves_the_symphonia_error_on_deck_load() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let odd = dir.path().join("track.xyz");
    std::fs::write(&odd, b"not audio").unwrap();
    let (deck, _audio) = Deck::new(1 << 12);
    let err = with_binary("no-such-ffmpeg-binary", || deck.load(&odd)).unwrap_err();
    assert!(matches!(err, SourceError::Unsupported(_)), "{err:?}");
    assert!(!deck.is_loaded());
}

#[test]
fn ffmpeg_failure_on_deck_load_is_a_typed_error() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    if !ffmpeg_available() {
        eprintln!("skipped: no ffmpeg executable available");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let odd = dir.path().join("track.xyz");
    std::fs::write(&odd, b"not audio").unwrap();
    let (deck, _audio) = Deck::new(1 << 12);
    let err = deck.load(&odd).unwrap_err();
    assert!(matches!(err, SourceError::Ffmpeg(_)), "{err:?}");
}

#[test]
fn stems_decode_through_the_fallback() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let Some(file) = encode(dir.path(), "opus", &["-c:a", "libopus"]) else {
        return;
    };
    let stems = tempfile::tempdir().unwrap();
    for name in STEM_FILES {
        std::fs::copy(&file, stems.path().join(name)).unwrap();
    }
    let (deck, _audio) = Deck::new(1 << 12);
    deck.load(&file).unwrap();
    deck.load_stems(stems.path()).unwrap();
    assert!(deck.has_stems());
}

//! The library part of `--smoke` (window build). Writes generated tracks into a temporary folder,
//! ingests them through a watch folder, and loads the first into deck 0 through the track loader.
//! The window then runs its frames with the library attached, and the caller checks the table and
//! the deck. Nothing here plays audio: the engine stays on the mock device.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use om_engine::DjEngine;
use om_ui::library::{LibraryServices, TrackLoader};

use crate::error::AppError;

/// Tracks written and ingested by the smoke run.
pub const TRACKS: usize = 3;

/// How long ingest may take before the smoke run fails.
const INGEST_TIMEOUT: Duration = Duration::from_secs(20);

/// Length of each generated track, in seconds.
const SECONDS: f64 = 2.0;
const RATE: u32 = 44_100;

/// Artist and genre tags of the generated tracks, cycled.
const DEMO_TAGS: [(&str, &str); 3] = [
    ("Sine Collective", "Techno"),
    ("Test Tone Orchestra", "House"),
    ("Hertz & The Waves", "Ambient"),
];

/// A per-run folder under the system temp directory (the library file and the music live there).
pub fn scratch_dir() -> PathBuf {
    std::env::temp_dir().join(format!("openmixer-smoke-{}", std::process::id()))
}

/// Writes a 16-bit mono PCM WAV with a sine tone at `freq` Hz.
pub fn write_wav(path: &Path, freq: f32) -> io::Result<()> {
    write_wav_tagged(path, freq, &[])
}

/// A RIFF `LIST`/`INFO` chunk holding `tags` as `(four-character id, text)` pairs, e.g. `INAM`
/// (title), `IART` (artist), `IGNR` (genre). Text is NUL-terminated and padded to an even length.
fn info_chunk(tags: &[(&str, &str)]) -> Vec<u8> {
    let mut body = b"INFO".to_vec();
    for (id, text) in tags {
        let mut bytes = text.as_bytes().to_vec();
        bytes.push(0);
        body.extend_from_slice(id.as_bytes());
        body.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        body.extend_from_slice(&bytes);
        if bytes.len() % 2 == 1 {
            body.push(0);
        }
    }
    let mut chunk = b"LIST".to_vec();
    chunk.extend_from_slice(&(body.len() as u32).to_le_bytes());
    chunk.extend_from_slice(&body);
    chunk
}

/// Like [`write_wav`], with INFO tags (artist, genre, title) the library ingest reads.
pub fn write_wav_tagged(path: &Path, freq: f32, tags: &[(&str, &str)]) -> io::Result<()> {
    let frames = (SECONDS * f64::from(RATE)) as u32;
    let data_len = frames * 2;
    let info = if tags.is_empty() {
        Vec::new()
    } else {
        info_chunk(tags)
    };
    let mut out = Vec::with_capacity(44 + data_len as usize + info.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len + info.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for n in 0..frames {
        let phase = f64::from(n) * f64::from(freq) * std::f64::consts::TAU / f64::from(RATE);
        let sample = (phase.sin() * 0.25 * f64::from(i16::MAX)) as i16;
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out.extend_from_slice(&info);
    let mut file = fs::File::create(path)?;
    file.write_all(&out)
}

/// Writes [`TRACKS`] generated tracks under `root/music`, watches that folder and waits until the
/// library holds them. Returns the track count.
pub fn ingest_generated(services: &mut LibraryServices, root: &Path) -> Result<usize, AppError> {
    let music = root.join("music");
    fs::create_dir_all(&music)?;
    for i in 0..TRACKS {
        let freq = 220.0 * (i as f32 + 1.0);
        let (artist, genre) = DEMO_TAGS[i % DEMO_TAGS.len()];
        write_wav_tagged(
            &music.join(format!("smoke_{i}.wav")),
            freq,
            &[
                ("INAM", &format!("smoke_{i}")),
                ("IART", artist),
                ("IGNR", genre),
            ],
        )?;
    }
    let folders = services
        .watch_folders()
        .ok_or(AppError::Smoke("the library has no watch folders"))?;
    folders.add(&music)?;

    let deadline = Instant::now() + INGEST_TIMEOUT;
    loop {
        services.pump();
        let count = services.library().tracks().count()?;
        let count = usize::try_from(count).unwrap_or(0);
        if count >= TRACKS {
            return Ok(count);
        }
        if Instant::now() >= deadline {
            return Err(AppError::SmokeTracks {
                count,
                expected: TRACKS,
            });
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Loads the first ingested track (by path) into deck 0 through the loader. Returns its id.
pub fn load_first_into_deck0(
    services: &mut LibraryServices,
    loader: &mut TrackLoader,
    engine: &DjEngine,
) -> Result<i64, AppError> {
    let mut tracks = services.library().tracks().all()?;
    tracks.sort_by(|a, b| a.path.cmp(&b.path));
    let first = tracks
        .into_iter()
        .next()
        .ok_or(AppError::Smoke("the library is empty"))?;
    loader.load_track(engine, services, 0, &first, false)?;
    Ok(first.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_wav_has_a_riff_header_and_the_expected_length() {
        let dir = std::env::temp_dir().join(format!("om-smoke-wav-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tone.wav");
        write_wav(&path, 440.0).unwrap();
        let bytes = fs::read(&path).unwrap();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(bytes.len(), 44 + (SECONDS * f64::from(RATE)) as usize * 2);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn tagged_wav_carries_a_consistent_riff_size_and_the_tags() {
        let dir = std::env::temp_dir().join(format!("om-smoke-tags-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tagged.wav");
        write_wav_tagged(&path, 440.0, &[("IART", "Odd"), ("IGNR", "House")]).unwrap();
        let bytes = fs::read(&path).unwrap();
        let riff = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
        assert_eq!(riff + 8, bytes.len());
        assert!(bytes.windows(4).any(|w| w == b"IART"));
        assert!(bytes.windows(5).any(|w| w == b"House"));
        fs::remove_dir_all(&dir).unwrap();
    }
}

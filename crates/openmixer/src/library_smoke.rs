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

/// A per-run folder under the system temp directory (the library file and the music live there).
pub fn scratch_dir() -> PathBuf {
    std::env::temp_dir().join(format!("openmixer-smoke-{}", std::process::id()))
}

/// Writes a 16-bit mono PCM WAV with a sine tone at `freq` Hz.
pub fn write_wav(path: &Path, freq: f32) -> io::Result<()> {
    let frames = (SECONDS * f64::from(RATE)) as u32;
    let data_len = frames * 2;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
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
        write_wav(&music.join(format!("smoke_{i}.wav")), freq)?;
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
}

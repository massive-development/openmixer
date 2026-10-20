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
/// How long the automix demo waits for its tracks to be analysed.
const ANALYSIS_TIMEOUT: Duration = Duration::from_secs(120);

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
    write_pcm(path, SECONDS, tags, |n| {
        let phase = f64::from(n) * f64::from(freq) * std::f64::consts::TAU / f64::from(RATE);
        (phase.sin() * 0.25 * f64::from(i16::MAX)) as i16
    })
}

/// A click track at `bpm`: a broadband percussive hit (deterministic noise over a 180 Hz body,
/// 12 ms decay) on every beat. The BPM analysis needs transients like these to find a grid.
/// Used by the automix demo, whose tracks must be analysed before automix can queue them.
pub fn write_click_track(
    path: &Path,
    bpm: f64,
    seconds: f64,
    tags: &[(&str, &str)],
) -> io::Result<()> {
    let beat = f64::from(RATE) * 60.0 / bpm;
    let rate = f64::from(RATE);
    write_pcm(path, seconds, tags, move |n| {
        let within = f64::from(n) % beat;
        let t = within / rate;
        if t >= 0.1 {
            return 0;
        }
        // A hash of the sample index: the same noise every run, no RNG state.
        let hash = u64::from(n).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 40;
        let noise = (hash as f64 / f64::from(1u32 << 24)) * 2.0 - 1.0;
        let body = (f64::from(n) * 180.0 * std::f64::consts::TAU / rate).sin();
        let envelope = (-t / 0.012).exp();
        ((0.6 * noise + 0.4 * body) * envelope * 0.7 * f64::from(i16::MAX)) as i16
    })
}

/// Writes a 16-bit mono PCM WAV of `seconds`, sample `n` being `sample(n)`, with optional INFO tags.
fn write_pcm(
    path: &Path,
    seconds: f64,
    tags: &[(&str, &str)],
    sample: impl Fn(u32) -> i16,
) -> io::Result<()> {
    let frames = (seconds * f64::from(RATE)) as u32;
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
        out.extend_from_slice(&sample(n).to_le_bytes());
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
    watch_and_wait(services, &music, TRACKS)
}

/// Tempos of the automix demo's click tracks: close enough that the tempo gap allows every pick.
pub const DEMO_AUTOMIX_BPM: [f64; 3] = [120.0, 122.0, 124.0];
/// Length of each automix demo track: long enough to arm and transition on screen.
pub const DEMO_AUTOMIX_SECONDS: f64 = 24.0;

/// Writes [`DEMO_AUTOMIX_BPM`] click tracks under `root/music` and ingests them, like
/// [`ingest_generated`]. The tracks are analysed in the background as usual.
pub fn ingest_demo_automix(services: &mut LibraryServices, root: &Path) -> Result<usize, AppError> {
    let music = root.join("music");
    fs::create_dir_all(&music)?;
    for (i, bpm) in DEMO_AUTOMIX_BPM.iter().enumerate() {
        let (artist, genre) = DEMO_TAGS[i % DEMO_TAGS.len()];
        write_click_track(
            &music.join(format!("click_{i}.wav")),
            *bpm,
            DEMO_AUTOMIX_SECONDS,
            &[
                ("INAM", &format!("Click {bpm:.0}")),
                ("IART", artist),
                ("IGNR", genre),
            ],
        )?;
    }
    watch_and_wait(services, &music, DEMO_AUTOMIX_BPM.len())
}

/// Queues every track for analysis, as the browser's Analyse action does. Automix only queues
/// tracks with a beat grid, so the demo needs the analysis to run.
pub fn queue_all_for_analysis(services: &mut LibraryServices) -> Result<usize, AppError> {
    let tracks = services.library().tracks().all()?;
    let mut queued = 0;
    for track in tracks {
        if services.prep_queue_mut().enqueue(track.id, false).is_ok() {
            queued += 1;
        }
    }
    Ok(queued)
}

/// Pumps the library until every track has a beat grid (the automix queue picks only analysed
/// tracks), or the time runs out. The demo starts its deck after this, so automix has candidates
/// from the first tick.
pub fn wait_for_analysis(services: &mut LibraryServices) -> Result<(), AppError> {
    let deadline = Instant::now() + ANALYSIS_TIMEOUT;
    loop {
        services.pump();
        let tracks = services.library().tracks().all()?;
        if tracks.iter().all(|track| services.has_beat_grid(track)) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(AppError::Smoke("the demo tracks were not analysed in time"));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Watches `music` and pumps the library until it holds the tracks written there.
fn watch_and_wait(
    services: &mut LibraryServices,
    music: &Path,
    expected: usize,
) -> Result<usize, AppError> {
    let folders = services
        .watch_folders()
        .ok_or(AppError::Smoke("the library has no watch folders"))?;
    folders.add(music)?;

    let deadline = Instant::now() + INGEST_TIMEOUT;
    loop {
        services.pump();
        let count = services.library().tracks().count()?;
        let count = usize::try_from(count).unwrap_or(0);
        if count >= expected {
            return Ok(count);
        }
        if Instant::now() >= deadline {
            return Err(AppError::SmokeTracks { count, expected });
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

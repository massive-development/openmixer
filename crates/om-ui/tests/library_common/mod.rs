//! Shared fixtures for the library UI tests: services over an in-memory library with instant fake
//! analysers, seeded tracks, real WAV files and a pump helper.
#![allow(dead_code)]

use om_testing::signals::sine_amp;
use om_testing::wait::{wait_until, DEFAULT_POLL};
use om_testing::wav::write_pcm16_wav;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use om_analysis::{
    AnalysisContext, Analyzer, AnalyzerRegistry, AudioClip, BeatGridModel, BpmResult, KeyResult,
    MusicalKey, WaveformData, WaveformLevel, WaveformPoint,
};
use om_library::{Library, TrackRecord};
use om_ui::library::{LibraryServices, Options};

/// Analyser that returns the same payload for every clip.
pub struct FixedAnalyzer {
    id: String,
    payload: String,
}

impl Analyzer for FixedAnalyzer {
    fn id(&self) -> &str {
        &self.id
    }

    fn version(&self) -> i32 {
        1
    }

    fn analyze(&self, _clip: &AudioClip, _context: &dyn AnalysisContext) -> Option<String> {
        Some(self.payload.clone())
    }
}

pub fn waveform_json() -> String {
    let base = WaveformLevel {
        samples_per_point: 64,
        points: vec![
            WaveformPoint {
                min: -0.5,
                max: 0.5,
                loudness: 0.4,
                centroid: 0.3,
                spread: 0.2,
            };
            64
        ],
    };
    WaveformData::new(8000.0, 64 * 64, base).to_json()
}

/// A format 1 payload (three band levels per point), as the first release cached it.
pub fn legacy_waveform_json() -> String {
    format!(
        r#"{{"version":1,"sampleRate":8000.0,"numSamples":4096,"samplesPerPoint":64,"points":"{}"}}"#,
        "IOBgUEAg4GBQQCDgYFBAIOBgUEAg4GBQQCDgYFBAIOBgUEAg4GBQQCDgYFBAIOBgUEAg4GBQQCDgYFBAIOBgUEAg4GBQQCDgYFBAIOBgUEAg4GBQQCDgYFBAIOBgUEAg4GBQQCDgYFBAIOBgUEAg4GBQQCDgYFBAIOBgUEAg4GBQQCDgYFBAIOBgUEAg4GBQQCDgYFBAIOBgUEAg4GBQQCDgYFBAIOBgUEAg4GBQQCDgYFBAIOBgUEAg4GBQQCDgYFBAIOBgUEAg4GBQQCDgYFBAIOBgUEAg4GBQQCDgYFBAIOBgUEAg4GBQQCDgYFBAIOBgUEAg4GBQQCDgYFBAIOBgUEAg4GBQQCDgYFBAIOBgUEAg4GBQQCDgYFBAIOBgUEAg4GBQQCDgYFBAIOBgUEAg4GBQQCDgYFBAIOBgUEA="
    )
}

pub fn bpm_json(bpm: f64) -> String {
    BpmResult {
        bpm,
        confidence: 0.9,
        grid: BeatGridModel::constant(bpm, 8000.0, 0.0, 4),
    }
    .to_json()
}

pub fn fake_analyzers() -> Arc<AnalyzerRegistry> {
    registry_with_waveform(None)
}

/// The fake analysers, with `waveform` standing in for the fake waveform one when given.
pub fn registry_with_waveform(waveform: Option<Arc<dyn Analyzer>>) -> Arc<AnalyzerRegistry> {
    let registry = AnalyzerRegistry::new();
    let key = KeyResult {
        key: MusicalKey::new(9, true), // A minor = 8A
        confidence: 0.9,
        chroma: [0.0; 12],
        tonal: true,
    };
    let analyzers: [(&str, String); 4] = [
        ("bpm", bpm_json(128.0)),
        ("key", key.to_json()),
        ("waveform", waveform_json()),
        ("structure", "{}".to_owned()),
    ];
    for (id, payload) in analyzers {
        if let (true, Some(real)) = (id == "waveform", &waveform) {
            registry.add(real.clone()).expect("unique ids");
            continue;
        }
        registry
            .add(Arc::new(FixedAnalyzer {
                id: id.to_owned(),
                payload,
            }))
            .expect("unique ids");
    }
    Arc::new(registry)
}

/// In-memory options with the fake analysers; any error fails the test.
pub fn memory_options() -> Options {
    let mut o = Options::for_path(":memory:");
    o.analyzers = Some(fake_analyzers());
    o.on_error = Some(Box::new(|m| panic!("unexpected library error: {m}")));
    o
}

pub fn make_services(options: Options) -> LibraryServices {
    match LibraryServices::create(options) {
        Ok(services) => services,
        Err(error) => panic!("library should open: {error}"),
    }
}

/// Seeded track fields.
pub struct Seed {
    pub title: &'static str,
    pub artist: &'static str,
    pub bpm: Option<f64>,
    pub camelot: &'static str,
    pub genre: &'static str,
    pub rating: i32,
}

thread_local! {
    static COUNTER: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Inserts a track; `path` defaults to a unique `/music/N_title.wav`.
pub fn add_track(lib: &Library, s: &Seed, path: Option<&str>) -> i64 {
    let n = COUNTER.with(|c| {
        c.set(c.get() + 1);
        c.get()
    });
    let track = TrackRecord {
        path: path.map_or_else(|| format!("/music/{n}_{}.wav", s.title), str::to_owned),
        title: s.title.to_owned(),
        artist: s.artist.to_owned(),
        bpm: s.bpm,
        camelot: s.camelot.to_owned(),
        genre: s.genre.to_owned(),
        rating: s.rating,
        duration_sec: 245.0,
        ..TrackRecord::default()
    };
    lib.tracks().insert(&track).expect("insert track")
}

/// Alpha, Bravo, Charlie and an unanalysed Delta; returns their ids in that order.
pub fn seed_four(lib: &Library) -> Vec<i64> {
    vec![
        add_track(
            lib,
            &Seed {
                title: "Alpha",
                artist: "Zed",
                bpm: Some(120.0),
                camelot: "8A",
                genre: "House",
                rating: 5,
            },
            None,
        ),
        add_track(
            lib,
            &Seed {
                title: "Bravo",
                artist: "Yan",
                bpm: Some(128.0),
                camelot: "9A",
                genre: "Techno",
                rating: 3,
            },
            None,
        ),
        add_track(
            lib,
            &Seed {
                title: "Charlie",
                artist: "Xi",
                bpm: Some(124.0),
                camelot: "3B",
                genre: "House",
                rating: 1,
            },
            None,
        ),
        add_track(
            lib,
            &Seed {
                title: "Delta",
                artist: "Wu",
                bpm: None,
                camelot: "",
                genre: "",
                rating: 0,
            },
            None,
        ),
    ]
}

/// A 16-bit mono WAV with a sine tone at half scale, written under `path`'s parent directory as
/// needed.
pub fn write_wav(path: &Path, seconds: f64, freq: f32) -> PathBuf {
    const RATE: u32 = 44_100;
    let frames = (seconds * f64::from(RATE)) as usize;
    let pcm: Vec<i16> = sine_amp(f64::from(freq), 0.5, f64::from(RATE), frames)
        .into_iter()
        .map(|s| (s * 32767.0) as i16)
        .collect();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create test folder");
    }
    write_pcm16_wav(path, 1, RATE, &pcm);
    path.to_path_buf()
}

/// Pumps the services until `done` holds or the timeout passes.
pub fn pump_until(
    services: &mut LibraryServices,
    mut done: impl FnMut(&LibraryServices) -> bool,
) -> bool {
    wait_until(Duration::from_secs(8), DEFAULT_POLL, || {
        services.pump();
        done(services)
    })
}

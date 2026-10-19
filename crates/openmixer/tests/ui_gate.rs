//! UI verification gate (Todo 31). Port of `tools/ui_gate.cpp` and `tools/ui_gate/UiRig.h`, `UiScenario.h`,
//! at the logic layer: the real composition (engine, router, preferences) runs over a paced
//! `MockAudioDevice` (silent, in memory only), and a scripted DJ operates the om-ui widget models
//! (transport panel, bindings) through the same string-id router the window uses. No display needed.
//!
//! Short run: `cargo test -p openmixer --test ui_gate` (about 8 s). Long run (10 minutes):
//! `cargo test -p openmixer --test ui_gate -- --ignored`.

use om_audio::{AudioDevice, MockAudioDevice, OutputRouter};
use om_core::ParameterTarget;
use om_ui::binding::{DeckBinding, LevelSource, MixerBinding};
use om_ui::widgets::transport::{TransportButton, TransportPanel};
use openmixer::composition::{AppComposition, DECKS};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const RATE: u32 = 48_000;
const BLOCK: usize = 256;
/// Peaks above this count as "the meter moved".
const METER_FLOOR: f32 = 0.01;
const TICK: Duration = Duration::from_millis(50);
const SHORT_SECONDS: u64 = 8;
const LONG_SECONDS: u64 = 600;

/// Writes a 16-bit stereo WAV of a sine tone, so the deck decoder has real audio to stream.
fn write_tone(path: &Path, frequency: f64, seconds: f64) {
    let frames = (f64::from(RATE) * seconds) as usize;
    let mut data = Vec::with_capacity(frames * 4);
    for n in 0..frames {
        let phase = 2.0 * std::f64::consts::PI * frequency * n as f64 / f64::from(RATE);
        let sample = (phase.sin() * 0.5 * f64::from(i16::MAX)) as i16;
        for _ in 0..2 {
            data.extend_from_slice(&sample.to_le_bytes());
        }
    }
    let mut wav = Vec::with_capacity(44 + data.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&2u16.to_le_bytes()); // stereo
    wav.extend_from_slice(&RATE.to_le_bytes());
    wav.extend_from_slice(&(RATE * 4).to_le_bytes());
    wav.extend_from_slice(&4u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
    wav.extend_from_slice(&data);
    fs::write(path, wav).expect("write tone");
}

/// Scratch folder under cargo's per-target tmp dir, removed on drop.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("ui-gate-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch dir");
        Self(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// What the gate saw over the run.
#[derive(Debug, Default)]
struct Report {
    callbacks: u64,
    xruns: u64,
    non_finite: u64,
    peak_master: f32,
    peak_channel: f32,
    mismatches: u32,
}

/// Runs the scripted session for `seconds` of paced audio and returns what it observed. Panics on
/// setup failures; invariant checks are left to the caller so the failure message names them.
fn run_gate(seconds: u64) -> Report {
    let scratch = Scratch::new(&format!("{seconds}s"));
    let tracks = [
        scratch.0.join("aurora.wav"),
        scratch.0.join("borealis.wav"),
    ];
    write_tone(&tracks[0], 220.0, 12.0);
    write_tone(&tracks[1], 330.0, 12.0);

    // Preferences from a fresh file: the composition loads defaults and applies them.
    let mut composition = AppComposition::new(&scratch.0.join("preferences.json"), None);
    let audio = composition.take_audio().expect("audio side");
    let mut device = MockAudioDevice::new(f64::from(RATE), BLOCK, 0, 4);
    device.start(Box::new(audio)).expect("start mock device");
    let (_router, pairs) = OutputRouter::new();
    composition.apply_preferences(&pairs);
    device.start_realtime();

    let engine = Arc::clone(&composition.engine);
    let target: Arc<dyn ParameterTarget> = composition.router.clone();
    let transports: Vec<TransportPanel> = (0..DECKS)
        .map(|d| TransportPanel::new(DeckBinding::new(Arc::clone(&engine), Arc::clone(&target), d)))
        .collect();
    let mixer = MixerBinding::new(Arc::clone(&engine), Arc::clone(&target));

    let mut report = Report::default();
    let expect = |report: &mut Report, ok: bool| {
        if !ok {
            report.mismatches += 1;
        }
    };

    // The DJ loads one track per deck and waits for the decoders.
    for (d, track) in tracks.iter().enumerate() {
        engine.deck(d).load(track).expect("load track");
    }
    let loaded_by = Instant::now() + Duration::from_secs(5);
    while !(engine.deck(0).is_loaded() && engine.deck(1).is_loaded()) {
        assert!(Instant::now() < loaded_by, "decks did not load");
        thread::sleep(TICK);
    }

    // Faders up, then deck 0 plays (controls must reflect the press).
    mixer.send(&MixerBinding::channel_id(0, "fader"), 0.8);
    mixer.send(&MixerBinding::channel_id(1, "fader"), 0.8);
    expect(&mut report, (engine.mixer().fader(0) - 0.8).abs() < 1e-3);
    transports[0].press(TransportButton::Play);
    expect(&mut report, transports[0].state(TransportButton::Play));
    expect(&mut report, engine.deck(0).is_playing());

    let started = Instant::now();
    let total = Duration::from_secs(seconds);
    let mut deck1_started = false;
    let mut next_sample = Instant::now();
    while started.elapsed() < total {
        let t = started.elapsed();
        // Scripted moves: deck 1 comes in at 2 s, channel 1 is cued at 4 s, the crossfader sweeps.
        if !deck1_started && t >= Duration::from_secs(2) {
            transports[1].press(TransportButton::Play);
            deck1_started = true;
        }
        if t >= Duration::from_secs(4) {
            engine.mixer().set_pfl(1, true);
        }
        let sweep = (t.as_secs_f32() / total.as_secs_f32().max(1.0)).min(1.0);
        engine.mixer().set_crossfader(sweep);

        if Instant::now() >= next_sample {
            next_sample += Duration::from_millis(250);
            let (l, r) = engine.mixer().master_peaks();
            report.peak_master = report.peak_master.max(l).max(r);
            report.peak_channel = report.peak_channel.max(engine.mixer().channel_peak(0));
            for sample in device.last_output().iter().flatten() {
                if !sample.is_finite() {
                    report.non_finite += 1;
                }
            }
        }
        thread::sleep(TICK);
    }

    // Controls still reflect the engine after the run.
    expect(&mut report, transports[0].state(TransportButton::Play) == engine.deck(0).is_playing());
    expect(&mut report, transports[1].state(TransportButton::Play) == engine.deck(1).is_playing());
    expect(&mut report, deck1_started && engine.deck(1).is_playing());
    expect(&mut report, (engine.mixer().fader(0) - 0.8).abs() < 1e-3);

    device.stop_realtime();
    let stats = device.latency_stats();
    report.callbacks = stats.callback_count;
    report.xruns = stats.xrun_count;
    device.stop();
    report
}

fn check(report: &Report) {
    println!("ui_gate: {report:?}");
    assert!(report.callbacks > 0, "the device never ran the callback");
    assert_eq!(report.xruns, 0, "xruns on the paced mock device");
    assert_eq!(report.non_finite, 0, "non-finite output samples");
    assert!(report.peak_master > METER_FLOOR, "the master meter never moved");
    assert!(report.peak_channel > METER_FLOOR, "the channel meter never moved");
    assert_eq!(report.mismatches, 0, "controls and engine state disagree");
}

#[test]
fn scripted_dj_session_meets_the_gate() {
    let started = Instant::now();
    let report = run_gate(SHORT_SECONDS);
    check(&report);
    assert!(
        started.elapsed() < Duration::from_secs(60),
        "the short gate must stay well under a minute"
    );
}

#[test]
#[ignore = "10-minute gate: cargo test -p openmixer --test ui_gate -- --ignored"]
fn scripted_dj_session_ten_minutes() {
    check(&run_gate(LONG_SECONDS));
}

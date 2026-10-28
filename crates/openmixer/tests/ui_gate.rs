//! UI verification gate, at the logic layer: the real composition (engine, router, preferences) runs over a paced
//! `MockAudioDevice` (silent, in memory only), and a scripted DJ operates the om-ui widget models
//! (transport panel, bindings) through the same string-id router the window uses. No display needed.
//!
//! Short run: `cargo test -p openmixer --test ui_gate` (about 8 s). Long run (10 minutes):
//! `cargo test -p openmixer --test ui_gate -- --ignored`.

use om_audio::{AudioDevice, MockAudioDevice, OutputRouter};
use om_core::param_id::ChannelParam;
use om_core::{CommandSink, ParameterTarget};
use om_engine::sampler::PadMode;
use om_testing::signals::sine_amp;
use om_testing::wav::write_pcm16_wav;
use om_ui::binding::{DeckBinding, LevelSource, MixerBinding};
use om_ui::sample_bank::SampleBank;
use om_ui::sampler_binding::SamplerBinding;
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
    let samples: Vec<i16> = sine_amp(frequency, 0.5, f64::from(RATE), frames)
        .into_iter()
        .flat_map(|s| {
            let v = (s * f32::from(i16::MAX)) as i16;
            [v, v]
        })
        .collect();
    write_pcm16_wav(path, 2, RATE, &samples);
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
    /// Presses sent through the sampler pad view-model (one-shot clicks and a gate hold).
    pad_presses: u32,
}

/// Runs the scripted session for `seconds` of paced audio and returns what it observed. Panics on
/// setup failures; invariant checks are left to the caller so the failure message names them.
fn run_gate(seconds: u64) -> Report {
    let scratch = Scratch::new(&format!("{seconds}s"));
    let tracks = [scratch.0.join("aurora.wav"), scratch.0.join("borealis.wav")];
    // Long enough that neither deck reaches the end of its file during the run (4 s of margin), so the
    // 10-minute gate streams ten minutes of audio. At 8 s this is the 12 s the short gate always used.
    let track_seconds = (seconds + 4) as f64;
    write_tone(&tracks[0], 220.0, track_seconds);
    write_tone(&tracks[1], 330.0, track_seconds);

    // Preferences from a fresh file: the composition loads defaults and applies them.
    let mut composition = AppComposition::new(&scratch.0.join("preferences.json"), None);
    let audio = composition.take_audio().expect("audio side");
    let mut device = MockAudioDevice::new(f64::from(RATE), BLOCK, 0, 4);
    device.start(Box::new(audio)).expect("start mock device");
    let (_router, pairs) = OutputRouter::new();
    composition.apply_preferences(&pairs, None, None);
    device.start_realtime();

    let engine = Arc::clone(&composition.engine);
    let router = Arc::clone(&composition.router);
    let transports: Vec<TransportPanel> = (0..DECKS)
        .map(|d| {
            TransportPanel::new(DeckBinding::typed(
                Arc::clone(&engine),
                Arc::clone(&router),
                d,
            ))
        })
        .collect();
    let mixer = MixerBinding::typed(Arc::clone(&engine), Arc::clone(&router));

    // The sampler pads, through the same view-model the window uses: a bank in the scratch folder,
    // a one-shot kick on pad 1 and a gate on pad 4.
    let bank = Arc::new(SampleBank::open(
        scratch.0.join("samples"),
        Arc::clone(&engine),
        composition.stem_command_sink(),
    ));
    let pads = SamplerBinding::new(
        Arc::clone(&engine),
        Some(router.clone() as Arc<dyn CommandSink>),
        router.clone() as Arc<dyn ParameterTarget>,
        Some(Arc::clone(&bank)),
    );
    let kick = scratch.0.join("kick.wav");
    let hold = scratch.0.join("hold.wav");
    write_tone(&kick, 80.0, 0.5);
    write_tone(&hold, 440.0, 1.0);
    pads.load(0, &kick).expect("load kick");
    pads.load(3, &hold).expect("load hold");
    bank.settle(Duration::from_secs(10));
    pads.set_mode(3, PadMode::Gate).expect("gate mode");

    let mut report = Report::default();
    // Counts a failed check and names the line it came from, so a mismatch is found from the log.
    let expect = |report: &mut Report, ok: bool, line: u32| {
        if !ok {
            eprintln!("ui_gate: mismatch at line {line}");
            report.mismatches += 1;
        }
    };

    // The DJ loads one track per deck and waits for the decoders.
    for (d, track) in tracks.iter().enumerate() {
        engine.deck(d).load(track).expect("load track");
    }
    om_testing::assert_eventually!(
        Duration::from_secs(5),
        || engine.deck(0).is_loaded() && engine.deck(1).is_loaded(),
        "decks did not load"
    );

    // Faders up, then deck 0 plays (controls must reflect the press).
    mixer.send_channel(0, ChannelParam::Fader, 0.8);
    mixer.send_channel(1, ChannelParam::Fader, 0.8);
    expect(
        &mut report,
        (engine.mixer().fader(0) - 0.8).abs() < 1e-3,
        line!(),
    );
    transports[0].press(TransportButton::Play);
    expect(
        &mut report,
        transports[0].state(TransportButton::Play),
        line!(),
    );
    expect(&mut report, engine.deck(0).is_playing(), line!());

    let started = Instant::now();
    let total = Duration::from_secs(seconds);
    let mut deck1_started = false;
    let mut last_press_second = u32::MAX;
    // The gate is pressed once at 3 s and released once at 3.5 s.
    let mut gate_phase = 0_u8;
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
        // A kick press every second; the gate pad is held from 3 s to 3.5 s.
        let whole = t.as_millis() / 1000;
        if whole as u32 != last_press_second {
            last_press_second = whole as u32;
            expect(&mut report, pads.press(0, 0.8), line!());
            report.pad_presses += 1;
        }
        if t >= Duration::from_secs(3) && gate_phase == 0 {
            expect(&mut report, pads.press(3, 1.0), line!());
            gate_phase = 1;
            report.pad_presses += 1;
        }
        if t >= Duration::from_millis(3500) && gate_phase == 1 {
            expect(&mut report, pads.release(3), line!());
            gate_phase = 2;
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
    expect(
        &mut report,
        transports[0].state(TransportButton::Play) == engine.deck(0).is_playing(),
        line!(),
    );
    expect(
        &mut report,
        transports[1].state(TransportButton::Play) == engine.deck(1).is_playing(),
        line!(),
    );
    expect(
        &mut report,
        deck1_started && engine.deck(1).is_playing(),
        line!(),
    );
    expect(
        &mut report,
        (engine.mixer().fader(0) - 0.8).abs() < 1e-3,
        line!(),
    );
    // The pads show what the bank loaded, and no sampler command was dropped.
    let view = pads.pads();
    expect(&mut report, view[0].loaded && view[3].loaded, line!());
    expect(&mut report, view[3].mode == PadMode::Gate, line!());
    expect(
        &mut report,
        engine.sampler().dropped_commands() == 0,
        line!(),
    );
    bank.pump();

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
    assert!(
        report.peak_master > METER_FLOOR,
        "the master meter never moved"
    );
    assert!(
        report.peak_channel > METER_FLOOR,
        "the channel meter never moved"
    );
    assert_eq!(report.mismatches, 0, "controls and engine state disagree");
    assert!(
        report.pad_presses > 0,
        "the sampler pads were never pressed"
    );
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

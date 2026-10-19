#![forbid(unsafe_code)]

//! Headless engine soak: two decks through the mixer on a paced mock device while a scripted DJ
//! moves controls, loops, hotcues, key-lock, FX and seeks. Checks these invariants:
//!
//! - no NaN or Inf and no overflow (|sample| > 4) on the outputs;
//! - at most 5 xruns (callbacks over the buffer's time budget);
//! - no steady-state deck underruns (settle windows after seeks, loops, hotcues and key-lock);
//! - a sane latency monitor (callbacks ran at the configured rate, stats are finite).
//!
//! The device is `MockAudioDevice`: nothing is rendered to a sound card, and the tracks are only
//! ever read by the engine. Run the short soak with `cargo test -p om-audio --test soak`, the
//! 10-minute soak with `cargo test -p om-audio --test soak -- --ignored --nocapture`.

#[path = "soak/checker.rs"]
mod checker;
#[path = "soak/scenario.rs"]
mod scenario;
#[path = "soak/tone.rs"]
mod tone;

use checker::{OutputChecker, OutputReport};
use om_audio::{AudioDevice, DjEngine, LatencyStats, MockAudioDevice};
use om_core::{ParameterRouter, ParameterTarget};
use om_testing::fs::TempDir;
use scenario::{ActionCounts, Scenario, CONTROL_PERIOD};
use std::time::{Duration, Instant};

const RATE: u32 = 48_000;
const BLOCK: usize = 256;
const TONE_SECONDS: u32 = 6;
/// Xrun budget: shared runners hiccup, systemic xruns still fail.
const MAX_XRUNS: u64 = 5;
/// Underrun budget.
const MAX_STEADY_UNDERRUNS: u32 = 0;

struct Outcome {
    seconds: u32,
    latency: LatencyStats,
    output: OutputReport,
    steady_underruns: u32,
    counts: ActionCounts,
}

impl Outcome {
    /// Everything the soak asserts, as human-readable failures. Empty means pass.
    fn failures(&self) -> Vec<String> {
        let mut failed = Vec::new();
        let stats = &self.latency;
        let expected_callbacks = f64::from(self.seconds) * f64::from(RATE) / BLOCK as f64;
        if stats.callback_count == 0 {
            failed.push("no audio callbacks ran".to_owned());
        } else if (stats.callback_count as f64) < 0.9 * expected_callbacks {
            failed.push(format!(
                "device ran {} callbacks, expected about {expected_callbacks:.0}",
                stats.callback_count
            ));
        }
        if stats.sample_rate != f64::from(RATE) || stats.buffer_size != BLOCK {
            failed.push(format!(
                "latency monitor configured as {} Hz / {} frames",
                stats.sample_rate, stats.buffer_size
            ));
        }
        let finite = [
            stats.max_callback_us,
            stats.avg_callback_us,
            stats.cpu_load,
            stats.budget_us,
        ]
        .iter()
        .all(|v| v.is_finite() && *v >= 0.0);
        if !finite {
            failed.push(format!("latency stats not finite: {stats:?}"));
        }
        if stats.xrun_count > MAX_XRUNS {
            failed.push(format!("{} xruns, budget {MAX_XRUNS}", stats.xrun_count));
        }
        if self.steady_underruns > MAX_STEADY_UNDERRUNS {
            failed.push(format!(
                "{} steady-state underruns, budget {MAX_STEADY_UNDERRUNS}",
                self.steady_underruns
            ));
        }
        if self.output.non_finite > 0 {
            failed.push(format!(
                "{} non-finite output samples",
                self.output.non_finite
            ));
        }
        if self.output.overflow > 0 {
            failed.push(format!(
                "{} output samples over {}",
                self.output.overflow,
                checker::OVERFLOW_LIMIT
            ));
        }
        failed
    }

    fn print(&self) {
        let c = &self.counts;
        let s = &self.latency;
        println!(
            "soak: actions loops={} loop-exits={} hotcue-jumps={} keylock-toggles={} fx-changes={} seeks={}",
            c.loops, c.loop_exits, c.hotcue_jumps, c.keylock_toggles, c.fx_changes, c.seeks
        );
        println!(
            "soak: {} callbacks, xruns={}, max callback {:.0} us (budget {:.0} us), avg cpu {:.1}%",
            s.callback_count,
            s.xrun_count,
            s.max_callback_us,
            s.budget_us,
            100.0 * s.cpu_load
        );
        println!(
            "soak: steady-state deck underruns={}, non-finite={}, overflow={}, peak={:.3}",
            self.steady_underruns, self.output.non_finite, self.output.overflow, self.output.peak
        );
    }
}

/// Runs the scripted soak for `seconds` of paced (wall-clock) time and returns the outcome.
fn run_soak(seconds: u32) -> Outcome {
    let dir = TempDir::new("openmixer_soak");
    let (engine, audio) = DjEngine::new(2);
    let router = ParameterRouter::new();
    engine.register(&router);
    let (checked, output) = OutputChecker::new(audio);
    let mut device = MockAudioDevice::new(f64::from(RATE), BLOCK, 0, 4);

    for d in 0..engine.num_decks() {
        let hz = 220.0 * (d + 1) as f64;
        let file = dir.file(&format!("tone{d}.wav"));
        tone::write_tone(&file, hz, TONE_SECONDS, RATE);
        engine.deck(d).load(&file).expect("soak: load tone");
        engine.deck(d).play();
        assert!(router.set_parameter(&format!("mixer.ch{d}.fader"), 0.8));
    }

    device.start(Box::new(checked)).expect("soak: start device");
    device.start_realtime();

    let mut scenario = Scenario::new(&engine, &router);
    let start = Instant::now();
    let mut last_tick = Duration::ZERO;
    scenario.begin(0);
    while start.elapsed() < Duration::from_secs(u64::from(seconds)) {
        let elapsed = start.elapsed();
        engine.update_control((elapsed - last_tick).as_secs_f64());
        last_tick = elapsed;
        scenario.step(elapsed.as_millis() as u64, elapsed.as_secs_f64());
        std::thread::sleep(CONTROL_PERIOD);
    }

    device.stop_realtime();
    device.stop();
    Outcome {
        seconds,
        latency: device.latency_stats(),
        output: output.report(),
        steady_underruns: scenario.steady_underruns(),
        counts: scenario.counts(),
    }
}

fn check(seconds: u32) {
    let outcome = run_soak(seconds);
    outcome.print();
    let failed = outcome.failures();
    assert!(failed.is_empty(), "soak failed:\n  {}", failed.join("\n  "));
}

/// 30 s of paced simulated mix. Roughly 30 s of wall time.
#[test]
fn soak_short() {
    check(30);
}

/// The 10-minute paced-mix soak gate. Takes 10 minutes of wall time.
#[test]
#[ignore = "10-minute soak: cargo test -p om-audio --test soak -- --ignored --nocapture"]
fn soak_10min() {
    check(600);
}

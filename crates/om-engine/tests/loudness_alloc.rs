//! Allocation proof for the loudness meter. A guard allocator aborts the process on any heap
//! allocation made inside an `assert_no_alloc` scope on the current thread, so these tests pass only
//! if the guarded code never allocates. Debug builds only (the guard is compiled out in release).
//!
//! * The audio side: the tap, and the whole engine callback with the meter enabled.
//! * The worker side: `ebur128` itself in steady state, in the histogram mode the meter uses.
#![cfg(debug_assertions)]

use assert_no_alloc::{assert_no_alloc, AllocDisabler};
use om_engine::{AudioCallback, DjEngine, Loudness, LoudnessAnalyser};

#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

const RATE: f64 = 48_000.0;

fn tone(frames: usize, level: f32) -> Vec<f32> {
    (0..frames)
        .map(|n| level * (2.0 * std::f32::consts::PI * 1000.0 * n as f32 / RATE as f32).sin())
        .collect()
}

#[test]
fn the_audio_side_tap_does_not_allocate() {
    let (mut tap, control) = Loudness::create();
    tap.prepare(RATE);
    control.enable().expect("worker starts");
    let block = tone(512, 0.3);
    // Enough blocks to fill the ring faster than the worker drains it, so both the copy path and
    // the full-ring (drop and count) path run under the guard.
    assert_no_alloc(|| {
        for _ in 0..4000 {
            tap.process(&block, &block);
        }
    });
    assert!(
        control.reading().dropped_frames > 0,
        "the full-ring path was not exercised"
    );
}

#[test]
fn the_engine_callback_with_the_meter_on_does_not_allocate() {
    let (engine, mut audio) = DjEngine::new(2);
    engine.loudness().enable().expect("worker starts");
    audio.prepare(RATE, 256);
    let mut left = vec![0.0_f32; 256];
    let mut right = vec![0.0_f32; 256];
    assert_no_alloc(|| {
        for _ in 0..2000 {
            let mut outputs: [&mut [f32]; 2] = [&mut left, &mut right];
            audio.process(&[], &mut outputs);
        }
    });
    assert!(
        om_testing::wait::wait_until(
            std::time::Duration::from_secs(10),
            om_testing::wait::DEFAULT_POLL,
            || engine.loudness().reading().frames_measured > 0
        ),
        "the worker saw no audio"
    );
}

#[test]
fn ebur128_does_not_allocate_while_measuring_in_histogram_mode() {
    let mut analyser = LoudnessAnalyser::new(48_000).expect("meter");
    let mono = tone(48_000, 0.3);
    let stereo: Vec<f32> = mono.iter().flat_map(|v| [*v, *v]).collect();
    // Warm up: any one-time growth (history, scratch) happens here, outside the guard.
    for _ in 0..8 {
        analyser.add_interleaved(&stereo);
    }
    let _ = analyser.reading();
    assert_no_alloc(|| {
        for _ in 0..60 {
            for chunk in stereo.chunks(8192) {
                analyser.add_interleaved(chunk);
                let _ = analyser.reading();
            }
        }
    });
}

/// Why the meter uses histogram mode. In queue mode the crate's gating history is a `VecDeque` that
/// holds every gating block of the session, so it allocates once past its initial reservation (500 s
/// of audio) or when asked for the global loudness or range. The guard aborts the process, so the
/// check runs the child test below in a second process and expects it to die.
#[test]
fn ebur128_queue_mode_allocates_which_is_why_the_meter_does_not_use_it() {
    let exe = std::env::current_exe().expect("test binary path");
    let status = std::process::Command::new(exe)
        .args(["--exact", "queue_mode_child", "--nocapture"])
        .env("OM_QUEUE_MODE_CHILD", "1")
        .stderr(std::process::Stdio::null())
        .status()
        .expect("child runs");
    assert!(
        !status.success(),
        "queue mode no longer allocates in steady state; histogram mode may not be needed"
    );
}

#[test]
fn queue_mode_child() {
    if std::env::var_os("OM_QUEUE_MODE_CHILD").is_none() {
        return;
    }
    use ebur128::{EbuR128, Mode};
    let mut meter = EbuR128::new(
        2,
        48_000,
        Mode::M | Mode::S | Mode::I | Mode::LRA | Mode::TRUE_PEAK,
    )
    .expect("queue meter");
    let stereo: Vec<f32> = tone(48_000, 0.3).iter().flat_map(|v| [*v, *v]).collect();
    for _ in 0..8 {
        meter.add_frames_f32(&stereo).expect("add");
    }
    assert_no_alloc(|| {
        // 10 minutes: past the 5000 gating blocks (500 s) the crate reserves up front.
        for _ in 0..600 {
            meter.add_frames_f32(&stereo).expect("add");
            let _ = meter.loudness_global();
            let _ = meter.loudness_range();
        }
    });
}

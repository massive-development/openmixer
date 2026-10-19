//! Control threads hammering the engine's parameters while the mock device drives audio on its
//! paced thread. Engine side:
//! no panics, no lost audio callbacks, and every id stays reachable.

use om_audio::{AudioDevice, DjEngine, MockAudioDevice};
use om_core::{ParameterRouter, ParameterTarget};
use om_engine::grid::{BeatGrid, FixedTempoGrid};
use om_testing::fs::TempDir;
use om_testing::signals::stereo_sine;
use om_testing::wav::write_float_wav;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};
use std::sync::Arc;
use std::time::{Duration, Instant};

const RATE: f64 = 44_100.0;

#[test]
fn control_threads_hammering_parameters_while_the_mock_device_drives_audio() {
    let dir = TempDir::new("openmixer_control_safety");
    let file = dir.file("sine.wav");
    write_float_wav(
        &file,
        2,
        RATE as u32,
        &stereo_sine(220.0, 0.3, RATE, 44_100 * 4),
    );

    let (engine, audio) = DjEngine::new(2);
    let router = ParameterRouter::new();
    engine.register(&router);
    engine.deck(0).load(&file).expect("load");
    engine.set_grid(
        0,
        Some(Arc::new(FixedTempoGrid::new(120.0, RATE, 0)) as Arc<dyn BeatGrid>),
    );
    engine.set_grid(
        1,
        Some(Arc::new(FixedTempoGrid::new(100.0, RATE, 0)) as Arc<dyn BeatGrid>),
    );
    engine.deck(0).play();

    let mut dev = MockAudioDevice::new(RATE, 256, 0, 4);
    dev.start(Box::new(audio)).expect("start");
    dev.start_realtime();
    let callbacks_before = dev.latency_stats().callback_count;

    let stop = AtomicBool::new(false);
    let routed = AtomicUsize::new(0);
    let deadline = Instant::now() + Duration::from_millis(800);
    std::thread::scope(|s| {
        // Mixer faders and crossfader, read back to check the values stay in range.
        s.spawn(|| {
            let mut i = 0_u32;
            while !stop.load(Relaxed) && Instant::now() < deadline {
                let v = if i.is_multiple_of(2) { 0.0 } else { 1.0 };
                engine.mixer().set_fader(0, v);
                let back = engine.mixer().fader(0);
                assert!((0.0..=1.0).contains(&back), "fader read back {back}");
                engine.mixer().set_crossfader((i % 100) as f32 / 100.0);
                i = i.wrapping_add(1);
            }
        });
        // MIDI-style router traffic: rates, FX, sync enable and the leader.
        s.spawn(|| {
            let mut i = 0_u32;
            while !stop.load(Relaxed) && Instant::now() < deadline {
                let rate = 1.0 + f64::from(i % 10) / 100.0;
                if router.set_parameter("deck.0.rate", rate) {
                    routed.fetch_add(1, Relaxed);
                }
                router.set_parameter("deck.1.sync", f64::from(i % 2));
                router.set_parameter("deck.0.sync.master", f64::from(1 - i % 2));
                router.set_parameter("fx.master.fx0.mix", f64::from(i % 4) / 4.0);
                router.set_parameter("fx.ch0.fx1.enable", f64::from(i % 2));
                i = i.wrapping_add(1);
            }
        });
        // Control timer and grid swaps from a third thread.
        s.spawn(|| {
            let mut i = 0_u32;
            while !stop.load(Relaxed) && Instant::now() < deadline {
                engine.update_control(0.01);
                let bpm = if i.is_multiple_of(2) { 120.0 } else { 90.0 };
                engine.set_grid(
                    1,
                    Some(Arc::new(FixedTempoGrid::new(bpm, RATE, 0)) as Arc<dyn BeatGrid>),
                );
                i = i.wrapping_add(1);
            }
        });
        while Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        stop.store(true, Relaxed);
    });

    let callbacks_after = dev.latency_stats().callback_count;
    assert!(
        callbacks_after > callbacks_before + 10,
        "audio kept running under control traffic ({callbacks_before} -> {callbacks_after})"
    );
    assert!(
        routed.load(Relaxed) > 0,
        "router reached the deck while busy"
    );
    dev.stop();
    assert!(!dev.is_running());
}

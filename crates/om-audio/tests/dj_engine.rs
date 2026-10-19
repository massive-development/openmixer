//! Port of tests/unit/DjEngineTest.cpp, driven through the mock device.

mod common;

use common::{magnitude, sine_stereo, wait_until, write_float_wav, TempDir};
use om_audio::{AudioDevice, DjEngine, MockAudioDevice};
use std::time::Duration;

const RATE: f64 = 44_100.0;
const FRAMES: usize = 44_100;

fn write_sine(dir: &TempDir) -> std::path::PathBuf {
    let path = dir.file("sine.wav");
    write_float_wav(
        &path,
        2,
        RATE as u32,
        &sine_stereo(440.0, 0.5, FRAMES, RATE),
    );
    path
}

/// Renders blocks until `cond` holds, or gives up after 3 s (the decoder has no signal to wait on).
fn pump_until(dev: &MockAudioDevice, mut cond: impl FnMut() -> bool) -> bool {
    wait_until(
        || {
            let done = cond();
            if !done {
                dev.render_block();
            }
            done
        },
        Duration::from_secs(3),
    )
}

#[test]
fn silent_when_nothing_is_loaded() {
    let (_engine, audio) = DjEngine::new(2);
    let mut dev = MockAudioDevice::new(RATE, 256, 0, 4);
    dev.start(Box::new(audio)).expect("start");
    dev.render_blocks(4);
    for channel in dev.last_output() {
        assert_eq!(magnitude(&channel[..256]), 0.0);
    }
}

#[test]
fn deck_audio_reaches_master_pfl_reaches_cue_fader_gates_master() {
    let dir = TempDir::new("openmixer_engine_test");
    let file = write_sine(&dir);
    let (engine, audio) = DjEngine::new(2);
    let mut dev = MockAudioDevice::new(RATE, 256, 0, 4);
    engine.deck(0).load(&file).expect("load");
    dev.start(Box::new(audio)).expect("start");
    engine.mixer().set_pfl(0, true);
    engine.mixer().set_fader(0, 0.0);
    engine.deck(0).play();

    assert!(pump_until(&dev, || engine.deck(0).position_frames() > 2000));
    let out = dev.last_output();
    assert!(
        magnitude(&out[0][..256]) < 1e-6,
        "fader at zero silences master"
    );
    assert!(
        magnitude(&out[2][..256]) > 0.1,
        "PFL carries pre-fader audio"
    );

    engine.mixer().set_fader(0, 1.0);
    dev.render_blocks(4);
    let out = dev.last_output();
    assert!(magnitude(&out[0][..256]) > 0.1);
    assert!(engine.mixer().channel_peak(0) > 0.1);
}

#[test]
fn works_on_a_two_channel_and_a_one_channel_device() {
    for outs in [1, 2] {
        let dir = TempDir::new("openmixer_engine_test");
        let file = write_sine(&dir);
        let (engine, audio) = DjEngine::new(2);
        let mut dev = MockAudioDevice::new(RATE, 256, 0, outs);
        engine.deck(1).load(&file).expect("load");
        dev.start(Box::new(audio)).expect("start");
        engine.deck(1).play();
        assert!(
            pump_until(&dev, || engine.deck(1).position_frames() > 2000),
            "outs={outs}"
        );
        // Unpaced pumping can outrun the decoder: let it refill before sampling output.
        wait_until(
            || engine.deck(1).buffered_frames() >= 1024,
            Duration::from_secs(3),
        );
        dev.render_blocks(2);
        assert!(magnitude(&dev.last_output()[0][..256]) > 0.1, "outs={outs}");
    }
}

#[test]
fn stop_hands_the_engine_back_and_a_restart_keeps_deck_and_mixer_state() {
    let dir = TempDir::new("openmixer_engine_test");
    let file = write_sine(&dir);
    let (engine, audio) = DjEngine::new(2);
    let mut dev = MockAudioDevice::new(RATE, 256, 0, 4);
    engine.deck(0).load(&file).expect("load");
    dev.start(Box::new(audio)).expect("start");
    engine.mixer().set_fader(0, 0.5);
    engine.mixer().set_crossfader(0.25);
    engine.deck(0).play();
    assert!(pump_until(&dev, || engine.deck(0).position_frames() > 2000));
    let before = engine.deck(0).position_frames();

    let engine_audio = dev.stop().expect("stop returns the callback it was driving");
    assert!(!dev.is_running());
    assert!(engine.deck(0).is_loaded(), "stop keeps the loaded track");
    assert!(engine.deck(0).is_playing(), "stop does not pause the deck");

    dev.start(engine_audio).expect("restart with the returned engine");
    assert!(engine.deck(0).is_loaded());
    assert!(
        pump_until(&dev, || engine.deck(0).position_frames() > before + 2000),
        "the deck keeps playing from its position after a restart"
    );
    assert_eq!(engine.mixer().fader(0), 0.5, "fader survives a restart");
    assert_eq!(engine.mixer().crossfader(), 0.25, "crossfader survives a restart");
    assert!(magnitude(&dev.last_output()[0][..256]) > 0.1);
}

/// Port of DeckPlaybackSlow.DeckRunsUnderAMockAudioDevice: the deck plays through the paced mock
/// device thread, not through manual renders.
#[test]
fn deck_runs_under_a_mock_audio_device() {
    let dir = TempDir::new("openmixer_engine_test");
    let file = write_sine(&dir);
    let (engine, audio) = DjEngine::new(1);
    let mut dev = MockAudioDevice::new(RATE, 256, 0, 2);
    engine.deck(0).load(&file).expect("load");
    dev.start(Box::new(audio)).expect("start");
    dev.render_blocks(1);
    assert!(
        pump_until(&dev, || engine.deck(0).buffered_frames() > 1024),
        "decoder did not buffer in time"
    );
    engine.deck(0).play();
    dev.start_realtime();
    assert!(
        wait_until(
            || engine.deck(0).position_frames() > 1000,
            Duration::from_secs(5)
        ),
        "the paced device did not advance the deck"
    );
    assert!(engine.deck(0).position_frames() > 1000);
    assert!(dev.latency_stats().callback_count > 0);
    dev.stop();
}

/// A track that fails to load, or a truncated file, while another deck plays must leave the
/// playing deck and the engine running.
#[test]
fn a_bad_or_truncated_file_mid_play_does_not_stop_the_engine() {
    let dir = TempDir::new("openmixer_engine_test");
    let good = write_sine(&dir);
    let bytes = std::fs::read(&good).expect("read fixture");
    let truncated = dir.file("truncated.wav");
    std::fs::write(&truncated, &bytes[..bytes.len() / 10]).expect("write truncated");
    let garbage = dir.file("garbage.wav");
    std::fs::write(&garbage, b"RIFF but not a wave file at all").expect("write garbage");

    let (engine, audio) = DjEngine::new(2);
    let mut dev = MockAudioDevice::new(RATE, 256, 0, 4);
    engine.deck(0).load(&good).expect("load");
    dev.start(Box::new(audio)).expect("start");
    engine.deck(0).play();
    assert!(pump_until(&dev, || engine.deck(0).position_frames() > 2000));

    assert!(engine.deck(1).load(&garbage).is_err(), "garbage is rejected");
    // A truncated file may load (its readable head plays) or be rejected; either is fine.
    let _ = engine.deck(1).load(&truncated);
    engine.deck(1).play();
    let before = engine.deck(0).position_frames();
    assert!(
        pump_until(&dev, || engine.deck(0).position_frames() > before + 4000),
        "the playing deck keeps running after the bad load"
    );
    assert!(!engine.deck(0).is_errored());
    assert!(!engine.deck(1).is_errored(), "a bad file is not a decoder fault");
    assert!(
        magnitude(&dev.last_output()[0][..256]) > 0.1,
        "the playing deck is still audible on master"
    );
}

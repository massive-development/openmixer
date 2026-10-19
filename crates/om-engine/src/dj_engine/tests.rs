//! Engine-level cases from tests/unit/DjEngineTest.cpp that need no device. The device-driven
//! cases live in `crates/om-audio/tests/dj_engine.rs`.

use super::DjEngine;
use crate::callback::AudioCallback;

const RATE: f64 = 44_100.0;

fn run(audio: &mut impl AudioCallback, channels: usize, frames: usize) -> Vec<Vec<f32>> {
    let mut bufs = vec![vec![-9.0_f32; frames]; channels];
    let mut outs: Vec<&mut [f32]> = bufs.iter_mut().map(Vec::as_mut_slice).collect();
    audio.process(&[], &mut outs);
    bufs
}

#[test]
fn silent_when_nothing_is_loaded() {
    let (_engine, mut audio) = DjEngine::new(2);
    audio.prepare(RATE, 256);
    for buf in run(&mut audio, 4, 256) {
        assert!(buf.iter().all(|&s| s == 0.0));
    }
}

#[test]
fn oversize_blocks_are_chunked() {
    let (_engine, mut audio) = DjEngine::new(2);
    audio.prepare(RATE, 64);
    for buf in run(&mut audio, 2, 300) {
        assert!(
            buf.iter().all(|&s| s == 0.0),
            "300 frames rendered in 64-frame chunks"
        );
    }
}

#[test]
fn unprepared_engine_outputs_silence() {
    let (_engine, mut audio) = DjEngine::new(2);
    for buf in run(&mut audio, 4, 128) {
        assert!(buf.iter().all(|&s| s == 0.0));
    }
}

#[test]
fn channel_count_is_clamped_to_the_mixer() {
    let (engine, _audio) = DjEngine::new(9);
    assert_eq!(engine.num_decks(), 4);
}

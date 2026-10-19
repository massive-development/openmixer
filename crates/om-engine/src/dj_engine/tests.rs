//! Engine-level cases that need no device. The device-driven
//! cases live in `crates/om-audio/tests/dj_engine.rs`.

use super::DjEngine;
use crate::callback::AudioCallback;
use crate::mixer::ChannelInput;
use crate::ProcessStage;
use om_core::ParameterTarget;
use std::sync::{Arc, Mutex};

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

/// Records every `prepare` call a probe stage receives.
#[derive(Clone, Default)]
struct PrepareLog(Arc<Mutex<Vec<(f64, usize)>>>);

impl PrepareLog {
    fn last(&self) -> Option<(f64, usize)> {
        self.0.lock().unwrap().last().copied()
    }
}

struct Probe(PrepareLog);

impl ProcessStage for Probe {
    fn prepare(&mut self, sample_rate: f64, max_block_size: usize) {
        self.0
             .0
            .lock()
            .unwrap()
            .push((sample_rate, max_block_size));
    }

    fn process(&mut self, _left: &mut [f32], _right: &mut [f32]) {}
}

#[test]
fn stages_are_prepared_at_the_device_rate_and_block() {
    for rate in [48_000.0, 96_000.0] {
        let (engine, mut audio) = DjEngine::new(2);
        // Queued before prepare: must reach its slot and be prepared with the device rate.
        let queued = PrepareLog::default();
        assert!(engine
            .mixer()
            .set_channel_insert(0, 1, Some(Box::new(Probe(queued.clone())))));
        audio.prepare(rate, 256);
        assert_eq!(
            queued.last(),
            Some((rate, 256)),
            "rate {rate}: queued stage"
        );

        // Queued after prepare: uses the rate and block the engine was prepared with.
        let late = PrepareLog::default();
        assert!(engine
            .mixer()
            .set_channel_insert(1, 1, Some(Box::new(Probe(late.clone())))));
        assert_eq!(late.last(), Some((rate, 256)), "rate {rate}: late stage");
    }
}

#[test]
fn echo_repeat_lands_on_the_beat_grid_at_48_khz() {
    const RATE: f64 = 48_000.0;
    const FRAMES: usize = 256;
    const BLOCKS: usize = 120;
    let (engine, mut audio) = DjEngine::new(2);
    audio.prepare(RATE, FRAMES);
    let rack = engine.fx().channel_rack(0).expect("channel 0 rack");
    let echo = rack.find_unit("echo").expect("echo in the default rack");
    rack.set_tempo(120.0);
    for (id, value) in [
        (format!("ch0.fx{echo}.enable"), 1.0),
        (format!("ch0.fx{echo}.mix"), 1.0),
        (format!("ch0.fx{echo}.feedback"), 0.0),
        (format!("ch0.fx{echo}.time"), 1.0),
    ] {
        assert!(engine.fx().set_parameter(&id, value), "{id}");
    }

    let total = FRAMES * BLOCKS;
    let mut impulse = vec![0.0_f32; total];
    impulse[0] = 1.0;
    let (mut ml, mut mr) = (vec![0.0_f32; total], vec![0.0_f32; total]);
    let (mut cl, mut cr) = (vec![0.0_f32; total], vec![0.0_f32; total]);
    let inputs = [ChannelInput {
        left: Some(&impulse),
        right: Some(&impulse),
    }];
    audio
        .mixer
        .process(&inputs, &mut ml, &mut mr, &mut cl, &mut cr);

    // The dry impulse sits at frame 0; skip past it and look for the repeat.
    let peak = ml
        .iter()
        .enumerate()
        .skip(1000)
        .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
        .map(|(i, _)| i)
        .unwrap();
    // One beat at 120 BPM is 0.5 s: 24000 frames at 48 kHz (22050 would mean 44.1 kHz). The
    // channel EQ and filter add a few milliseconds, so allow 500 frames; the rates differ by 1950.
    assert!(
        (peak as i64 - 24_000).abs() <= 500,
        "echo repeat at frame {peak}, expected about 24000"
    );
}

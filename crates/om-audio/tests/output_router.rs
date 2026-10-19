//! Engine-backed cases of tests/unit/OutputRouterTest.cpp: the router's device output must equal
//! the engine's own master and cue output, block for block. The rest of that file is ported in
//! `om-engine/tests/output_router.rs`.

mod common;

use common::{magnitude, sine_stereo, wait_until, write_float_wav, TempDir};
use om_audio::{AudioCallback, DjEngine, OutputRouter};
use std::time::Duration;

const RATE: f64 = 44_100.0;
const BLOCK: usize = 256;

/// Forwards to the real engine and keeps what it rendered, so each router block can be compared
/// with the engine's own output.
struct CapturingEngine<'a> {
    engine: &'a mut dyn AudioCallback,
    capture: [Vec<f32>; 4],
    captured: usize,
}

impl AudioCallback for CapturingEngine<'_> {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        self.engine.process(inputs, outputs);
        let n = outputs[0].len();
        for (c, out) in outputs.iter().enumerate() {
            self.capture[c][..n].copy_from_slice(&out[..n]);
        }
        self.captured = n;
    }
}

fn expect_router_matches_engine(device_channels: usize) {
    let dir = TempDir::new("openmixer_router_test");
    let file = dir.file("tone.wav");
    write_float_wav(
        &file,
        2,
        RATE as u32,
        &sine_stereo(220.0, 0.3, 44_100 * 6, RATE),
    );

    let (engine, mut audio) = DjEngine::new(2);
    audio.prepare(RATE, BLOCK);
    engine.deck(0).load(&file).expect("load");
    // Service the seek handshake and let the decoder run ahead before playing.
    let mut pump = [[0.0_f32; 64]; 4];
    assert!(wait_until(
        || {
            let [a, b, c, d] = &mut pump;
            let mut outs: [&mut [f32]; 4] = [a, b, c, d];
            audio.process(&[], &mut outs);
            engine.deck(0).buffered_frames() > 8192
        },
        Duration::from_secs(20)
    ));
    engine.deck(0).play();

    let mut router = OutputRouter::new().0;
    router.prepare(BLOCK);
    let mut source = CapturingEngine {
        engine: &mut audio,
        capture: std::array::from_fn(|_| vec![0.0; BLOCK]),
        captured: 0,
    };
    let mut device: Vec<Vec<f32>> = vec![vec![-9.0; BLOCK]; device_channels];
    let mut peak = 0.0_f32;
    for block in 0..40 {
        wait_until(
            || engine.deck(0).buffered_frames() >= 2048,
            Duration::from_secs(20),
        );
        {
            let mut outs: Vec<&mut [f32]> = device.iter_mut().map(Vec::as_mut_slice).collect();
            router.process(&mut source, &mut outs, BLOCK);
        }
        assert_eq!(source.captured, BLOCK);
        for (c, (dev, cap)) in device.iter().zip(&source.capture).enumerate() {
            let label = if c < 2 { "master" } else { "cue" };
            assert!(
                dev[..BLOCK] == cap[..BLOCK],
                "{label} ch{c} block {block} differs from the engine"
            );
        }
        peak = peak.max(magnitude(&device[0]));
    }
    assert!(peak > 0.01, "the comparison must run on real audio");
}

#[test]
fn matches_the_engine_directly_on_a_two_channel_device() {
    expect_router_matches_engine(2);
}

#[test]
fn matches_the_engine_directly_on_a_four_channel_device() {
    expect_router_matches_engine(4);
}

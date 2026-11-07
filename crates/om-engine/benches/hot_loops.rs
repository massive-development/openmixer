//! Audio-thread hot loops (issue #226): bounds-check cost. Run with `cargo bench -p om-engine`.

use divan::{black_box, Bencher};
use om_engine::fx::{FxFactory, FxUnit};
use om_engine::ProcessStage;

const SR: f64 = 48_000.0;
const BLOCK: usize = 512;

fn main() {
    divan::main();
}

fn input() -> (Vec<f32>, Vec<f32>) {
    let l = (0..BLOCK)
        .map(|i| ((i as f32) * 0.05).sin() * 0.5)
        .collect();
    let r = (0..BLOCK)
        .map(|i| ((i as f32) * 0.07).cos() * 0.5)
        .collect();
    (l, r)
}

#[divan::bench(args = FxFactory::builtin_names())]
fn fx_unit(bencher: Bencher, name: &str) {
    let mut unit = FxUnit::new(FxFactory::with_built_ins().create(name).expect("built-in"));
    unit.set_enabled(true);
    unit.set_mix(1.0);
    unit.prepare(SR, BLOCK);
    let (l0, r0) = input();
    let (mut l, mut r) = (l0.clone(), r0.clone());
    // Settle the enable ramp before timing.
    for _ in 0..64 {
        l.copy_from_slice(&l0);
        r.copy_from_slice(&r0);
        unit.process(&mut l, &mut r);
    }
    bencher.bench_local(|| {
        l.copy_from_slice(&l0);
        r.copy_from_slice(&r0);
        unit.process(black_box(&mut l), black_box(&mut r));
    });
}

/// Eight looping pads, one 512-frame block, fractional pitch so the cubic path runs.
#[divan::bench]
fn sampler_8_loop_pads(bencher: Bencher) {
    use om_engine::{PadMode, Sample, SamplerAudio};
    use std::sync::Arc;
    let (mut audio, control) = SamplerAudio::new();
    audio.prepare(SR, BLOCK);
    let data: Vec<f32> = (0..48_000)
        .flat_map(|n| {
            let v = ((n as f32) * 0.01).sin() * 0.5;
            [v, -v]
        })
        .collect();
    let sample = Arc::new(Sample::from_interleaved("bench", SR, data).expect("sample"));
    for pad in 0..8u8 {
        control.assign(pad, Arc::clone(&sample)).expect("assign");
        control.set_mode(pad, PadMode::Loop);
        control.set_pitch(pad, 0.37 + f32::from(pad));
        control.trigger(pad, 1.0);
    }
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    // Let the commands apply and the start ramps finish.
    for _ in 0..8 {
        audio.process(&mut l, &mut r);
    }
    bencher.bench_local(|| {
        audio.process(black_box(&mut l), black_box(&mut r));
    });
}

/// Four-channel mixer, one 512-frame block, PFL on for every channel.
#[divan::bench]
fn mixer_4ch(bencher: Bencher) {
    use om_engine::{ChannelInput, Mixer};
    let (mut mixer, ctl) = Mixer::new(4);
    mixer.prepare(SR, BLOCK);
    for ch in 0..4 {
        ctl.set_pfl(ch, true);
    }
    let (l, r) = input();
    let mut ml = vec![0.0; BLOCK];
    let mut mr = vec![0.0; BLOCK];
    let mut cl = vec![0.0; BLOCK];
    let mut cr = vec![0.0; BLOCK];
    bencher.bench_local(|| {
        let inputs = [ChannelInput {
            left: Some(&l),
            right: Some(&r),
        }; 4];
        mixer.process(black_box(&inputs), &mut ml, &mut mr, &mut cl, &mut cr);
    });
}

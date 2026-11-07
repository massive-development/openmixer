//! Analysis hot loops (issue #226): bounds-check cost. Run with `cargo bench -p om-analysis`.

use divan::{black_box, Bencher};
use om_analysis::{
    compute_band_onsets, compute_chroma, AudioClip, ChromaOptions, FrequencyBand, MagnitudeStft,
    NullAnalysisContext, OnsetOptions,
};

const RATE: f64 = 44_100.0;

fn main() {
    divan::main();
}

/// Ten seconds of a decaying two-tone burst train, deterministic.
fn clip() -> AudioClip {
    let n = (RATE * 10.0) as usize;
    let samples = (0..n)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let env = (-(t * 2.0).fract() * 8.0).exp();
            env * (0.6 * (t * 440.0 * std::f32::consts::TAU).sin()
                + 0.3 * (t * 1_320.0 * std::f32::consts::TAU).sin())
        })
        .collect();
    AudioClip::new(samples, RATE)
}

#[divan::bench]
fn stft_frame(bencher: Bencher) {
    let clip = clip();
    let mut stft = MagnitudeStft::new(10, 512);
    let mut mags = Vec::new();
    let mut index = 0;
    bencher.bench_local(|| {
        stft.compute_frame(black_box(&clip.samples), index, &mut mags);
        index = (index + 1) % 800;
    });
}

#[divan::bench(sample_count = 20)]
fn band_onsets(bencher: Bencher) {
    let clip = clip();
    let bands = [FrequencyBand::default()];
    let options = OnsetOptions::default();
    bencher.bench_local(|| {
        compute_band_onsets(black_box(&clip), &options, &bands, &NullAnalysisContext)
    });
}

#[divan::bench(sample_count = 20)]
fn chroma(bencher: Bencher) {
    let clip = clip();
    let options = ChromaOptions::default();
    bencher.bench_local(|| compute_chroma(black_box(&clip), &options, &NullAnalysisContext));
}

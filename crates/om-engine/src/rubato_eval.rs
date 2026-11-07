//! Evidence for the rubato evaluation (#221): rubato's `set_resample_ratio` and
//! `process_into_buffer` allocate nothing, and what rubato would cost and buy against the deck's
//! linear-interpolation varispeed (`deck/playback.rs`, `render_interpolated`). The global
//! allocator is installed by `alloc_guard`; this module only opens guard scopes.

use assert_no_alloc::assert_no_alloc;
use audioadapter_buffers::direct::InterleavedSlice;
use rubato::{
    Adjustable, Async, FixedAsync, PolynomialDegree, Resampler, SincInterpolationParameters,
    SincInterpolationType, WindowFunction,
};
use std::time::Instant;

const BLOCK: usize = 256;
const RATE: f64 = 44_100.0;

fn sinc(len: usize) -> Async<f32> {
    let params = SincInterpolationParameters {
        sinc_len: len,
        f_cutoff: None,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 128,
        window: WindowFunction::BlackmanHarris2,
    };
    Async::<f32>::new_sinc(1.0, 1.5, &params, BLOCK, 2, FixedAsync::Output).unwrap()
}

fn poly() -> Async<f32> {
    Async::<f32>::new_poly(
        1.0,
        1.5,
        PolynomialDegree::Cubic,
        BLOCK,
        2,
        FixedAsync::Output,
    )
    .unwrap()
}

fn tone(freq: f64, frames: usize) -> Vec<f32> {
    (0..frames)
        .flat_map(|n| {
            let v = (std::f64::consts::TAU * freq * n as f64 / RATE).sin() as f32 * 0.5;
            [v, v]
        })
        .collect()
}

/// Runs `blocks` blocks, moving the ratio each block (ramped and not, in and out of bounds).
fn drive(resampler: &mut Async<f32>, input: &[f32], out: &mut [f32], blocks: usize) {
    let mut read = 0usize;
    for b in 0..blocks {
        let ratio = 1.0 + 0.08 * ((b as f64) * 0.37).sin();
        let ramp = b % 2 == 0;
        resampler.set_resample_ratio(ratio, ramp).unwrap();
        // Out of bounds is an error value, not an allocation.
        assert!(resampler.set_resample_ratio(100.0, false).is_err());
        resampler.set_resample_ratio(ratio, ramp).unwrap();
        let need = resampler.input_frames_next();
        let src = InterleavedSlice::new(&input[read * 2..(read + need) * 2], 2, need).unwrap();
        let mut dst = InterleavedSlice::new_mut(&mut out[..BLOCK * 2], 2, BLOCK).unwrap();
        resampler.process_into_buffer(&src, &mut dst, None).unwrap();
        read += need;
    }
}

#[test]
fn rubato_ratio_changes_and_processing_do_not_allocate() {
    let input = tone(1000.0, BLOCK * 2 * 200);
    let mut out = vec![0.0f32; BLOCK * 2];
    for mut resampler in [sinc(128), poly()] {
        // Warm up outside the guard so lazily sized state is not counted against construction.
        drive(&mut resampler, &input, &mut out, 4);
        resampler.reset();
        assert_no_alloc(|| drive(&mut resampler, &input, &mut out, 150));
    }
}

/// `cargo test -p om-engine rubato_against_linear -- --ignored --nocapture` prints cost and error
/// (the test profile optimises dependencies, so rubato runs optimised).
#[test]
#[ignore = "prints timings and error figures for the PR; not an assertion"]
fn rubato_against_linear_varispeed() {
    // Cost per 256-frame stereo block.
    let input = tone(1000.0, BLOCK * 2 * 2000);
    for (name, mut resampler) in [
        ("sinc128", sinc(128)),
        ("sinc256", sinc(256)),
        ("poly3", poly()),
    ] {
        let start = Instant::now();
        drive(&mut resampler, &input, &mut vec![0.0f32; BLOCK * 2], 1000);
        let per_block = start.elapsed().as_secs_f64() / 1000.0;
        println!(
            "{name}: {:.1} us per {BLOCK}-frame block ({:.2}% of the {:.2} ms budget), delay {} frames",
            per_block * 1e6,
            per_block / (BLOCK as f64 / RATE) * 100.0,
            BLOCK as f64 / RATE * 1e3,
            resampler.output_delay()
        );
    }

    // Error against the analytic tone at a +8% pitch step (ratio 1.08 source frames per output).
    let step = 1.08f64;
    let freq = 10_000.0;
    let n_in = 40_000usize;
    let src = tone(freq, n_in);
    let frames = (n_in as f64 / step) as usize - 400;
    let mut linear = Vec::with_capacity(frames);
    for i in 0..frames {
        let pos = i as f64 * step;
        let k = pos.floor() as usize;
        let t = (pos - k as f64) as f32;
        linear.push(src[2 * k] + (src[2 * k + 2] - src[2 * k]) * t);
    }
    // Residual after a least-squares fit of a sine at the output frequency (so a constant delay
    // and a gain change are not counted), relative to the ideal tone's rms: the distortion,
    // images and aliasing the interpolator adds. Also reports the fitted gain.
    let residual = |got: &[f32], skip: usize| {
        let w = std::f64::consts::TAU * freq * step / RATE;
        let (mut ss, mut cc, mut sc, mut sy, mut cy, mut yy) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let n = got.len() - skip;
        for (i, &v) in got.iter().enumerate().skip(skip) {
            let (sn, cs) = (w * i as f64).sin_cos();
            let y = f64::from(v);
            ss += sn * sn;
            cc += cs * cs;
            sc += sn * cs;
            sy += sn * y;
            cy += cs * y;
            yy += y * y;
        }
        let det = ss * cc - sc * sc;
        let (a, b) = ((sy * cc - cy * sc) / det, (cy * ss - sy * sc) / det);
        let explained = a * sy + b * cy;
        let rms_resid = ((yy - explained).max(0.0) / n as f64).sqrt();
        (rms_resid / (0.5 / 2f64.sqrt()), a.hypot(b) / 0.5)
    };
    let (r, g) = residual(&linear, 400);
    println!(
        "10 kHz at step 1.08: linear residual {:.1} dB, gain {g:.3}",
        20.0 * r.log10()
    );
    let mut resampler = sinc(256);
    resampler.set_resample_ratio(1.0 / step, false).unwrap();
    let mut got = Vec::new();
    let mut out = vec![0.0f32; BLOCK * 2];
    let mut read = 0usize;
    while got.len() < frames + resampler.output_delay() {
        let need = resampler.input_frames_next();
        if (read + need) * 2 > src.len() {
            break;
        }
        let s = InterleavedSlice::new(&src[read * 2..(read + need) * 2], 2, need).unwrap();
        let mut d = InterleavedSlice::new_mut(&mut out[..BLOCK * 2], 2, BLOCK).unwrap();
        resampler.process_into_buffer(&s, &mut d, None).unwrap();
        read += need;
        got.extend(out.chunks_exact(2).map(|f| f[0]));
    }
    let delay = resampler.output_delay();
    let got = &got[..got.len().min(frames + delay)];
    let (r, g) = residual(got, 400 + delay);
    println!(
        "10 kHz at step 1.08: rubato sinc256 residual {:.1} dB, gain {g:.3} (delay {delay} frames)",
        20.0 * r.log10()
    );
}

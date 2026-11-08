//! Offline sample-rate conversion helper shared by the sampler and the stem pipeline: a rubato
//! synchronous FFT resampler whose delay is a whole number of output frames. Control thread only;
//! building one allocates.

use rubato::{Fft, FixedSync, ResamplerConstructionError, WindowFunction};

/// Smallest FFT block worth using, in input frames. Shorter blocks narrow the passband.
const MIN_BLOCK_FRAMES: usize = 512;

fn gcd(a: usize, b: usize) -> usize {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

/// Builds a fixed-input FFT resampler from `rate_in` to `rate_out` Hz (whole rates).
///
/// rubato reports its delay as `fft_size_out / 2`, rounded down. An odd output block therefore
/// leaves a half-frame error after the delay is trimmed (about 0.07 rad at 1 kHz), so the block is
/// a whole number `k` of the smallest valid block (`rate_in / g` in, `rate_out / g` out, `g` the
/// gcd of the rates), with `k` even whenever that smallest output block is odd.
pub fn fft_resampler(
    rate_in: usize,
    rate_out: usize,
    channels: usize,
) -> Result<Fft<f32>, ResamplerConstructionError> {
    let g = gcd(rate_in, rate_out).max(1);
    let (min_in, min_out) = ((rate_in / g).max(1), rate_out / g);
    let mut k = MIN_BLOCK_FRAMES.div_ceil(min_in);
    if min_out % 2 == 1 && k % 2 == 1 {
        k += 1;
    }
    Fft::<f32>::new_custom(
        rate_in,
        rate_out,
        k * min_in,
        1,
        channels,
        WindowFunction::BlackmanHarris2,
        FixedSync::Input,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rubato::Resampler;

    #[test]
    fn delay_is_a_whole_number_of_frames_for_common_rate_pairs() {
        for (a, b) in [
            (96_000, 44_100),
            (44_100, 48_000),
            (48_000, 44_100),
            (22_050, 44_100),
            (44_100, 44_100),
            (88_200, 44_100),
        ] {
            let r = fft_resampler(a, b, 1).unwrap();
            let g = gcd(a, b);
            let block_out = r.input_frames_next() / (a / g) * (b / g);
            assert_eq!(
                r.output_delay() * 2,
                block_out,
                "{a}->{b}: fractional delay"
            );
        }
    }
}

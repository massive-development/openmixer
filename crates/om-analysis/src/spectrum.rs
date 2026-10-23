//! Spectral summary of audio frames for the waveform: loudness, and the log-frequency mean and
//! variance of the spectrum. Positions are octaves above [`CENTROID_MIN_HZ`] in the moments and
//! 0..1 in stored points. Stateless maths plus [`FrameAnalyzer`], which holds scratch buffers
//! (one instance per thread).

use crate::MagnitudeStft;

/// Lowest frequency the centroid axis resolves; bass below it counts as this.
pub const CENTROID_MIN_HZ: f64 = 25.0;
/// log2(16 kHz / 25 Hz): the centroid axis spans this many octaves, so 8 bits step about 1/27 octave.
pub const CENTROID_OCTAVES: f32 = 9.321_928;
/// Spread is stored as octaves of standard deviation up to this value.
pub const SPREAD_MAX_OCTAVES: f32 = 4.0;
const TILT_REFERENCE_HZ: f64 = 1000.0;
/// Exponent on the frequency tilt of the bin weights; lifts highs so hats show up against bass.
const TILT_EXPONENT: f64 = 0.5;
const SILENT_ENERGY: f64 = 1e-12;

/// Energy (mean square) with the mean and variance of log frequency, in octaves, it carries.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Moment {
    pub energy: f32,
    pub mean: f32,
    pub variance: f32,
}

/// Pools moments: energy adds up as a share-weighted mean, the mean is energy weighted, and the
/// variance is the pooled one (within each part plus the spread of the part means). Silent input
/// gives a silent, centred result.
pub fn pool<I>(parts: I) -> Moment
where
    I: IntoIterator<Item = (f32, Moment)>,
    I::IntoIter: Clone,
{
    let parts = parts.into_iter();
    let weight = |(share, m): (f32, Moment)| f64::from(share) * f64::from(m.energy);
    let share_sum: f64 = parts.clone().map(|(s, _)| f64::from(s)).sum();
    let total: f64 = parts.clone().map(weight).sum();
    if share_sum <= 0.0 {
        return Moment::default();
    }
    let energy = (total / share_sum) as f32;
    if total <= SILENT_ENERGY {
        return Moment {
            energy,
            ..Moment::default()
        };
    }
    let mean = parts
        .clone()
        .map(|p| weight(p) * f64::from(p.1.mean))
        .sum::<f64>()
        / total;
    let variance = parts
        .map(|p| {
            let off = f64::from(p.1.mean) - mean;
            weight(p) * (f64::from(p.1.variance) + off * off)
        })
        .sum::<f64>()
        / total;
    Moment {
        energy,
        mean: mean as f32,
        variance: variance as f32,
    }
}

struct BinTerm {
    octave: f32,
    weight: f32,
}

/// Turns frames of samples into [`Moment`]s through a Hann-windowed FFT.
pub struct FrameAnalyzer {
    stft: MagnitudeStft,
    magnitudes: Vec<f32>,
    terms: Vec<(usize, BinTerm)>,
}

impl FrameAnalyzer {
    pub fn new(fft_order: u32, hop: usize, sample_rate: f64) -> Self {
        let stft = MagnitudeStft::new(fft_order, hop);
        let terms = (1..stft.num_bins())
            .filter_map(|bin| {
                let hz = stft.bin_hz(bin, sample_rate);
                (CENTROID_MIN_HZ..=CENTROID_MIN_HZ * 2f64.powf(f64::from(CENTROID_OCTAVES)))
                    .contains(&hz)
                    .then(|| {
                        let tilt = (hz / TILT_REFERENCE_HZ).powf(TILT_EXPONENT);
                        let octave = (hz / CENTROID_MIN_HZ).log2();
                        (
                            bin,
                            BinTerm {
                                octave: octave as f32,
                                weight: tilt as f32,
                            },
                        )
                    })
            })
            .collect();
        Self {
            stft,
            magnitudes: Vec::new(),
            terms,
        }
    }

    pub fn hop(&self) -> usize {
        self.stft.hop()
    }

    pub fn frame_size(&self) -> usize {
        self.stft.frame_size()
    }

    pub fn num_frames(&self, num_samples: usize) -> usize {
        self.stft.num_frames(num_samples)
    }

    /// The moment of frame `index`. Energy is the Hann-weighted mean square over the whole band.
    pub fn frame(&mut self, samples: &[f32], index: usize) -> Moment {
        self.stft
            .compute_frame(samples, index, &mut self.magnitudes);
        let m = &self.magnitudes;
        let last = m.len() - 1;
        let sum_sq: f32 =
            m.iter().map(|v| v * v).sum::<f32>() * 2.0 - m[0] * m[0] - m[last] * m[last];
        // Hann scaling of the STFT magnitudes: sum of squares over 6 is the mean square.
        let energy = sum_sq / 6.0;
        let (mut w, mut wx, mut wxx) = (0.0f32, 0.0f32, 0.0f32);
        for (bin, t) in &self.terms {
            let weight = m[*bin] * t.weight;
            w += weight;
            wx += weight * t.octave;
            wxx += weight * t.octave * t.octave;
        }
        if f64::from(w) <= SILENT_ENERGY {
            return Moment {
                energy,
                ..Moment::default()
            };
        }
        let mean = wx / w;
        Moment {
            energy,
            mean,
            variance: (wxx / w - mean * mean).max(0.0),
        }
    }
}

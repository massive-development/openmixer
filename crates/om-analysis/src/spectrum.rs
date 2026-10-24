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
/// Exponent on the frequency tilt of the bin power weights; lifts highs against the bass-heavy
/// slope of music so hats and vocals move the centroid.
const TILT_EXPONENT: f64 = 0.5;
const SILENT_ENERGY: f64 = 1e-12;
/// Edges of the waveform's three bands (our own choice, not taken from any product).
pub const LOW_EDGE_HZ: f64 = 200.0;
pub const HIGH_EDGE_HZ: f64 = 3000.0;
/// Each crossover is a raised cosine in log frequency, this many octaves either side of the edge,
/// so the energy of a bin is shared out smoothly between neighbouring bands.
pub const CROSSOVER_HALF_OCTAVES: f64 = 0.25;

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
    /// Per bin, the share of its power that goes to the low, mid and high band (sums to 1).
    band_weights: Vec<[f32; 3]>,
    /// What `magnitudes` holds: the samples' address and length and the frame index, so a
    /// [`Self::frame`] followed by [`Self::frame_bands`] of the same frame transforms once.
    transformed: Option<(usize, usize, usize)>,
}

/// Weight of the part above `edge_hz` in a raised-cosine crossover of +/- [`CROSSOVER_HALF_OCTAVES`]
/// (0 well below the edge, 1 well above, 0.5 at it).
fn crossover_above(hz: f64, edge_hz: f64) -> f64 {
    let octaves = (hz / edge_hz).log2();
    let x = ((octaves / CROSSOVER_HALF_OCTAVES + 1.0) / 2.0).clamp(0.0, 1.0);
    0.5 - 0.5 * (std::f64::consts::PI * x).cos()
}

fn band_weights_at(hz: f64) -> [f32; 3] {
    if hz <= 0.0 {
        return [1.0, 0.0, 0.0];
    }
    let above_low = crossover_above(hz, LOW_EDGE_HZ);
    let above_high = crossover_above(hz, HIGH_EDGE_HZ);
    [
        (1.0 - above_low) as f32,
        (above_low * (1.0 - above_high)) as f32,
        above_high as f32,
    ]
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
        let band_weights = (0..stft.num_bins())
            .map(|bin| band_weights_at(stft.bin_hz(bin, sample_rate)))
            .collect();
        Self {
            stft,
            magnitudes: Vec::new(),
            terms,
            band_weights,
            transformed: None,
        }
    }

    /// Fills `magnitudes` for frame `index`, unless they already hold exactly that frame.
    fn transform(&mut self, samples: &[f32], index: usize) {
        let key = (samples.as_ptr() as usize, samples.len(), index);
        if self.transformed != Some(key) {
            self.stft
                .compute_frame(samples, index, &mut self.magnitudes);
            self.transformed = Some(key);
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
        self.transform(samples, index);
        let m = &self.magnitudes;
        let last = m.len() - 1;
        let sum_sq: f32 =
            m.iter().map(|v| v * v).sum::<f32>() * 2.0 - m[0] * m[0] - m[last] * m[last];
        // Hann scaling of the STFT magnitudes: sum of squares over 6 is the mean square.
        let energy = sum_sq / 6.0;
        let (mut w, mut wx, mut wxx) = (0.0f32, 0.0f32, 0.0f32);
        for (bin, t) in &self.terms {
            let weight = m[*bin] * m[*bin] * t.weight;
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

    /// Energy share of frame `index` in the low, mid and high band: the bin powers weighted by the
    /// crossovers, over the total. Sums to 1, or all zero for a silent frame. Shares the FFT with
    /// [`Self::frame`] when called for the same frame.
    pub(crate) fn frame_bands(&mut self, samples: &[f32], index: usize) -> [f32; 3] {
        self.transform(samples, index);
        let mut sums = [0.0f32; 3];
        let mut total = 0.0f32;
        for (m, w) in self.magnitudes.iter().zip(&self.band_weights) {
            let power = m * m;
            total += power;
            for (sum, weight) in sums.iter_mut().zip(w) {
                *sum += power * weight;
            }
        }
        if f64::from(total) <= SILENT_ENERGY {
            return [0.0; 3];
        }
        sums.map(|v| v / total)
    }
}

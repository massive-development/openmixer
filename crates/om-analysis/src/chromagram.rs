//! Pitch-class profile of a clip: long-window STFT bins folded onto 12 semitone classes (A4 = 440 Hz),
//! each frame normalised so loud passages do not dominate, then summed over the clip.

use crate::{AnalysisContext, AudioClip, MagnitudeStft};

/// Index 0 = C.
pub type Chroma = [f64; 12];

const SILENCE_FLOOR: f64 = 0.01;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChromaOptions {
    /// 8192-sample frames: about 5 Hz bins at 44.1 kHz.
    pub fft_order: u32,
    pub hop: usize,
    pub min_hz: f64,
    pub max_hz: f64,
    pub tuning_hz: f64,
}

impl Default for ChromaOptions {
    fn default() -> Self {
        Self {
            fft_order: 13,
            hop: 4096,
            min_hz: 100.0,
            max_hz: 5000.0,
            tuning_hz: 440.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct BinWeight {
    bin: usize,
    pitch_class: usize,
    weight: f32,
}

/// Bins near a semitone centre count fully, fading to zero half way between semitones.
fn build_bin_map(
    stft: &MagnitudeStft,
    options: &ChromaOptions,
    sample_rate: f64,
) -> Vec<BinWeight> {
    (1..stft.num_bins())
        .filter_map(|bin| {
            let hz = stft.bin_hz(bin, sample_rate);
            if hz < options.min_hz || hz > options.max_hz {
                return None;
            }
            let midi = 69.0 + 12.0 * (hz / options.tuning_hz).log2();
            let nearest = midi.round();
            let weight = 1.0 - 2.0 * (midi - nearest).abs();
            (weight > 0.0).then(|| BinWeight {
                bin,
                pitch_class: (nearest as i64).rem_euclid(12) as usize,
                weight: weight as f32,
            })
        })
        .collect()
}

fn frame_chroma(magnitudes: &[f32], map: &[BinWeight]) -> Chroma {
    let mut c = [0.0f64; 12];
    for w in map {
        c[w.pitch_class] += f64::from(w.weight * magnitudes[w.bin].sqrt());
    }
    c
}

fn accumulate_normalised(total: &mut Chroma, frame: &Chroma) {
    let peak = frame.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if peak < SILENCE_FLOOR {
        return;
    }
    for (t, f) in total.iter_mut().zip(frame) {
        *t += f / peak;
    }
}

/// `None` when cancelled; all zeros for silence.
pub fn compute_chroma(
    clip: &AudioClip,
    options: &ChromaOptions,
    context: &dyn AnalysisContext,
) -> Option<Chroma> {
    let mut stft = MagnitudeStft::new(options.fft_order, options.hop);
    let map = build_bin_map(&stft, options, clip.sample_rate);
    let frames = stft.num_frames(clip.samples.len());

    let mut total = [0.0f64; 12];
    let mut magnitudes = Vec::new();
    for i in 0..frames {
        if i % 64 == 0 && context.is_cancelled() {
            return None;
        }
        stft.compute_frame(&clip.samples, i, &mut magnitudes);
        accumulate_normalised(&mut total, &frame_chroma(&magnitudes, &map));
        context.set_progress((i + 1) as f32 / frames as f32);
    }
    Some(total)
}

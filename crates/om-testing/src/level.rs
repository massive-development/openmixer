//! Level measures. DB conversions use a 1e-12 floor so silence maps to a finite value.

/// Linear gain (or amplitude ratio) to decibels: `20 * log10(ratio)`, floored at -240 dB.
pub fn gain_to_db(ratio: f64) -> f64 {
    20.0 * ratio.max(1e-12).log10()
}

/// Largest absolute sample.
pub fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()))
}

/// Root mean square of all samples; 0 for an empty slice.
pub fn rms(samples: &[f32]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f64 = samples.iter().map(|&s| f64::from(s) * f64::from(s)).sum();
    (sum / samples.len() as f64).sqrt()
}

/// RMS of the second half of the buffer, after filters have settled.
pub fn rms_tail(samples: &[f32]) -> f64 {
    rms(&samples[samples.len() / 2..])
}

/// RMS of a sine with peak amplitude `amp`.
pub fn sine_rms(amp: f32) -> f64 {
    f64::from(amp) / 2.0_f64.sqrt()
}

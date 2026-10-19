//! Onset times from an onset envelope: local maxima above an adaptive (local mean) threshold and a
//! global floor, at least `min_gap_seconds` apart. Port of `src/analysis/OnsetPicker.{h,cpp}`.

use crate::OnsetEnvelope;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OnsetPickOptions {
    pub min_gap_seconds: f64,
    /// Radius of the adaptive threshold window.
    pub local_window_seconds: f64,
    /// A peak must exceed this multiple of the local mean...
    pub local_factor: f32,
    /// ...and this fraction of the envelope maximum.
    pub global_fraction: f32,
}

impl Default for OnsetPickOptions {
    fn default() -> Self {
        Self {
            min_gap_seconds: 0.05,
            local_window_seconds: 0.1,
            local_factor: 1.5,
            global_fraction: 0.1,
        }
    }
}

/// `v[i]` is the largest in `[i - radius, i + radius]`, with ties going to the earliest index.
fn is_local_max(v: &[f32], i: usize, radius: usize) -> bool {
    let lo = i.saturating_sub(radius);
    let hi = (i + radius).min(v.len() - 1);
    (lo..=hi).all(|j| !(v[j] > v[i] || (v[j] == v[i] && j < i)))
}

fn local_mean(v: &[f32], i: usize, radius: usize) -> f32 {
    let lo = i.saturating_sub(radius);
    let hi = (i + radius).min(v.len() - 1);
    let sum: f64 = v[lo..=hi].iter().map(|&x| f64::from(x)).sum();
    (sum / (hi - lo + 1) as f64) as f32
}

/// Onset times in seconds, in envelope order.
pub fn pick_onsets(env: &OnsetEnvelope, options: &OnsetPickOptions) -> Vec<f64> {
    let v = &env.values;
    if v.is_empty() || env.frame_rate <= 0.0 {
        return Vec::new();
    }

    let max = v.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let floor = options.global_fraction * max;
    let gap = (options.min_gap_seconds * env.frame_rate).max(1.0) as usize;
    let window = (options.local_window_seconds * env.frame_rate).max(1.0) as usize;
    (0..v.len())
        .filter(|&i| {
            v[i] > floor
                && is_local_max(v, i, gap)
                && v[i] >= options.local_factor * local_mean(v, i, window)
        })
        .map(|i| env.time_of(i as f64))
        .collect()
}

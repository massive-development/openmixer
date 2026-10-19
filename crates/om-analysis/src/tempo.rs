//! Tempo and phase search steps on an onset envelope.
//! Indices are envelope frames (fractional); periods are frames per beat.

use crate::{jlimit, OnsetEnvelope};

/// A tempo and beat phase. `phase` is an envelope index of a beat, in `[0, period)`; `score` is the
/// mean envelope value on the beat positions.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct BeatPhase {
    pub bpm: f64,
    pub phase: f64,
    pub score: f64,
}

pub fn period_frames(env: &OnsetEnvelope, bpm: f64) -> f64 {
    env.frame_rate * 60.0 / bpm
}

fn autocorrelation(v: &[f32], lag: usize) -> f64 {
    if lag >= v.len() {
        return 0.0;
    }
    let sum: f64 = (0..v.len() - lag)
        .map(|i| f64::from(v[i]) * f64::from(v[i + lag]))
        .sum();
    sum / (v.len() - lag) as f64
}

/// Rewards lags whose double is also periodic, which suppresses off-beat (1.5x) peaks.
fn lag_score(v: &[f32], lag: usize) -> f64 {
    autocorrelation(v, lag) + 0.5 * autocorrelation(v, 2 * lag)
}

fn parabolic_offset(left: f64, centre: f64, right: f64) -> f64 {
    let denom = left - 2.0 * centre + right;
    if denom.abs() > 1.0e-12 {
        jlimit(-0.5, 0.5, 0.5 * (left - right) / denom)
    } else {
        0.0
    }
}

fn beat_score(env: &OnsetEnvelope, period: f64, phase: f64) -> f64 {
    let end = env.values.len() as f64;
    let mut sum = 0.0f64;
    let mut count = 0usize;
    let mut idx = phase;
    while idx < end {
        sum += f64::from(env.at(idx));
        count += 1;
        idx += period;
    }
    if count > 0 {
        sum / count as f64
    } else {
        0.0
    }
}

#[derive(Clone, Copy)]
struct SearchBox {
    bpm_lo: f64,
    bpm_hi: f64,
    bpm_step: f64,
    phase_lo: f64,
    phase_hi: f64,
    phase_step: f64,
}

fn search_box(env: &OnsetEnvelope, span: SearchBox) -> BeatPhase {
    let mut best = BeatPhase::default();
    let mut bpm = span.bpm_lo;
    while bpm <= span.bpm_hi {
        let period = period_frames(env, bpm);
        let phase_hi = span.phase_hi.min(span.phase_lo + period);
        let mut phase = span.phase_lo;
        while phase < phase_hi {
            let score = beat_score(env, period, phase);
            if score > best.score {
                best = BeatPhase { bpm, phase, score };
            }
            phase += span.phase_step;
        }
        bpm += span.bpm_step;
    }
    best
}

fn wrap_phase(phase: f64, period: f64) -> f64 {
    phase - period * (phase / period).floor()
}

/// Interpolated position of the strongest envelope peak within `+-radius` of `index`; `None` if flat.
fn local_peak(env: &OnsetEnvelope, index: f64, radius: f64) -> Option<f64> {
    let v = &env.values;
    let lo = (index - radius).max(1.0).ceil() as i64;
    let hi = (index + radius).min(v.len() as f64 - 2.0).floor() as i64;
    let mut best: i64 = -1;
    for i in lo..=hi {
        if best < 0 || v[i as usize] > v[best as usize] {
            best = i;
        }
    }
    if best < 0 || v[best as usize] <= 0.0 {
        return None;
    }
    let b = best as usize;
    Some(best as f64 + parabolic_offset(f64::from(v[b - 1]), f64::from(v[b]), f64::from(v[b + 1])))
}

#[derive(Default)]
struct LineFit {
    slope: f64,
    intercept: f64,
    count: usize,
}

/// Least-squares fit of onset peak index against beat number.
fn fit_beat_line(env: &OnsetEnvelope, beats: BeatPhase, period: f64) -> LineFit {
    let (mut n, mut sx, mut sy, mut sxx, mut sxy) = (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64);
    let end = env.values.len() as f64;
    let mut idx = beats.phase;
    let mut k = 0usize;
    while idx < end {
        if let Some(peak) = local_peak(env, idx, period * 0.15) {
            let x = k as f64;
            n += 1.0;
            sx += x;
            sy += peak;
            sxx += x * x;
            sxy += x * peak;
        }
        idx += period;
        k += 1;
    }
    let denom = n * sxx - sx * sx;
    if n < 4.0 || denom.abs() < 1.0e-9 {
        return LineFit::default();
    }
    let slope = (n * sxy - sx * sy) / denom;
    LineFit {
        slope,
        intercept: (sy - slope * sx) / n,
        count: n as usize,
    }
}

/// Autocorrelation peak within `[min_bpm, max_bpm]`; 0 when the envelope is too short or flat.
pub fn estimate_coarse_bpm(env: &OnsetEnvelope, min_bpm: f64, max_bpm: f64) -> f64 {
    let v = &env.values;
    let lag_lo = period_frames(env, max_bpm).floor() as usize;
    let lag_hi = period_frames(env, min_bpm).ceil() as usize;
    if lag_lo < 2 || lag_hi * 2 >= v.len() {
        return 0.0;
    }

    let mut best_lag = 0usize;
    let mut best_score = 0.0f64;
    for lag in lag_lo..=lag_hi {
        let s = lag_score(v, lag);
        if s > best_score {
            best_lag = lag;
            best_score = s;
        }
    }
    if best_lag == 0 {
        return 0.0;
    }

    let lag = best_lag as f64
        + parabolic_offset(
            lag_score(v, best_lag - 1),
            best_score,
            lag_score(v, best_lag + 1),
        );
    jlimit(min_bpm, max_bpm, env.frame_rate * 60.0 / lag)
}

/// Joint fine search of tempo (within 2% of the coarse value) and beat phase.
pub fn refine_beat_phase(env: &OnsetEnvelope, coarse_bpm: f64) -> BeatPhase {
    if coarse_bpm <= 0.0 {
        return BeatPhase::default();
    }
    let period = period_frames(env, coarse_bpm);
    let coarse = search_box(
        env,
        SearchBox {
            bpm_lo: coarse_bpm * 0.98,
            bpm_hi: coarse_bpm * 1.02,
            bpm_step: 0.05,
            phase_lo: 0.0,
            phase_hi: period,
            phase_step: 0.5,
        },
    );
    let mut best = search_box(
        env,
        SearchBox {
            bpm_lo: coarse.bpm - 0.06,
            bpm_hi: coarse.bpm + 0.06,
            bpm_step: 0.002,
            phase_lo: coarse.phase - 1.0,
            phase_hi: coarse.phase + 1.0,
            phase_step: 0.05,
        },
    );
    best.phase = wrap_phase(best.phase, period_frames(env, best.bpm));
    fit_to_onsets(env, best)
}

/// Least-squares line through the onset peak nearest each beat: sub-frame tempo and phase. Only
/// trusted as a small correction (within 1% of the searched period).
pub fn fit_to_onsets(env: &OnsetEnvelope, beats: BeatPhase) -> BeatPhase {
    let period = period_frames(env, beats.bpm);
    let fit = fit_beat_line(env, beats, period);
    if fit.count == 0 || (fit.slope / period - 1.0).abs() > 0.01 {
        return beats;
    }
    BeatPhase {
        bpm: env.frame_rate * 60.0 / fit.slope,
        phase: wrap_phase(fit.intercept, fit.slope),
        score: beats.score,
    }
}

/// Which beat of the bar (0..beats_per_bar, counted from `phase`) carries the most low-band energy.
/// Ties go to the earliest beat.
pub fn find_downbeat_offset(
    low_band: &OnsetEnvelope,
    beats: &BeatPhase,
    beats_per_bar: i32,
) -> i32 {
    if beats_per_bar <= 1 || beats.bpm <= 0.0 {
        return 0;
    }
    let bar = beats_per_bar as usize;
    let period = period_frames(low_band, beats.bpm);
    let end = low_band.values.len() as f64;
    let mut sums = vec![0.0f64; bar];
    let mut idx = beats.phase;
    let mut beat = 0usize;
    while idx < end {
        let peak = low_band
            .at(idx - 1.0)
            .max(low_band.at(idx))
            .max(low_band.at(idx + 1.0));
        sums[beat % bar] += f64::from(peak);
        idx += period;
        beat += 1;
    }

    let mut best = 0usize;
    for (i, &sum) in sums.iter().enumerate().skip(1) {
        if sum > sums[best] {
            best = i;
        }
    }
    best as i32
}

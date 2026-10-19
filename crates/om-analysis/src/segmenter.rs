//! Loudness curve and section boundaries for structure analysis (feeds Automix). Sections are cut
//! where the smoothed energy before and after a point differs most, then labelled by loudness.

use crate::AudioClip;

/// Dynamic range of the energy curve, in dB.
const RANGE_DB: f64 = 40.0;

#[derive(Debug, Clone, PartialEq)]
pub struct EnergyCurve {
    pub hop_seconds: f64,
    /// 0..1: RMS in dB mapped over the 40 dB below the clip's loudest window.
    pub values: Vec<f32>,
}

impl Default for EnergyCurve {
    fn default() -> Self {
        Self {
            hop_seconds: 0.5,
            values: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Section {
    pub start_seconds: f64,
    pub end_seconds: f64,
    /// Mean of the curve over the section.
    pub energy: f32,
    /// "intro", "main", "breakdown" or "outro".
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SegmentOptions {
    /// Energy is compared over this span either side of a boundary.
    pub window_seconds: f64,
    pub min_section_seconds: f64,
    /// Energy step needed for a boundary.
    pub min_change: f32,
    /// Sections this close to the loud level count as main.
    pub main_within_db: f64,
}

impl Default for SegmentOptions {
    fn default() -> Self {
        Self {
            window_seconds: 8.0,
            min_section_seconds: 8.0,
            min_change: 0.15,
            main_within_db: 6.0,
        }
    }
}

/// Prefix sums for O(1) range means over `[from, to)`.
struct PrefixMean {
    sums: Vec<f64>,
}

impl PrefixMean {
    fn new(values: &[f32]) -> Self {
        let mut sums = Vec::with_capacity(values.len() + 1);
        sums.push(0.0);
        for &v in values {
            let last = *sums.last().unwrap_or(&0.0);
            sums.push(last + f64::from(v));
        }
        Self { sums }
    }

    fn mean(&self, from: usize, to: usize) -> f64 {
        if to > from {
            (self.sums[to] - self.sums[from]) / (to - from) as f64
        } else {
            0.0
        }
    }
}

fn novelty_curve(curve: &EnergyCurve, span: usize) -> Vec<f64> {
    let prefix = PrefixMean::new(&curve.values);
    let n = curve.values.len();
    let mut novelty = vec![0.0f64; n];
    for (i, slot) in novelty.iter_mut().enumerate().skip(1) {
        let ahead = prefix.mean(i, n.min(i + span));
        let behind = prefix.mean(i.saturating_sub(span), i);
        *slot = (ahead - behind).abs();
    }
    novelty
}

/// Strongest change points first, each at least `min_gap` from the others and from both ends.
fn pick_boundaries(novelty: &[f64], min_gap: usize, min_change: f64) -> Vec<usize> {
    let mut order: Vec<usize> = (0..novelty.len()).collect();
    order.sort_by(|&a, &b| novelty[b].total_cmp(&novelty[a]));

    let mut picked: Vec<usize> = Vec::new();
    for i in order {
        if novelty[i] < min_change {
            break;
        }
        let far_from = |j: usize| i.abs_diff(j) >= min_gap;
        if far_from(0) && far_from(novelty.len()) && picked.iter().all(|&p| far_from(p)) {
            picked.push(i);
        }
    }
    picked.sort_unstable();
    picked
}

/// The loudest section sets the reference, so steady tracks are a single main section.
fn label_sections(sections: &mut [Section], main_within_db: f64) {
    let loud = sections.iter().fold(0.0f32, |acc, s| acc.max(s.energy));
    let threshold = loud - (main_within_db / RANGE_DB) as f32;
    for s in sections.iter_mut() {
        s.label = if s.energy >= threshold {
            "main"
        } else {
            "breakdown"
        }
        .into();
    }
    if sections.len() < 2 {
        return;
    }
    if sections[0].label != "main" {
        sections[0].label = "intro".into();
    }
    let last = sections.len() - 1;
    if sections[last].label != "main" {
        sections[last].label = "outro".into();
    }
}

/// Loudness curve of `clip`, one value per `hop_seconds`.
pub fn compute_energy_curve(clip: &AudioClip, hop_seconds: f64) -> EnergyCurve {
    let mut curve = EnergyCurve {
        hop_seconds,
        values: Vec::new(),
    };
    let hop = (hop_seconds * clip.sample_rate).max(1.0) as usize;
    let samples = &clip.samples;
    let db: Vec<f64> = samples
        .chunks(hop)
        .map(|block| {
            let sum: f64 = block.iter().map(|&s| f64::from(s) * f64::from(s)).sum();
            10.0 * (sum / block.len() as f64 + 1.0e-12).log10()
        })
        .collect();
    let Some(top) = db.iter().copied().reduce(f64::max) else {
        return curve;
    };
    curve.values = db
        .iter()
        .map(|d| ((d - (top - RANGE_DB)) / RANGE_DB).clamp(0.0, 1.0) as f32)
        .collect();
    curve
}

/// Sections of `curve`, labelled. Empty for an empty curve.
pub fn segment_sections(curve: &EnergyCurve, options: &SegmentOptions) -> Vec<Section> {
    if curve.values.is_empty() || curve.hop_seconds <= 0.0 {
        return Vec::new();
    }

    let span = (options.window_seconds / curve.hop_seconds)
        .round()
        .max(1.0) as usize;
    let min_gap = (options.min_section_seconds / curve.hop_seconds)
        .round()
        .max(1.0) as usize;
    let mut cuts = pick_boundaries(
        &novelty_curve(curve, span),
        min_gap,
        f64::from(options.min_change),
    );
    cuts.insert(0, 0);
    cuts.push(curve.values.len());

    let prefix = PrefixMean::new(&curve.values);
    let mut sections: Vec<Section> = cuts
        .windows(2)
        .map(|w| Section {
            start_seconds: w[0] as f64 * curve.hop_seconds,
            end_seconds: w[1] as f64 * curve.hop_seconds,
            energy: prefix.mean(w[0], w[1]) as f32,
            label: String::new(),
        })
        .collect();
    label_sections(&mut sections, options.main_within_db);
    sections
}

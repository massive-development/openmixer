//! Cost limits from the spectrum design: analysis time at most 3 times the format 1 pass on the
//! same fixture, and memory at most 1.5 times. The numbers are printed so a run records them
//! (`cargo test -p om-analysis cost -- --nocapture`).

use super::*;
use crate::waveform::legacy_reference::build_base_level;
use std::time::{Duration, Instant};

const TIME_LIMIT_RATIO: f64 = 3.0;
const MEMORY_LIMIT_RATIO: f64 = 1.5;
const RUNS: usize = 3;

fn fastest<T>(mut run: impl FnMut() -> T) -> Duration {
    (0..RUNS)
        .map(|_| {
            let started = Instant::now();
            std::hint::black_box(run());
            started.elapsed()
        })
        .min()
        .unwrap_or_default()
}

#[test]
fn analysis_time_stays_within_three_times_the_old_pass() {
    let clip = mix_fixture_clip(MixStyle::Full, 120.0);
    let old = fastest(|| build_base_level(&clip));
    let new = fastest(|| build(&clip));
    let ratio = new.as_secs_f64() / old.as_secs_f64();
    eprintln!(
        "waveform cost: old {old:?}, new {new:?}, ratio {ratio:.2} (limit {TIME_LIMIT_RATIO})"
    );
    assert!(ratio <= TIME_LIMIT_RATIO, "ratio {ratio:.2}");
}

#[test]
fn memory_stays_within_one_and_a_half_times_the_old_pyramid() {
    // The old point held five f32 values (min, max, three bands) and 5 payload bytes.
    let (old_memory, old_payload) = (5 * std::mem::size_of::<f32>(), 5);
    let memory = std::mem::size_of::<WaveformPoint>();
    let clip = mix_fixture_clip(MixStyle::Full, 30.0);
    let data = build(&clip);
    let payload = crate::waveform_codec::encode_points(&data.levels()[0].points).len() as f64
        / data.levels()[0].points.len() as f64;
    eprintln!("waveform memory: {memory} B per point (was {old_memory}), payload {payload} B (was {old_payload})");
    assert!(memory as f64 <= MEMORY_LIMIT_RATIO * old_memory as f64);
    assert!(payload <= MEMORY_LIMIT_RATIO * old_payload as f64);
    let pyramid: usize = data.levels().iter().map(|l| l.points.len()).sum();
    let fine = data.levels()[0].points.len();
    assert!(
        (pyramid as f64) < fine as f64 * 1.4,
        "levels add about a third"
    );
}

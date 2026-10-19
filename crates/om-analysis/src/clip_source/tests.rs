//! Port of the `ContentHash` cases in `tests/unit/AnalysisCacheTest.cpp`.

use super::*;
use crate::test_signals::{write_float_wav, TempFile};
use crate::NullAnalysisContext;

fn ramp(n: usize, rate: f64) -> AudioClip {
    AudioClip::new((0..n).map(|i| i as f32 / n as f32).collect(), rate)
}

#[test]
fn clip_hash_depends_on_samples_and_rate() {
    let a = hash_clip(&ramp(100, 44_100.0));
    assert_eq!(a.len(), 64);
    assert_eq!(a, hash_clip(&ramp(100, 44_100.0)));
    assert_ne!(a, hash_clip(&ramp(101, 44_100.0)));
    assert_ne!(a, hash_clip(&ramp(100, 48_000.0)));
}

#[test]
fn file_source_decodes_mono_and_hashes_bytes_not_name() {
    let stereo: Vec<f32> = (0..1000).flat_map(|_| [0.5f32, -0.25]).collect();
    let temp_a = TempFile::new("wav");
    let temp_b = TempFile::new("wav");
    write_float_wav(temp_a.path(), 2, 48_000, &stereo);
    write_float_wav(temp_b.path(), 2, 48_000, &stereo);

    let mut a = FileClipSource::new(temp_a.path());
    let mut b = FileClipSource::new(temp_b.path());
    assert_eq!(a.content_hash(), b.content_hash());

    let clip = a.load(&NullAnalysisContext).expect("stereo wav decodes");
    assert_eq!(clip.sample_rate, 48_000.0);
    assert_eq!(clip.samples.len(), 1000);
    assert!((clip.samples[500] - 0.125).abs() < 1.0e-5);

    let mut missing = FileClipSource::new("/nonexistent/file.wav");
    assert_eq!(missing.content_hash(), None);
    assert!(missing.load(&NullAnalysisContext).is_none());
}

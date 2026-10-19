//! `ContentHash` cases.

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

/// A 32-bit float mono WAV whose `data` chunk declares `declared_bytes` but holds only `samples`.
fn write_lying_wav(path: &Path, declared_bytes: u32, samples: &[f32]) {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&36_u32.wrapping_add(declared_bytes).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&3u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&48_000u32.to_le_bytes());
    bytes.extend_from_slice(&(48_000u32 * 4).to_le_bytes());
    bytes.extend_from_slice(&4u16.to_le_bytes());
    bytes.extend_from_slice(&32u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&declared_bytes.to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    std::fs::write(path, bytes).expect("write lying wav");
}

#[test]
fn a_header_length_far_beyond_the_data_fails_the_job_instead_of_caching_silence() {
    // The header claims about 3 hours of mono audio (2 GB of data); only one second is there.
    let temp = TempFile::new("wav");
    write_lying_wav(temp.path(), 0x7FFF_FFF0, &vec![0.5_f32; 48_000]);
    let mut source = FileClipSource::new(temp.path());
    assert!(source.load(&NullAnalysisContext).is_none());
}

#[test]
fn a_small_shortfall_keeps_only_the_frames_the_decoder_produced() {
    // The header claims 100 000 frames, the data holds 99 000 of 0.5: no zero padding may appear.
    let temp = TempFile::new("wav");
    write_lying_wav(temp.path(), 100_000 * 4, &vec![0.5_f32; 99_000]);
    let mut source = FileClipSource::new(temp.path());
    let clip = source
        .load(&NullAnalysisContext)
        .expect("a small shortfall still loads");
    assert_eq!(clip.samples.len(), 99_000);
    assert!(clip.samples.iter().all(|s| (*s - 0.5).abs() < 1.0e-6));
}

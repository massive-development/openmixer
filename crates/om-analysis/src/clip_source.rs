//! Where a job's audio comes from. The content hash keys the cache, so it must depend only on the
//! audio bytes (renaming or moving a file keeps its results). Port of `src/analysis/ClipSource.*`.
//! Called on a worker thread.

use crate::{AnalysisContext, AudioClip};
use om_engine::deck::SourceReader;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Frames decoded per progress report.
const READ_BLOCK: usize = 1 << 16;
/// Longest clip analysed. Longer files are analysed from their first four hours.
const MAX_LOAD_SECONDS: f64 = 4.0 * 3600.0;
/// Memory reserved up front. The buffer grows past this as the audio is decoded, so a bogus
/// length in a container header cannot make the reservation itself huge.
const RESERVE_SECONDS: f64 = 600.0;

pub trait ClipSource: Send {
    /// Hex SHA-256 of the audio; `None` when the source is unreadable.
    fn content_hash(&mut self) -> Option<String>;
    /// Decodes the whole clip as mono. `None` when it cannot be decoded.
    fn load(&mut self, context: &dyn AnalysisContext) -> Option<AudioClip>;
    fn describe(&self) -> String;
}

/// Any format symphonia can decode (the same probe and decoder the deck uses). Channels are
/// averaged to mono at the file's own rate. Files with more than two channels contribute their
/// first two (the deck's stereo reader), so surround mixes are analysed from the front pair.
#[derive(Debug, Clone)]
pub struct FileClipSource {
    path: PathBuf,
}

impl FileClipSource {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl ClipSource for FileClipSource {
    fn content_hash(&mut self) -> Option<String> {
        hash_file_content(&self.path)
    }

    fn load(&mut self, context: &dyn AnalysisContext) -> Option<AudioClip> {
        let mut reader = SourceReader::open(&self.path).ok()?;
        let info = reader.info();
        let claimed = usize::try_from(info.length_frames)
            .ok()
            .filter(|&n| n > 0)?;
        let cap = (info.sample_rate * MAX_LOAD_SECONDS) as usize;
        let total = claimed.min(cap);
        let reserve = total.min((info.sample_rate * RESERVE_SECONDS) as usize);

        let mut samples: Vec<f32> = Vec::with_capacity(reserve);
        let mut block = vec![[0.0f32; 2]; READ_BLOCK];
        let mut decoded = 0usize;
        while decoded < total {
            if context.is_cancelled() {
                return None;
            }
            let frames = READ_BLOCK.min(total - decoded);
            let got = reader.read_stereo(decoded as i64, &mut block[..frames]);
            // Only frames the decoder produced are kept: silence past the end never enters the clip.
            samples.extend(block[..got].iter().map(|f| (f[0] + f[1]) * 0.5));
            decoded += got;
            if got < frames {
                break;
            }
            context.set_progress(decoded as f32 / total as f32);
        }
        // A header that overstates the length is tolerated only for a sliver (1 % or one second);
        // anything more means the file is damaged or lying, and the job fails.
        let tolerance = (total / 100).max(info.sample_rate as usize);
        if total - decoded > tolerance || samples.is_empty() {
            return None;
        }
        Some(AudioClip::new(samples, info.sample_rate))
    }

    fn describe(&self) -> String {
        self.path.display().to_string()
    }
}

/// Already-decoded audio (tests, stems, recordings); hashed from the samples and rate.
#[derive(Debug, Clone)]
pub struct MemoryClipSource {
    clip: AudioClip,
    label: String,
}

impl MemoryClipSource {
    pub fn new(clip: AudioClip, label: impl Into<String>) -> Self {
        Self {
            clip,
            label: label.into(),
        }
    }
}

impl ClipSource for MemoryClipSource {
    fn content_hash(&mut self) -> Option<String> {
        Some(hash_clip(&self.clip))
    }

    fn load(&mut self, _context: &dyn AnalysisContext) -> Option<AudioClip> {
        Some(self.clip.clone())
    }

    fn describe(&self) -> String {
        self.label.clone()
    }
}

/// Hex SHA-256 of a file's bytes; `None` when it cannot be opened or read.
pub fn hash_file_content(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; READ_BLOCK * 4];
    loop {
        let read = file.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Some(hex(&hasher.finalize()))
}

/// Hex SHA-256 of the sample bytes (little-endian `f32`) followed by the sample rate.
pub fn hash_clip(clip: &AudioClip) -> String {
    let mut hasher = Sha256::new();
    for sample in &clip.samples {
        hasher.update(sample.to_le_bytes());
    }
    hasher.update(clip.sample_rate.to_le_bytes());
    hex(&hasher.finalize())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests;

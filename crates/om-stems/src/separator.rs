//! Stem separation extension point. A separator turns one clip into named stems (same length and rate
//! as the input). Backends (the planned Demucs ONNX one, or
//! plugins) implement [`StemSeparator`]. Calls may run on analysis worker threads, so implementations
//! are `Send + Sync`.

use om_analysis::{AnalysisContext, AudioClip};
use std::collections::BTreeMap;
use std::fmt;

/// Version of this trait. A separator reporting another version is not used.
pub const INTERFACE_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum StemKind {
    Drums,
    Bass,
    Vocals,
    Other,
}

impl StemKind {
    pub const ALL: [StemKind; 4] = [Self::Drums, Self::Bass, Self::Vocals, Self::Other];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Drums => "drums",
            Self::Bass => "bass",
            Self::Vocals => "vocals",
            Self::Other => "other",
        }
    }
}

impl fmt::Display for StemKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Mono samples per stem, all at one rate.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StemSet {
    pub sample_rate: f64,
    pub stems: BTreeMap<StemKind, Vec<f32>>,
}

impl StemSet {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            sample_rate,
            stems: BTreeMap::new(),
        }
    }

    /// True when all four stems are present and each has `expected_len` samples.
    pub fn is_complete(&self, expected_len: usize) -> bool {
        self.stems.len() == StemKind::ALL.len()
            && self.stems.values().all(|s| s.len() == expected_len)
    }
}

pub trait StemSeparator: Send + Sync {
    fn interface_version(&self) -> u32 {
        INTERFACE_VERSION
    }

    fn id(&self) -> &str;

    /// Part of the cache key for stored stems: bump it when the output changes.
    fn version(&self) -> i32;

    /// `None` when cancelled or the model is unavailable.
    fn separate(&self, clip: &AudioClip, context: &dyn AnalysisContext) -> Option<StemSet>;
}

/// A separator that makes no model: it splits the input into four equal quarters, so the stems sum
/// back to the input. For tests and for wiring the rest of the pipeline before the ONNX backend.
#[derive(Debug, Clone)]
pub struct MockSeparator {
    id: String,
    version: i32,
}

impl MockSeparator {
    pub fn new() -> Self {
        Self {
            id: "mock-separator".to_owned(),
            version: 1,
        }
    }
}

impl Default for MockSeparator {
    fn default() -> Self {
        Self::new()
    }
}

impl StemSeparator for MockSeparator {
    fn id(&self) -> &str {
        &self.id
    }

    fn version(&self) -> i32 {
        self.version
    }

    fn separate(&self, clip: &AudioClip, context: &dyn AnalysisContext) -> Option<StemSet> {
        if context.is_cancelled() {
            return None;
        }
        let quarter: Vec<f32> = clip.samples.iter().map(|s| s * 0.25).collect();
        let mut set = StemSet::new(clip.sample_rate);
        for kind in StemKind::ALL {
            set.stems.insert(kind, quarter.clone());
        }
        Some(set)
    }
}

//! Stem separation extension point (interface v2). A separator pulls a stereo clip from a
//! [`StereoSource`] and pushes four stereo stems into a [`StemSink`], both in chunks: no whole-track
//! buffer is needed on either side. Stems come out at the input's own sample rate and length, so
//! frame positions match the original file. Calls may run on worker threads, so implementations are
//! `Send + Sync`.

use om_analysis::AnalysisContext;
use std::collections::BTreeMap;
use std::fmt;
use thiserror::Error;

/// Version of this trait. A separator reporting another version is not used.
/// v2: stereo stems and a streamed `separate_to` (v1 returned whole mono buffers).
pub const INTERFACE_VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == name)
    }
}

impl fmt::Display for StemKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum StemError {
    #[error("cancelled")]
    Cancelled,
    #[error("source: {0}")]
    Source(String),
    #[error("sink: {0}")]
    Sink(String),
    #[error("inference: {0}")]
    Inference(String),
    #[error("invalid input: {0}")]
    Invalid(String),
}

/// One stereo stem.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StemBuffer {
    pub left: Vec<f32>,
    pub right: Vec<f32>,
}

impl StemBuffer {
    pub fn len(&self) -> usize {
        self.left.len()
    }

    pub fn is_empty(&self) -> bool {
        self.left.is_empty()
    }
}

/// Stereo stems, all at one rate.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StemSet {
    pub sample_rate: f64,
    pub stems: BTreeMap<StemKind, StemBuffer>,
}

impl StemSet {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            sample_rate,
            stems: BTreeMap::new(),
        }
    }

    /// A stem whose two channels are the same mono signal (the mock separator's shape).
    pub fn from_mono(
        sample_rate: f64,
        mono: impl IntoIterator<Item = (StemKind, Vec<f32>)>,
    ) -> Self {
        let mut set = Self::new(sample_rate);
        for (kind, samples) in mono {
            set.stems.insert(
                kind,
                StemBuffer {
                    right: samples.clone(),
                    left: samples,
                },
            );
        }
        set
    }

    /// True when all four stems are present and each has `expected_len` frames per channel.
    pub fn is_complete(&self, expected_len: usize) -> bool {
        self.stems.len() == StemKind::ALL.len()
            && self
                .stems
                .values()
                .all(|s| s.left.len() == expected_len && s.right.len() == expected_len)
    }
}

/// Pull side: stereo audio in chunks.
pub trait StereoSource {
    fn sample_rate(&self) -> f64;
    /// Frames in the clip when known (used for progress only).
    fn total_frames(&self) -> Option<u64>;
    /// Fills up to `left.len()` frames (`left` and `right` have equal length). Returns the frames
    /// written; 0 means the end.
    fn read(&mut self, left: &mut [f32], right: &mut [f32]) -> Result<usize, StemError>;
}

/// Push side: for every chunk a separator calls `write` once per [`StemKind`], each with the same
/// frame count; chunks of one kind arrive in order and add up to the input length.
pub trait StemSink {
    fn write(&mut self, kind: StemKind, left: &[f32], right: &[f32]) -> Result<(), StemError>;
}

pub trait StemSeparator: Send + Sync {
    fn interface_version(&self) -> u32 {
        INTERFACE_VERSION
    }

    fn id(&self) -> &str;

    /// Part of the cache key for stored stems: bump it when the output changes.
    fn version(&self) -> i32;

    /// The rate the model works at; the pipeline resamples in and out around it. The stems still
    /// leave at the input's rate.
    fn native_rate(&self) -> f64;

    /// Streams the whole input into `sink`. `Err(StemError::Cancelled)` when `ctx` cancels. Progress
    /// goes to `ctx.set_progress` as a non-decreasing 0..1 value.
    fn separate_to(
        &self,
        input: &mut dyn StereoSource,
        sink: &mut dyn StemSink,
        ctx: &dyn AnalysisContext,
    ) -> Result<(), StemError>;
}

/// An in-memory [`StereoSource`] (tests, tools, short clips).
#[derive(Debug, Clone)]
pub struct SliceSource<'a> {
    left: &'a [f32],
    right: &'a [f32],
    rate: f64,
    pos: usize,
}

impl<'a> SliceSource<'a> {
    pub fn stereo(left: &'a [f32], right: &'a [f32], rate: f64) -> Self {
        let n = left.len().min(right.len());
        Self {
            left: &left[..n],
            right: &right[..n],
            rate,
            pos: 0,
        }
    }

    /// Mono input is presented as two identical channels.
    pub fn mono(samples: &'a [f32], rate: f64) -> Self {
        Self::stereo(samples, samples, rate)
    }
}

impl StereoSource for SliceSource<'_> {
    fn sample_rate(&self) -> f64 {
        self.rate
    }

    fn total_frames(&self) -> Option<u64> {
        Some(self.left.len() as u64)
    }

    fn read(&mut self, left: &mut [f32], right: &mut [f32]) -> Result<usize, StemError> {
        let n = left.len().min(right.len()).min(self.left.len() - self.pos);
        left[..n].copy_from_slice(&self.left[self.pos..self.pos + n]);
        right[..n].copy_from_slice(&self.right[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

/// Decodes any file the deck can play, as stereo at the file's own rate (the deck's reader: first
/// two channels, mono duplicated).
pub struct FileStereoSource {
    reader: om_engine::deck::SourceReader,
    rate: f64,
    total: u64,
    pos: u64,
}

impl FileStereoSource {
    pub fn open(path: &std::path::Path) -> Result<Self, StemError> {
        let reader = om_engine::deck::SourceReader::open(path)
            .map_err(|e| StemError::Source(format!("{}: {e}", path.display())))?;
        let info = reader.info();
        Ok(Self {
            rate: info.sample_rate,
            total: info.length_frames.max(0) as u64,
            reader,
            pos: 0,
        })
    }
}

impl StereoSource for FileStereoSource {
    fn sample_rate(&self) -> f64 {
        self.rate
    }

    fn total_frames(&self) -> Option<u64> {
        Some(self.total)
    }

    fn read(&mut self, left: &mut [f32], right: &mut [f32]) -> Result<usize, StemError> {
        let want = left
            .len()
            .min(right.len())
            .min((self.total - self.pos) as usize);
        if want == 0 {
            return Ok(0);
        }
        let mut block = vec![[0.0f32; 2]; want];
        let got = self.reader.read_stereo(self.pos as i64, &mut block);
        for (i, f) in block[..got].iter().enumerate() {
            left[i] = f[0];
            right[i] = f[1];
        }
        self.pos += got as u64;
        Ok(got)
    }
}

/// Collects everything written into a [`StemSet`] (use for clips that fit in memory).
#[derive(Debug, Default)]
pub struct StemSetSink {
    set: StemSet,
    /// Largest single chunk received, in frames (tests assert the streaming bound with it).
    pub max_chunk: usize,
}

impl StemSetSink {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            set: StemSet::new(sample_rate),
            max_chunk: 0,
        }
    }

    pub fn into_set(self) -> StemSet {
        self.set
    }
}

impl StemSink for StemSetSink {
    fn write(&mut self, kind: StemKind, left: &[f32], right: &[f32]) -> Result<(), StemError> {
        if left.len() != right.len() {
            return Err(StemError::Sink("channel length mismatch".into()));
        }
        self.max_chunk = self.max_chunk.max(left.len());
        let buf = self.set.stems.entry(kind).or_default();
        buf.left.extend_from_slice(left);
        buf.right.extend_from_slice(right);
        Ok(())
    }
}

/// A separator that makes no model: it splits the input into four equal quarters, so the stems sum
/// back to the input. For tests and for wiring the rest of the pipeline.
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

const MOCK_CHUNK: usize = 4096;

impl StemSeparator for MockSeparator {
    fn id(&self) -> &str {
        &self.id
    }

    fn version(&self) -> i32 {
        self.version
    }

    /// The mock does no resampling; this only reports the usual model rate.
    fn native_rate(&self) -> f64 {
        44_100.0
    }

    fn separate_to(
        &self,
        input: &mut dyn StereoSource,
        sink: &mut dyn StemSink,
        ctx: &dyn AnalysisContext,
    ) -> Result<(), StemError> {
        let total = input.total_frames();
        let mut left = vec![0.0f32; MOCK_CHUNK];
        let mut right = vec![0.0f32; MOCK_CHUNK];
        let mut done = 0u64;
        loop {
            if ctx.is_cancelled() {
                return Err(StemError::Cancelled);
            }
            let n = input.read(&mut left, &mut right)?;
            if n == 0 {
                break;
            }
            left[..n].iter_mut().for_each(|s| *s *= 0.25);
            right[..n].iter_mut().for_each(|s| *s *= 0.25);
            for kind in StemKind::ALL {
                sink.write(kind, &left[..n], &right[..n])?;
            }
            done += n as u64;
            if let Some(t) = total.filter(|&t| t > 0) {
                ctx.set_progress((done as f32 / t as f32).min(1.0));
            }
        }
        ctx.set_progress(1.0);
        Ok(())
    }
}

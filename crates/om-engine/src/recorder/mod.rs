//! Mix recording: a tap on the final master output, a lock-free ring, and a writer thread.
//!
//! [`Recorder::new`] returns the audio half ([`RecorderTap`], owned by `DjEngineAudio`) and the
//! control half ([`RecorderControl`]). The tap runs after master gain and the master inserts, so the
//! file holds exactly what goes to the master outputs (not the cue mix). While recording is off the
//! tap is one relaxed atomic load. While on, the audio thread only pushes interleaved `f32` frames
//! into a preallocated `rtrb` ring; when the ring is full the frames are dropped and counted, never
//! waited for. The writer thread drains the ring into a streaming WAV file (see `wav`), flushes about
//! once a second, and rewrites the header sizes every few seconds so a crash leaves a playable file.
//! A file is split into `-partN` files before the 4 GiB WAV limit without losing a frame.
//!
//! Threads: `RecorderTap::process` on the audio thread only; everything else on control threads.

mod wav;
mod writer;

use om_core::atomic::{AtomicF32, AtomicF64};
use om_core::sync::LockExt;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;
use wav::WavOut;

/// Ring size in samples: two seconds of stereo at 96 kHz.
const RING_SAMPLES: usize = 2 * 96_000 * 2;
/// A recording stops with an error below this much free space.
pub const MIN_FREE_BYTES: u64 = 200 * 1024 * 1024;
/// The status warns below this much free space.
pub const WARN_FREE_BYTES: u64 = 1024 * 1024 * 1024;
/// Largest data chunk of one file: the WAV size field is 32 bits; leave a safety margin.
pub const DEFAULT_MAX_DATA_BYTES: u64 = u32::MAX as u64 - 1024 * 1024;

/// Sample format of the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RecordFormat {
    Pcm16,
    #[default]
    Pcm24,
    Float32,
}

impl RecordFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pcm16 => "pcm16",
            Self::Pcm24 => "pcm24",
            Self::Float32 => "float32",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "pcm16" => Some(Self::Pcm16),
            "pcm24" => Some(Self::Pcm24),
            "float32" => Some(Self::Float32),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RecordState {
    #[default]
    Idle,
    Recording,
    /// The writer stopped itself (disk error or low disk); `stop` returns the summary.
    Error,
}

impl RecordState {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Recording,
            2 => Self::Error,
            _ => Self::Idle,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RecordError {
    #[error("already recording")]
    AlreadyRecording,
    #[error("not recording")]
    NotRecording,
    #[error("the audio device has not been started yet")]
    NotPrepared,
    #[error("invalid recording folder: {0}")]
    BadFolder(String),
    #[error("not enough free disk space ({0} bytes)")]
    LowDisk(u64),
    #[error("recording i/o: {0}")]
    Io(#[from] std::io::Error),
}

/// Free bytes on the volume holding the path, or `None` when unknown (the check is then skipped).
pub type FreeSpaceFn = Arc<dyn Fn(&Path) -> Option<u64> + Send + Sync>;

/// Free space through the platform's `df` (no unsafe code, no extra dependency). `None` when `df`
/// is missing or its output is unparseable.
pub fn free_space_df(path: &Path) -> Option<u64> {
    let out = std::process::Command::new("df")
        .arg("-Pk")
        .arg(path)
        .output()
        .ok()?;
    let text = String::from_utf8(out.stdout).ok()?;
    let line = text.lines().nth(1)?;
    let kb: u64 = line.split_whitespace().nth(3)?.parse().ok()?;
    kb.checked_mul(1024)
}

#[derive(Clone)]
pub struct RecordSettings {
    pub folder: PathBuf,
    pub format: RecordFormat,
    pub stop_on_low_disk: bool,
    /// Data bytes per file before the split. Injectable for tests.
    pub max_data_bytes: u64,
    pub header_interval: Duration,
    pub flush_interval: Duration,
    pub free_space: FreeSpaceFn,
    /// Test hook: the writer thread does not drain while this is set.
    pub stall: Option<Arc<AtomicBool>>,
    /// Optional file stem (sanitised); the default is `OpenMixer-YYYY-MM-DD-HHMMSS` (UTC).
    pub file_stem: Option<String>,
}

impl RecordSettings {
    pub fn new(folder: impl Into<PathBuf>) -> Self {
        Self {
            folder: folder.into(),
            format: RecordFormat::default(),
            stop_on_low_disk: true,
            max_data_bytes: DEFAULT_MAX_DATA_BYTES,
            header_interval: Duration::from_secs(5),
            flush_interval: Duration::from_secs(1),
            free_space: Arc::new(free_space_df),
            stall: None,
            file_stem: None,
        }
    }
}

/// What the user sees while recording.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecordStatus {
    pub state: RecordState,
    pub elapsed_s: f64,
    pub bytes: u64,
    pub dropped_frames: u64,
    pub peak_l: f32,
    pub peak_r: f32,
    pub file: Option<PathBuf>,
    /// Free space below [`WARN_FREE_BYTES`].
    pub low_disk_warning: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RecordSummary {
    pub files: Vec<PathBuf>,
    pub frames: u64,
    pub bytes: u64,
    pub dropped_frames: u64,
    pub duration_s: f64,
    pub error: Option<String>,
}

#[derive(Default)]
pub(crate) struct Shared {
    enabled: AtomicBool,
    state: AtomicU8,
    rate: AtomicF64,
    dropped: AtomicU64,
    frames: AtomicU64,
    bytes: AtomicU64,
    peak_l: AtomicF32,
    peak_r: AtomicF32,
    low_disk: AtomicBool,
    error: Mutex<Option<String>>,
    file: Mutex<Option<PathBuf>>,
}

/// Audio half. Part of `DjEngineAudio`.
pub struct RecorderTap {
    shared: Arc<Shared>,
    tx: rtrb::Producer<f32>,
}

struct Running {
    handle: JoinHandle<writer::Outcome>,
    stop: Arc<AtomicBool>,
    abort: Arc<AtomicBool>,
    rx_back: std::sync::mpsc::Receiver<rtrb::Consumer<f32>>,
}

struct Inner {
    rx: Option<rtrb::Consumer<f32>>,
    running: Option<Running>,
}

/// Control half. Methods take `&self`.
pub struct RecorderControl {
    shared: Arc<Shared>,
    inner: Mutex<Inner>,
}

/// Namespace for [`Recorder::new`].
pub struct Recorder;

impl Recorder {
    pub fn new() -> (RecorderTap, RecorderControl) {
        let shared = Arc::new(Shared::default());
        let (tx, rx) = rtrb::RingBuffer::new(RING_SAMPLES);
        (
            RecorderTap {
                shared: Arc::clone(&shared),
                tx,
            },
            RecorderControl {
                shared,
                inner: Mutex::new(Inner {
                    rx: Some(rx),
                    running: None,
                }),
            },
        )
    }
}

impl RecorderTap {
    /// Records the sample rate the file will carry. Control/prepare time.
    pub fn prepare(&mut self, sample_rate: f64) {
        self.shared.rate.store(sample_rate);
    }

    /// Audio thread. `left` and `right` have equal length. Never blocks or allocates.
    pub fn process(&mut self, left: &[f32], right: &[f32]) {
        if !self.shared.enabled.load(Relaxed) {
            return;
        }
        let (mut pl, mut pr) = (0.0_f32, 0.0_f32);
        for (l, r) in left.iter().zip(right) {
            pl = pl.max(l.abs());
            pr = pr.max(r.abs());
        }
        // Peaks hold the larger of this block and a decayed previous value.
        self.shared
            .peak_l
            .store(pl.max(self.shared.peak_l.load() * 0.9));
        self.shared
            .peak_r
            .store(pr.max(self.shared.peak_r.load() * 0.9));

        let frames = left.len().min(right.len());
        let take = (frames * 2).min(self.tx.slots()) & !1;
        if take > 0 {
            if let Ok(chunk) = self.tx.write_chunk_uninit(take) {
                let it = left.iter().zip(right).flat_map(|(l, r)| [*l, *r]);
                chunk.fill_from_iter(it);
            }
        }
        let lost = (frames - take / 2) as u64;
        if lost > 0 {
            self.shared.dropped.fetch_add(lost, Relaxed);
        }
    }
}

impl RecorderControl {
    /// Starts a recording. Control thread.
    pub fn start(&self, settings: &RecordSettings) -> Result<(), RecordError> {
        let mut inner = self.inner.lock_unpoisoned();
        self.reap_finished(&mut inner);
        if inner.running.is_some() {
            return Err(RecordError::AlreadyRecording);
        }
        let rate = self.shared.rate.load();
        if rate < 1.0 {
            return Err(RecordError::NotPrepared);
        }
        let folder = checked_folder(&settings.folder)?;
        std::fs::create_dir_all(&folder)
            .map_err(|e| RecordError::BadFolder(format!("{}: {e}", folder.display())))?;
        let free = (settings.free_space)(&folder);
        if let Some(free) = free {
            if free < MIN_FREE_BYTES {
                return Err(RecordError::LowDisk(free));
            }
        }
        let stem = match &settings.file_stem {
            Some(s) => sanitise_stem(s),
            None => default_stem(),
        };
        let stem = unique_stem(&folder, &stem);
        let first = WavOut::create(&part_path(&folder, &stem, 1), settings.format, rate as u32)
            .map_err(|e| RecordError::BadFolder(format!("{}: {e}", folder.display())))?;

        let mut rx = inner.rx.take().ok_or(RecordError::AlreadyRecording)?;
        // Frames from before this recording are stale.
        let stale = rx.slots();
        if let Ok(chunk) = rx.read_chunk(stale) {
            chunk.commit_all();
        }
        let sh = &self.shared;
        sh.dropped.store(0, Relaxed);
        sh.frames.store(0, Relaxed);
        sh.bytes.store(wav::HEADER_LEN, Relaxed);
        sh.peak_l.store(0.0);
        sh.peak_r.store(0.0);
        sh.low_disk
            .store(free.is_some_and(|f| f < WARN_FREE_BYTES), Relaxed);
        *sh.error.lock_unpoisoned() = None;
        *sh.file.lock_unpoisoned() = Some(first.path.clone());
        sh.state.store(1, Relaxed);

        let stop = Arc::new(AtomicBool::new(false));
        let abort = Arc::new(AtomicBool::new(false));
        let (back_tx, rx_back) = std::sync::mpsc::channel();
        let ctx = writer::Context {
            shared: Arc::clone(&self.shared),
            settings: settings.clone(),
            folder,
            stem,
            rate: rate as u32,
            first: Some(first),
            rx,
            stop: Arc::clone(&stop),
            abort: Arc::clone(&abort),
            back: back_tx,
        };
        let handle = std::thread::Builder::new()
            .name("om-record-writer".into())
            .spawn(move || writer::run(ctx))
            .map_err(RecordError::Io)?;
        inner.running = Some(Running {
            handle,
            stop,
            abort,
            rx_back,
        });
        sh.enabled.store(true, Relaxed);
        Ok(())
    }

    /// Stops, drains the ring, finalises the file(s). Control thread.
    pub fn stop(&self) -> Result<RecordSummary, RecordError> {
        let mut inner = self.inner.lock_unpoisoned();
        let running = inner.running.take().ok_or(RecordError::NotRecording)?;
        self.shared.enabled.store(false, Relaxed);
        running.stop.store(true, Relaxed);
        Ok(self.join(&mut inner, running))
    }

    /// Test hook: ends the writer without finalising, as a crash would.
    #[doc(hidden)]
    pub fn crash_for_test(&self) {
        let mut inner = self.inner.lock_unpoisoned();
        if let Some(running) = inner.running.take() {
            self.shared.enabled.store(false, Relaxed);
            running.abort.store(true, Relaxed);
            self.join(&mut inner, running);
        }
    }

    fn join(&self, inner: &mut Inner, running: Running) -> RecordSummary {
        let outcome = running.handle.join().unwrap_or_else(|_| writer::Outcome {
            error: Some("the recording writer panicked".into()),
            ..Default::default()
        });
        inner.rx = running.rx_back.recv().ok();
        self.shared.state.store(0, Relaxed);
        let rate = self.shared.rate.load().max(1.0);
        RecordSummary {
            files: outcome.files,
            frames: outcome.frames,
            bytes: outcome.bytes,
            dropped_frames: self.shared.dropped.load(Relaxed),
            duration_s: outcome.frames as f64 / rate,
            error: outcome.error,
        }
    }

    /// A writer that stopped itself leaves state Error until `stop` or the next `start` collects it.
    fn reap_finished(&self, inner: &mut Inner) {
        if inner
            .running
            .as_ref()
            .is_some_and(|r| r.handle.is_finished())
        {
            if let Some(running) = inner.running.take() {
                self.join(inner, running);
            }
        }
    }

    /// Lock-free read of the state, for the snapshot and the control timer.
    pub fn state(&self) -> RecordState {
        RecordState::from_u8(self.shared.state.load(Relaxed))
    }

    /// The lock-free part of the status, for [`EngineSnapshot`](om_core::EngineSnapshot).
    pub fn snapshot(&self) -> om_core::RecordSnapshot {
        let state = self.state();
        om_core::RecordSnapshot {
            recording: state == RecordState::Recording,
            failed: state == RecordState::Error,
            elapsed_s: self.shared.frames.load(Relaxed) as f64 / self.shared.rate.load().max(1.0),
            dropped_frames: self.shared.dropped.load(Relaxed),
            peak_left: self.shared.peak_l.load(),
            peak_right: self.shared.peak_r.load(),
        }
    }

    pub fn status(&self) -> RecordStatus {
        let rate = self.shared.rate.load().max(1.0);
        RecordStatus {
            state: self.state(),
            elapsed_s: self.shared.frames.load(Relaxed) as f64 / rate,
            bytes: self.shared.bytes.load(Relaxed),
            dropped_frames: self.shared.dropped.load(Relaxed),
            peak_l: self.shared.peak_l.load(),
            peak_r: self.shared.peak_r.load(),
            file: self.shared.file.lock_unpoisoned().clone(),
            low_disk_warning: self.shared.low_disk.load(Relaxed),
            error: self.shared.error.lock_unpoisoned().clone(),
        }
    }
}

/// Rejects empty folders and `..` components (a settings value or a hostile id must not climb out).
fn checked_folder(folder: &Path) -> Result<PathBuf, RecordError> {
    if folder.as_os_str().is_empty() {
        return Err(RecordError::BadFolder("empty path".into()));
    }
    if folder.components().any(|c| c == Component::ParentDir) {
        return Err(RecordError::BadFolder("'..' is not allowed".into()));
    }
    Ok(folder.to_path_buf())
}

fn sanitise_stem(s: &str) -> String {
    let clean: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(80)
        .collect();
    if clean.is_empty() {
        "OpenMixer".into()
    } else {
        clean
    }
}

fn part_path(folder: &Path, stem: &str, part: u32) -> PathBuf {
    if part <= 1 {
        folder.join(format!("{stem}.wav"))
    } else {
        folder.join(format!("{stem}-part{part}.wav"))
    }
}

/// A stem none of whose first two files exist yet.
fn unique_stem(folder: &Path, stem: &str) -> String {
    let taken = |s: &str| part_path(folder, s, 1).exists() || part_path(folder, s, 2).exists();
    if !taken(stem) {
        return stem.to_string();
    }
    (1..)
        .map(|n| format!("{stem}-{n}"))
        .find(|s| !taken(s))
        .unwrap_or_else(|| stem.to_string())
}

fn default_stem() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    stem_from_unix(secs)
}

/// `OpenMixer-YYYY-MM-DD-HHMMSS` for a Unix time (UTC; the engine has no time zone database).
pub fn stem_from_unix(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Civil-from-days (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "OpenMixer-{y:04}-{m:02}-{d:02}-{:02}{:02}{:02}",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests;

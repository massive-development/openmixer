//! The sample bank: the samples on the sampler's pads, and what each pad sounds like.
//!
//! The bank lives in `<data dir>/samples/`: WAV (or other decodable) files sit beside
//! `sampler.json`, which maps pads to files. Layout of the file (version 1):
//!
//! ```text
//! {"version": 1, "pads": [{"pad": 0, "file": "kick.wav", "name": "Kick",
//!                          "gain": 1.0, "pitch": 0.0, "mode": "oneShot"}]}
//! ```
//!
//! `file` is relative to `samples/` or absolute. `mode` is `oneShot`, `gate` or `loop`. Bad entries
//! are skipped with a note (pad out of range, duplicate, no file); bad values fall back to defaults.
//! A missing sample file is not an error: the pad stays empty and a note says so.
//!
//! Decoding never runs on the caller's thread: [`SampleBank::import_file`] and the start-up load
//! queue a job on a worker, and [`SampleBank::pump`] (called from a control tick) installs the
//! results through `Command::SamplerAssign`. A decode that fails leaves the pad's previous sample
//! in place.

use crate::preferences::data_dir;
use om_core::jobs::{JobHandle, WorkerPool};
use om_core::sync::LockExt;
use om_core::{Command, CommandSink, SampleId};
use om_engine::sampler::{PadMode, Sample, SamplerError, MAX_PAD_GAIN, MAX_PITCH_SEMITONES, PADS};
use om_engine::DjEngine;
use serde_json::{Map, Value};
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use thiserror::Error;

/// Folder under the data dir that holds the sample files and `sampler.json`.
pub const SAMPLES_DIR: &str = "samples";
/// The pad map, beside the sample files.
pub const BANK_FILE: &str = "sampler.json";
/// Schema version written to disk. Higher versions are refused and left untouched.
pub const BANK_VERSION: i64 = 1;
/// Highest number of `name-N.ext` copies tried before an import gives up.
const MAX_COPIES: u32 = 999;

/// `<data dir>/samples`, the default bank folder.
pub fn default_dir() -> PathBuf {
    data_dir().join(SAMPLES_DIR)
}

/// What one pad holds, as stored.
#[derive(Debug, Clone, PartialEq)]
pub struct PadEntry {
    /// Relative to the bank folder, or absolute.
    pub file: String,
    pub name: String,
    /// Linear, 0 to [`MAX_PAD_GAIN`].
    pub gain: f32,
    /// Semitones, within the engine's pitch limit.
    pub pitch: f32,
    pub mode: PadMode,
}

impl PadEntry {
    /// An entry with the default gain, pitch and mode.
    pub fn new(file: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            file: file.into(),
            name: name.into(),
            gain: 1.0,
            pitch: 0.0,
            mode: PadMode::OneShot,
        }
    }
}

/// One slot per pad, `None` when the pad has no entry.
pub type BankPads = [Option<PadEntry>; PADS];

#[derive(Debug, Error)]
pub enum BankError {
    #[error("sampler.json: not a JSON object")]
    NotAnObject,
    #[error("sampler.json: unsupported version {0}")]
    UnsupportedVersion(i64),
    #[error("sampler.json: invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("pad {0} does not exist")]
    BadPad(u8),
    #[error("{0}")]
    Sample(#[from] SamplerError),
    #[error("the sampler rejected the sample: {0}")]
    Rejected(String),
    #[error("the pad was cleared or reloaded while this was decoding")]
    Superseded,
    #[error("the bank file is from a newer version and is not overwritten")]
    NotWritable,
}

/// A parsed `sampler.json`: the pads, and a line per thing that was skipped or defaulted.
#[derive(Debug, Default)]
pub struct Parsed {
    pub pads: BankPads,
    pub issues: Vec<String>,
}

/// Parses the bank file text. Fails only for JSON that is not an object or a newer version; every
/// other problem is an issue and the entry falls back.
pub fn parse(text: &str) -> Result<Parsed, BankError> {
    let root: Value = serde_json::from_str(text)?;
    let object = root.as_object().ok_or(BankError::NotAnObject)?;
    let mut parsed = Parsed::default();
    match object.get("version") {
        None => parsed
            .issues
            .push("no version in sampler.json; read as version 1".into()),
        Some(v) => match v.as_i64() {
            Some(n) if n == BANK_VERSION => {}
            Some(n) if n > BANK_VERSION => return Err(BankError::UnsupportedVersion(n)),
            _ => parsed
                .issues
                .push(format!("bad version {v}; read as version 1")),
        },
    }
    let Some(list) = object.get("pads") else {
        return Ok(parsed);
    };
    let Some(items) = list.as_array() else {
        parsed
            .issues
            .push("pads is not a list; no pads loaded".into());
        return Ok(parsed);
    };
    for (index, item) in items.iter().enumerate() {
        match parse_pad(item, &mut parsed.issues) {
            Ok((pad, entry)) => {
                if parsed.pads[pad].is_some() {
                    parsed.issues.push(format!(
                        "entry {index}: pad {} listed twice; kept the first",
                        pad + 1
                    ));
                } else {
                    parsed.pads[pad] = Some(entry);
                }
            }
            Err(reason) => parsed
                .issues
                .push(format!("entry {index} skipped: {reason}")),
        }
    }
    Ok(parsed)
}

/// One pad entry. `Err` skips the entry; `issues` collects the values that fell back.
fn parse_pad(item: &Value, issues: &mut Vec<String>) -> Result<(usize, PadEntry), String> {
    let object = item.as_object().ok_or("not an object")?;
    let pad = object
        .get("pad")
        .and_then(Value::as_i64)
        .filter(|p| (0..PADS as i64).contains(p))
        .ok_or_else(|| {
            format!(
                "pad {} is not in 0..{}",
                object
                    .get("pad")
                    .map_or_else(|| "(missing)".into(), Value::to_string),
                PADS
            )
        })? as usize;
    let file = object
        .get("file")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("pad {}: no file", pad + 1))?
        .to_owned();
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map_or_else(|| stem_of(&file), str::to_owned);
    let gain = number(
        object,
        "gain",
        1.0,
        0.0,
        f64::from(MAX_PAD_GAIN),
        pad,
        issues,
    );
    let pitch = number(
        object,
        "pitch",
        0.0,
        -f64::from(MAX_PITCH_SEMITONES),
        f64::from(MAX_PITCH_SEMITONES),
        pad,
        issues,
    );
    let mode = match object.get("mode") {
        None => PadMode::OneShot,
        Some(v) => v.as_str().and_then(parse_mode).unwrap_or_else(|| {
            issues.push(format!("pad {}: unknown mode {v}; using oneShot", pad + 1));
            PadMode::OneShot
        }),
    };
    Ok((
        pad,
        PadEntry {
            file,
            name,
            gain,
            pitch,
            mode,
        },
    ))
}

/// A finite number in `lo..=hi`. Missing means the default; a wrong type or a non-finite value is an
/// issue and the default too.
fn number(
    object: &Map<String, Value>,
    key: &str,
    default: f64,
    lo: f64,
    hi: f64,
    pad: usize,
    issues: &mut Vec<String>,
) -> f32 {
    match object.get(key) {
        None => default as f32,
        Some(v) => match v.as_f64().filter(|n| n.is_finite()) {
            Some(n) => n.clamp(lo, hi) as f32,
            None => {
                issues.push(format!(
                    "pad {}: {key} {v} is not a number; using {default}",
                    pad + 1
                ));
                default as f32
            }
        },
    }
}

fn parse_mode(name: &str) -> Option<PadMode> {
    match name {
        "oneShot" => Some(PadMode::OneShot),
        "gate" => Some(PadMode::Gate),
        "loop" => Some(PadMode::Loop),
        _ => None,
    }
}

/// The name a mode is stored under.
pub fn mode_name(mode: PadMode) -> &'static str {
    match mode {
        PadMode::OneShot => "oneShot",
        PadMode::Gate => "gate",
        PadMode::Loop => "loop",
    }
}

/// The bank file text for `pads`, pretty-printed.
pub fn to_json(pads: &BankPads) -> String {
    let entries: Vec<Value> = pads
        .iter()
        .enumerate()
        .filter_map(|(pad, entry)| {
            entry.as_ref().map(|e| {
                serde_json::json!({
                    "pad": pad,
                    "file": e.file,
                    "name": e.name,
                    "gain": e.gain,
                    "pitch": e.pitch,
                    "mode": mode_name(e.mode),
                })
            })
        })
        .collect();
    let root = serde_json::json!({ "version": BANK_VERSION, "pads": entries });
    serde_json::to_string_pretty(&root).unwrap_or_else(|_| "{}".to_owned())
}

fn stem_of(file: &str) -> String {
    Path::new(file)
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("sample")
        .to_owned()
}

/// Work for the decode thread.
enum Job {
    /// Copy `source` into `dir` (new name on a clash), then decode it.
    Import {
        pad: usize,
        source: PathBuf,
        dir: PathBuf,
        rate: f64,
        generation: u64,
    },
    /// Decode a file the bank already names.
    Load {
        pad: usize,
        file: String,
        path: PathBuf,
        name: String,
        rate: f64,
        generation: u64,
    },
}

/// A decode result waiting for [`SampleBank::pump`].
struct Finished {
    pad: usize,
    /// The pad's generation when the job was queued; stale once the pad is cleared or reloaded.
    generation: u64,
    /// What the failure is about (the file or source name), for the note.
    about: String,
    /// The name the pad entry will carry.
    name: String,
    outcome: Result<(String, Arc<Sample>), BankError>,
}

pub struct SampleBank {
    dir: PathBuf,
    engine: Arc<DjEngine>,
    sink: Arc<dyn CommandSink>,
    pool: WorkerPool<Job>,
    finished: Arc<Mutex<Vec<Finished>>>,
    /// Decodes queued or running: incremented on submit, decremented once the result is queued.
    outstanding: Arc<AtomicUsize>,
    pads: Mutex<BankPads>,
    /// Bumped when a decode is queued for a pad and when the pad is cleared. A finished decode
    /// installs only if its generation is still the pad's, so a clear (or a newer load) wins.
    generations: [AtomicU64; PADS],
    notes: Mutex<Vec<String>>,
    writable: bool,
}

impl SampleBank {
    /// Reads `dir/sampler.json` (missing means empty). A corrupt file is copied to `sampler.json.bad`
    /// and the bank starts empty; a newer version is kept as it is and the bank will not save over
    /// it. Every issue becomes a note. Nothing is decoded or assigned yet: call [`SampleBank::apply`].
    pub fn open(dir: PathBuf, engine: Arc<DjEngine>, sink: Arc<dyn CommandSink>) -> Self {
        let mut notes = Vec::new();
        let mut writable = true;
        let mut pads: BankPads = Default::default();
        let file = dir.join(BANK_FILE);
        if file.is_file() {
            match fs::read_to_string(&file)
                .map_err(BankError::from)
                .and_then(|text| parse(&text))
            {
                Ok(parsed) => {
                    notes.extend(parsed.issues);
                    pads = parsed.pads;
                }
                Err(BankError::UnsupportedVersion(v)) => {
                    writable = false;
                    notes.push(format!(
                        "sampler.json is version {v}; samples are off and the file is kept"
                    ));
                }
                Err(error) => {
                    let mut bad = file.as_os_str().to_owned();
                    bad.push(".bad");
                    let copied = fs::copy(&file, PathBuf::from(bad)).is_ok();
                    notes.push(format!(
                        "sampler.json could not be read ({error}){}",
                        if copied {
                            "; kept as sampler.json.bad"
                        } else {
                            ""
                        }
                    ));
                }
            }
        }
        let finished: Arc<Mutex<Vec<Finished>>> = Arc::new(Mutex::new(Vec::new()));
        let outstanding = Arc::new(AtomicUsize::new(0));
        let sink_queue = Arc::clone(&finished);
        let done_count = Arc::clone(&outstanding);
        let pool = WorkerPool::new(1, "sample-decode", move |_ctx, job: Job| {
            let result = run_job(&job);
            sink_queue.lock_unpoisoned().push(result);
            done_count.fetch_sub(1, Ordering::AcqRel);
        });
        Self {
            dir,
            engine,
            sink,
            pool,
            finished,
            outstanding,
            pads: Mutex::new(pads),
            generations: std::array::from_fn(|_| AtomicU64::new(0)),
            notes: Mutex::new(notes),
            writable,
        }
    }

    /// The bank folder.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// A copy of every pad's entry.
    pub fn pads(&self) -> BankPads {
        self.pads.lock_unpoisoned().clone()
    }

    /// The entry on `pad`, if any.
    pub fn entry(&self, pad: u8) -> Option<PadEntry> {
        self.pads.lock_unpoisoned().get(usize::from(pad))?.clone()
    }

    /// True when `pad` has an entry whose file is not there.
    pub fn is_missing(&self, pad: u8) -> bool {
        self.entry(pad)
            .is_some_and(|e| !self.resolve(&e.file).is_file())
    }

    /// Where a stored file name points: a relative name is inside the bank folder.
    pub fn resolve(&self, file: &str) -> PathBuf {
        let path = Path::new(file);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.dir.join(path)
        }
    }

    /// The notes raised since the last call (issues, missing files, failed decodes), oldest first.
    pub fn take_notes(&self) -> Vec<String> {
        std::mem::take(&mut *self.notes.lock_unpoisoned())
    }

    /// Applies the stored bank: gain, pitch and mode to every pad with an entry; each file that
    /// exists is queued for decode (installed by [`SampleBank::pump`]); a missing file leaves the
    /// pad empty with a note. Returns how many decodes were queued.
    pub fn apply(&self) -> usize {
        let rate = self.engine.sampler().engine_rate();
        let pads = self.pads();
        let mut queued = 0;
        for (pad, entry) in pads.iter().enumerate() {
            let Some(entry) = entry else { continue };
            self.apply_settings(pad, entry.gain, entry.pitch, entry.mode);
            let path = self.resolve(&entry.file);
            if !path.is_file() {
                self.note(format!(
                    "pad {}: {} is missing; the pad is empty",
                    pad + 1,
                    entry.file
                ));
                continue;
            }
            self.submit(Job::Load {
                pad,
                file: entry.file.clone(),
                path,
                name: entry.name.clone(),
                rate,
                generation: self.next_generation(pad),
            });
            queued += 1;
        }
        queued
    }

    /// Queues `source` for `pad`: it is decoded and, if it is not already in the bank folder,
    /// copied in (a clashing name becomes `name-2.wav`, `name-3.wav`, ...). Returns the job handle;
    /// the sample reaches the pad on the next [`SampleBank::pump`] after the job finishes. The pad's
    /// gain, pitch and mode are kept. A bad pad number fails before anything is queued.
    pub fn import_file(&self, pad: u8, source: &Path) -> Result<JobHandle, BankError> {
        let index = check_pad(pad)?;
        if !self.writable {
            return Err(BankError::NotWritable);
        }
        let job = Job::Import {
            pad: index,
            source: source.to_path_buf(),
            dir: self.dir.clone(),
            rate: self.engine.sampler().engine_rate(),
            generation: self.next_generation(index),
        };
        Ok(self.submit(job))
    }

    /// Installs the decodes that have finished: each is registered and sent as
    /// `Command::SamplerAssign`, the entry is updated and the bank file saved. Failed decodes become
    /// notes and leave the pad as it was. Call from a control tick. Returns how many were installed.
    pub fn pump(&self) -> usize {
        let done: Vec<Finished> = std::mem::take(&mut *self.finished.lock_unpoisoned());
        let mut installed = 0;
        for result in done {
            match result.outcome {
                Ok((file, sample)) => {
                    match self.install(result.pad, result.generation, file, &result.name, sample) {
                        Ok(()) => installed += 1,
                        Err(error) => self.note(format!(
                            "pad {}: {} could not be assigned ({error})",
                            result.pad + 1,
                            result.about
                        )),
                    }
                }
                Err(error) => self.note(format!(
                    "pad {}: {} could not be loaded ({error}); the pad keeps its sample",
                    result.pad + 1,
                    result.about
                )),
            }
        }
        installed
    }

    /// Waits up to `timeout` for every queued decode to finish, then installs them. For tests and
    /// start-up code that can block; the UI calls [`SampleBank::pump`] instead.
    pub fn settle(&self, timeout: Duration) -> usize {
        let deadline = std::time::Instant::now() + timeout;
        while self.outstanding.load(Ordering::Acquire) > 0 && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(2));
        }
        self.pump()
    }

    /// Empties `pad` (its sample fades out) and removes its entry. The file stays in the folder.
    pub fn clear_pad(&self, pad: u8) -> Result<(), BankError> {
        let index = check_pad(pad)?;
        // Under the pads lock, like `install`, so a decode finishing now cannot slip in after it.
        let mut pads = self.pads.lock_unpoisoned();
        self.next_generation(index);
        self.engine.sampler().clear(pad)?;
        pads[index] = None;
        self.save_locked(&pads)
    }

    /// Sets the gain, pitch and mode of `pad` and stores them when the pad has an entry. Applied at
    /// once to the engine (voices already sounding take the new pitch and gain).
    pub fn set_settings(
        &self,
        pad: u8,
        gain: f32,
        pitch: f32,
        mode: PadMode,
    ) -> Result<(), BankError> {
        let index = check_pad(pad)?;
        let gain = clamp_gain(gain);
        let pitch = clamp_pitch(pitch);
        self.apply_settings(index, gain, pitch, mode);
        let mut pads = self.pads.lock_unpoisoned();
        if let Some(entry) = pads[index].as_mut() {
            entry.gain = gain;
            entry.pitch = pitch;
            entry.mode = mode;
            self.save_locked(&pads)?;
        }
        Ok(())
    }

    /// Writes `sampler.json` atomically (temp file, then rename).
    pub fn save(&self) -> Result<(), BankError> {
        let pads = self.pads.lock_unpoisoned();
        self.save_locked(&pads)
    }

    fn save_locked(&self, pads: &BankPads) -> Result<(), BankError> {
        if !self.writable {
            return Err(BankError::NotWritable);
        }
        fs::create_dir_all(&self.dir)?;
        let path = self.dir.join(BANK_FILE);
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(".tmp");
        let tmp = PathBuf::from(tmp);
        fs::write(&tmp, to_json(pads))?;
        fs::rename(&tmp, &path)?;
        Ok(())
    }

    fn submit(&self, job: Job) -> JobHandle {
        self.outstanding.fetch_add(1, Ordering::AcqRel);
        self.pool.submit(job)
    }

    fn apply_settings(&self, pad: usize, gain: f32, pitch: f32, mode: PadMode) {
        let Ok(index) = u8::try_from(pad) else { return };
        let sampler = self.engine.sampler();
        sampler.set_gain(index, gain);
        sampler.set_pitch(index, pitch);
        sampler.set_mode(index, mode);
    }

    fn next_generation(&self, pad: usize) -> u64 {
        self.generations[pad].fetch_add(1, Ordering::AcqRel) + 1
    }

    fn install(
        &self,
        pad: usize,
        generation: u64,
        file: String,
        name: &str,
        sample: Arc<Sample>,
    ) -> Result<(), BankError> {
        let pad_u8 = u8::try_from(pad).map_err(|_| BankError::BadPad(u8::MAX))?;
        // The generation check and the assign are atomic with respect to `clear_pad`.
        let mut pads = self.pads.lock_unpoisoned();
        if self.generations[pad].load(Ordering::Acquire) != generation {
            return Err(BankError::Superseded);
        }
        let id: SampleId = self.engine.register_sample(sample);
        let sent = self.sink.send(Command::SamplerAssign {
            pad: pad_u8,
            sample: id,
        });
        self.engine.forget_sample(id);
        sent.map_err(|e| BankError::Rejected(e.to_string()))?;
        let previous = pads[pad].take();
        let (gain, pitch, mode) = previous
            .as_ref()
            .map_or((1.0, 0.0, PadMode::OneShot), |p| (p.gain, p.pitch, p.mode));
        pads[pad] = Some(PadEntry {
            file,
            name: name.to_owned(),
            gain,
            pitch,
            mode,
        });
        if self.writable {
            if let Err(error) = self.save_locked(&pads) {
                self.note(format!("sampler.json was not saved: {error}"));
            }
        }
        Ok(())
    }

    fn note(&self, text: String) {
        self.notes.lock_unpoisoned().push(text);
    }
}

fn check_pad(pad: u8) -> Result<usize, BankError> {
    if usize::from(pad) < PADS {
        Ok(usize::from(pad))
    } else {
        Err(BankError::BadPad(pad))
    }
}

fn clamp_gain(gain: f32) -> f32 {
    if gain.is_finite() {
        gain.clamp(0.0, MAX_PAD_GAIN)
    } else {
        1.0
    }
}

fn clamp_pitch(pitch: f32) -> f32 {
    if pitch.is_finite() {
        pitch.clamp(-MAX_PITCH_SEMITONES, MAX_PITCH_SEMITONES)
    } else {
        0.0
    }
}

/// Decode-thread body. Decodes first, so a bad file is never copied into the bank.
fn run_job(job: &Job) -> Finished {
    match job {
        Job::Import {
            pad,
            source,
            dir,
            rate,
            generation,
        } => {
            let about = source.display().to_string();
            let outcome = Sample::load(source, *rate)
                .map_err(BankError::from)
                .and_then(|sample| {
                    let file = copy_into_bank(dir, source)?;
                    Ok((file, Arc::new(sample)))
                });
            let name = stem_of(&source.to_string_lossy());
            Finished {
                pad: *pad,
                generation: *generation,
                about,
                name,
                outcome,
            }
        }
        Job::Load {
            pad,
            file,
            path,
            name,
            rate,
            generation,
        } => {
            let outcome = Sample::load(path, *rate)
                .map(|sample| (file.clone(), Arc::new(sample)))
                .map_err(BankError::from);
            Finished {
                pad: *pad,
                generation: *generation,
                about: file.clone(),
                name: name.clone(),
                outcome,
            }
        }
    }
}

/// Copies `source` into `dir` under its own name, or `name-N.ext` when that name is taken, and
/// returns the stored name. A file already in `dir` is used where it is.
fn copy_into_bank(dir: &Path, source: &Path) -> Result<String, BankError> {
    fs::create_dir_all(dir)?;
    let base = source
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| !n.is_empty())
        .ok_or_else(|| BankError::Io(io::Error::new(io::ErrorKind::InvalidInput, "no file name")))?
        .to_owned();
    if let (Ok(a), Ok(b)) = (
        source
            .parent()
            .map_or_else(|| Ok(PathBuf::new()), fs::canonicalize),
        fs::canonicalize(dir),
    ) {
        if a == b {
            return Ok(base);
        }
    }
    let stem = stem_of(&base);
    let ext = Path::new(&base)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    for n in 1..=MAX_COPIES {
        let candidate = if n == 1 {
            base.clone()
        } else {
            format!("{stem}-{n}{ext}")
        };
        let target = dir.join(&candidate);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
        {
            Ok(mut out) => {
                let copied =
                    File::open(source).and_then(|mut input| io::copy(&mut input, &mut out));
                if let Err(error) = copied {
                    drop(out);
                    let _ = fs::remove_file(&target);
                    return Err(error.into());
                }
                return Ok(candidate);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(BankError::Io(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "too many samples with this name",
    )))
}

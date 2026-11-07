//! EBU R128 loudness metering of the master bus (momentary, short-term, integrated, loudness range
//! and true peak), measured by the `ebur128` crate off the audio thread.
//!
//! The audio callback holds a [`LoudnessTap`]. When metering is enabled it copies each master block
//! into a preallocated `rtrb` ring and does nothing else: no allocation, lock or syscall. A worker
//! thread owned by [`LoudnessControl`] drains the ring into a [`LoudnessAnalyser`] and publishes a
//! [`LoudnessReading`] through atomics, which [`LoudnessControl::reading`] returns on any thread.
//!
//! Metering is off until [`LoudnessControl::enable`] is called, which also starts the worker the
//! first time. An engine that never enables it pays one atomic load per block and no thread. If the
//! ring ever fills (worker stalled), the lost frames are counted in
//! [`LoudnessReading::dropped_frames`] and the measurement should be treated as suspect until
//! [`LoudnessControl::reset`].

mod analyser;
mod worker;

#[cfg(test)]
mod tests;

pub use analyser::LoudnessAnalyser;

use om_core::atomic::AtomicF32;
use om_core::sync::LockExt;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::Relaxed, Ordering::Release};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

/// Ring capacity in samples (interleaved stereo): about 1.4 s at 48 kHz, so the worker can stall
/// for a second before frames are lost. Even, so a stereo frame never straddles the wrap.
const RING_SAMPLES: usize = 1 << 17;

/// Loudness values for a meter. Loudness is in LUFS, loudness range in LU, true peak in dBTP.
/// "Nothing yet" is `-inf` for every level and `0.0` for the range, so a meter can clamp to its
/// floor without a special case.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoudnessReading {
    /// Last 400 ms, ungated.
    pub momentary_lufs: f32,
    /// Last 3 s, ungated.
    pub short_term_lufs: f32,
    /// Since the last reset, gated (EBU R128 / ITU-R BS.1770 two-stage gate).
    pub integrated_lufs: f32,
    /// EBU Tech 3342 loudness range since the last reset.
    pub loudness_range_lu: f32,
    /// Highest 4x-oversampled peak per channel (left, right) since the last reset.
    pub true_peak_dbtp: [f32; 2],
    /// Frames lost because the ring was full since the last reset.
    pub dropped_frames: u64,
    /// Frames measured since the last reset; tells "no audio arrived" from "audio is silent".
    pub frames_measured: u64,
}

impl LoudnessReading {
    /// Nothing measured.
    pub const SILENT: Self = Self {
        momentary_lufs: f32::NEG_INFINITY,
        short_term_lufs: f32::NEG_INFINITY,
        integrated_lufs: f32::NEG_INFINITY,
        loudness_range_lu: 0.0,
        true_peak_dbtp: [f32::NEG_INFINITY; 2],
        dropped_frames: 0,
        frames_measured: 0,
    };

    /// The higher of the two channels' true peaks.
    pub fn true_peak_max_dbtp(&self) -> f32 {
        self.true_peak_dbtp[0].max(self.true_peak_dbtp[1])
    }
}

impl Default for LoudnessReading {
    fn default() -> Self {
        Self::SILENT
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LoudnessError {
    #[error("loudness meter rejected the configuration: {0}")]
    Meter(String),
    #[error("could not start the loudness worker thread: {0}")]
    Spawn(#[from] std::io::Error),
}

pub(crate) struct Shared {
    enabled: AtomicBool,
    stop: AtomicBool,
    reset: AtomicBool,
    rate: AtomicU32,
    dropped: AtomicU64,
    frames: AtomicU64,
    momentary: AtomicF32,
    short_term: AtomicF32,
    integrated: AtomicF32,
    range: AtomicF32,
    peak_l: AtomicF32,
    peak_r: AtomicF32,
}

impl Shared {
    fn new() -> Self {
        let s = LoudnessReading::SILENT;
        Self {
            enabled: AtomicBool::new(false),
            stop: AtomicBool::new(false),
            reset: AtomicBool::new(false),
            rate: AtomicU32::new(0),
            dropped: AtomicU64::new(0),
            frames: AtomicU64::new(0),
            momentary: AtomicF32::new(s.momentary_lufs),
            short_term: AtomicF32::new(s.short_term_lufs),
            integrated: AtomicF32::new(s.integrated_lufs),
            range: AtomicF32::new(s.loudness_range_lu),
            peak_l: AtomicF32::new(s.true_peak_dbtp[0]),
            peak_r: AtomicF32::new(s.true_peak_dbtp[1]),
        }
    }

    fn publish(&self, r: &LoudnessReading) {
        self.momentary.store(r.momentary_lufs);
        self.short_term.store(r.short_term_lufs);
        self.integrated.store(r.integrated_lufs);
        self.range.store(r.loudness_range_lu);
        self.peak_l.store(r.true_peak_dbtp[0]);
        self.peak_r.store(r.true_peak_dbtp[1]);
    }

    fn publish_silent(&self) {
        self.publish(&LoudnessReading::SILENT);
        self.frames.store(0, Relaxed);
    }
}

/// Audio half. Part of `DjEngineAudio`.
pub struct LoudnessTap {
    shared: Arc<Shared>,
    tx: rtrb::Producer<f32>,
}

struct Inner {
    rx: Option<rtrb::Consumer<f32>>,
    worker: Option<JoinHandle<()>>,
}

/// Control half. Methods take `&self`. Dropping it stops and joins the worker.
pub struct LoudnessControl {
    shared: Arc<Shared>,
    inner: Mutex<Inner>,
}

/// Namespace for [`Loudness::create`].
pub struct Loudness;

impl Loudness {
    pub fn create() -> (LoudnessTap, LoudnessControl) {
        let shared = Arc::new(Shared::new());
        let (tx, rx) = rtrb::RingBuffer::new(RING_SAMPLES);
        (
            LoudnessTap {
                shared: Arc::clone(&shared),
                tx,
            },
            LoudnessControl {
                shared,
                inner: Mutex::new(Inner {
                    rx: Some(rx),
                    worker: None,
                }),
            },
        )
    }
}

impl LoudnessTap {
    /// Tells the worker the stream's sample rate. Control or prepare time, not the audio thread.
    pub fn prepare(&mut self, sample_rate: f64) {
        self.shared.rate.store(sample_rate.round() as u32, Relaxed);
    }

    /// Audio thread. `left` and `right` have equal length. Never blocks or allocates.
    pub fn process(&mut self, left: &[f32], right: &[f32]) {
        if !self.shared.enabled.load(Relaxed) {
            return;
        }
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

impl LoudnessControl {
    /// Starts measuring. The first call spawns the worker. Control thread.
    pub fn enable(&self) -> Result<(), LoudnessError> {
        let mut inner = self.inner.lock_unpoisoned();
        if inner.worker.is_none() {
            let rx = inner.rx.take().ok_or_else(|| {
                LoudnessError::Meter("the loudness ring is already owned".to_owned())
            })?;
            let shared = Arc::clone(&self.shared);
            let spawned = std::thread::Builder::new()
                .name("om-loudness".to_owned())
                .spawn(move || worker::run(&shared, rx));
            match spawned {
                Ok(handle) => inner.worker = Some(handle),
                Err(e) => return Err(LoudnessError::Spawn(e)),
            }
        }
        self.shared.enabled.store(true, Release);
        Ok(())
    }

    /// Stops feeding the meter; the last reading stays readable. The worker idles.
    pub fn disable(&self) {
        self.shared.enabled.store(false, Release);
    }

    pub fn is_enabled(&self) -> bool {
        self.shared.enabled.load(Relaxed)
    }

    /// Clears integrated loudness, loudness range, true peak and the dropped-frame count. The
    /// worker applies it on its next pass (within about 10 ms).
    pub fn reset(&self) {
        self.shared.dropped.store(0, Relaxed);
        self.shared.reset.store(true, Release);
    }

    /// The latest published values. Any thread, lock-free. The fields are read one by one, so a
    /// reading taken mid-publish can mix two consecutive updates; for a meter that is invisible.
    pub fn reading(&self) -> LoudnessReading {
        let s = &self.shared;
        LoudnessReading {
            momentary_lufs: s.momentary.load(),
            short_term_lufs: s.short_term.load(),
            integrated_lufs: s.integrated.load(),
            loudness_range_lu: s.range.load(),
            true_peak_dbtp: [s.peak_l.load(), s.peak_r.load()],
            dropped_frames: s.dropped.load(Relaxed),
            frames_measured: s.frames.load(Relaxed),
        }
    }
}

impl Drop for LoudnessControl {
    fn drop(&mut self) {
        self.shared.stop.store(true, Release);
        let worker = self.inner.lock_unpoisoned().worker.take();
        if let Some(handle) = worker {
            if handle.join().is_err() {
                tracing::warn!("loudness worker panicked");
            }
        }
    }
}

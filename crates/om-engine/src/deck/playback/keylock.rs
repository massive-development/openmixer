//! Key-lock render path: source frames go through a [`Stretcher`] and the stretcher's output is
//! played, so tempo follows rate while pitch stays put.
//!
//! Switching on primes the stretcher while the interpolator keeps playing: each block pulls a few
//! hundred source frames, tees them into the stretcher, and discards stretcher output until it lines
//! up with the interpolator's playhead. Only then does the stretcher take over (`Mode::On`), so the
//! switch costs no gap and no CPU spike. Leaving the path replays the frames the stretcher had
//! taken in but not yet played, so nothing is skipped.

use super::{clear_tail, Playback};
use crate::deck::shared::Shared;
use om_core::FrameRing;
use om_stretch::{RubberBandStretcher, Stretcher};
use std::sync::atomic::Ordering::Relaxed;

/// Frames the interpolator replays before pulling new ones, and the newest frames kept for a
/// replay on leaving the path (a 16384-frame ring).
const RING_FRAMES: usize = 16_384;
/// Below this rate the stretch ratio explodes, so the deck is silent.
const MIN_KEYLOCK_RATE: f64 = 0.05;
/// Extra frames pulled per block while priming, which bounds the CPU cost.
const PRIME_CHUNK: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    Off,
    Priming,
    On,
}

pub(super) struct KeylockState {
    /// Prepared on the control thread; `None` until then, and the keylock path is skipped.
    pub(super) stretch: Option<Box<dyn Stretcher>>,
    pub(super) mode: Mode,
    /// Stretcher input staging, one buffer per channel, `max_feed` frames long.
    tee_in: [Vec<f32>; 2],
    /// Stretcher output discarded while aligning, `max_feed` frames long.
    sink: [Vec<f32>; 2],
    tee_count: usize,
    /// Stretcher output frames still to drop after the start pad.
    discard: usize,
    /// Interpolator pops since priming began.
    pub(super) priming_pops: i64,
    /// Source frames the discarded stretcher output has covered so far.
    aligned_source: f64,
    stretch_ratio: f64,
    /// Source index of the newest frame fed to the stretcher.
    last_fed_idx: i64,
    /// Frames the interpolator replays before pulling new ones.
    pub(super) lookahead: FrameRing,
    /// Newest frames the interpolator has taken in, for replay on leaving the path.
    pub(super) history: FrameRing,
}

impl KeylockState {
    pub(super) fn new() -> Self {
        Self {
            stretch: None,
            mode: Mode::Off,
            tee_in: [Vec::new(), Vec::new()],
            sink: [Vec::new(), Vec::new()],
            tee_count: 0,
            discard: 0,
            priming_pops: 0,
            aligned_source: 0.0,
            stretch_ratio: 1.0,
            last_fed_idx: 0,
            lookahead: FrameRing::new(RING_FRAMES),
            history: FrameRing::new(RING_FRAMES),
        }
    }

    /// Called when the playhead is repositioned: a priming run restarts from the new position.
    pub(super) fn on_playhead_reset(&mut self) {
        self.lookahead.clear();
        if self.mode == Mode::Priming {
            self.mode = Mode::Off;
        }
    }

    fn max_feed(&self) -> usize {
        self.tee_in[0].len()
    }
}

impl Playback {
    /// Creates the stretcher for `sample_rate` and sizes its buffers. Control thread, before audio
    /// runs. On failure the key-lock path stays off and the deck plays unstretched.
    pub(in crate::deck) fn prepare_keylock(&mut self, sample_rate: u32, max_block: usize) {
        let mut stretch: Box<dyn Stretcher> = Box::new(RubberBandStretcher::new());
        self.keylock.mode = Mode::Off;
        self.keylock.stretch = match stretch.prepare(sample_rate, 2, max_block.max(1)) {
            Ok(()) => {
                let max_feed = stretch.max_feed();
                let k = &mut self.keylock;
                for buf in k.tee_in.iter_mut().chain(k.sink.iter_mut()) {
                    buf.clear();
                    buf.resize(max_feed, 0.0);
                }
                Some(stretch)
            }
            Err(error) => {
                tracing::warn!(%error, "key-lock unavailable: stretcher did not prepare");
                None
            }
        };
    }

    /// Key-lock render for one block. Only called when the flag is set and the stretcher is prepared.
    pub(super) fn render_keylocked(&mut self, sh: &Shared, left: &mut [f32], right: &mut [f32]) {
        let rate = sh.rate.load();
        if rate < MIN_KEYLOCK_RATE {
            clear_tail(left, right, 0);
            return;
        }
        let device_over_file = sh.device_sample_rate.load() / sh.file_sample_rate.load();
        let ratio = device_over_file / rate;
        self.keylock.stretch_ratio = ratio;
        // Every block may change the ratio (a moving rate or a sync nudge). `set_ratios` compares
        // before it calls into Rubber Band, which re-plans on each real change. What the allocation
        // guard in `alloc_guard.rs` covers: the Rust side of this path, which allocates nothing even
        // with the ratio changing every block. What it does not cover: allocations inside Rubber
        // Band, and its re-planning CPU cost, which is not measured by any test here.
        if let Some(stretch) = self.keylock.stretch.as_mut() {
            stretch.set_ratios(ratio, 1.0 / device_over_file);
        }

        if self.keylock.mode == Mode::Off {
            self.begin_priming();
        }
        if self.keylock.mode == Mode::Priming {
            self.render_interpolated(sh, left, right);
            self.step_priming(sh);
            return;
        }
        self.render_stretched(sh, left, right);
    }

    /// Resets the stretcher and feeds it its start pad, so priming starts from a clean stream.
    fn begin_priming(&mut self) {
        let k = &mut self.keylock;
        let Some(stretch) = k.stretch.as_mut() else {
            return;
        };
        stretch.reset();
        let max_feed = stretch.max_feed() as i64;
        let mut pad = stretch.start_pad() as i64;
        while pad > 0 && max_feed > 0 {
            stretch.process_silence(pad.min(max_feed) as usize);
            pad -= max_feed;
        }
        k.discard = stretch.start_delay();
        k.aligned_source = 0.0;
        k.tee_count = 0;
        k.priming_pops = 0;
        k.mode = Mode::Priming;
        if self.primed {
            // Pulled already but not yet heard: it must reach the stretcher too.
            let (frame, index) = (self.next, self.next_idx);
            self.tee_frame(frame, index);
        }
    }

    /// One priming step, after the interpolator has rendered its block: pull ahead, flush the tee,
    /// and switch to the stretcher once its output is aligned.
    fn step_priming(&mut self, sh: &Shared) {
        self.pull_ahead(sh, PRIME_CHUNK);
        self.flush_tee();
        if self.discard_towards_alignment() {
            self.keylock.mode = Mode::On;
            self.keylock.lookahead.clear();
        }
    }

    /// Pulls up to `frames` source frames into the lookahead ring, teeing each into the stretcher.
    fn pull_ahead(&mut self, sh: &Shared, frames: usize) {
        for _ in 0..frames {
            if self.keylock.lookahead.len() >= self.keylock.lookahead.capacity() {
                break;
            }
            let Some((index, frame)) = self.pull_frame(sh) else {
                break;
            };
            self.keylock.lookahead.push(frame, index);
            self.tee_frame(frame, index);
        }
    }

    /// Drops stretcher output until it lines up with the interpolator's playhead. True once aligned.
    fn discard_towards_alignment(&mut self) -> bool {
        let ratio = self.keylock.stretch_ratio;
        let target = (self.keylock.priming_pops - 1) as f64 + self.phase;
        let mut avail = self.stretch_available();
        while avail > 0 {
            if self.keylock.discard > 0 {
                let n = avail.min(self.keylock.discard);
                self.keylock.discard -= self.retrieve_to_sink(n);
                avail = self.stretch_available();
                continue;
            }
            let behind = target - self.keylock.aligned_source;
            if behind <= 0.0 {
                break;
            }
            let k = avail.min((behind * ratio).ceil() as usize);
            let got = self.retrieve_to_sink(k);
            self.keylock.aligned_source += got as f64 / ratio;
            avail = self.stretch_available();
        }
        self.keylock.discard == 0 && self.keylock.aligned_source >= target
    }

    /// Plays stretcher output into the block, pulling and teeing more source frames as needed.
    fn render_stretched(&mut self, sh: &Shared, left: &mut [f32], right: &mut [f32]) {
        let total = left.len();
        let mut done = 0;
        while done < total {
            if self.stretch_available() > 0 {
                done += self.drain_stretch(&mut left[done..], &mut right[done..]);
            } else if !self.feed_stretch(sh) {
                break;
            }
        }
        if done < total {
            self.note_starved(sh);
            clear_tail(left, right, done);
        }
        let position = (self.keylock.last_fed_idx - self.stretch_latency_frames()).max(0);
        sh.position_frames.store(position, Relaxed);
    }

    /// Switches off the key-lock path. Frames the stretcher took in but has not played go back into
    /// the lookahead, so the interpolator replays them.
    pub(super) fn leave_keylock(&mut self) {
        if self.keylock.mode == Mode::On {
            let n = self.stretch_latency_frames().max(0) as usize;
            self.keylock
                .history
                .copy_newest_to(&mut self.keylock.lookahead, n);
            self.primed = false;
            self.have_first = false;
        }
        self.keylock.mode = Mode::Off;
    }

    /// Stores one source frame for the stretcher and the replay history. The stretcher input is
    /// flushed first when it is full.
    fn tee_frame(&mut self, frame: [f32; 2], index: i64) {
        let k = &mut self.keylock;
        if k.max_feed() == 0 {
            return;
        }
        if k.tee_count == k.max_feed() {
            self.flush_tee();
        }
        let k = &mut self.keylock;
        k.tee_in[0][k.tee_count] = frame[0];
        k.tee_in[1][k.tee_count] = frame[1];
        k.tee_count += 1;
        k.last_fed_idx = index;
        k.history.push(frame, index);
    }

    fn flush_tee(&mut self) {
        let k = &mut self.keylock;
        let n = k.tee_count;
        if n == 0 {
            return;
        }
        if let Some(stretch) = k.stretch.as_mut() {
            stretch.process(&[&k.tee_in[0][..n], &k.tee_in[1][..n]]);
        }
        k.tee_count = 0;
    }

    /// Tops the stretcher input up to what it needs and flushes it. False when no source frame is
    /// left to feed.
    fn feed_stretch(&mut self, sh: &Shared) -> bool {
        let need = match self.keylock.stretch.as_ref() {
            Some(stretch) => stretch.samples_required().max(1),
            None => return false,
        };
        while self.keylock.tee_count < need {
            let Some((index, frame)) = self.next_source_frame(sh) else {
                break;
            };
            self.tee_frame(frame, index);
        }
        if self.keylock.tee_count == 0 {
            return false;
        }
        self.flush_tee();
        true
    }

    /// Copies up to `left.len()` stretcher frames into `left` and `right`. Returns the count.
    fn drain_stretch(&mut self, left: &mut [f32], right: &mut [f32]) -> usize {
        let Some(stretch) = self.keylock.stretch.as_mut() else {
            return 0;
        };
        let n = stretch.available().min(left.len()).min(right.len());
        stretch.retrieve(&mut [&mut left[..n], &mut right[..n]])
    }

    /// Retrieves up to `n` stretcher frames into the discard sink.
    fn retrieve_to_sink(&mut self, n: usize) -> usize {
        let k = &mut self.keylock;
        let [left, right] = &mut k.sink;
        let n = n.min(left.len()).min(right.len());
        match k.stretch.as_mut() {
            Some(stretch) => stretch.retrieve(&mut [&mut left[..n], &mut right[..n]]),
            None => 0,
        }
    }

    fn stretch_available(&self) -> usize {
        self.keylock
            .stretch
            .as_ref()
            .map_or(0, |stretch| stretch.available())
    }

    /// Output frames between the newest source frame fed and what is audible now.
    fn stretch_latency_frames(&self) -> i64 {
        let Some(stretch) = self.keylock.stretch.as_ref() else {
            return 0;
        };
        let queued = (stretch.start_delay() + stretch.available()) as f64;
        (queued / self.keylock.stretch_ratio).round() as i64
    }

    /// Whether the key-lock path is on or priming, which the render dispatch needs to know to
    /// leave it.
    pub(super) fn keylock_engaged(&self) -> bool {
        self.keylock.mode != Mode::Off
    }

    /// Next source frame for the interpolator: replayed lookahead first, then the ring or a loop
    /// slot. While priming, frames pulled from the source are also teed into the stretcher.
    pub(super) fn next_source_frame(&mut self, sh: &Shared) -> Option<(i64, [f32; 2])> {
        if let Some((frame, index)) = self.keylock.lookahead.pop() {
            return Some((index, frame));
        }
        let (index, frame) = self.pull_frame(sh)?;
        if self.keylock.mode == Mode::Priming {
            self.tee_frame(frame, index);
        }
        Some((index, frame))
    }
}

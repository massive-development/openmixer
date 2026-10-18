//! Audio-thread side of a deck: services the seek handshake, pulls frames from the ring and
//! renders them with linear interpolation at `rate * file_rate / device_rate`. Allocation-free
//! and lock-free. Loop slots and key-lock join this path in later tasks.

use super::shared::Shared;
use om_core::SpscConsumer;
use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};

pub(super) struct Playback {
    ring: SpscConsumer<f32>,
    primed: bool,
    have_first: bool,
    phase: f64,
    prev: [f32; 2],
    next: [f32; 2],
    prev_idx: i64,
    next_idx: i64,
    /// Next source frame the interpolator will pull.
    read_idx: i64,
    /// Source frame at the ring's read head.
    ring_idx: i64,
    ring_wanted: i64,
    last_control_ack: u32,
    /// Floats consumed from the ring so far (mirrors `Shared::read_samples`).
    read_samples: u64,
}

impl Playback {
    pub(super) fn new(ring: SpscConsumer<f32>) -> Self {
        Self {
            ring,
            primed: false,
            have_first: false,
            phase: 0.0,
            prev: [0.0; 2],
            next: [0.0; 2],
            prev_idx: 0,
            next_idx: 0,
            read_idx: 0,
            ring_idx: 0,
            ring_wanted: 0,
            last_control_ack: 0,
            read_samples: 0,
        }
    }

    /// Renders one block. Both slices have the same length. Silence unless playing and loaded.
    pub(super) fn render(&mut self, sh: &Shared, left: &mut [f32], right: &mut [f32]) {
        let running =
            self.service_handshake(sh) && sh.playing.load(Relaxed) && sh.loaded.load(Relaxed);
        if !running {
            clear_tail(left, right, 0);
            return;
        }
        self.render_interpolated(sh, left, right);
    }

    fn service_handshake(&mut self, sh: &Shared) -> bool {
        let req = sh.request_epoch.load(Acquire);
        let ack = sh.ack_epoch.load(Acquire);
        if ack != req {
            return false;
        }

        let seq = sh.ack_seq.load(Acquire);
        if sh.drained_seq.load(Relaxed) != seq {
            // Everything buffered before the ack is stale.
            let stale = self.ring.readable_count();
            let skipped = self.ring.skip(stale);
            self.consume(sh, skipped);
            self.ring_idx = sh.ack_target.load(Relaxed);
            sh.drained_seq.store(seq, Release);
        }
        if ack != self.last_control_ack {
            self.reset_playhead(sh, ack);
        }
        true
    }

    fn reset_playhead(&mut self, sh: &Shared, ack: u32) {
        self.last_control_ack = ack;
        self.read_idx = self.ring_idx;
        self.ring_wanted = self.ring_idx;
        self.prev_idx = self.ring_idx;
        self.next_idx = self.ring_idx;
        self.primed = false;
        self.have_first = false;
        self.phase = 0.0;
        sh.position_frames.store(self.ring_idx, Relaxed);
    }

    fn consume(&mut self, sh: &Shared, samples: usize) {
        self.read_samples += samples as u64;
        sh.read_samples.store(self.read_samples, Relaxed);
    }

    fn reached_end(&self, sh: &Shared) -> bool {
        // Re-check readability after seeing end-of-stream: the decoder's last write may race.
        if self.ring_idx == self.read_idx
            && sh.end_of_stream.load(Acquire)
            && self.ring.readable_count() < 2
        {
            sh.playing.store(false, Relaxed);
            sh.finished.store(true, Relaxed);
            return true;
        }
        false
    }

    fn note_starved(&self, sh: &Shared) {
        if !self.reached_end(sh) {
            sh.underruns.fetch_add(1, Relaxed);
        }
    }

    /// Asks the decoder to reposition the ring. Ignored while a request for the same target is
    /// already outstanding.
    fn request_ring(&mut self, sh: &Shared, target: i64) {
        if self.ring_wanted == target {
            return;
        }
        self.ring_wanted = target;
        sh.ring_req_target.store(target, Relaxed);
        sh.ring_req_base.store(self.last_control_ack, Relaxed);
        sh.ring_req_epoch.fetch_add(1, Release);
    }

    fn read_ring_frame(&mut self, sh: &Shared, out: &mut [f32; 2]) -> bool {
        if self.ring_idx != self.read_idx {
            self.request_ring(sh, self.read_idx);
            return false;
        }
        if self.ring.read(out) < 2 {
            return false;
        }
        self.consume(sh, 2);
        self.ring_idx += 1;
        // Reading moves the ring: a later request to an old target is new again.
        self.ring_wanted = self.ring_idx;
        true
    }

    /// Pulls the next source frame and its index. Returns false when the ring is empty.
    fn pull_frame(&mut self, sh: &Shared, out: &mut [f32; 2]) -> Option<i64> {
        if !self.read_ring_frame(sh, out) {
            return None;
        }
        let index = self.read_idx;
        self.read_idx += 1;
        Some(index)
    }

    fn pop_frame(&mut self, sh: &Shared) -> bool {
        let mut frame = [0.0_f32; 2];
        let Some(index) = self.pull_frame(sh, &mut frame) else {
            return false;
        };
        self.prev = self.next;
        self.next = frame;
        self.prev_idx = self.next_idx;
        self.next_idx = index;
        true
    }

    fn prime_interpolator(&mut self, sh: &Shared) -> bool {
        if !self.have_first {
            let mut frame = [0.0_f32; 2];
            let Some(index) = self.pull_frame(sh, &mut frame) else {
                return false;
            };
            self.next = frame;
            self.next_idx = index;
            self.have_first = true;
        }
        if !self.pop_frame(sh) {
            return false;
        }
        self.primed = true;
        self.phase = 0.0;
        true
    }

    fn render_interpolated(&mut self, sh: &Shared, left: &mut [f32], right: &mut [f32]) {
        if !self.primed && !self.prime_interpolator(sh) {
            self.note_starved(sh);
            clear_tail(left, right, 0);
            return;
        }

        let ratio = sh.rate.load() * (sh.file_sample_rate.load() / sh.device_sample_rate.load());
        for i in 0..left.len() {
            while self.phase >= 1.0 {
                if !self.pop_frame(sh) {
                    self.note_starved(sh);
                    clear_tail(left, right, i);
                    sh.position_frames.store(self.prev_idx, Relaxed);
                    return;
                }
                self.phase -= 1.0;
            }
            let t = self.phase as f32;
            left[i] = self.prev[0] + (self.next[0] - self.prev[0]) * t;
            right[i] = self.prev[1] + (self.next[1] - self.prev[1]) * t;
            self.phase += ratio;
        }
        sh.position_frames.store(self.prev_idx, Relaxed);
    }
}

/// Zeroes `left` and `right` from index `from` onwards.
fn clear_tail(left: &mut [f32], right: &mut [f32], from: usize) {
    for s in left.iter_mut().skip(from) {
        *s = 0.0;
    }
    for s in right.iter_mut().skip(from) {
        *s = 0.0;
    }
}

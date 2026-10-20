//! Audio-thread side of a deck: services the seek handshake, pulls frames from the ring and
//! renders them with linear interpolation at `rate * file_rate / device_rate`, or through
//! the key-lock stretcher when that is on (`keylock.rs`). Allocation-free and lock-free.

use super::loop_state::LoopSnapshot;
use super::prefetch::PrefetchSlot;
use super::shared::Shared;
use keylock::KeylockState;
use om_dsp::{StemMixer, StemMixerControl, STEM_FRAME_FLOATS};
use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};
use std::sync::Arc;

mod keylock;

pub(super) struct Playback {
    ring: rtrb::Consumer<f32>,
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
    /// Loop bounds as of the current block.
    loop_snap: LoopSnapshot,
    /// Prefetch slot the playhead reads from while inside a loop, held until released.
    slot: Option<usize>,
    /// Load id of the track being played; prefetch slots from other tracks are unusable.
    playing_load_id: u32,
    keylock: KeylockState,
    /// Floats per ring frame, adopted from the decoder's ack: 2 in plain mode, 8 in stem mode.
    stride: usize,
    /// Folds eight-float stem frames into stereo at pull time, ahead of the interpolator and the
    /// key-lock stretcher, so a stem deck still has one of each.
    mixer: StemMixer,
}

impl Playback {
    pub(super) fn new(ring: rtrb::Consumer<f32>, stems: Arc<StemMixerControl>) -> Self {
        Self {
            stride: 2,
            mixer: StemMixer::new(stems),
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
            loop_snap: LoopSnapshot::default(),
            slot: None,
            playing_load_id: 0,
            keylock: KeylockState::new(),
        }
    }

    /// Renders one block. Both slices have the same length. Silence unless playing and loaded.
    pub(super) fn render(&mut self, sh: &Shared, left: &mut [f32], right: &mut [f32]) {
        let running = !sh.errored.load(Relaxed)
            && self.service_handshake(sh)
            && sh.playing.load(Relaxed)
            && sh.loaded.load(Relaxed);
        if !running {
            clear_tail(left, right, 0);
            return;
        }
        if let Some(snap) = sh.loop_state.load() {
            self.loop_snap = snap;
        }
        if self.stride > 2 {
            self.mixer.update_targets();
        }
        if sh.keylock.load(Relaxed) && self.keylock.stretch.is_some() {
            self.render_keylocked(sh, left, right);
            return;
        }
        if self.keylock_engaged() {
            self.leave_keylock();
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
            let stale = self.ring.slots();
            let skipped = discard(&mut self.ring, stale);
            self.consume(sh, skipped);
            self.ring_idx = sh.ack_target.load(Relaxed);
            self.stride = (sh.ack_stride.load(Relaxed) as usize).clamp(2, STEM_FRAME_FLOATS);
            sh.drained_seq.store(seq, Release);
        }
        if ack != self.last_control_ack {
            self.reset_playhead(sh, ack);
        }
        true
    }

    fn reset_playhead(&mut self, sh: &Shared, ack: u32) {
        self.last_control_ack = ack;
        self.playing_load_id = sh.ack_load_id.load(Relaxed);
        self.read_idx = self.ring_idx;
        self.ring_wanted = self.ring_idx;
        self.prev_idx = self.ring_idx;
        self.next_idx = self.ring_idx;
        self.primed = false;
        self.have_first = false;
        self.phase = 0.0;
        self.keylock.on_playhead_reset();
        self.release_slot(sh);
        if self.stride > 2 {
            // Stem mode: start from the control state, at the file's rate, with quiet filters.
            self.mixer.prepare(sh.file_sample_rate.load());
        }
        sh.position_frames.store(self.ring_idx, Relaxed);
    }

    fn consume(&mut self, sh: &Shared, samples: usize) {
        self.read_samples += samples as u64;
        sh.read_samples.store(self.read_samples, Relaxed);
    }

    fn reached_end(&self, sh: &Shared) -> bool {
        // Re-check readability after seeing end-of-stream: the decoder's last write may race.
        if self.slot.is_none()
            && self.ring_idx == self.read_idx
            && sh.end_of_stream.load(Acquire)
            && self.ring.slots() < self.stride
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

    fn release_slot(&mut self, sh: &Shared) {
        if self.slot.is_some() {
            sh.prefetch.release();
        }
        self.slot = None;
    }

    fn current_slot<'a>(&self, sh: &'a Shared) -> Option<&'a PrefetchSlot> {
        self.slot.map(|index| sh.prefetch.slot(index))
    }

    /// True when the held slot holds the playing track's loop start.
    fn slot_usable(&self, sh: &Shared) -> bool {
        self.current_slot(sh).is_some_and(|s| {
            s.load_id() == self.playing_load_id
                && s.start() == self.loop_snap.start
                && s.stride() == self.stride
        })
    }

    /// Switches to the prefetched loop start when the playhead wraps. Falls back to a ring
    /// reposition (not gapless) when no usable slot is published.
    fn enter_loop_slot(&mut self, sh: &Shared) {
        if !self.slot_usable(sh) {
            self.slot = sh.prefetch.acquire();
            if !self.slot_usable(sh) {
                self.release_slot(sh);
                self.request_ring(sh, self.read_idx);
                return;
            }
        }
        let Some(slot) = self.current_slot(sh) else {
            return;
        };
        if !slot.contains(self.read_idx) {
            self.release_slot(sh);
            self.request_ring(sh, self.read_idx);
            return;
        }
        let next_ring_target = slot.start() + slot.length();
        self.request_ring(sh, next_ring_target);
    }

    fn read_slot_frame(&mut self, sh: &Shared) -> Option<[f32; STEM_FRAME_FLOATS]> {
        let slot = self.current_slot(sh)?;
        if !slot.contains(self.read_idx) {
            self.release_slot(sh);
            return None;
        }
        Some(slot.frame(self.read_idx - slot.start()))
    }

    fn read_ring_frame(&mut self, sh: &Shared) -> Option<[f32; STEM_FRAME_FLOATS]> {
        if self.ring_idx != self.read_idx {
            self.request_ring(sh, self.read_idx);
            return None;
        }
        let mut out = [0.0_f32; STEM_FRAME_FLOATS];
        let (got, _) = self.ring.pop_partial_slice(&mut out[..self.stride]);
        if got.len() < self.stride {
            return None;
        }
        self.consume(sh, self.stride);
        self.ring_idx += 1;
        // Reading moves the ring: a later request to an old target is new again.
        self.ring_wanted = self.ring_idx;
        Some(out)
    }

    /// Pulls the next source frame and its index. None when neither the slot nor the ring has it.
    fn pull_frame(&mut self, sh: &Shared) -> Option<(i64, [f32; 2])> {
        let loop_snap = self.loop_snap;
        if loop_snap.active() && self.read_idx >= loop_snap.end && self.read_idx > loop_snap.start {
            // Modulo keeps beat phase when a loop shrinks under the playhead.
            let len = loop_snap.end - loop_snap.start;
            self.read_idx = loop_snap.start + (self.read_idx - loop_snap.start) % len;
            self.enter_loop_slot(sh);
        }
        let raw = self
            .read_slot_frame(sh)
            .or_else(|| self.read_ring_frame(sh))?;
        let frame = if self.stride > 2 {
            self.mixer.mix_frame(&raw)
        } else {
            [raw[0], raw[1]]
        };
        let index = self.read_idx;
        self.read_idx += 1;
        Some((index, frame))
    }

    fn pop_frame(&mut self, sh: &Shared) -> bool {
        let Some((index, frame)) = self.next_source_frame(sh) else {
            return false;
        };
        self.keylock.priming_pops += 1;
        self.prev = self.next;
        self.next = frame;
        self.prev_idx = self.next_idx;
        self.next_idx = index;
        true
    }

    fn prime_interpolator(&mut self, sh: &Shared) -> bool {
        if !self.have_first {
            let Some((index, frame)) = self.next_source_frame(sh) else {
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
/// Discards up to `n` buffered items from the decode ring; returns how many were discarded.
fn discard(ring: &mut rtrb::Consumer<f32>, n: usize) -> usize {
    let count = n.min(ring.slots());
    if count > 0 {
        if let Ok(chunk) = ring.read_chunk(count) {
            chunk.commit_all();
        }
    }
    count
}

fn clear_tail(left: &mut [f32], right: &mut [f32], from: usize) {
    for s in left.iter_mut().skip(from) {
        *s = 0.0;
    }
    for s in right.iter_mut().skip(from) {
        *s = 0.0;
    }
}

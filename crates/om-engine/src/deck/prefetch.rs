//! Pre-decoded audio from a loop start, so the audio thread can jump there without waiting on the
//! decoder. The decoder thread fills and publishes; the audio thread acquires and releases through
//! a hazard-slot protocol (sequentially consistent).
//!
//! Frames are stored as `AtomicF32` cells so the protocol needs no `unsafe`; every access is a
//! plain load or store on the audio thread.

use om_core::atomic::AtomicF32;
use std::sync::atomic::{AtomicI32, Ordering::Relaxed};
use om_dsp::STEM_FRAME_FLOATS;
use std::sync::atomic::{AtomicI64, AtomicU32, Ordering::SeqCst};

/// Floats reserved per frame: a slot holds stereo frames (2 used) or stem frames (all 8).
const CELL_STRIDE: usize = STEM_FRAME_FLOATS;

/// One prefetched run of frames starting at source frame `start`. A frame is `stride` floats:
/// 2 for a plain file, 8 in stem mode (L/R of drums, bass, vocals, other).
pub struct PrefetchSlot {
    frames: Box<[AtomicF32]>,
    start: AtomicI64,
    length: AtomicI64,
    load_id: AtomicU32,
    stride: AtomicU32,
}

impl PrefetchSlot {
    fn new(frames_per_slot: usize) -> Self {
        Self {
            frames: (0..frames_per_slot * CELL_STRIDE)
                .map(|_| AtomicF32::new(0.0))
                .collect(),
            start: AtomicI64::new(0),
            length: AtomicI64::new(0),
            load_id: AtomicU32::new(0),
            stride: AtomicU32::new(2),
        }
    }

    /// Floats per frame the slot was filled with.
    pub fn stride(&self) -> usize {
        self.stride.load(Relaxed) as usize
    }

    pub fn start(&self) -> i64 {
        self.start.load(Relaxed)
    }

    pub fn length(&self) -> i64 {
        self.length.load(Relaxed)
    }

    pub fn load_id(&self) -> u32 {
        self.load_id.load(Relaxed)
    }

    /// True when `frame` (source index) falls inside the slot.
    pub fn contains(&self, frame: i64) -> bool {
        let start = self.start();
        frame >= start && frame < start + self.length()
    }

    /// Frame at `offset` (counted from the slot start): the first `stride` floats are filled,
    /// the rest are zero.
    pub fn frame(&self, offset: i64) -> [f32; CELL_STRIDE] {
        let stride = self.stride();
        let base = offset as usize * CELL_STRIDE;
        let mut out = [0.0_f32; CELL_STRIDE];
        for (cell, slot) in out.iter_mut().zip(&self.frames[base..base + stride]) {
            *cell = slot.load();
        }
        out
    }
}

pub struct LoopPrefetch {
    capacity: i64,
    slots: [PrefetchSlot; 2],
    /// Slot the audio thread may read, or -1.
    published: AtomicI32,
    /// Slot the audio thread is reading, or -1.
    in_use: AtomicI32,
}

impl LoopPrefetch {
    pub fn new(frames_per_slot: i64) -> Self {
        let capacity = frames_per_slot.max(0);
        let n = capacity as usize;
        Self {
            capacity,
            slots: [PrefetchSlot::new(n), PrefetchSlot::new(n)],
            published: AtomicI32::new(-1),
            in_use: AtomicI32::new(-1),
        }
    }

    pub fn capacity(&self) -> i64 {
        self.capacity
    }

    pub fn is_published(&self) -> bool {
        self.published.load(SeqCst) >= 0
    }

    /// Decoder thread: unpublishes, then returns the index of a slot the audio thread is not
    /// reading. The caller writes frames and metadata into it, then calls [`Self::publish`].
    pub fn begin_fill(&self) -> usize {
        self.published.store(-1, SeqCst);
        if self.in_use.load(SeqCst) == 0 {
            1
        } else {
            0
        }
    }

    pub fn slot(&self, index: usize) -> &PrefetchSlot {
        &self.slots[index]
    }

    /// Decoder thread: writes one frame (2 or 8 floats) into a slot being filled.
    pub fn write_frame(&self, index: usize, offset: usize, frame: &[f32]) {
        let frames = &self.slots[index].frames;
        for (cell, v) in frames[offset * CELL_STRIDE..].iter().zip(frame) {
            cell.store(*v);
        }
    }

    /// Decoder thread: records what the slot now holds.
    pub fn set_meta(&self, index: usize, start: i64, length: i64, load_id: u32, stride: usize) {
        let slot = &self.slots[index];
        slot.start.store(start, Relaxed);
        slot.length.store(length, Relaxed);
        slot.load_id.store(load_id, Relaxed);
        slot.stride.store(stride as u32, Relaxed);
    }

    pub fn publish(&self, index: usize) {
        self.published.store(index as i32, SeqCst);
    }

    /// Decoder thread: unpublishes without a replacement (on reader hand-over).
    pub fn withdraw(&self) {
        self.published.store(-1, SeqCst);
    }

    /// Audio thread: the published slot index, held until [`Self::release`] or the next acquire.
    pub fn acquire(&self) -> Option<usize> {
        let idx = self.published.load(SeqCst);
        if idx < 0 {
            self.in_use.store(-1, SeqCst);
            return None;
        }
        self.in_use.store(idx, SeqCst);
        if self.published.load(SeqCst) != idx {
            // The decoder withdrew it between the two loads.
            self.in_use.store(-1, SeqCst);
            return None;
        }
        Some(idx as usize)
    }

    pub fn release(&self) {
        self.in_use.store(-1, SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_publish_acquire_and_withdraw() {
        let pf = LoopPrefetch::new(4);
        assert!(!pf.is_published());
        assert_eq!(pf.acquire(), None);
        let idx = pf.begin_fill();
        pf.write_frame(idx, 0, &[0.5, -0.5]);
        pf.set_meta(idx, 10, 1, 7, 2);
        pf.publish(idx);
        assert!(pf.is_published());
        let got = pf.acquire().expect("published slot");
        assert_eq!(got, idx);
        assert!(pf.slot(got).contains(10));
        assert!(!pf.slot(got).contains(11));
        assert_eq!(pf.slot(got).frame(0), [0.5, -0.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        // The filler must pick the other slot while the reader holds this one.
        assert_ne!(pf.begin_fill(), got);
        pf.release();
        pf.withdraw();
        assert!(!pf.is_published());
        assert_eq!(pf.acquire(), None);
    }
}

//! Maps the engine's fixed outputs (0/1 master, 2/3 headphone cue) onto chosen stereo pairs of the
//! device. Pair `p` is device channels `2p` and `2p + 1`.
//!
//! Threads: [`OutputPairs`] setters are atomics for the control thread. [`OutputRouter::process`]
//! runs on the audio thread after `prepare`.

use crate::callback::AudioCallback;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;

/// Pair value meaning "no output".
pub const NO_PAIR: i32 = -1;
const ENGINE_CHANNELS: usize = 4;

#[derive(Debug)]
struct PairCells {
    master: AtomicI32,
    cue: AtomicI32,
}

/// Control-thread handle to the pair selection.
#[derive(Debug, Clone)]
pub struct OutputPairs {
    cells: Arc<PairCells>,
}

impl OutputPairs {
    pub fn set_master_pair(&self, pair: i32) {
        self.cells.master.store(pair, Ordering::Relaxed);
    }

    pub fn set_cue_pair(&self, pair: i32) {
        self.cells.cue.store(pair, Ordering::Relaxed);
    }

    pub fn master_pair(&self) -> i32 {
        self.cells.master.load(Ordering::Relaxed)
    }

    pub fn cue_pair(&self) -> i32 {
        self.cells.cue.load(Ordering::Relaxed)
    }
}

pub struct OutputRouter {
    cells: Arc<PairCells>,
    scratch: [Vec<f32>; ENGINE_CHANNELS],
}

impl OutputRouter {
    /// Master on pair 0 and cue on pair 1. Control thread.
    pub fn new() -> (Self, OutputPairs) {
        let cells = Arc::new(PairCells {
            master: AtomicI32::new(0),
            cue: AtomicI32::new(1),
        });
        let router = Self {
            cells: Arc::clone(&cells),
            scratch: std::array::from_fn(|_| Vec::new()),
        };
        (router, OutputPairs { cells })
    }

    /// Router that reads its pair selection from `pairs`, so a device can be reopened without losing
    /// the control handle's settings. Control thread.
    pub fn with_pairs(pairs: &OutputPairs) -> Self {
        Self {
            cells: Arc::clone(&pairs.cells),
            scratch: std::array::from_fn(|_| Vec::new()),
        }
    }

    /// Control thread. Sizes the engine scratch for `max_block_size` frames per chunk.
    pub fn prepare(&mut self, max_block_size: usize) {
        for buffer in &mut self.scratch {
            buffer.clear();
            buffer.resize(max_block_size.max(1), 0.0);
        }
    }

    /// Renders `source` in chunks and writes the master and cue pairs into `device_out`. Unrouted
    /// device channels are silenced. A mono device (fewer than two outputs) gets the source
    /// directly, with no pair mapping.
    pub fn process(
        &mut self,
        source: &mut dyn AudioCallback,
        device_out: &mut [&mut [f32]],
        num_samples: usize,
    ) {
        if device_out.len() < 2 {
            match device_out {
                [] => source.process(&[], &mut []),
                [mono, ..] => source.process(&[], &mut [&mut mono[..num_samples]]),
            }
            return;
        }
        let chunk = self.scratch[0].len();
        if chunk == 0 {
            // prepare() not called yet: silence rather than loop without progress.
            for channel in device_out.iter_mut() {
                channel[..num_samples].fill(0.0);
            }
            return;
        }
        // One read per block so the pair cannot change between chunks.
        let master = self.cells.master.load(Ordering::Relaxed);
        let cue = self.cells.cue.load(Ordering::Relaxed);
        let mut done = 0;
        while done < num_samples {
            let n = chunk.min(num_samples - done);
            {
                let [a, b, c, d] = &mut self.scratch;
                let mut engine: [&mut [f32]; ENGINE_CHANNELS] =
                    [&mut a[..n], &mut b[..n], &mut c[..n], &mut d[..n]];
                source.process(&[], &mut engine);
            }
            route(device_out, &self.scratch, done, n, master, cue);
            done += n;
        }
    }
}

fn route(
    device_out: &mut [&mut [f32]],
    scratch: &[Vec<f32>; ENGINE_CHANNELS],
    offset: usize,
    n: usize,
    master: i32,
    cue: i32,
) {
    for channel in device_out.iter_mut() {
        channel[offset..offset + n].fill(0.0);
    }
    copy_pair(device_out, scratch, master, 0, offset, n);
    if cue != master {
        copy_pair(device_out, scratch, cue, 2, offset, n);
    }
}

/// Copies engine channels `first` and `first + 1` onto device pair `pair`. An out-of-range pair is
/// ignored so a stale preference cannot write past the device.
fn copy_pair(
    device_out: &mut [&mut [f32]],
    scratch: &[Vec<f32>; ENGINE_CHANNELS],
    pair: i32,
    first: usize,
    offset: usize,
    n: usize,
) {
    let Ok(pair) = usize::try_from(pair) else {
        return;
    };
    if pair * 2 + 1 >= device_out.len() {
        return;
    }
    for side in 0..2 {
        device_out[pair * 2 + side][offset..offset + n]
            .copy_from_slice(&scratch[first + side][..n]);
    }
}

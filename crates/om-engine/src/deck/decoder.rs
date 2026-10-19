//! The decoder worker: owns the file reader and the producer half of the frame ring. It services
//! seek and ring-reposition requests from the audio thread's handshake and keeps the ring topped
//! up in chunks. Never runs on the audio thread.

use super::shared::Shared;
use super::source::SourceReader;
use om_core::SpscProducer;
use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};
use std::sync::Arc;
use std::time::Duration;

/// Frames decoded per ring write.
pub(super) const CHUNK_FRAMES: usize = 2048;
const IDLE_SLEEP: Duration = Duration::from_millis(3);

pub(super) struct DecoderWorker {
    shared: Arc<Shared>,
    ring: SpscProducer<f32>,
    reader: Option<SourceReader>,
    read_pos: i64,
    local_ack: u32,
    local_seq: u32,
    local_ring_req: u32,
    local_written: u64,
    reader_load_id: u32,
    /// Loop start the prefetch slot currently holds, or -1.
    prefetched_start: i64,
    chunk: Vec<[f32; 2]>,
}

impl DecoderWorker {
    pub(super) fn new(shared: Arc<Shared>, ring: SpscProducer<f32>) -> Self {
        Self {
            shared,
            ring,
            reader: None,
            read_pos: 0,
            local_ack: 0,
            local_seq: 0,
            local_ring_req: 0,
            local_written: 0,
            reader_load_id: 0,
            prefetched_start: -1,
            chunk: vec![[0.0; 2]; CHUNK_FRAMES],
        }
    }

    pub(super) fn run(mut self) {
        while !self.shared.stop.load(Acquire) {
            // Epoch first, reader second: a reader is published before the epoch bump naming it.
            let epoch = self.shared.request_epoch.load(Acquire);
            self.take_pending_reader();
            if epoch != self.local_ack {
                self.sync_to(epoch);
            } else {
                self.service_ring_request();
            }
            self.service_prefetch();

            if self.can_produce() {
                self.produce_chunk();
            } else {
                std::thread::sleep(IDLE_SLEEP);
            }
        }
    }

    fn take_pending_reader(&mut self) {
        let Ok(mut pending) = self.shared.pending.lock() else {
            return;
        };
        if pending.has {
            self.reader = pending.reader.take();
            self.reader_load_id = pending.load_id;
            pending.has = false;
            self.shared.prefetch.withdraw();
            self.prefetched_start = -1;
        }
    }

    fn length(&self) -> i64 {
        self.reader.as_ref().map_or(0, |r| r.info().length_frames)
    }

    /// Publishes a repositioned read head. `ack_seq` is bumped last-but-one so the audio thread
    /// sees a consistent target when it observes the new sequence number.
    fn publish_ack(&mut self, target: i64) {
        let len = self.length();
        self.read_pos = target.clamp(0, len);
        self.shared.ack_target.store(self.read_pos, Relaxed);
        self.shared
            .end_of_stream
            .store(self.reader.is_none() || self.read_pos >= len, Relaxed);
        self.local_seq = self.local_seq.wrapping_add(1);
        self.shared.ack_seq.store(self.local_seq, Release);
    }

    fn sync_to(&mut self, epoch: u32) {
        let target = self.shared.seek_target.load(Relaxed);
        self.publish_ack(target);
        self.shared.ack_load_id.store(self.reader_load_id, Relaxed);
        self.local_ack = epoch;
        // After ack_seq: the audio thread reads the epoch first.
        self.shared.ack_epoch.store(epoch, Release);
    }

    /// Fills a prefetch slot when the loop start changes. Loops that are not set, or are invalid,
    /// leave the slot alone.
    fn service_prefetch(&mut self) {
        if self.reader.is_none() {
            return;
        }
        let Some(loop_snap) = self.shared.loop_state.load() else {
            return;
        };
        if !loop_snap.valid() || loop_snap.start == self.prefetched_start {
            return;
        }
        self.fill_prefetch(loop_snap.start);
    }

    fn fill_prefetch(&mut self, start: i64) {
        let prefetch = &self.shared.prefetch;
        let index = prefetch.begin_fill();
        let len = (self.length() - start).clamp(0, prefetch.capacity());
        let mut done = 0;
        while done < len {
            let n = (len - done).min(CHUNK_FRAMES as i64) as usize;
            if let Some(reader) = self.reader.as_mut() {
                reader.read_stereo(start + done, &mut self.chunk[..n]);
            }
            for (i, frame) in self.chunk[..n].iter().enumerate() {
                prefetch.write_frame(index, done as usize + i, *frame);
            }
            done += n as i64;
        }
        prefetch.set_meta(index, start, len, self.reader_load_id);
        prefetch.publish(index);
        self.prefetched_start = start;
    }

    fn service_ring_request(&mut self) {
        let req = self.shared.ring_req_epoch.load(Acquire);
        if req == self.local_ring_req || self.shared.drained_seq.load(Acquire) != self.local_seq {
            return;
        }
        self.local_ring_req = req;
        // A request whose base predates the latest control seek is stale and dropped.
        if self.shared.ring_req_base.load(Relaxed) == self.local_ack {
            let target = self.shared.ring_req_target.load(Relaxed);
            self.publish_ack(target);
        }
    }

    /// Produce only when the latest control seek or load is acknowledged. The epoch is re-read
    /// here: a reader can be taken in the same pass that the epoch naming it is bumped, and
    /// producing from that reader before its ack would hand the audio thread stale frames.
    fn can_produce(&self) -> bool {
        self.reader.is_some()
            && self.local_ack == self.shared.request_epoch.load(Acquire)
            && self.shared.drained_seq.load(Acquire) == self.local_seq
            && self.read_pos < self.length()
            && self.ring.writable_count() >= CHUNK_FRAMES * 2
    }

    fn produce_chunk(&mut self) {
        #[cfg(test)]
        if self.shared.inject_decoder_panic.load(Relaxed) {
            panic!("injected decoder fault");
        }
        let len = self.length();
        let n = (len - self.read_pos).min(CHUNK_FRAMES as i64) as usize;
        let Some(reader) = self.reader.as_mut() else {
            return;
        };
        reader.read_stereo(self.read_pos, &mut self.chunk[..n]);
        let written = self.ring.write(self.chunk[..n].as_flattened());
        self.local_written += written as u64;
        self.shared
            .written_samples
            .store(self.local_written, Relaxed);
        self.read_pos += n as i64;
        if self.read_pos >= len {
            self.shared.end_of_stream.store(true, Release);
        }
    }
}

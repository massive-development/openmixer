//! The decoder worker: owns the file reader and the producer half of the frame ring. It services
//! seek and ring-reposition requests from the audio thread's handshake and keeps the ring topped
//! up in chunks. Never runs on the audio thread.

use super::shared::Shared;
use super::source::SourceReader;
use super::stems::StemReaders;
use om_dsp::STEM_FRAME_FLOATS;
use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};
use std::sync::Arc;
use std::time::Duration;

/// Frames decoded per ring write.
pub(super) const CHUNK_FRAMES: usize = 2048;
const IDLE_SLEEP: Duration = Duration::from_millis(3);

pub(super) struct DecoderWorker {
    shared: Arc<Shared>,
    ring: rtrb::Producer<f32>,
    /// The main file: metadata, length, and the frames in plain mode.
    reader: Option<SourceReader>,
    /// Stem files, read in lockstep instead of `reader` while present.
    stems: Option<StemReaders>,
    /// Floats per ring frame: 2 in plain mode, 8 in stem mode.
    stride: usize,
    /// Epoch a hand-over must be acknowledged at before anything is produced from its sources.
    hold_epoch: Option<u32>,
    read_pos: i64,
    local_ack: u32,
    local_seq: u32,
    local_ring_req: u32,
    local_written: u64,
    reader_load_id: u32,
    /// Loop start the prefetch slot currently holds, or -1.
    prefetched_start: i64,
    chunk: Vec<[f32; 2]>,
    stem_chunk: Vec<[f32; STEM_FRAME_FLOATS]>,
    /// Test-only hook run on the decoder thread before each chunk. A panic in it takes the same
    /// fault path as a decode failure, which is what the fault test exercises.
    #[cfg(test)]
    pub(super) before_chunk: Option<Box<dyn FnMut() + Send>>,
}

impl DecoderWorker {
    pub(super) fn new(shared: Arc<Shared>, ring: rtrb::Producer<f32>) -> Self {
        Self {
            shared,
            ring,
            reader: None,
            stems: None,
            stride: 2,
            hold_epoch: None,
            read_pos: 0,
            local_ack: 0,
            local_seq: 0,
            local_ring_req: 0,
            local_written: 0,
            reader_load_id: 0,
            prefetched_start: -1,
            chunk: vec![[0.0; 2]; CHUNK_FRAMES],
            stem_chunk: vec![[0.0; STEM_FRAME_FLOATS]; CHUNK_FRAMES],
            #[cfg(test)]
            before_chunk: None,
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
        let changed = pending.has || pending.has_stems;
        if pending.has {
            self.reader = pending.reader.take();
            self.reader_load_id = pending.load_id;
            pending.has = false;
        }
        if pending.has_stems {
            self.stems = pending.stems.take();
            self.stride = if self.stems.is_some() {
                STEM_FRAME_FLOATS
            } else {
                2
            };
            pending.has_stems = false;
        }
        if changed {
            self.reader_load_id = pending.load_id;
            self.hold_epoch = Some(pending.min_epoch);
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
        self.shared.ack_stride.store(self.stride as u32, Relaxed);
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
        if self
            .hold_epoch
            .is_some_and(|h| epoch.wrapping_sub(h) < u32::MAX / 2)
        {
            self.hold_epoch = None;
        }
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
            if let Some(stems) = self.stems.as_mut() {
                stems.read_frames(start + done, &mut self.stem_chunk[..n]);
                for (i, frame) in self.stem_chunk[..n].iter().enumerate() {
                    prefetch.write_frame(index, done as usize + i, frame);
                }
            } else {
                if let Some(reader) = self.reader.as_mut() {
                    reader.read_stereo(start + done, &mut self.chunk[..n]);
                }
                for (i, frame) in self.chunk[..n].iter().enumerate() {
                    prefetch.write_frame(index, done as usize + i, frame);
                }
            }
            done += n as i64;
        }
        prefetch.set_meta(index, start, len, self.reader_load_id, self.stride);
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
        // The ring holds `capacity` floats. Plain mode keeps the depth the ring always had
        // (`capacity / 4` floats, two per frame); stem mode buffers the same number of frames
        // at eight floats each, so `capacity * stride / 8` floats in either mode.
        let capacity = self.ring.buffer().capacity();
        let free = self.ring.slots();
        let need = CHUNK_FRAMES * self.stride;
        self.reader.is_some()
            && self.hold_epoch.is_none()
            && self.local_ack == self.shared.request_epoch.load(Acquire)
            && self.shared.drained_seq.load(Acquire) == self.local_seq
            && self.read_pos < self.length()
            && free >= need
            && capacity - free + need <= capacity * self.stride / STEM_FRAME_FLOATS
    }

    fn produce_chunk(&mut self) {
        #[cfg(test)]
        if let Some(hook) = self.before_chunk.as_mut() {
            hook();
        }
        let len = self.length();
        let n = (len - self.read_pos).min(CHUNK_FRAMES as i64) as usize;
        let written = if let Some(stems) = self.stems.as_mut() {
            stems.read_frames(self.read_pos, &mut self.stem_chunk[..n]);
            let (written, _) = self
                .ring
                .push_partial_slice(self.stem_chunk[..n].as_flattened());
            written.len()
        } else {
            let Some(reader) = self.reader.as_mut() else {
                return;
            };
            reader.read_stereo(self.read_pos, &mut self.chunk[..n]);
            let (written, _) = self.ring.push_partial_slice(self.chunk[..n].as_flattened());
            written.len()
        };
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

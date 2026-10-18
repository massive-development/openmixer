//! State shared between control threads, the decoder thread and the audio thread. Everything the
//! audio thread reads is an atomic; the only lock is the control-to-decoder reader hand-over,
//! which the audio thread never takes.

use super::hotcues::HotcueBank;
use super::source::SourceReader;
use crate::fx::math::AtomicF64;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, AtomicU64};
use std::sync::Mutex;

/// Control-to-decoder hand-over. `has` distinguishes "no change" from "unload" (`reader: None`).
#[derive(Default)]
pub(super) struct Pending {
    pub has: bool,
    pub reader: Option<SourceReader>,
    pub load_id: u32,
    pub load_counter: u32,
}

pub(super) struct Shared {
    pub playing: AtomicBool,
    pub loaded: AtomicBool,
    pub finished: AtomicBool,
    pub end_of_stream: AtomicBool,
    pub stop: AtomicBool,

    pub rate: AtomicF64,
    pub device_sample_rate: AtomicF64,
    pub file_sample_rate: AtomicF64,
    pub length_frames: AtomicI64,
    pub position_frames: AtomicI64,
    pub underruns: AtomicU32,

    /// Control seek handshake: `request_epoch` is bumped by control, `ack_epoch` by the decoder
    /// after it publishes `ack_target` (and bumps `ack_seq`). The audio thread drains on `ack_seq`.
    pub request_epoch: AtomicU32,
    pub ack_epoch: AtomicU32,
    pub ack_seq: AtomicU32,
    pub drained_seq: AtomicU32,
    pub seek_target: AtomicI64,
    pub ack_target: AtomicI64,

    /// Audio-thread ring repositions (loops, later): based on a control ack so stale ones drop.
    pub ring_req_epoch: AtomicU32,
    pub ring_req_base: AtomicU32,
    pub ring_req_target: AtomicI64,

    /// Floats written to / read from the ring. Their difference is the buffered sample count and
    /// is readable from any thread without touching the consumer.
    pub written_samples: AtomicU64,
    pub read_samples: AtomicU64,

    pub pending: Mutex<Pending>,
    pub hotcues: HotcueBank,
}

impl Shared {
    /// Initial state of an unloaded deck. Device and file rates match the C++ defaults (44.1 kHz).
    pub fn new() -> Self {
        Self {
            playing: AtomicBool::new(false),
            loaded: AtomicBool::new(false),
            finished: AtomicBool::new(false),
            end_of_stream: AtomicBool::new(false),
            stop: AtomicBool::new(false),
            rate: AtomicF64::new(1.0),
            device_sample_rate: AtomicF64::new(44_100.0),
            file_sample_rate: AtomicF64::new(44_100.0),
            length_frames: AtomicI64::new(0),
            position_frames: AtomicI64::new(0),
            underruns: AtomicU32::new(0),
            request_epoch: AtomicU32::new(0),
            ack_epoch: AtomicU32::new(0),
            ack_seq: AtomicU32::new(0),
            drained_seq: AtomicU32::new(0),
            seek_target: AtomicI64::new(0),
            ack_target: AtomicI64::new(0),
            ring_req_epoch: AtomicU32::new(0),
            ring_req_base: AtomicU32::new(0),
            ring_req_target: AtomicI64::new(0),
            written_samples: AtomicU64::new(0),
            read_samples: AtomicU64::new(0),
            pending: Mutex::new(Pending::default()),
            hotcues: HotcueBank::default(),
        }
    }
}

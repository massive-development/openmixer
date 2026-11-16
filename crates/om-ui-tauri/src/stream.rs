//! The snapshot stream: packs `DjEngine::snapshot()` into a packet at a fixed rate and hands it to a
//! sink. Runs on its own control-side thread (`om-ui-stream`), never the audio thread.

use crate::pacer::Pacer;
use om_engine::DjEngine;
use om_ui_bridge::pack_snapshot;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Instant;

/// Name of the stream thread.
pub const STREAM_THREAD: &str = "om-ui-stream";

/// Where packets go. `send` returns `false` once the receiver is gone, which ends the stream.
pub trait PacketSink: Send {
    fn send(&mut self, bytes: &[u8]) -> bool;
}

/// How often the stream sends.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StreamConfig {
    pub hz: f64,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self { hz: 30.0 }
    }
}

/// Sends one packet per period until `stop` is set or the sink fails. One buffer is reused for
/// every packet.
pub fn run_stream(
    engine: Arc<DjEngine>,
    mut sink: impl PacketSink,
    config: StreamConfig,
    stop: Arc<AtomicBool>,
) {
    let mut packet = Vec::new();
    let mut pacer = Pacer::new(config.hz);
    while !stop.load(Ordering::Acquire) {
        fill_packet(&engine, &mut packet);
        if !sink.send(&packet) {
            tracing::debug!("snapshot stream: the receiver is gone");
            return;
        }
        pacer.wait();
    }
}

/// Starts [`run_stream`] on a thread named [`STREAM_THREAD`].
pub fn spawn_stream(
    engine: Arc<DjEngine>,
    sink: impl PacketSink + 'static,
    config: StreamConfig,
    stop: Arc<AtomicBool>,
) -> std::io::Result<JoinHandle<()>> {
    std::thread::Builder::new()
        .name(STREAM_THREAD.into())
        .spawn(move || run_stream(engine, sink, config, stop))
}

fn fill_packet(engine: &DjEngine, packet: &mut Vec<u8>) {
    let pulled = Instant::now();
    let snapshot = engine.snapshot();
    let age_ms = pulled.elapsed().as_secs_f32() * 1000.0;
    pack_snapshot(&snapshot, age_ms, packet);
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use om_testing::wait::wait_until;
    use om_ui_bridge::{packet_len, PACKET_MAGIC};
    use std::sync::Mutex;
    use std::time::Duration;

    pub(crate) type Seen = Arc<Mutex<Vec<Vec<u8>>>>;

    pub(crate) struct RecordingSink(Seen);

    impl PacketSink for RecordingSink {
        fn send(&mut self, bytes: &[u8]) -> bool {
            self.0.lock().unwrap().push(bytes.to_vec());
            true
        }
    }

    pub(crate) fn recording_sink() -> (RecordingSink, Seen) {
        let seen = Seen::default();
        (RecordingSink(Arc::clone(&seen)), seen)
    }

    struct FailingSink;

    impl PacketSink for FailingSink {
        fn send(&mut self, _: &[u8]) -> bool {
            false
        }
    }

    pub(crate) fn test_engine() -> Arc<DjEngine> {
        Arc::new(DjEngine::new(2).0)
    }

    fn spawn(sink: impl PacketSink + 'static, hz: f64, stop: &Arc<AtomicBool>) -> JoinHandle<()> {
        spawn_stream(test_engine(), sink, StreamConfig { hz }, Arc::clone(stop)).unwrap()
    }

    #[test]
    fn stream_sends_valid_packets() {
        let (sink, seen) = recording_sink();
        let stop = Arc::new(AtomicBool::new(false));
        let handle = spawn(sink, 200.0, &stop);
        let enough = || seen.lock().unwrap().len() >= 3;
        assert!(wait_until(
            Duration::from_secs(5),
            Duration::from_millis(2),
            enough
        ));
        stop.store(true, Ordering::Relaxed);
        handle.join().unwrap();
        let first = &seen.lock().unwrap()[0];
        assert_eq!(first.len(), packet_len(2, 2));
        assert_eq!(
            u32::from_le_bytes(first[0..4].try_into().unwrap()),
            PACKET_MAGIC
        );
    }

    #[test]
    fn stream_stops_when_sink_fails() {
        let stop = Arc::new(AtomicBool::new(false));
        spawn(FailingSink, StreamConfig::default().hz, &stop)
            .join()
            .unwrap();
    }

    #[test]
    fn stream_rate_is_respected() {
        let (sink, seen) = recording_sink();
        let stop = Arc::new(AtomicBool::new(false));
        let handle = spawn(sink, 30.0, &stop);
        std::thread::sleep(Duration::from_millis(500));
        stop.store(true, Ordering::Relaxed);
        handle.join().unwrap();
        let n = seen.lock().unwrap().len();
        assert!((12..=18).contains(&n), "{n} packets in 500 ms at 30 Hz");
    }

    #[test]
    fn the_stream_thread_is_named() {
        let stop = Arc::new(AtomicBool::new(true));
        let handle = spawn(FailingSink, 30.0, &stop);
        assert_eq!(handle.thread().name(), Some(STREAM_THREAD));
        handle.join().unwrap();
    }
}

//! The one running snapshot stream of the window. `start_stream` replaces it; closing the window
//! stops it for good. Called from Tauri's IPC threads and from `run` after the window closes.

use crate::lock;
use crate::stream::{spawn_stream, PacketSink, StreamConfig};
use om_engine::DjEngine;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

/// Holds the running stream, if any. The stream thread never takes this lock, so joining it
/// while the lock is held cannot deadlock.
#[derive(Default)]
pub(crate) struct StreamSlot {
    state: Mutex<SlotState>,
}

#[derive(Default)]
struct SlotState {
    running: Option<Running>,
    closed: bool,
}

struct Running {
    stop: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

impl StreamSlot {
    /// Stops the running stream, then starts one into `sink`. Does nothing once closed.
    pub(crate) fn start(
        &self,
        engine: Arc<DjEngine>,
        sink: impl PacketSink + 'static,
        config: StreamConfig,
    ) -> std::io::Result<()> {
        let mut state = lock(&self.state);
        stop_running(&mut state);
        if state.closed {
            tracing::debug!("snapshot stream: not started, the window is closing");
            return Ok(());
        }
        let stop = Arc::new(AtomicBool::new(false));
        let thread = spawn_stream(engine, sink, config, Arc::clone(&stop))?;
        state.running = Some(Running { stop, thread });
        Ok(())
    }

    /// Stops the running stream and refuses new ones; the window has closed.
    pub(crate) fn close(&self) {
        let mut state = lock(&self.state);
        state.closed = true;
        stop_running(&mut state);
    }
}

fn stop_running(state: &mut SlotState) {
    let Some(running) = state.running.take() else {
        return;
    };
    running.stop.store(true, Ordering::Release);
    if running.thread.join().is_err() {
        tracing::error!("snapshot stream: the thread panicked");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stream::tests::{recording_sink, test_engine};
    use om_testing::wait::wait_until;
    use std::time::Duration;

    const FAST: StreamConfig = StreamConfig { hz: 200.0 };

    fn wait_for(seen: &crate::stream::tests::Seen, n: usize) -> bool {
        wait_until(Duration::from_secs(5), Duration::from_millis(2), || {
            seen.lock().unwrap().len() >= n
        })
    }

    #[test]
    fn a_new_stream_stops_the_previous_one() {
        let slot = StreamSlot::default();
        let (first, first_seen) = recording_sink();
        slot.start(test_engine(), first, FAST).unwrap();
        assert!(wait_for(&first_seen, 1));
        let (second, second_seen) = recording_sink();
        slot.start(test_engine(), second, FAST).unwrap();
        let frozen = first_seen.lock().unwrap().len();
        assert!(wait_for(&second_seen, 5));
        assert_eq!(first_seen.lock().unwrap().len(), frozen);
        slot.close();
    }

    #[test]
    fn close_stops_the_stream_and_refuses_new_ones() {
        let slot = StreamSlot::default();
        let (sink, seen) = recording_sink();
        slot.start(test_engine(), sink, FAST).unwrap();
        assert!(wait_for(&seen, 1));
        slot.close();
        let after_close = seen.lock().unwrap().len();
        let (late, late_seen) = recording_sink();
        slot.start(test_engine(), late, FAST).unwrap();
        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(seen.lock().unwrap().len(), after_close);
        assert!(late_seen.lock().unwrap().is_empty());
    }
}

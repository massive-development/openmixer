//! The loudness worker thread: drains the audio ring into a [`LoudnessAnalyser`] and publishes the
//! reading through atomics.

use super::analyser::{LoudnessAnalyser, CHANNELS};
use super::Shared;
use std::sync::atomic::Ordering::{Acquire, Relaxed};
use std::time::Duration;

/// Poll period while enabled. Far below the 400 ms momentary window.
const POLL: Duration = Duration::from_millis(10);
/// Poll period while the meter is disabled.
const IDLE: Duration = Duration::from_millis(50);
/// Samples read from the ring per pass (about 85 ms of stereo at 48 kHz).
const CHUNK: usize = 8192;

pub(super) fn run(shared: &Shared, mut rx: rtrb::Consumer<f32>) {
    let mut scratch = vec![0.0_f32; CHUNK];
    let mut analyser: Option<LoudnessAnalyser> = None;
    let mut failed_rate = 0_u32;
    let mut was_enabled = false;

    while !shared.stop.load(Acquire) {
        if !shared.enabled.load(Relaxed) {
            was_enabled = false;
            std::thread::sleep(IDLE);
            continue;
        }
        if !was_enabled {
            // Audio from before this enable is stale.
            drain(&mut rx);
            was_enabled = true;
        }

        let rate = shared.rate.load(Relaxed);
        if rate == 0 || rate == failed_rate {
            std::thread::sleep(POLL);
            continue;
        }
        if analyser.as_ref().map(LoudnessAnalyser::sample_rate) != Some(rate) {
            match LoudnessAnalyser::new(rate) {
                Ok(a) => {
                    analyser = Some(a);
                    shared.publish_silent();
                }
                Err(e) => {
                    tracing::warn!("loudness meter unavailable at {rate} Hz: {e}");
                    failed_rate = rate;
                    analyser = None;
                    continue;
                }
            }
        }
        let Some(meter) = analyser.as_mut() else {
            continue;
        };

        if shared.reset.swap(false, Relaxed) {
            drain(&mut rx);
            meter.reset();
            shared.publish_silent();
        }

        let take = rx.slots().min(CHUNK) & !(CHANNELS as usize - 1);
        if take == 0 {
            std::thread::sleep(POLL);
            continue;
        }
        if let Ok(chunk) = rx.read_chunk(take) {
            let (a, b) = chunk.as_slices();
            scratch[..a.len()].copy_from_slice(a);
            scratch[a.len()..take].copy_from_slice(b);
            chunk.commit_all();
        }
        meter.add_interleaved(&scratch[..take]);
        shared
            .frames
            .fetch_add((take / CHANNELS as usize) as u64, Relaxed);
        shared.publish(&meter.reading());
    }
}

fn drain(rx: &mut rtrb::Consumer<f32>) {
    let stale = rx.slots();
    if let Ok(chunk) = rx.read_chunk(stale) {
        chunk.commit_all();
    }
}

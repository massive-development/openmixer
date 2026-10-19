//! Decks: streaming file playback. A decoder thread fills a stereo frame ring from a symphonia
//! source; the audio thread reads it, interpolates to the device rate and renders stereo blocks.
//!
//! Threads: control methods (`load`, `seek`, `set_parameter`, ...) take `&self` and may be called
//! from any non-audio thread. `prepare` and the [`AudioCallback`] methods belong to the audio side.
//! Loops wrap on the audio thread into a decoder-prefetched slot, so loop jumps are gapless.
//! Key-lock renders through a [`om_stretch::Stretcher`] (see `playback/keylock.rs`).

mod decoder;
mod hotcues;
mod loop_state;
mod playback;
mod prefetch;
mod shared;
mod source;

use crate::callback::AudioCallback;
use crate::quantizer::Loop;
use decoder::DecoderWorker;
use loop_state::LoopSnapshot;
use om_core::sync::LockExt;
use om_core::{spsc, ParameterTarget};
use playback::Playback;
use shared::Shared;
use std::panic::AssertUnwindSafe;
use std::path::Path;
use std::sync::atomic::Ordering::{Relaxed, Release};
use std::sync::Arc;
use std::thread::JoinHandle;

pub use hotcues::NUM_CUES;
pub use prefetch::LoopPrefetch;
pub use source::{SourceError, SourceReader, TrackInfo};

/// Default loop prefetch size in source frames.
pub const DEFAULT_PREFETCH_FRAMES: i64 = 1 << 16;

/// Default ring size in source frames.
pub const DEFAULT_RING_FRAMES: usize = 1 << 16;
/// Rate clamp range.
pub const MAX_RATE: f64 = 8.0;
const DEFAULT_DEVICE_RATE: f64 = 44_100.0;
const DEFAULT_MONO_SINK: usize = 4096;

/// Control-side handle: loading, transport, hotcues and the parameter surface. Shareable across
/// control threads. Owns the decoder thread, which stops when the handle is dropped.
pub struct Deck {
    shared: Arc<Shared>,
    decoder: Option<JoinHandle<()>>,
}

/// Audio-side owner: renders blocks. Moves to the audio thread and implements [`AudioCallback`].
pub struct DeckAudio {
    shared: Arc<Shared>,
    playback: Playback,
    /// Right-channel sink for mono outputs; sized in `prepare`.
    mono_sink: Vec<f32>,
}

impl Deck {
    /// Creates an empty deck with the default loop prefetch size. See [`Deck::with_prefetch`].
    pub fn new(ring_frames: usize) -> (Self, DeckAudio) {
        Self::with_prefetch(ring_frames, DEFAULT_PREFETCH_FRAMES)
    }

    /// Creates an empty deck and starts its decoder thread. `ring_frames` is rounded up to a power
    /// of two by the ring; `prefetch_frames` bounds the loop prefetch slot. Control thread.
    pub fn with_prefetch(ring_frames: usize, prefetch_frames: i64) -> (Self, DeckAudio) {
        let (producer, consumer) = spsc::<f32>(ring_frames.max(1) * 2);
        let shared = Arc::new(Shared::new(prefetch_frames));
        let worker = DecoderWorker::new(Arc::clone(&shared), producer);
        let failed = Arc::clone(&shared);
        let decoder = std::thread::Builder::new()
            .name("openmixer-deck-decoder".into())
            .spawn(move || {
                // A panic must not unwind out of the thread unobserved: the deck records it and
                // stays silent from then on.
                let outcome = std::panic::catch_unwind(AssertUnwindSafe(|| worker.run()));
                if let Err(payload) = outcome {
                    let message = payload
                        .downcast_ref::<&str>()
                        .map(|s| (*s).to_owned())
                        .or_else(|| payload.downcast_ref::<String>().cloned())
                        .unwrap_or_else(|| "unknown panic".to_owned());
                    tracing::error!(error = %message, "deck decoder thread failed; deck is silenced");
                    failed.errored.store(true, Release);
                    failed.playing.store(false, Relaxed);
                }
            })
            .expect("spawn deck decoder thread");
        let audio = DeckAudio {
            shared: Arc::clone(&shared),
            playback: Playback::new(consumer),
            mono_sink: vec![0.0; DEFAULT_MONO_SINK],
        };
        (
            Self {
                shared,
                decoder: Some(decoder),
            },
            audio,
        )
    }

    /// Opens `path` and hands it to the decoder. Blocks the calling (control) thread while the
    /// container is probed. On error the deck is left untouched.
    pub fn load(&self, path: &Path) -> Result<(), SourceError> {
        let reader = SourceReader::open(path)?;
        let info = reader.info();
        let sh = &self.shared;
        sh.playing.store(false, Relaxed);
        sh.finished.store(false, Relaxed);
        sh.file_sample_rate.store(info.sample_rate);
        sh.length_frames.store(info.length_frames, Relaxed);
        sh.loop_state.store(LoopSnapshot::default());
        sh.hotcues.clear_all();
        self.hand_over(Some(reader));
        sh.loaded.store(true, Relaxed);
        self.request_reposition(0);
        Ok(())
    }

    /// Drops the current track. The decoder releases its reader.
    pub fn unload(&self) {
        let sh = &self.shared;
        sh.playing.store(false, Relaxed);
        sh.loaded.store(false, Relaxed);
        sh.length_frames.store(0, Relaxed);
        sh.loop_state.store(LoopSnapshot::default());
        sh.hotcues.clear_all();
        self.hand_over(None);
        self.request_reposition(0);
    }

    fn hand_over(&self, reader: Option<SourceReader>) {
        let mut pending = self.shared.pending.lock_unpoisoned();
        if reader.is_some() {
            pending.load_counter = pending.load_counter.wrapping_add(1);
            pending.load_id = pending.load_counter;
        }
        pending.reader = reader;
        pending.has = true;
    }

    pub fn play(&self) {
        self.set_playing(true);
    }

    pub fn pause(&self) {
        self.set_playing(false);
    }

    pub fn set_playing(&self, playing: bool) {
        self.shared.playing.store(playing, Relaxed);
    }

    /// Jumps to `source_frame` (clamped to zero from below) and clears the finished flag. Jumping
    /// outside an active loop exits it.
    pub fn seek(&self, source_frame: i64) {
        let loop_ = self.loop_state();
        if self.is_looping() && (source_frame < loop_.start || source_frame >= loop_.end) {
            self.exit_loop();
        }
        self.shared.finished.store(false, Relaxed);
        self.request_reposition(source_frame.max(0));
    }

    fn request_reposition(&self, target: i64) {
        self.shared.seek_target.store(target, Relaxed);
        self.shared.request_epoch.fetch_add(1, Release);
    }

    /// Sets loop bounds in source frames, clamped to the track. Returns false when fewer than two
    /// frames remain. The loop becomes active immediately. Quantize the bounds first with
    /// [`crate::quantizer`].
    pub fn set_loop(&self, loop_: Loop) -> bool {
        let len = self.length_frames();
        let start = loop_.start.clamp(0, len);
        let end = loop_.end.clamp(0, len);
        if end - start < 2 {
            return false;
        }
        self.shared.loop_state.store(LoopSnapshot {
            start,
            end,
            enabled: true,
        });
        true
    }

    /// Stops looping but keeps the bounds for [`Deck::reloop`].
    pub fn exit_loop(&self) {
        self.set_loop_enabled(false);
    }

    /// Resumes looping on the last bounds, if any.
    pub fn reloop(&self) {
        self.set_loop_enabled(true);
    }

    fn set_loop_enabled(&self, enabled: bool) {
        // Under the writer lock, so a concurrent `set_loop` cannot be undone by this toggle.
        self.shared
            .loop_state
            .update(|snap| snap.valid().then_some(LoopSnapshot { enabled, ..*snap }));
    }

    pub fn is_looping(&self) -> bool {
        self.shared
            .loop_state
            .load()
            .is_some_and(|snap| snap.active())
    }

    /// Current loop bounds; the default (`0..0`) when none is set.
    pub fn loop_state(&self) -> Loop {
        match self.shared.loop_state.load() {
            Some(snap) => Loop {
                start: snap.start,
                end: snap.end,
            },
            None => Loop::default(),
        }
    }

    /// True when the loop start is prefetched, so a loop jump is gapless.
    pub fn is_loop_prefetched(&self) -> bool {
        self.shared.prefetch.is_published()
    }

    pub fn set_rate(&self, rate: f64) {
        self.shared.rate.store(rate.clamp(0.0, MAX_RATE));
    }

    pub fn rate(&self) -> f64 {
        self.shared.rate.load()
    }

    pub fn is_loaded(&self) -> bool {
        self.shared.loaded.load(Relaxed)
    }

    pub fn is_playing(&self) -> bool {
        self.shared.playing.load(Relaxed)
    }

    /// True once the decoder thread has panicked. The deck then renders silence for good: a new
    /// `Deck` is needed to play again.
    pub fn is_errored(&self) -> bool {
        self.shared.errored.load(Relaxed)
    }

    pub fn has_finished(&self) -> bool {
        self.shared.finished.load(Relaxed)
    }

    pub fn length_frames(&self) -> i64 {
        self.shared.length_frames.load(Relaxed)
    }

    pub fn file_sample_rate(&self) -> f64 {
        self.shared.file_sample_rate.load()
    }

    pub fn position_frames(&self) -> i64 {
        self.shared.position_frames.load(Relaxed)
    }

    pub fn underrun_count(&self) -> u32 {
        self.shared.underruns.load(Relaxed)
    }

    /// Stereo frames currently buffered in the ring, readable from any thread.
    pub fn buffered_frames(&self) -> usize {
        let written = self.shared.written_samples.load(Relaxed);
        let read = self.shared.read_samples.load(Relaxed);
        (written.saturating_sub(read) / 2) as usize
    }

    /// Triggers hotcue `index`: sets an empty cue at the playhead, otherwise jumps to it.
    pub fn trigger_hotcue(&self, index: usize) {
        match self.shared.hotcues.get(index) {
            Some(cue) => self.seek(cue),
            None => self.shared.hotcues.set(index, self.position_frames()),
        }
    }

    pub fn clear_hotcue(&self, index: usize) {
        self.shared.hotcues.clear(index);
    }

    pub fn set_hotcue(&self, index: usize, frame: i64) {
        self.shared.hotcues.set(index, frame);
    }

    pub fn hotcue(&self, index: usize) -> Option<i64> {
        self.shared.hotcues.get(index)
    }

    /// Key-lock: tempo follows rate while pitch stays put. Needs [`DeckAudio::prepare`] first.
    pub fn set_keylock(&self, enabled: bool) {
        self.shared.keylock.store(enabled, Relaxed);
    }

    pub fn is_keylock(&self) -> bool {
        self.shared.keylock.load(Relaxed)
    }
}

impl Drop for Deck {
    fn drop(&mut self) {
        self.shared.stop.store(true, Release);
        if let Some(handle) = self.decoder.take() {
            let _ = handle.join();
        }
    }
}

impl DeckAudio {
    /// Renders one stereo block. Both slices have the same length.
    pub fn render_stereo(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.playback.render(&self.shared, left, right);
    }

    /// Sets the device rate used for interpolation and sizes the mono sink. Control thread,
    /// before audio runs.
    pub fn prepare(&mut self, sample_rate: f64, max_block_size: usize) {
        let rate = if sample_rate > 0.0 {
            sample_rate
        } else {
            DEFAULT_DEVICE_RATE
        };
        self.shared.device_sample_rate.store(rate);
        self.mono_sink.resize(max_block_size.max(1), 0.0);
        self.playback
            .prepare_keylock(rate as u32, max_block_size.max(1));
    }
}

impl AudioCallback for DeckAudio {
    fn prepare(&mut self, sample_rate: f64, max_block_size: usize) {
        DeckAudio::prepare(self, sample_rate, max_block_size);
    }

    fn process(&mut self, _inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        match outputs {
            [] => {}
            [mono] => {
                // Render through the sink in chunks; the left output receives the left channel.
                let total = mono.len();
                let cap = self.mono_sink.len();
                let mut done = 0;
                while done < total {
                    let n = (total - done).min(cap);
                    self.playback.render(
                        &self.shared,
                        &mut mono[done..done + n],
                        &mut self.mono_sink[..n],
                    );
                    done += n;
                }
            }
            [left, right, rest @ ..] => {
                self.playback.render(&self.shared, left, right);
                for extra in rest {
                    extra.fill(0.0);
                }
            }
        }
    }
}

impl ParameterTarget for Deck {
    /// Ids: `play` (>= 0.5 plays), `rate`, `seek` (0..1 of length), `loop.exit` and `loop.reloop`
    /// (>= 0.5 acts), `hotcueN` (>= 0.5 triggers), `hotcueN.clear`. Unknown ids return false.
    fn set_parameter(&self, id: &str, value: f64) -> bool {
        match id {
            "play" => self.set_playing(value >= 0.5),
            "rate" => self.set_rate(value),
            "keylock" => self.set_keylock(value >= 0.5),
            "loop.exit" => {
                if value >= 0.5 {
                    self.exit_loop();
                }
            }
            "loop.reloop" => {
                if value >= 0.5 {
                    self.reloop();
                }
            }
            "seek" => {
                let len = self.length_frames() as f64;
                self.seek((value.clamp(0.0, 1.0) * len) as i64);
            }
            _ => {
                let Some((index, clear)) = parse_hotcue_id(id) else {
                    return false;
                };
                if value >= 0.5 {
                    if clear {
                        self.clear_hotcue(index);
                    } else {
                        self.trigger_hotcue(index);
                    }
                }
            }
        }
        true
    }
}

/// Parses `hotcueN` or `hotcueN.clear`. Returns the index and whether it is a clear.
fn parse_hotcue_id(id: &str) -> Option<(usize, bool)> {
    let rest = id.strip_prefix("hotcue")?;
    let (digits, action) = rest.split_once('.').unwrap_or((rest, ""));
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let index: usize = digits.parse().ok()?;
    if !hotcues::HotcueBank::in_range(index) {
        return None;
    }
    match action {
        "" => Some((index, false)),
        "clear" => Some((index, true)),
        _ => None,
    }
}
#[cfg(test)]
mod keylock_tests;
#[cfg(test)]
mod loop_tests;
#[cfg(test)]
pub(crate) mod tests;

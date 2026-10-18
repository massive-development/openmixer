//! Decks: streaming file playback. A decoder thread fills a stereo frame ring from a symphonia
//! source; the audio thread reads it, interpolates to the device rate and renders stereo blocks.
//!
//! Threads: control methods (`load`, `seek`, `set_parameter`, ...) take `&self` and may be called
//! from any non-audio thread. `prepare` and the [`AudioCallback`] methods belong to the audio side.
//! Loops, slip, sync and key-lock are added on top of this core in later tasks.

mod decoder;
mod hotcues;
mod playback;
mod shared;
mod source;

use crate::callback::AudioCallback;
use decoder::DecoderWorker;
use om_core::{spsc, ParameterTarget};
use playback::Playback;
use shared::Shared;
use std::path::Path;
use std::sync::atomic::Ordering::{Relaxed, Release};
use std::sync::Arc;
use std::thread::JoinHandle;

pub use hotcues::NUM_CUES;
pub use source::{SourceError, SourceReader, TrackInfo};

/// Default ring size in source frames (C++ `Deck` default).
pub const DEFAULT_RING_FRAMES: usize = 1 << 16;
/// Rate clamp range (C++ `setRate`).
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
    /// Creates an empty deck and starts its decoder thread. `ring_frames` is rounded up to a power
    /// of two by the ring. Control thread.
    pub fn new(ring_frames: usize) -> (Self, DeckAudio) {
        let (producer, consumer) = spsc::<f32>(ring_frames.max(1) * 2);
        let shared = Arc::new(Shared::new());
        let worker = DecoderWorker::new(Arc::clone(&shared), producer);
        let decoder = std::thread::Builder::new()
            .name("openmixer-deck-decoder".into())
            .spawn(move || worker.run())
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
        sh.hotcues.clear_all();
        self.hand_over(None);
        self.request_reposition(0);
    }

    fn hand_over(&self, reader: Option<SourceReader>) {
        let mut pending = match self.shared.pending.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
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

    /// Jumps to `source_frame` (clamped to zero from below) and clears the finished flag.
    pub fn seek(&self, source_frame: i64) {
        self.shared.finished.store(false, Relaxed);
        self.request_reposition(source_frame.max(0));
    }

    fn request_reposition(&self, target: i64) {
        self.shared.seek_target.store(target, Relaxed);
        self.shared.request_epoch.fetch_add(1, Release);
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
    /// Ids: `play` (>= 0.5 plays), `rate`, `seek` (0..1 of length), `hotcueN` (>= 0.5 triggers),
    /// `hotcueN.clear`. Unknown ids return false.
    fn set_parameter(&self, id: &str, value: f64) -> bool {
        match id {
            "play" => self.set_playing(value >= 0.5),
            "rate" => self.set_rate(value),
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
pub(crate) mod tests;

//! The whole signal path: decks feed the mixer, which drives master (outputs 0/1) and headphone cue
//! (outputs 2/3).
//!
//! [`DjEngine::new`] returns two halves. [`DjEngine`] is the control side: deck handles, sync, FX,
//! the mixer control and the timer step, shareable across control threads. [`DjEngineAudio`] is the
//! audio side: it implements [`AudioCallback`] and moves to the device. Output channels: 0/1 master,
//! 2/3 headphone cue; a 2-channel device gets master only and a 1-channel device gets master left.
//!
//! Threads: control methods from any non-audio thread; `prepare`/`process` on the audio side only.

use crate::callback::AudioCallback;
use crate::deck::{Deck, DeckAudio, DEFAULT_RING_FRAMES};
use crate::deck_control::{DeckBank, DeckControl};
use crate::fx_routing::FxRouting;
use crate::grid::BeatGrid;
use crate::mixer::{ChannelInput, Mixer, MixerControl, MAX_CHANNELS};
use crate::sync::Sync;
use om_core::param_id::{Namespace, STEMS_PER_DECK};
use om_core::{
    AutomixSnapshot, ChannelSnapshot, DeckSnapshot, EngineSnapshot, MixerSnapshot, ParameterRouter,
    SamplerSnapshot, StemSnapshot,
};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::Arc;

/// Control side of the engine. See the module docs.
pub struct DjEngine {
    decks: Vec<Arc<Deck>>,
    controls: Vec<Arc<DeckControl>>,
    sync: Arc<Sync>,
    mixer: Arc<MixerControl>,
    fx: Arc<FxRouting>,
    /// Control-timer ticks so far; stamps [`EngineSnapshot::seq`].
    ticks: AtomicU64,
}

/// Audio side of the engine. Implements [`AudioCallback`].
pub struct DjEngineAudio {
    decks: Vec<DeckAudio>,
    mixer: Mixer,
    deck_buffers: Vec<StereoBuffer>,
    master: StereoBuffer,
    cue: StereoBuffer,
    max_block: usize,
}

struct StereoBuffer {
    left: Vec<f32>,
    right: Vec<f32>,
}

impl StereoBuffer {
    fn new() -> Self {
        Self {
            left: Vec::new(),
            right: Vec::new(),
        }
    }

    fn resize(&mut self, n: usize) {
        self.left.clear();
        self.left.resize(n, 0.0);
        self.right.clear();
        self.right.resize(n, 0.0);
    }
}

impl DjEngine {
    /// One deck per mixer channel. `num_decks` is clamped to the mixer's channel range. Control
    /// thread.
    pub fn new(num_decks: usize) -> (Self, DjEngineAudio) {
        let (mixer, mixer_control) = Mixer::new(num_decks);
        let count = mixer.num_channels();
        let sync = Arc::new(Sync::new(count));
        let mut decks = Vec::with_capacity(count);
        let mut deck_audio = Vec::with_capacity(count);
        let mut controls = Vec::with_capacity(count);
        for index in 0..count {
            let (deck, audio) = Deck::new(DEFAULT_RING_FRAMES);
            let deck = Arc::new(deck);
            controls.push(Arc::new(DeckControl::new(
                Arc::clone(&deck),
                Arc::clone(&sync),
                index,
            )));
            decks.push(deck);
            deck_audio.push(audio);
        }
        let fx = Arc::new(FxRouting::new(&mixer_control));
        let engine = Self {
            decks,
            controls,
            sync,
            mixer: Arc::new(mixer_control),
            fx,
            ticks: AtomicU64::new(0),
        };
        let audio = DjEngineAudio {
            decks: deck_audio,
            mixer,
            deck_buffers: (0..count).map(|_| StereoBuffer::new()).collect(),
            master: StereoBuffer::new(),
            cue: StereoBuffer::new(),
            max_block: 0,
        };
        (engine, audio)
    }

    pub fn num_decks(&self) -> usize {
        self.decks.len()
    }

    pub fn deck(&self, index: usize) -> &Deck {
        &self.decks[index]
    }

    pub fn mixer(&self) -> &MixerControl {
        &self.mixer
    }

    /// Shared handle to the mixer's control side, for holders that outlive a borrow (plugin
    /// stages that must be detached later).
    pub fn mixer_handle(&self) -> Arc<MixerControl> {
        Arc::clone(&self.mixer)
    }

    pub fn fx(&self) -> &FxRouting {
        &self.fx
    }

    pub fn sync(&self) -> &Sync {
        &self.sync
    }

    pub fn deck_control(&self, index: usize) -> &DeckControl {
        &self.controls[index]
    }

    /// Beat grid for a deck (`None` removes it); the analysis pipeline plugs in here.
    pub fn set_grid(&self, deck: usize, grid: Option<Arc<dyn BeatGrid>>) {
        self.controls[deck].set_grid(grid);
    }

    /// Registers the control surface with `router`: the `deck`, `mixer`, `fx` and `sync` namespaces.
    pub fn register(&self, router: &ParameterRouter) {
        router.register_typed(
            Namespace::Deck,
            Arc::new(DeckBank::new(self.controls.clone())),
        );
        router.register_typed(Namespace::Mixer, self.mixer.clone());
        router.register_typed(Namespace::Fx, self.fx.clone());
        router.register_typed(Namespace::Sync, self.sync.clone());
    }

    /// Control-timer step (about 20-50 Hz, control thread): sync rates and phase, FX tempo, slip
    /// ghosts.
    pub fn update_control(&self, elapsed_seconds: f64) {
        let decks: Vec<Option<&Deck>> = self.decks.iter().map(|d| Some(d.as_ref())).collect();
        self.sync.apply(&decks);
        self.fx.follow_decks(&decks, &self.sync);
        for control in &self.controls {
            control.tick(elapsed_seconds);
        }
        self.ticks.fetch_add(1, Relaxed);
    }

    /// One deck's state, copied out without allocating. Control thread only.
    ///
    /// Takes no audio lock: deck values are atomics, loop bounds a seqlock read, and the sync and
    /// slip state are control-side mutexes that only control threads take.
    pub fn deck_snapshot(&self, index: usize) -> DeckSnapshot {
        let deck = &self.decks[index];
        let control = &self.controls[index];
        let bounds = deck.loop_state();
        DeckSnapshot {
            loaded: deck.is_loaded(),
            playing: deck.is_playing(),
            keylock: deck.is_keylock(),
            looping: deck.is_looping(),
            slip: control.is_slipping(),
            quantize: control.is_quantize(),
            sync: self.sync.is_enabled(index),
            sync_master: self.sync.leader() == Some(index),
            rate: deck.rate(),
            position_frames: deck.position_frames(),
            length_frames: deck.length_frames(),
            file_sample_rate: deck.file_sample_rate(),
            loop_start: bounds.start,
            loop_end: bounds.end,
            hotcues: std::array::from_fn(|cue| deck.hotcue(cue)),
        }
    }

    /// The whole engine as plain values, for the UI and MIDI feedback. Control thread only.
    ///
    /// Pull-based: the caller asks, the engine answers from its atomics and control-side getters.
    /// It never blocks the audio thread and takes no mixer or deck audio lock (the same rule as
    /// [`DeckSnapshot`]; the mixer side is atomics only). Approximate: fields are read one by one,
    /// so a snapshot can mix two audio blocks. Stems, sampler and automix are empty placeholders.
    pub fn snapshot(&self) -> EngineSnapshot {
        let decks: Vec<DeckSnapshot> = (0..self.decks.len())
            .map(|index| self.deck_snapshot(index))
            .collect();
        let mixer = MixerSnapshot {
            channels: (0..self.mixer.num_channels())
                .map(|ch| ChannelSnapshot {
                    trim: self.mixer.trim(ch),
                    fader: self.mixer.fader(ch),
                    filter: self.mixer.filter(ch),
                    eq: std::array::from_fn(|band| self.mixer.eq(ch, band)),
                    pfl: self.mixer.pfl(ch),
                    peak_left: self.mixer.channel_peak_left(ch),
                    peak_right: self.mixer.channel_peak_right(ch),
                })
                .collect(),
            crossfader: self.mixer.crossfader(),
            master_gain: self.mixer.master_gain(),
            cue_level: self.mixer.cue_level(),
            cue_mix: self.mixer.cue_mix(),
            master_peak_left: self.mixer.master_peak_left(),
            master_peak_right: self.mixer.master_peak_right(),
        };
        let stems = decks
            .iter()
            .map(|_| [StemSnapshot; STEMS_PER_DECK as usize])
            .collect();
        EngineSnapshot {
            seq: self.ticks.load(Relaxed),
            decks,
            mixer,
            stems,
            sampler: SamplerSnapshot,
            automix: AutomixSnapshot,
        }
    }
}

impl DjEngineAudio {
    /// Renders `n` frames (at most `max_block`) through every deck, the mixer and the output map.
    fn render_block(&mut self, outputs: &mut [&mut [f32]], offset: usize, n: usize) {
        for (deck, buf) in self.decks.iter_mut().zip(self.deck_buffers.iter_mut()) {
            deck.render_stereo(&mut buf.left[..n], &mut buf.right[..n]);
        }
        let mut inputs = [ChannelInput::default(); MAX_CHANNELS];
        for (slot, buf) in inputs.iter_mut().zip(self.deck_buffers.iter()) {
            *slot = ChannelInput {
                left: Some(&buf.left[..n]),
                right: Some(&buf.right[..n]),
            };
        }
        let count = self.deck_buffers.len();
        self.mixer.process(
            &inputs[..count],
            &mut self.master.left[..n],
            &mut self.master.right[..n],
            &mut self.cue.left[..n],
            &mut self.cue.right[..n],
        );

        let channels = outputs.len();
        for (c, out) in outputs.iter_mut().enumerate() {
            let source: Option<&[f32]> = match c {
                0 => Some(&self.master.left[..n]),
                1 if channels >= 2 => Some(&self.master.right[..n]),
                2 if channels >= 4 => Some(&self.cue.left[..n]),
                3 if channels >= 4 => Some(&self.cue.right[..n]),
                _ => None,
            };
            let dst = &mut out[offset..offset + n];
            match source {
                Some(src) => dst.copy_from_slice(src),
                None => dst.fill(0.0),
            }
        }
    }
}

impl AudioCallback for DjEngineAudio {
    fn prepare(&mut self, sample_rate: f64, max_block_size: usize) {
        self.max_block = max_block_size.max(1);
        for deck in &mut self.decks {
            deck.prepare(sample_rate, self.max_block);
        }
        self.mixer.prepare(sample_rate, self.max_block);
        for buf in &mut self.deck_buffers {
            buf.resize(self.max_block);
        }
        self.master.resize(self.max_block);
        self.cue.resize(self.max_block);
    }

    fn process(&mut self, _inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        let total = outputs.first().map_or(0, |out| out.len());
        if self.max_block == 0 {
            // prepare() not called yet: silence rather than render at an unknown block size.
            for out in outputs.iter_mut() {
                out.fill(0.0);
            }
            return;
        }
        let mut done = 0;
        while done < total {
            let n = (total - done).min(self.max_block);
            self.render_block(outputs, done, n);
            done += n;
        }
    }
}

#[cfg(test)]
mod tests;

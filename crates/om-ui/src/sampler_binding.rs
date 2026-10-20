//! The sampler's eight pads as a view-model: what each pad shows (from the engine's snapshot and
//! the sample bank), and the writes a pad gets from the window or the keyboard. Presses and
//! releases are typed `sampler.pad<N>.trigger|gate` [`Command::Param`]s (or the string façade with
//! no sink); a gate pad holds while pressed. Gain, pitch and mode go to the sample bank, which
//! stores them and applies them to the sampler.

use std::path::Path;
use std::sync::{Arc, Mutex};

use om_core::jobs::JobHandle;
use om_core::param_id::SamplerParam;
use om_core::sync::LockExt;
use om_core::{CommandSink, ParamId, ParameterTarget};
use om_engine::sampler::{PadMode, MAX_PAD_GAIN, MAX_PITCH_SEMITONES, PADS};
use om_engine::DjEngine;

use crate::binding::deliver;
use crate::sample_bank::{BankError, SampleBank};

/// The keys that press pads 1 to 8 in the window, in pad order.
pub const PAD_KEYS: [char; PADS] = ['1', '2', '3', '4', '5', '6', '7', '8'];

/// The pad a key press names (`"1"` to `"8"`), if any.
pub fn pad_for_key(key: &str) -> Option<usize> {
    let mut chars = key.chars();
    let c = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    PAD_KEYS.iter().position(|&k| k == c)
}

/// The mode's name on screen.
pub fn mode_label(mode: PadMode) -> &'static str {
    match mode {
        PadMode::OneShot => "one-shot",
        PadMode::Gate => "gate",
        PadMode::Loop => "loop",
    }
}

/// One line on how a pad of this mode behaves, shown next to the mode.
pub fn mode_hint(mode: PadMode) -> &'static str {
    match mode {
        PadMode::OneShot => "plays to the end",
        PadMode::Gate => "plays while held",
        PadMode::Loop => "tap to start, tap again to stop",
    }
}

/// The next mode in the pad's toggle order: one-shot, gate, loop, one-shot.
pub fn next_mode(mode: PadMode) -> PadMode {
    match mode {
        PadMode::OneShot => PadMode::Gate,
        PadMode::Gate => PadMode::Loop,
        PadMode::Loop => PadMode::OneShot,
    }
}

/// One pad as the window draws it.
#[derive(Debug, Clone, PartialEq)]
pub struct PadView {
    pub pad: usize,
    /// The key that presses it (`1` to `8`).
    pub key: char,
    /// The sample's name, or `Empty`.
    pub name: String,
    pub loaded: bool,
    /// The file is in the bank but not on disk.
    pub missing: bool,
    /// A decode for this pad is queued or running.
    pub loading: bool,
    pub playing: bool,
    pub gain: f32,
    pub pitch: f32,
    pub mode: PadMode,
    pub selected: bool,
}

/// Routes the sampler pads. Cheap to clone; clones share the selection, the path fields and the
/// pending loads.
#[derive(Clone)]
pub struct SamplerBinding {
    pub engine: Arc<DjEngine>,
    /// Typed writes. `None` falls back to `target`.
    pub sink: Option<Arc<dyn CommandSink>>,
    /// String writes, used when there is no sink.
    pub target: Arc<dyn ParameterTarget>,
    /// The sample bank, when the app has one. Without it, loads are refused and settings are not
    /// stored.
    pub bank: Option<Arc<SampleBank>>,
    selected: Arc<Mutex<usize>>,
    paths: Arc<Mutex<[String; PADS]>>,
    pending: Arc<Mutex<Vec<(usize, JobHandle)>>>,
}

impl SamplerBinding {
    /// A binding that writes through `sink` (typed commands) and `target` (strings, when no sink).
    pub fn new(
        engine: Arc<DjEngine>,
        sink: Option<Arc<dyn CommandSink>>,
        target: Arc<dyn ParameterTarget>,
        bank: Option<Arc<SampleBank>>,
    ) -> Self {
        Self {
            engine,
            sink,
            target,
            bank,
            selected: Arc::new(Mutex::new(0)),
            paths: Arc::new(Mutex::new(Default::default())),
            pending: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// The selected pad (the one the per-pad controls and the path field act on).
    pub fn selected(&self) -> usize {
        *self.selected.lock_unpoisoned()
    }

    /// Selects `pad`. Out-of-range pads are ignored.
    pub fn select(&self, pad: usize) -> bool {
        if pad < PADS {
            *self.selected.lock_unpoisoned() = pad;
            true
        } else {
            false
        }
    }

    /// The path text typed for `pad` (empty when none).
    pub fn path(&self, pad: usize) -> String {
        self.paths
            .lock_unpoisoned()
            .get(pad)
            .cloned()
            .unwrap_or_default()
    }

    /// Replaces the path text for `pad`. Out-of-range pads are ignored.
    pub fn set_path(&self, pad: usize, text: &str) {
        if let Some(slot) = self.paths.lock_unpoisoned().get_mut(pad) {
            *slot = text.to_owned();
        }
    }

    /// The mode of `pad` as the engine has it.
    pub fn mode(&self, pad: usize) -> Option<PadMode> {
        self.engine.snapshot().sampler.pads.get(pad).map(|p| p.mode)
    }

    /// Starts `pad` at `velocity` (0 to 1): a gate pad is held open until [`SamplerBinding::release`],
    /// the others start a voice. False for a bad pad or a rejected write.
    pub fn press(&self, pad: usize, velocity: f32) -> bool {
        let Some(mode) = self.mode(pad) else {
            return false;
        };
        let Ok(index) = u8::try_from(pad) else {
            return false;
        };
        if mode == PadMode::Gate {
            self.send(
                ParamId::Sampler {
                    pad: index,
                    p: SamplerParam::Gate,
                },
                1.0,
            )
        } else {
            self.send(
                ParamId::Sampler {
                    pad: index,
                    p: SamplerParam::Trigger,
                },
                f64::from(velocity.clamp(0.0, 1.0)),
            )
        }
    }

    /// Lets go of `pad`: ends a gate's hold. Loops latch (press again to stop) and one-shots play
    /// out, so both ignore it.
    pub fn release(&self, pad: usize) -> bool {
        let Ok(index) = u8::try_from(pad) else {
            return false;
        };
        if pad >= PADS {
            return false;
        }
        let mode = self.mode(pad).unwrap_or(PadMode::OneShot);
        let p = if mode == PadMode::Gate {
            SamplerParam::Gate
        } else {
            SamplerParam::Trigger
        };
        self.send(ParamId::Sampler { pad: index, p }, 0.0)
    }

    /// Sets the pad's gain (linear, 0 to 1.5), keeping its pitch and mode.
    pub fn set_gain(&self, pad: usize, gain: f32) -> Result<(), BankError> {
        let (_, pitch, mode) = self.current(pad);
        self.store(pad, gain.clamp(0.0, MAX_PAD_GAIN), pitch, mode)
    }

    /// Sets the pad's pitch in semitones (-24 to 24), keeping its gain and mode.
    pub fn set_pitch(&self, pad: usize, semitones: f32) -> Result<(), BankError> {
        let (gain, _, mode) = self.current(pad);
        self.store(
            pad,
            gain,
            semitones.clamp(-MAX_PITCH_SEMITONES, MAX_PITCH_SEMITONES),
            mode,
        )
    }

    /// Sets the pad's mode, keeping its gain and pitch.
    pub fn set_mode(&self, pad: usize, mode: PadMode) -> Result<(), BankError> {
        let (gain, pitch, _) = self.current(pad);
        self.store(pad, gain, pitch, mode)
    }

    /// Moves the pad to the next mode (one-shot, gate, loop). Returns the new mode.
    pub fn cycle_mode(&self, pad: usize) -> Result<PadMode, BankError> {
        let current = self.mode(pad).unwrap_or(PadMode::OneShot);
        let next = next_mode(current);
        self.set_mode(pad, next)?;
        Ok(next)
    }

    /// Loads `path` into `pad` through the bank: decoded on its worker, installed on a later pump.
    /// The handle finishes when the decode does. Needs a bank.
    pub fn load(&self, pad: usize, path: &Path) -> Result<JobHandle, BankError> {
        let bank = self.bank.as_ref().ok_or(BankError::NotWritable)?;
        let index = u8::try_from(pad).map_err(|_| BankError::BadPad(u8::MAX))?;
        let handle = bank.import_file(index, path)?;
        self.pending.lock_unpoisoned().push((pad, handle.clone()));
        Ok(handle)
    }

    /// Loads the path typed for `pad`, when there is one.
    pub fn load_typed_path(&self, pad: usize) -> Result<JobHandle, BankError> {
        let text = self.path(pad);
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err(BankError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "type a path first",
            )));
        }
        self.load(pad, Path::new(trimmed))
    }

    /// Empties `pad` and drops its entry from the bank.
    pub fn clear(&self, pad: usize) -> Result<(), BankError> {
        let index = u8::try_from(pad).map_err(|_| BankError::BadPad(u8::MAX))?;
        match &self.bank {
            Some(bank) => bank.clear_pad(index),
            None => self.engine.sampler().clear(index).map_err(BankError::from),
        }
    }

    /// True while a decode for `pad` is queued or running.
    pub fn loading(&self, pad: usize) -> bool {
        self.pending
            .lock_unpoisoned()
            .iter()
            .any(|(p, handle)| *p == pad && !handle.is_finished())
    }

    /// Forgets the loads that have finished. Call from the UI tick.
    pub fn prune_pending(&self) {
        self.pending
            .lock_unpoisoned()
            .retain(|(_, handle)| !handle.is_finished());
    }

    /// The eight pads, in order, as the window draws them.
    pub fn pads(&self) -> Vec<PadView> {
        let snapshot = self.engine.snapshot().sampler;
        let selected = self.selected();
        (0..PADS)
            .map(|pad| {
                let state = snapshot.pads[pad];
                let entry = self.bank.as_ref().and_then(|b| b.entry(pad as u8));
                let name = match (&entry, state.loaded) {
                    (Some(e), _) => e.name.clone(),
                    (None, true) => format!("Pad {}", pad + 1),
                    (None, false) => "Empty".to_owned(),
                };
                PadView {
                    pad,
                    key: PAD_KEYS[pad],
                    name,
                    loaded: state.loaded,
                    missing: self.bank.as_ref().is_some_and(|b| b.is_missing(pad as u8)),
                    loading: self.loading(pad),
                    playing: state.playing,
                    gain: state.gain,
                    pitch: state.pitch,
                    mode: state.mode,
                    selected: pad == selected,
                }
            })
            .collect()
    }

    /// One line about the selected pad, for the strip's caption.
    pub fn caption(&self) -> String {
        let pad = self.selected();
        let view = &self.pads()[pad];
        let state = if view.missing {
            "file missing"
        } else if view.loading {
            "loading"
        } else if view.loaded {
            "loaded"
        } else {
            "empty"
        };
        format!(
            "Pad {} ({}): {} · {} · {:+.1} st · {:.0}%",
            pad + 1,
            view.key,
            state,
            mode_label(view.mode),
            view.pitch,
            view.gain * 100.0
        )
    }

    fn send(&self, id: ParamId, value: f64) -> bool {
        deliver(self.sink.as_ref(), &self.target, id, value)
    }

    /// Gain, pitch and mode of `pad` as the engine has them now.
    fn current(&self, pad: usize) -> (f32, f32, PadMode) {
        match self.engine.snapshot().sampler.pads.get(pad) {
            Some(p) => (p.gain, p.pitch, p.mode),
            None => (1.0, 0.0, PadMode::OneShot),
        }
    }

    fn store(&self, pad: usize, gain: f32, pitch: f32, mode: PadMode) -> Result<(), BankError> {
        if pad >= PADS {
            return Err(BankError::BadPad(pad as u8));
        }
        match &self.bank {
            Some(bank) => bank.set_settings(pad as u8, gain, pitch, mode),
            None => {
                let sampler = self.engine.sampler();
                sampler.set_gain(pad as u8, gain);
                sampler.set_pitch(pad as u8, pitch);
                sampler.set_mode(pad as u8, mode);
                Ok(())
            }
        }
    }
}

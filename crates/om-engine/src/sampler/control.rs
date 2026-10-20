//! Control side of the sampler: what MIDI, the UI and tests call. Every method takes `&self`, runs
//! on a non-audio thread and never blocks on the audio thread.

use super::{
    PadMode, Sample, SamplerCmd, SamplerError, Shared, MAX_MASTER_GAIN, MAX_PAD_GAIN,
    MAX_PITCH_SEMITONES, PADS,
};
use om_core::param_id::SamplerParam;
use om_core::sync::LockExt;
use om_core::{PadSnapshot, ParamId, ParamTarget, SamplerSnapshot};
use std::hash::{Hash, Hasher};
use std::sync::atomic::Ordering::Relaxed;
use std::sync::{Arc, Mutex};

/// Pitch range the `sampler.pad<N>.pitch` id accepts, in semitones.
pub const PARAM_PITCH_RANGE: f32 = 12.0;

pub struct SamplerControl {
    shared: Arc<Shared>,
    commands: Mutex<rtrb::Producer<SamplerCmd>>,
    retired: Mutex<rtrb::Consumer<Arc<Sample>>>,
}

impl SamplerControl {
    pub(super) fn new(
        shared: Arc<Shared>,
        commands: rtrb::Producer<SamplerCmd>,
        retired: rtrb::Consumer<Arc<Sample>>,
    ) -> Self {
        Self {
            shared,
            commands: Mutex::new(commands),
            retired: Mutex::new(retired),
        }
    }

    /// Engine sample rate as last prepared; decode samples at this rate.
    pub fn engine_rate(&self) -> f64 {
        self.shared.sample_rate.load()
    }

    /// Queues `sample` for `pad`. Voices already playing the old sample finish it; new triggers use
    /// the new one. Fails for a bad pad or when the command ring is full.
    pub fn assign(&self, pad: u8, sample: Arc<Sample>) -> Result<(), SamplerError> {
        let index = check_pad(pad)?;
        self.reclaim();
        let hash = name_hash(sample.name());
        self.push(SamplerCmd::Assign { pad, sample })
            .map_err(|_| SamplerError::QueueFull)?;
        self.shared.name_hash[index].store(hash, Relaxed);
        self.shared.loaded[index].store(true, Relaxed);
        Ok(())
    }

    /// Empties `pad` and fades out its voices.
    pub fn clear(&self, pad: u8) -> Result<(), SamplerError> {
        let index = check_pad(pad)?;
        self.reclaim();
        self.push(SamplerCmd::Clear { pad })
            .map_err(|_| SamplerError::QueueFull)?;
        self.shared.name_hash[index].store(0, Relaxed);
        self.shared.loaded[index].store(false, Relaxed);
        Ok(())
    }

    /// Starts a voice on `pad` with `velocity` (0 to 1, clamped; 0 or less does nothing) in the
    /// pad's mode. A full ring drops the trigger and counts it.
    pub fn trigger(&self, pad: u8, velocity: f32) {
        self.trigger_with(pad, velocity, None);
    }

    /// As [`SamplerControl::trigger`], with `mode` overriding the pad's mode for this voice (a MIDI
    /// gate press).
    pub fn trigger_with(&self, pad: u8, velocity: f32, mode: Option<PadMode>) {
        if usize::from(pad) >= PADS || !velocity.is_finite() || velocity <= 0.0 {
            return;
        }
        // Before the push, so the audio side never sees a voice whose press is not yet marked held.
        self.shared.held[usize::from(pad)].store(true, Relaxed);
        self.send_or_count(SamplerCmd::Trigger {
            pad,
            velocity: velocity.min(1.0),
            mode,
        });
    }

    /// Releases the pad's gate voices. Loop voices latch (a second trigger stops them) and one-shots
    /// play out, so both ignore it.
    pub fn release(&self, pad: u8) {
        if usize::from(pad) < PADS {
            self.shared.held[usize::from(pad)].store(false, Relaxed);
            self.send_or_count(SamplerCmd::Release { pad });
        }
    }

    /// Fades out every voice.
    pub fn stop_all(&self) {
        for held in &self.shared.held {
            held.store(false, Relaxed);
        }
        self.send_or_count(SamplerCmd::StopAll);
    }

    /// Pad gain, linear, clamped to 0..=1.5. Non-finite values are ignored.
    pub fn set_gain(&self, pad: u8, gain: f32) {
        if let (Some(cell), true) = (self.shared.gain.get(usize::from(pad)), gain.is_finite()) {
            cell.store(gain.clamp(0.0, MAX_PAD_GAIN));
        }
    }

    /// Varispeed in semitones, clamped to +-24. Non-finite values are ignored. Applies to sounding
    /// voices too.
    pub fn set_pitch(&self, pad: u8, semitones: f32) {
        if let (Some(cell), true) = (
            self.shared.pitch.get(usize::from(pad)),
            semitones.is_finite(),
        ) {
            cell.store(semitones.clamp(-MAX_PITCH_SEMITONES, MAX_PITCH_SEMITONES));
        }
    }

    /// The pad's mode for voices started after this call.
    pub fn set_mode(&self, pad: u8, mode: PadMode) {
        if let Some(cell) = self.shared.mode.get(usize::from(pad)) {
            cell.store(mode.as_u8(), Relaxed);
        }
    }

    /// Sampler bus gain, linear, clamped to 0..=2.
    pub fn set_master_gain(&self, gain: f32) {
        if gain.is_finite() {
            self.shared
                .master_gain
                .store(gain.clamp(0.0, MAX_MASTER_GAIN));
        }
    }

    /// Frees samples the audio thread has let go of. Called by `assign` and `clear`, and by
    /// [`crate::DjEngine::update_control`] on the control tick. The audio side keeps working
    /// without it (finished voices stay reusable), but replaced samples are only freed here.
    pub fn reclaim(&self) {
        let mut retired = self.retired.lock_unpoisoned();
        while retired.pop().is_ok() {}
    }

    /// Trigger, release and stop commands dropped so far because a ring or voice slots were full.
    pub fn dropped_commands(&self) -> u64 {
        self.shared.dropped.load(Relaxed)
    }

    /// Pad and voice state as plain values, from atomics.
    pub fn snapshot(&self) -> SamplerSnapshot {
        let s = &self.shared;
        SamplerSnapshot {
            pads: std::array::from_fn(|pad| PadSnapshot {
                loaded: s.loaded[pad].load(Relaxed),
                playing: s.playing[pad].load(Relaxed) > 0,
                gain: s.gain[pad].load(),
                pitch: s.pitch[pad].load(),
                mode: PadMode::from_u8(s.mode[pad].load(Relaxed)),
                name_hash: s.name_hash[pad].load(Relaxed),
            }),
            voices_active: s.voices_active.load(Relaxed),
            dropped_cmds: s.dropped.load(Relaxed),
        }
    }

    fn push(&self, cmd: SamplerCmd) -> Result<(), ()> {
        self.commands.lock_unpoisoned().push(cmd).map_err(|_| ())
    }

    fn send_or_count(&self, cmd: SamplerCmd) {
        if self.push(cmd).is_err() {
            self.shared.dropped.fetch_add(1, Relaxed);
        }
    }
}

fn check_pad(pad: u8) -> Result<usize, SamplerError> {
    if usize::from(pad) < PADS {
        Ok(usize::from(pad))
    } else {
        Err(SamplerError::BadPad(pad))
    }
}

fn name_hash(name: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    name.hash(&mut hasher);
    hasher.finish().max(1)
}

impl ParamTarget for SamplerControl {
    /// `trigger`: the value is the velocity (0 to 1, above 1 clamps); 0 or less releases the pad (which only ends gate voices; a loop pad latches: trigger again to stop it).
    /// `gate`: 0.5 or more presses the pad as a gate (velocity 1, whatever the pad's mode), below
    /// 0.5 releases it; one id carries MIDI note on and note off. `volume`: linear gain 0 to 1.5.
    /// `pitch`: semitones, -12 to 12. Non-finite values are rejected.
    fn set_param(&self, id: &ParamId, value: f64) -> bool {
        let ParamId::Sampler { pad, p } = id else {
            return false;
        };
        if usize::from(*pad) >= PADS || !value.is_finite() {
            return false;
        }
        let pad = *pad;
        match p {
            SamplerParam::Trigger => {
                if value > 0.0 {
                    self.trigger(pad, value.min(1.0) as f32);
                } else {
                    self.release(pad);
                }
            }
            SamplerParam::Gate => {
                if value >= 0.5 {
                    self.trigger_with(pad, 1.0, Some(PadMode::Gate));
                } else {
                    self.release(pad);
                }
            }
            SamplerParam::Volume => {
                self.set_gain(pad, value.clamp(0.0, f64::from(MAX_PAD_GAIN)) as f32)
            }
            SamplerParam::Pitch => self.set_pitch(
                pad,
                value.clamp(-f64::from(PARAM_PITCH_RANGE), f64::from(PARAM_PITCH_RANGE)) as f32,
            ),
        }
        true
    }
}

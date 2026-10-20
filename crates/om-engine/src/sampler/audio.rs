//! Audio side of the sampler. Allocation-free, lock-free, drop-free: see the module docs in
//! [`super`].

use super::{
    PadMode, Sample, SamplerCmd, Shared, MAX_MASTER_GAIN, MAX_PAD_GAIN, MAX_PITCH_SEMITONES, PADS,
    RING_CAPACITY, SLOTS_PER_PAD, VOICES_PER_PAD,
};
use crate::stage::ProcessStage;
use rtrb::PushError;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;

/// Fade-in on every voice start, in output frames (linear, about 0.7 ms at 44.1 kHz). The envelope
/// after `i` frames is `(i + 1) / START_RAMP_FRAMES`.
pub(crate) const START_RAMP_FRAMES: f32 = 32.0;
/// Fade-out on release, steal and stop.
const FADE_SECONDS: f64 = 0.005;
const TOTAL_VOICES: usize = PADS * SLOTS_PER_PAD;

struct Voice {
    /// `Some` while the voice plays and, once it has finished, until the reference is handed back
    /// to the control side.
    sample: Option<Arc<Sample>>,
    active: bool,
    mode: PadMode,
    releasing: bool,
    /// Position in source frames.
    pos: f64,
    velocity: f32,
    env: f32,
    env_delta: f32,
    age: u64,
}

impl Voice {
    fn idle() -> Self {
        Self {
            sample: None,
            active: false,
            mode: PadMode::OneShot,
            releasing: false,
            pos: 0.0,
            velocity: 0.0,
            env: 0.0,
            env_delta: 0.0,
            age: 0,
        }
    }

    fn sounding(&self) -> bool {
        self.active && !self.releasing
    }
}

struct PadSlot {
    sample: Option<Arc<Sample>>,
    /// A replaced sample waiting to go back to the control side.
    limbo: Option<Arc<Sample>>,
    /// Gain (pad times bus) applied at the end of the last block; the next block ramps from it.
    applied_gain: f32,
}

/// The audio-side sampler. Build with [`SamplerAudio::new`]; a [`crate::Mixer`] owns it and runs it
/// before the master gain. It also implements [`ProcessStage`] (additive: it mixes into the buffers
/// it is given).
pub struct SamplerAudio {
    commands: rtrb::Consumer<SamplerCmd>,
    retired: rtrb::Producer<Arc<Sample>>,
    shared: Arc<Shared>,
    pads: [PadSlot; PADS],
    voices: [Voice; TOTAL_VOICES],
    engine_rate: f64,
    fade_delta: f32,
    age: u64,
}

impl SamplerAudio {
    /// The audio half and its control handle. Control thread.
    pub fn new() -> (Self, super::SamplerControl) {
        let shared = Arc::new(Shared::new());
        let (command_tx, command_rx) = rtrb::RingBuffer::new(RING_CAPACITY);
        let (retired_tx, retired_rx) = rtrb::RingBuffer::new(RING_CAPACITY);
        let audio = Self {
            commands: command_rx,
            retired: retired_tx,
            shared: Arc::clone(&shared),
            pads: std::array::from_fn(|_| PadSlot {
                sample: None,
                limbo: None,
                applied_gain: 1.0,
            }),
            voices: std::array::from_fn(|_| Voice::idle()),
            engine_rate: 44_100.0,
            fade_delta: fade_delta(44_100.0),
            age: 0,
        };
        (
            audio,
            super::SamplerControl::new(shared, command_tx, retired_rx),
        )
    }

    fn apply_commands(&mut self) {
        loop {
            // An assign or clear for a pad whose previous sample has not left yet waits, and the
            // commands behind it wait too, so order is kept.
            let blocked = match self.commands.peek() {
                Ok(SamplerCmd::Assign { pad, .. } | SamplerCmd::Clear { pad }) => self
                    .pads
                    .get(usize::from(*pad))
                    .is_some_and(|p| p.limbo.is_some()),
                _ => false,
            };
            if blocked {
                return;
            }
            let Ok(cmd) = self.commands.pop() else {
                return;
            };
            match cmd {
                SamplerCmd::Assign { pad, sample } => {
                    if let Some(slot) = self.pads.get_mut(usize::from(pad)) {
                        slot.limbo = slot.sample.replace(sample);
                    }
                }
                SamplerCmd::Clear { pad } => {
                    let pad = usize::from(pad);
                    if let Some(slot) = self.pads.get_mut(pad) {
                        slot.limbo = slot.sample.take();
                        self.release_pad(pad, true);
                    }
                }
                SamplerCmd::Trigger {
                    pad,
                    velocity,
                    mode,
                } => self.trigger(usize::from(pad), velocity, mode),
                SamplerCmd::Release { pad } => self.release_pad(usize::from(pad), false),
                SamplerCmd::StopAll => {
                    for v in &mut self.voices {
                        begin_release(v, self.fade_delta);
                    }
                }
            }
        }
    }

    fn trigger(&mut self, pad: usize, velocity: f32, mode: Option<PadMode>) {
        let Some(slot) = self.pads.get(pad) else {
            return;
        };
        let Some(sample) = slot.sample.clone() else {
            return;
        };
        let range = pad * SLOTS_PER_PAD..(pad + 1) * SLOTS_PER_PAD;
        // Steal the oldest sounding voice when the pad is full: it fades out, the new one fades in.
        let sounding = self.voices[range.clone()]
            .iter()
            .filter(|v| v.sounding())
            .count();
        if sounding >= VOICES_PER_PAD {
            if let Some(oldest) = self.voices[range.clone()]
                .iter_mut()
                .filter(|v| v.sounding())
                .min_by_key(|v| v.age)
            {
                begin_release(oldest, self.fade_delta);
            }
        }
        let Some(free) = self.voices[range].iter_mut().find(|v| v.sample.is_none()) else {
            // Every slot is busy or waiting to hand its sample back: drop the trigger and count it.
            self.shared.dropped.fetch_add(1, Relaxed);
            return;
        };
        self.age += 1;
        let mode = mode.unwrap_or_else(|| PadMode::from_u8(self.shared.mode[pad].load(Relaxed)));
        *free = Voice {
            sample: Some(sample),
            active: true,
            mode,
            releasing: false,
            pos: 0.0,
            velocity: velocity.clamp(0.0, 1.0),
            env: 0.0,
            env_delta: 1.0 / START_RAMP_FRAMES,
            age: self.age,
        };
    }

    /// Release (gate/loop voices fade out; one-shots ignore it). `all` releases every voice of the
    /// pad whatever its mode.
    fn release_pad(&mut self, pad: usize, all: bool) {
        let range = pad * SLOTS_PER_PAD..(pad + 1) * SLOTS_PER_PAD;
        for v in &mut self.voices[range] {
            if all || v.mode != PadMode::OneShot {
                begin_release(v, self.fade_delta);
            }
        }
    }

    /// Hands finished voices' samples and replaced pad samples back to the control side. What does
    /// not fit stays put and is retried next block.
    fn flush_retired(&mut self) {
        for v in &mut self.voices {
            if !v.active && v.sample.is_some() {
                retire(&mut self.retired, &mut v.sample);
            }
        }
        for p in &mut self.pads {
            if p.limbo.is_some() {
                retire(&mut self.retired, &mut p.limbo);
            }
        }
    }

    /// Mixes all voices into `left` and `right`.
    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        let n = left.len().min(right.len());
        if n == 0 {
            return;
        }
        let bus = self.shared.master_gain.load().clamp(0.0, MAX_MASTER_GAIN);
        let mut voices_active = 0_u32;
        for pad in 0..PADS {
            let gain = self.shared.gain[pad].load().clamp(0.0, MAX_PAD_GAIN) * bus;
            let pitch = self.shared.pitch[pad]
                .load()
                .clamp(-MAX_PITCH_SEMITONES, MAX_PITCH_SEMITONES);
            let ratio = (f64::from(pitch) / 12.0).exp2();
            let g0 = self.pads[pad].applied_gain;
            let dg = (gain - g0) / n as f32;
            let mut count = 0_u8;
            for v in &mut self.voices[pad * SLOTS_PER_PAD..(pad + 1) * SLOTS_PER_PAD] {
                if v.active {
                    render_voice(
                        v,
                        &mut left[..n],
                        &mut right[..n],
                        g0,
                        dg,
                        ratio,
                        self.engine_rate,
                    );
                    if v.active {
                        count += 1;
                    }
                }
            }
            self.pads[pad].applied_gain = gain;
            self.shared.playing[pad].store(count, Relaxed);
            voices_active += u32::from(count);
        }
        self.shared.voices_active.store(voices_active, Relaxed);
    }
}

impl ProcessStage for SamplerAudio {
    fn prepare(&mut self, sample_rate: f64, _max_block_size: usize) {
        if sample_rate.is_finite() && sample_rate > 0.0 {
            self.engine_rate = sample_rate;
            self.fade_delta = fade_delta(sample_rate);
            self.shared.sample_rate.store(sample_rate);
        }
    }

    /// Adds the sampler to `left` and `right`.
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        // Flush before the commands so finished voices free their slots for a trigger in this
        // block, and after so a pad sample just replaced or cleared leaves at once.
        self.flush_retired();
        self.apply_commands();
        self.flush_retired();
        self.render(left, right);
    }
}

fn fade_delta(rate: f64) -> f32 {
    (1.0 / (FADE_SECONDS * rate).max(1.0)) as f32
}

fn retire(ring: &mut rtrb::Producer<Arc<Sample>>, held: &mut Option<Arc<Sample>>) {
    if let Some(sample) = held.take() {
        if let Err(PushError::Full(sample)) = ring.push(sample) {
            *held = Some(sample);
        }
    }
}

/// Starts the fade-out from the current envelope. A voice that is already fading keeps going.
fn begin_release(v: &mut Voice, fade_delta: f32) {
    if v.active && !v.releasing {
        v.releasing = true;
        // The envelope may still be on its way up: fade from where it is.
        v.env_delta = -fade_delta;
        if v.env <= 0.0 {
            v.active = false;
        }
    }
}

/// Catmull-Rom cubic through `p1` and `p2` at fraction `t`; `t == 0` returns `p1` exactly.
#[inline]
fn cubic(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    p1 + 0.5
        * t
        * (p2 - p0 + t * (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3 + t * (3.0 * (p1 - p2) + p3 - p0)))
}

fn render_voice(
    v: &mut Voice,
    left: &mut [f32],
    right: &mut [f32],
    g0: f32,
    dg: f32,
    ratio: f64,
    engine_rate: f64,
) {
    let Some(sample) = v.sample.as_ref() else {
        v.active = false;
        return;
    };
    let data: &[f32] = sample.data();
    let len = data.len() / 2;
    let looping = v.mode == PadMode::Loop;
    let step = ratio * sample.sample_rate() / engine_rate;
    let fetch = |idx: isize| -> (f32, f32) {
        let i = if looping {
            idx.rem_euclid(len as isize) as usize
        } else if idx < 0 {
            0
        } else if idx as usize >= len {
            return (0.0, 0.0);
        } else {
            idx as usize
        };
        (data[i * 2], data[i * 2 + 1])
    };
    for (i, (l, r)) in left.iter_mut().zip(right.iter_mut()).enumerate() {
        if v.env_delta != 0.0 {
            v.env += v.env_delta;
            if v.env_delta > 0.0 {
                if v.env >= 1.0 {
                    v.env = 1.0;
                    v.env_delta = 0.0;
                }
            } else if v.env <= 0.0 {
                v.env = 0.0;
                v.active = false;
                return;
            }
        }
        if v.pos >= len as f64 {
            if looping {
                v.pos %= len as f64;
            } else {
                v.active = false;
                return;
            }
        }
        let base = v.pos.floor();
        let t = (v.pos - base) as f32;
        let b = base as isize;
        let (l0, r0) = fetch(b - 1);
        let (l1, r1) = fetch(b);
        let (l2, r2) = fetch(b + 1);
        let (l3, r3) = fetch(b + 2);
        let (sl, sr) = if t == 0.0 {
            (l1, r1)
        } else {
            (cubic(l0, l1, l2, l3, t), cubic(r0, r1, r2, r3, t))
        };
        let gain = (g0 + dg * (i as f32 + 1.0)) * v.env * v.velocity;
        *l += sl * gain;
        *r += sr * gain;
        v.pos += step;
    }
}

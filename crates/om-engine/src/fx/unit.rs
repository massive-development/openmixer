//! One FX slot: wraps an `Effect` with enable, wet/dry mix, tempo and tail handling. Usable alone
//! as a `ProcessStage` (channel/master insert) or inside an `FxRack`.
//!
//! Threads: `set_parameter`/`set_*` on the control thread (atomics, so `&self`); `prepare` before
//! insertion; `process` on the audio thread (fixed-size scratch, no allocation, locks or I/O).

use super::effect::{Effect, FxContext, ParamControl, MAX_BPM, MIN_BPM};
use super::math::{AtomicF32, AtomicF64, Smoother};
use super::wipe::SLICE;
use crate::ProcessStage;
use om_core::ParameterTarget;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const RELAXED: Ordering = Ordering::Relaxed;
const ENABLE_RAMP_SEC: f64 = 0.01;
const MIX_RAMP_SEC: f64 = 0.02;
const TEMPO_RAMP_SEC: f64 = 0.05;
const SILENCE: f32 = 1.0e-5; // -100 dBFS: a tail below this is finished

/// Samples per process chunk; the wet scratch below is sized for it.
pub const CHUNK: usize = 64;

fn samples_per_beat(sample_rate: f64, bpm: f64) -> f64 {
    sample_rate * 60.0 / bpm
}

/// Control-written, audio-read state. Shared by `Arc` with [`FxUnitControl`].
#[derive(Debug)]
struct UnitShared {
    enabled: AtomicBool,
    active: AtomicBool,
    mix_target: AtomicF32,
    bpm: AtomicF64,
}

pub struct FxUnit {
    effect: Box<dyn Effect>,
    shared: Arc<UnitShared>,

    ctx: FxContext,
    enable_ramp: Smoother,
    mix_ramp: Smoother,
    beat_samples: Smoother,
    wet_l: [f32; CHUNK],
    wet_r: [f32; CHUNK],
    on_gain: [f32; CHUNK],
    mix_gain: [f32; CHUNK],
    quiet_samples: usize,
    idle: bool,
    clearing: bool, // stale buffer is still being wiped in slices
}

impl FxUnit {
    pub const CHUNK: usize = CHUNK;
    /// Frames wiped per chunk (idle or engaged) after a tail ends.
    pub const CLEAR_SLICE: usize = SLICE;

    pub fn new(effect: Box<dyn Effect>) -> Self {
        Self {
            effect,
            shared: Arc::new(UnitShared {
                enabled: AtomicBool::new(false),
                active: AtomicBool::new(false),
                mix_target: AtomicF32::new(1.0),
                bpm: AtomicF64::new(120.0),
            }),
            ctx: FxContext::default(),
            enable_ramp: Smoother::default(),
            mix_ramp: Smoother::default(),
            beat_samples: Smoother::default(),
            wet_l: [0.0; CHUNK],
            wet_r: [0.0; CHUNK],
            on_gain: [0.0; CHUNK],
            mix_gain: [0.0; CHUNK],
            quiet_samples: 0,
            idle: true,
            clearing: false,
        }
    }

    pub fn set_enabled(&self, on: bool) {
        self.shared.enabled.store(on, RELAXED);
    }

    pub fn set_mix(&self, mix: f32) {
        if mix.is_finite() {
            self.shared.mix_target.store(mix.clamp(0.0, 1.0));
        }
    }

    pub fn set_tempo(&self, bpm: f64) {
        if bpm.is_finite() {
            self.shared.bpm.store(bpm.clamp(MIN_BPM, MAX_BPM));
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.shared.enabled.load(RELAXED)
    }

    /// False once a disabled unit has finished its tail and stopped processing.
    pub fn is_active(&self) -> bool {
        self.shared.active.load(RELAXED)
    }

    pub fn mix(&self) -> f32 {
        self.shared.mix_target.load()
    }

    pub fn tempo(&self) -> f64 {
        self.shared.bpm.load()
    }

    /// Control-thread handle that keeps driving this unit after it has been moved into a chain.
    pub fn control(&self) -> FxUnitControl {
        FxUnitControl {
            shared: Arc::clone(&self.shared),
            params: self.effect.params().control(),
            effect_name: self.effect.name(),
        }
    }

    pub fn effect(&self) -> &dyn Effect {
        self.effect.as_ref()
    }

    pub fn effect_mut(&mut self) -> &mut dyn Effect {
        self.effect.as_mut()
    }

    fn slice_clear(&mut self, frames: usize) {
        if self.clearing {
            self.clearing = !self.effect.clear_some(frames);
        }
    }

    fn compute_gains(&mut self, n: usize, on: bool) {
        let enable_target = if on { 1.0 } else { 0.0 };
        let mix = self.mix();
        for (g, m) in self.on_gain[..n]
            .iter_mut()
            .zip(self.mix_gain[..n].iter_mut())
        {
            *g = self.enable_ramp.step(enable_target);
            *m = self.mix_ramp.step(mix);
        }
    }

    fn mix_into(&self, left: &mut [f32], right: &mut [f32]) {
        let tail = self.effect.has_tail();
        for i in 0..left.len() {
            let g = self.on_gain[i];
            let m = self.mix_gain[i];
            let dry = 1.0 - m * g;
            let wet = m * if tail { 1.0 } else { g };
            left[i] = left[i] * dry + self.wet_l[i] * wet;
            right[i] = right[i] * dry + self.wet_r[i] * wet;
        }
    }

    fn wet_peak(&self, n: usize) -> f32 {
        let mut peak = 0.0_f32;
        for i in 0..n {
            peak = peak.max(self.wet_l[i].abs()).max(self.wet_r[i].abs());
        }
        peak
    }

    fn update_idle(&mut self, n: usize, on: bool) {
        if on || self.enable_ramp.current() > 0.0 {
            self.quiet_samples = 0;
            return;
        }
        if self.effect.has_tail() {
            self.quiet_samples = if self.wet_peak(n) < SILENCE {
                self.quiet_samples + n
            } else {
                0
            };
            if self.quiet_samples < self.effect.tail_silence_window(&self.ctx) {
                return;
            }
        }
        self.effect.begin_reset();
        self.clearing = true;
        self.idle = true;
        self.shared.active.store(false, RELAXED);
    }

    fn process_chunk(&mut self, left: &mut [f32], right: &mut [f32]) {
        let n = left.len();
        let on = self.is_enabled();
        if self.idle && !on {
            return self.slice_clear(SLICE);
        }
        if self.idle {
            self.idle = false;
            self.quiet_samples = 0;
            self.shared.active.store(true, RELAXED);
        }
        self.slice_clear(SLICE); // an early re-engage keeps wiping in slices
        let spb = samples_per_beat(self.ctx.sample_rate, self.tempo());
        self.ctx.samples_per_beat = f64::from(self.beat_samples.step(spb as f32));
        self.compute_gains(n, on);
        // Tail effects stop receiving input when disabled but keep ringing out.
        let tail = self.effect.has_tail();
        for i in 0..n {
            let send = if tail { self.on_gain[i] } else { 1.0 };
            self.wet_l[i] = left[i] * send;
            self.wet_r[i] = right[i] * send;
        }
        self.effect
            .process(&mut self.wet_l[..n], &mut self.wet_r[..n], &self.ctx);
        self.mix_into(left, right);
        self.update_idle(n, on);
    }
}

impl ParameterTarget for FxUnit {
    /// Ids: "enable" (>= 0.5 on), "mix" (0..1), "bpm", plus the effect's own names (e.g. "time").
    /// Returns `false` for unknown ids and non-finite values.
    fn set_parameter(&self, id: &str, value: f64) -> bool {
        self.control().set_parameter(id, value)
    }
}

/// Control-thread handle to one [`FxUnit`]: same ids, same atomics. Cheap to clone.
#[derive(Debug, Clone)]
pub struct FxUnitControl {
    shared: Arc<UnitShared>,
    params: ParamControl,
    effect_name: &'static str,
}

impl FxUnitControl {
    pub fn effect_name(&self) -> &'static str {
        self.effect_name
    }

    pub fn set_enabled(&self, on: bool) {
        self.shared.enabled.store(on, RELAXED);
    }

    pub fn set_mix(&self, mix: f32) {
        if mix.is_finite() {
            self.shared.mix_target.store(mix.clamp(0.0, 1.0));
        }
    }

    pub fn set_tempo(&self, bpm: f64) {
        if bpm.is_finite() {
            self.shared.bpm.store(bpm.clamp(MIN_BPM, MAX_BPM));
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.shared.enabled.load(RELAXED)
    }

    /// See [`FxUnit::is_active`].
    pub fn is_active(&self) -> bool {
        self.shared.active.load(RELAXED)
    }

    pub fn mix(&self) -> f32 {
        self.shared.mix_target.load()
    }

    pub fn tempo(&self) -> f64 {
        self.shared.bpm.load()
    }

    /// Descriptors and target values of the effect's own parameters.
    pub fn params(&self) -> &ParamControl {
        &self.params
    }
}

impl ParameterTarget for FxUnitControl {
    /// Ids: "enable" (>= 0.5 on), "mix" (0..1), "bpm", plus the effect's own names (e.g. "time").
    /// Returns `false` for unknown ids and non-finite values.
    fn set_parameter(&self, id: &str, value: f64) -> bool {
        match id {
            "enable" => {
                self.set_enabled(value >= 0.5);
                true
            }
            "mix" => {
                self.set_mix(value as f32);
                true
            }
            "bpm" => {
                self.set_tempo(value);
                true
            }
            _ => match self.params.find(id) {
                Some(index) => self.params.set(index, value as f32),
                None => false,
            },
        }
    }
}

impl ProcessStage for FxUnit {
    fn prepare(&mut self, sample_rate: f64, _max_block_size: usize) {
        let on = self.is_enabled();
        self.ctx.sample_rate = sample_rate;
        self.ctx.samples_per_beat = samples_per_beat(sample_rate, self.tempo());
        self.enable_ramp.set_time(sample_rate, ENABLE_RAMP_SEC);
        self.mix_ramp.set_time(sample_rate, MIX_RAMP_SEC);
        self.beat_samples
            .set_time(sample_rate / CHUNK as f64, TEMPO_RAMP_SEC);
        self.enable_ramp.snap(if on { 1.0 } else { 0.0 });
        self.mix_ramp.snap(self.mix());
        self.beat_samples.snap(self.ctx.samples_per_beat as f32);
        self.effect.prepare(sample_rate, CHUNK);
        self.idle = !on;
        self.clearing = false;
        self.quiet_samples = 0;
        self.shared.active.store(on, RELAXED);
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        debug_assert_eq!(left.len(), right.len());
        let total = left.len();
        for offset in (0..total).step_by(CHUNK) {
            let end = (offset + CHUNK).min(total);
            self.process_chunk(&mut left[offset..end], &mut right[offset..end]);
        }
    }
}

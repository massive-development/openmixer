//! The `Effect` trait: declares parameters, smooths them and renders a wet signal. Threads: the
//! `set_param` family is called on the control thread; `prepare` before insertion; `reset`,
//! `process` and `clear_some` on the audio thread (no allocation, locks or I/O).

use super::math::{AtomicF32, Smoother};
use std::sync::Arc;

/// Tempo range the FX buffers are sized for; beat-synced times are clamped to it.
pub const MIN_BPM: f64 = 50.0;
pub const MAX_BPM: f64 = 250.0;

/// Parameters per effect (C++ `Effect::kMaxParams`).
pub const MAX_PARAMS: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FxContext {
    pub sample_rate: f64,
    pub samples_per_beat: f64,
}

impl Default for FxContext {
    fn default() -> Self {
        Self {
            sample_rate: 44100.0,
            samples_per_beat: 22050.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParamSpec {
    pub name: &'static str,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    /// 0 = jump instantly (step-like controls).
    pub smooth_seconds: f64,
}

impl ParamSpec {
    pub const EMPTY: Self = Self {
        name: "",
        min: 0.0,
        max: 1.0,
        default: 0.0,
        smooth_seconds: 0.02,
    };

    pub const fn new(name: &'static str, min: f32, max: f32, default: f32) -> Self {
        Self {
            name,
            min,
            max,
            default,
            smooth_seconds: 0.02,
        }
    }

    pub const fn smooth(self, seconds: f64) -> Self {
        Self {
            smooth_seconds: seconds,
            ..self
        }
    }
}

/// Declared parameters with atomic targets (control thread) and per-sample smoothers (audio thread).
#[derive(Debug)]
pub struct ParamSet {
    specs: [ParamSpec; MAX_PARAMS],
    targets: Arc<[AtomicF32; MAX_PARAMS]>,
    block_targets: [f32; MAX_PARAMS],
    smoothers: [Smoother; MAX_PARAMS],
    len: usize,
}

impl ParamSet {
    pub fn new(specs: &[ParamSpec]) -> Self {
        let mut set = Self {
            specs: [ParamSpec::EMPTY; MAX_PARAMS],
            targets: Arc::new(std::array::from_fn(|_| AtomicF32::new(0.0))),
            block_targets: [0.0; MAX_PARAMS],
            smoothers: [Smoother::default(); MAX_PARAMS],
            len: 0,
        };
        for spec in specs.iter().take(MAX_PARAMS) {
            set.specs[set.len] = *spec;
            set.targets[set.len].store(spec.default);
            set.len += 1;
        }
        set
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Clamped like the C++ `getParamSpec`: out-of-range indices map to the nearest slot.
    pub fn spec(&self, index: usize) -> &ParamSpec {
        &self.specs[index.min(MAX_PARAMS - 1)]
    }

    pub fn find(&self, name: &str) -> Option<usize> {
        self.specs[..self.len].iter().position(|s| s.name == name)
    }

    /// Clamps to the declared range. Returns `false` for an unknown index or a non-finite value.
    pub fn set(&self, index: usize, value: f32) -> bool {
        store_clamped(&self.specs, self.len, &self.targets, index, value)
    }

    /// Control-thread view that shares the target cells, so a unit that has been moved into the
    /// audio chain can still be driven by name.
    pub fn control(&self) -> ParamControl {
        ParamControl {
            specs: self.specs,
            len: self.len,
            targets: Arc::clone(&self.targets),
        }
    }

    /// Current control-thread target; 0 for an unknown index.
    pub fn target(&self, index: usize) -> f32 {
        if index >= self.len {
            0.0
        } else {
            self.targets[index].load()
        }
    }

    /// Control thread (prepare): sets smoothing times and snaps to the current targets.
    pub fn prepare(&mut self, sample_rate: f64) {
        for i in 0..self.len {
            self.smoothers[i].set_time(sample_rate, self.specs[i].smooth_seconds);
            self.smoothers[i].snap(self.targets[i].load());
        }
    }

    /// Audio thread, once per `process` call: freezes this block's targets.
    pub fn latch(&mut self) {
        for i in 0..self.len {
            self.block_targets[i] = self.targets[i].load();
        }
    }

    /// Advance all smoothers by one sample; call once per rendered sample.
    pub fn tick(&mut self) {
        for i in 0..self.len {
            self.smoothers[i].step(self.block_targets[i]);
        }
    }

    /// Smoothed value of parameter `index` for the current sample.
    pub fn value(&self, index: usize) -> f32 {
        self.smoothers[index].current()
    }
}

fn store_clamped(
    specs: &[ParamSpec],
    len: usize,
    targets: &[AtomicF32; MAX_PARAMS],
    index: usize,
    value: f32,
) -> bool {
    if index >= len || !value.is_finite() {
        return false;
    }
    let s = &specs[index];
    targets[index].store(value.clamp(s.min, s.max));
    true
}

/// Control-thread handle to a [`ParamSet`]'s names, ranges and target cells. Clones share state.
#[derive(Debug, Clone)]
pub struct ParamControl {
    specs: [ParamSpec; MAX_PARAMS],
    len: usize,
    targets: Arc<[AtomicF32; MAX_PARAMS]>,
}

impl ParamControl {
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn find(&self, name: &str) -> Option<usize> {
        self.specs[..self.len].iter().position(|s| s.name == name)
    }

    /// Same contract as [`ParamSet::set`].
    pub fn set(&self, index: usize, value: f32) -> bool {
        store_clamped(&self.specs, self.len, &self.targets, index, value)
    }
}

pub trait Effect: Send + Sync {
    fn name(&self) -> &'static str;
    fn params(&self) -> &ParamSet;
    fn params_mut(&mut self) -> &mut ParamSet;

    /// Tail effects (echo, reverb) keep sounding after disable until they fall silent.
    fn has_tail(&self) -> bool {
        false
    }

    /// Continuous wet silence (samples) after which a tail counts as finished.
    fn tail_silence_window(&self, _ctx: &FxContext) -> usize {
        0
    }

    /// Effect-specific buffer sizing; called from `prepare` before `reset`.
    fn prepare_effect(&mut self, _sample_rate: f64, _max_block_size: usize) {}

    fn reset(&mut self);

    /// When a unit goes idle: reset small state now and leave big buffers to `clear_some` so no
    /// callback pays for a large memset. Defaults to a full reset.
    fn begin_reset(&mut self) {
        self.reset();
    }

    /// Wipes up to `max_frames` of stale buffer (bounded, shared across all buffers); true when
    /// clean. Until clean, stale slots must read as zero so the effect may run (see `wipe`).
    fn clear_some(&mut self, _max_frames: usize) -> bool {
        true
    }

    /// Render `left`/`right` in place: they hold the input and receive the fully wet output.
    fn render(&mut self, left: &mut [f32], right: &mut [f32], ctx: &FxContext);

    /// Control thread, before insertion: prepares smoothers and buffers, then resets.
    fn prepare(&mut self, sample_rate: f64, max_block_size: usize) {
        self.params_mut().prepare(sample_rate);
        self.prepare_effect(sample_rate, max_block_size);
        self.reset();
    }

    /// Audio thread: latches parameter targets for this block, then renders.
    fn process(&mut self, left: &mut [f32], right: &mut [f32], ctx: &FxContext) {
        self.params_mut().latch();
        self.render(left, right, ctx);
    }

    fn num_params(&self) -> usize {
        self.params().len()
    }

    fn param_spec(&self, index: usize) -> &ParamSpec {
        self.params().spec(index)
    }

    fn find_param(&self, name: &str) -> Option<usize> {
        self.params().find(name)
    }

    fn set_param(&self, index: usize, value: f32) -> bool {
        self.params().set(index, value)
    }

    fn param_target(&self, index: usize) -> f32 {
        self.params().target(index)
    }
}

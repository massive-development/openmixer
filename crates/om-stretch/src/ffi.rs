//! The only `unsafe` code in om-stretch: calls into Rubber Band's C API through `rubberband-sys`.
//!
//! Why this is not the safe `rubberband` crate: its `process`, `study` and `retrieve` collect a
//! `Vec` of channel pointers on every call, which allocates on the audio thread. Here the pointer
//! arrays are allocated once in [`Engine::new`] and refilled in place, so the per-block calls
//! allocate nothing on this side. Rubber Band's own allocations are a separate question (see the
//! crate docs and `docs/spikes/stretch.md`).

#![allow(unsafe_code)]

use rubberband_sys as sys;
use std::ptr::NonNull;

/// Real-time R3 ("finer") engine options, plus
/// `THREADING_NEVER`: the R3 engine has no threaded path, so the flag only forbids the R2 engine
/// from spawning threads if it were ever selected.
pub(crate) const REALTIME_R3_OPTIONS: sys::RubberBandOptions =
    (sys::RubberBandOption_RubberBandOptionProcessRealTime
        | sys::RubberBandOption_RubberBandOptionEngineFiner
        | sys::RubberBandOption_RubberBandOptionChannelsTogether
        | sys::RubberBandOption_RubberBandOptionPitchHighConsistency
        | sys::RubberBandOption_RubberBandOptionThreadingNever) as sys::RubberBandOptions;

/// One Rubber Band stretcher and the pointer and silence buffers its calls need.
pub(crate) struct Engine {
    state: NonNull<sys::RubberBandState_>,
    channels: usize,
    in_ptrs: Vec<*const f32>,
    out_ptrs: Vec<*mut f32>,
    /// Source of the channel pointers for [`Engine::process_silence`]. Never written.
    silence: Vec<f32>,
}

// SAFETY: the engine is owned by exactly one `Engine` value and every mutation goes through
// `&mut self`. Rubber Band keeps no thread-local state for an instance, so moving it to another
// thread is sound; the audio thread is the only caller after `prepare`.
unsafe impl Send for Engine {}

impl Engine {
    /// Creates a real-time stretcher for `channels` and sets its largest process call to
    /// `max_process` frames. Allocates; control thread only.
    pub(crate) fn new(sample_rate: u32, channels: usize, max_process: usize) -> Option<Self> {
        // SAFETY: plain constructor; the returned state is owned by the result and freed in `Drop`.
        let raw = unsafe {
            sys::rubberband_new(sample_rate, channels as u32, REALTIME_R3_OPTIONS, 1.0, 1.0)
        };
        let state = NonNull::new(raw)?;
        // SAFETY: `state` is a live engine created above.
        unsafe { sys::rubberband_set_max_process_size(state.as_ptr(), max_process as u32) };
        Some(Self {
            state,
            channels,
            in_ptrs: Vec::with_capacity(channels),
            out_ptrs: Vec::with_capacity(channels),
            silence: vec![0.0; max_process],
        })
    }

    pub(crate) fn reset(&mut self) {
        // SAFETY: live engine, exclusive access.
        unsafe { sys::rubberband_reset(self.state.as_ptr()) }
    }

    pub(crate) fn set_time_ratio(&mut self, ratio: f64) {
        // SAFETY: live engine, exclusive access.
        unsafe { sys::rubberband_set_time_ratio(self.state.as_ptr(), ratio) }
    }

    pub(crate) fn set_pitch_scale(&mut self, scale: f64) {
        // SAFETY: live engine, exclusive access.
        unsafe { sys::rubberband_set_pitch_scale(self.state.as_ptr(), scale) }
    }

    pub(crate) fn time_ratio(&self) -> f64 {
        // SAFETY: live engine; the getter only reads.
        unsafe { sys::rubberband_get_time_ratio(self.state.as_ptr()) }
    }

    pub(crate) fn pitch_scale(&self) -> f64 {
        // SAFETY: live engine; the getter only reads.
        unsafe { sys::rubberband_get_pitch_scale(self.state.as_ptr()) }
    }

    pub(crate) fn start_pad(&self) -> usize {
        // SAFETY: live engine; the getter only reads.
        unsafe { sys::rubberband_get_preferred_start_pad(self.state.as_ptr()) as usize }
    }

    pub(crate) fn start_delay(&self) -> usize {
        // SAFETY: live engine; the getter only reads.
        unsafe { sys::rubberband_get_start_delay(self.state.as_ptr()) as usize }
    }

    pub(crate) fn latency(&self) -> usize {
        // SAFETY: live engine; the getter only reads.
        unsafe { sys::rubberband_get_latency(self.state.as_ptr()) as usize }
    }

    pub(crate) fn samples_required(&self) -> usize {
        // SAFETY: live engine; the getter only reads.
        unsafe { sys::rubberband_get_samples_required(self.state.as_ptr()) as usize }
    }

    pub(crate) fn available(&self) -> usize {
        // SAFETY: live engine; the getter only reads. Rubber Band reports -1 on error.
        unsafe { sys::rubberband_available(self.state.as_ptr()).max(0) as usize }
    }

    /// Feeds `frames` frames of `input`, starting at `start` in each channel slice. The caller
    /// guarantees one slice per channel, each at least `start + frames` long, and
    /// `frames <= max_process`.
    pub(crate) fn process(&mut self, input: &[&[f32]], start: usize, frames: usize) {
        debug_assert_eq!(input.len(), self.channels);
        self.in_ptrs.clear();
        self.in_ptrs
            .extend(input.iter().map(|channel| channel[start..].as_ptr()));
        // SAFETY: one pointer per channel, each valid for `frames` reads: the caller's slices
        // cover `start..start + frames`. The pointer array lives until the call returns.
        unsafe {
            sys::rubberband_process(self.state.as_ptr(), self.in_ptrs.as_ptr(), frames as u32, 0)
        };
    }

    /// Feeds `frames` frames of silence. The caller guarantees `frames <= max_process`.
    pub(crate) fn process_silence(&mut self, frames: usize) {
        self.in_ptrs.clear();
        for _ in 0..self.channels {
            self.in_ptrs.push(self.silence.as_ptr());
        }
        // SAFETY: every pointer addresses `silence`, which holds `max_process` zeros, and the
        // caller guarantees `frames <= max_process`.
        unsafe {
            sys::rubberband_process(self.state.as_ptr(), self.in_ptrs.as_ptr(), frames as u32, 0)
        };
    }

    /// Copies up to `min(available, shortest output slice)` frames into `output`. Returns the
    /// number written.
    pub(crate) fn retrieve(&mut self, output: &mut [&mut [f32]]) -> usize {
        // Rubber Band writes one buffer per engine channel. Any other count would make it read
        // pointers past `out_ptrs` or write past the caller's slices, so refuse the call.
        if output.len() != self.channels {
            return 0;
        }
        let limit = output.iter().map(|c| c.len()).min().unwrap_or(0);
        let frames = limit.min(self.available());
        if frames == 0 {
            return 0;
        }
        self.out_ptrs.clear();
        self.out_ptrs
            .extend(output.iter_mut().map(|channel| channel.as_mut_ptr()));
        // SAFETY: one pointer per channel, each valid for `frames` writes (`frames` is bounded by
        // every slice's length). The pointer array lives until the call returns.
        let got = unsafe {
            sys::rubberband_retrieve(self.state.as_ptr(), self.out_ptrs.as_ptr(), frames as u32)
        };
        got as usize
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        // SAFETY: `state` came from `rubberband_new` and is deleted exactly once, here.
        unsafe { sys::rubberband_delete(self.state.as_ptr()) }
    }
}

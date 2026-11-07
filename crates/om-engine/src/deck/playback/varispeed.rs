//! Varispeed through rubato (#241): forward playback at a rate other than unity is resampled by a
//! band-limited `Async` resampler instead of linear interpolation. The linear path in `playback.rs`
//! stays for everything rubato cannot do: rate zero, steps outside `MIN_STEP..=MAX_STEP`, exact
//! unity (linear is bit-exact there and has no delay), key-lock priming and decks with no
//! resampler prepared.
//!
//! The resampler has a fixed output chunk (`CHUNK` frames) and a variable input, so source frames
//! are staged until a chunk's worth is available, each chunk is rendered into a small FIFO, and
//! blocks of any length are served from it. The ratio is set once per chunk with a ramp.
//!
//! The filter delays its output by `delay` source frames. The audible position is therefore not
//! the input-side index: every staged frame is recorded with its source index in a history ring,
//! the audible position is tracked as a count of staged frames minus the delay, and the published
//! `position_frames` is the index looked up in that history. That keeps the position right across
//! a loop wrap, where the input index jumps but the filter (which is not reset) carries on.

use super::{clear_tail, Path, Playback};
use crate::deck::shared::Shared;
use audioadapter_buffers::direct::InterleavedSlice;
use rubato::{
    Adjustable, Async, FixedAsync, PolynomialDegree, Resampler, SincInterpolationParameters,
    SincInterpolationType, WindowFunction,
};
use std::sync::atomic::Ordering::Relaxed;

/// Output frames per resampler chunk.
const CHUNK: usize = 64;
/// Frames of staged-input history (power of two). Covers the delay plus the largest input chunk.
const HIST: usize = 512;
const HIST_MASK: i64 = HIST as i64 - 1;
/// Smallest step (source frames per output frame) the rubato path takes. Below 1 the filter only
/// upsamples, which is alias-free; 0.25 leaves room for a 44.1 kHz file on a 96 kHz device at a
/// pitch bend.
pub(super) const MIN_STEP: f64 = 0.25;
/// Largest step the rubato path takes. The filter is built for unity, so larger steps alias the
/// top of the band; 2.0 is the cap.
pub(super) const MAX_STEP: f64 = 2.0;
/// Widest ratio the resampler is built to take, relative to unity, in either direction.
const MAX_RELATIVE: f64 = 1.0 / MIN_STEP;
/// Steps closer to 1 than this play on the linear path, which is exact there.
const UNITY_EPS: f64 = 1e-6;
/// Output frames over which a path switch blends from the last frame of the old path.
const DECLICK: usize = 64;
const SINC_LEN: usize = 128;

/// Interpolation quality of the deck's varispeed. An engine-level setting; there is no UI for it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VarispeedQuality {
    /// Linear interpolation: the cheapest, and the only one with no delay.
    Linear,
    /// Cubic polynomial (rubato), no anti-alias filter: cheap, better than linear below Nyquist/4.
    Polynomial,
    /// Windowed sinc, 128 taps (rubato): the default.
    #[default]
    Sinc,
}

pub(super) struct Varispeed {
    rs: Async<f32>,
    /// Source frames the filter delays its output by.
    delay: f64,
    /// Staged input, interleaved stereo.
    input: Vec<f32>,
    in_fill: usize,
    /// Output FIFO, interleaved stereo, and the part of it not yet served.
    out: Vec<f32>,
    out_pos: usize,
    out_len: usize,
    hist: Vec<[f32; 2]>,
    hist_idx: Vec<i64>,
    /// Frames staged since `begin`; staged frame `c` has history slot `c & HIST_MASK`.
    count: i64,
    /// Frames handed to the resampler since `begin`.
    fed: i64,
    /// Audible position (in staged-frame counts) at the start and end of the chunk in `out`.
    a0: f64,
    a1: f64,
    /// True from `begin` until the first chunk with audible position at or past zero is served:
    /// output before that is the filter ringing in from silence, not signal.
    warming: bool,
    /// Audible position of the last frame served, if any.
    last_pos: Option<f64>,
    ramp_left: usize,
    ramp_from: [f32; 2],
}

impl Varispeed {
    /// Builds the resampler. Allocates; control thread only. `None` for `Linear`, or when rubato
    /// refuses the parameters (logged), in which case the deck stays linear.
    pub(super) fn new(quality: VarispeedQuality) -> Option<Box<Self>> {
        let built = match quality {
            VarispeedQuality::Linear => return None,
            VarispeedQuality::Polynomial => (
                Async::<f32>::new_poly(
                    1.0,
                    MAX_RELATIVE,
                    PolynomialDegree::Cubic,
                    CHUNK,
                    2,
                    FixedAsync::Output,
                ),
                2.0,
            ),
            VarispeedQuality::Sinc => {
                let params = SincInterpolationParameters {
                    sinc_len: SINC_LEN,
                    f_cutoff: None,
                    interpolation: SincInterpolationType::Linear,
                    oversampling_factor: 128,
                    window: WindowFunction::BlackmanHarris2,
                };
                (
                    Async::<f32>::new_sinc(
                        1.0,
                        MAX_RELATIVE,
                        &params,
                        CHUNK,
                        2,
                        FixedAsync::Output,
                    ),
                    SINC_LEN as f64 / 2.0,
                )
            }
        };
        let (rs, delay) = match built {
            (Ok(rs), delay) => (rs, delay),
            (Err(error), _) => {
                tracing::warn!(%error, "varispeed resampler unavailable: deck stays linear");
                return None;
            }
        };
        let max_in = rs.input_frames_max();
        Some(Box::new(Self {
            rs,
            delay,
            input: vec![0.0; 2 * (max_in * 2 + CHUNK)],
            in_fill: 0,
            out: vec![0.0; 2 * CHUNK],
            out_pos: 0,
            out_len: 0,
            hist: vec![[0.0; 2]; HIST],
            hist_idx: vec![0; HIST],
            count: 0,
            fed: 0,
            a0: 0.0,
            a1: 0.0,
            warming: true,
            last_pos: None,
            ramp_left: 0,
            ramp_from: [0.0; 2],
        }))
    }

    /// Starts a clean stream: filter zeroed, nothing staged, nothing to serve.
    fn begin(&mut self) {
        self.rs.reset();
        self.in_fill = 0;
        self.out_pos = 0;
        self.out_len = 0;
        self.count = 0;
        self.fed = 0;
        self.a0 = -self.delay;
        self.a1 = -self.delay;
        self.warming = true;
        self.last_pos = None;
        self.ramp_left = 0;
    }

    fn stage(&mut self, index: i64, frame: [f32; 2]) {
        let slot = (self.count & HIST_MASK) as usize;
        self.hist[slot] = frame;
        self.hist_idx[slot] = index;
        self.count += 1;
        self.input[2 * self.in_fill] = frame[0];
        self.input[2 * self.in_fill + 1] = frame[1];
        self.in_fill += 1;
    }

    /// Renders the next chunk into `out`. False when the source ran dry before it could.
    fn produce(&mut self, pb: &mut Playback, sh: &Shared, step: f64) -> bool {
        // In range by construction (the caller checked the step against MIN_STEP..=MAX_STEP).
        let _ = self.rs.set_resample_ratio(1.0 / step, true);
        let need = self.rs.input_frames_next();
        while self.in_fill < need {
            let Some((index, frame)) = pb.next_source_frame(sh) else {
                return false;
            };
            self.stage(index, frame);
        }
        let Ok(src) = InterleavedSlice::new(&self.input[..2 * need], 2, need) else {
            return false;
        };
        let Ok(mut dst) = InterleavedSlice::new_mut(&mut self.out[..2 * CHUNK], 2, CHUNK) else {
            return false;
        };
        let Ok((used, made)) = self.rs.process_into_buffer(&src, &mut dst, None) else {
            return false;
        };
        self.input.copy_within(2 * used..2 * self.in_fill, 0);
        self.in_fill -= used;
        self.fed += used as i64;
        self.a0 = self.a1;
        self.a1 = self.fed as f64 - self.delay;
        self.out_len = made;
        self.out_pos = 0;
        if self.warming {
            if self.a1 <= 0.0 {
                self.out_pos = made;
            } else {
                if self.a0 < 0.0 {
                    let share = -self.a0 / (self.a1 - self.a0);
                    self.out_pos = ((share * made as f64).ceil() as usize).min(made);
                }
                self.warming = false;
            }
        }
        true
    }

    /// Audible position of output frame `j` of the chunk in `out`.
    fn position_of(&self, j: usize) -> f64 {
        self.a0 + (self.a1 - self.a0) * j as f64 / CHUNK as f64
    }

    /// Source index of the staged frame nearest below `pos` (a count), clamped to what is held.
    fn index_at(&self, pos: f64) -> Option<i64> {
        if self.count == 0 {
            return None;
        }
        let oldest = (self.count - HIST as i64).max(0);
        let c = (pos.floor() as i64).clamp(oldest, self.count - 1);
        Some(self.hist_idx[(c & HIST_MASK) as usize])
    }

    fn render(
        &mut self,
        pb: &mut Playback,
        sh: &Shared,
        left: &mut [f32],
        right: &mut [f32],
        step: f64,
    ) {
        let total = left.len();
        let mut done = 0;
        while done < total {
            if self.out_pos >= self.out_len {
                if !self.produce(pb, sh, step) {
                    pb.note_starved(sh);
                    clear_tail(left, right, done);
                    break;
                }
                continue;
            }
            let n = (total - done).min(self.out_len - self.out_pos);
            for k in 0..n {
                let (l, r) = (
                    self.out[2 * (self.out_pos + k)],
                    self.out[2 * (self.out_pos + k) + 1],
                );
                left[done + k] = l;
                right[done + k] = r;
            }
            self.last_pos = Some(self.position_of(self.out_pos + n - 1));
            self.out_pos += n;
            done += n;
        }
        self.apply_declick(&mut left[..done], &mut right[..done]);
        if let Some(index) = self.last_pos.and_then(|p| self.index_at(p)) {
            sh.position_frames.store(index, Relaxed);
        }
    }

    /// Blends the first frames of a block from the old path's last frame to the new signal.
    fn apply_declick(&mut self, left: &mut [f32], right: &mut [f32]) {
        let n = self.ramp_left.min(left.len());
        for k in 0..n {
            let w = 1.0 - (self.ramp_left - k) as f32 / DECLICK as f32;
            left[k] = self.ramp_from[0] * (1.0 - w) + left[k] * w;
            right[k] = self.ramp_from[1] * (1.0 - w) + right[k] * w;
        }
        self.ramp_left -= n;
    }
}

impl Playback {
    /// Builds (or drops) the varispeed resampler for `quality`. Allocates; control thread only,
    /// before audio runs.
    pub(in crate::deck) fn configure_varispeed(&mut self, quality: VarispeedQuality) {
        self.vs = Varispeed::new(quality);
        self.path = Path::Cold;
    }

    /// Whether this block's step is one the rubato path takes.
    pub(super) fn varispeed_applies(&self, step: f64) -> bool {
        self.vs.is_some()
            && (MIN_STEP..=MAX_STEP).contains(&step)
            && (!self.unity_bypass || (step - 1.0).abs() > UNITY_EPS)
    }

    pub(super) fn render_varispeed(
        &mut self,
        sh: &Shared,
        left: &mut [f32],
        right: &mut [f32],
        step: f64,
    ) {
        let Some(mut vs) = self.vs.take() else {
            self.render_interpolated(sh, left, right);
            return;
        };
        if self.path != Path::Rubato {
            self.enter_varispeed(&mut vs);
        }
        self.path = Path::Rubato;
        vs.render(self, sh, left, right, step);
        self.vs = Some(vs);
        self.remember_last_frame(left, right);
    }

    /// Switches from the linear path (or a cold start) to the rubato path. A frame the linear
    /// window pulled but has not played goes in first, and a mid-play switch blends from the last
    /// frame the linear path played.
    fn enter_varispeed(&mut self, vs: &mut Varispeed) {
        vs.begin();
        if self.primed || self.have_first {
            vs.stage(self.next_idx, self.next);
        }
        self.primed = false;
        self.have_first = false;
        if self.path == Path::Linear {
            vs.ramp_left = DECLICK;
            vs.ramp_from = self.last_out;
        }
    }

    /// Switches from the rubato path to the linear one: the frames the filter holds but has not
    /// played are replayed to the interpolator, so the position carries over.
    pub(super) fn leave_varispeed(&mut self) {
        if let Some(vs) = self.vs.as_ref() {
            if self.keylock.lookahead.is_empty() {
                let first = vs.last_pos.map_or(0, |p| p.floor() as i64 + 1);
                let first = first.max(vs.count - HIST as i64).max(0);
                for c in first..vs.count {
                    let slot = (c & HIST_MASK) as usize;
                    self.keylock
                        .lookahead
                        .push(vs.hist[slot], vs.hist_idx[slot]);
                }
            }
        }
        self.primed = false;
        self.have_first = false;
    }

    pub(super) fn remember_last_frame(&mut self, left: &[f32], right: &[f32]) {
        if let (Some(&l), Some(&r)) = (left.last(), right.last()) {
            self.last_out = [l, r];
        }
    }
}

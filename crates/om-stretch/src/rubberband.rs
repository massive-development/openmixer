//! [`Stretcher`] backed by Rubber Band 4.0.0, real-time mode with the R3 ("finer") engine.
//! Port of `src/engine/TimeStretch.cpp`: `max_feed` is `max(4096, 8 * max_block)`, so one call
//! covers `samples_required()` even at extreme ratios.

use crate::ffi::Engine;
use crate::{StretchError, Stretcher, MAX_CHANNELS};

const MIN_SAMPLE_RATE: u32 = 8_000;
const MAX_SAMPLE_RATE: u32 = 192_000;
const MIN_MAX_FEED: usize = 4096;
const MAX_FEED_BLOCKS: usize = 8;

/// Key-lock stretcher. Unprepared until [`Stretcher::prepare`] succeeds; every other method is an
/// inert no-op or returns a zero-like value before then.
#[derive(Default)]
pub struct RubberBandStretcher {
    engine: Option<Engine>,
    channels: usize,
    max_feed: usize,
}

impl RubberBandStretcher {
    /// An unprepared stretcher. Holds no engine and allocates nothing.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Stretcher for RubberBandStretcher {
    fn prepare(
        &mut self,
        sample_rate: u32,
        channels: usize,
        max_block: usize,
    ) -> Result<(), StretchError> {
        if !(MIN_SAMPLE_RATE..=MAX_SAMPLE_RATE).contains(&sample_rate) {
            return Err(StretchError::SampleRate(sample_rate));
        }
        if !(1..=MAX_CHANNELS).contains(&channels) {
            return Err(StretchError::ChannelCount(channels));
        }
        let max_feed = MIN_MAX_FEED.max(max_block.saturating_mul(MAX_FEED_BLOCKS));
        // Drop any earlier engine first so its memory is not held twice.
        self.engine = None;
        self.engine =
            Some(Engine::new(sample_rate, channels, max_feed).ok_or(StretchError::Create)?);
        self.channels = channels;
        self.max_feed = max_feed;
        Ok(())
    }

    fn is_prepared(&self) -> bool {
        self.engine.is_some()
    }

    fn reset(&mut self) {
        if let Some(engine) = self.engine.as_mut() {
            engine.reset();
        }
    }

    fn set_ratios(&mut self, time_ratio: f64, pitch_scale: f64) {
        let Some(engine) = self.engine.as_mut() else {
            return;
        };
        // Compare first, as TimeStretch::setRatios does: Rubber Band re-plans on every set.
        if engine.time_ratio() != time_ratio {
            engine.set_time_ratio(time_ratio);
        }
        if engine.pitch_scale() != pitch_scale {
            engine.set_pitch_scale(pitch_scale);
        }
    }

    fn time_ratio(&self) -> f64 {
        self.engine.as_ref().map_or(1.0, Engine::time_ratio)
    }

    fn pitch_scale(&self) -> f64 {
        self.engine.as_ref().map_or(1.0, Engine::pitch_scale)
    }

    fn start_pad(&self) -> usize {
        self.engine.as_ref().map_or(0, Engine::start_pad)
    }

    fn start_delay(&self) -> usize {
        self.engine.as_ref().map_or(0, Engine::start_delay)
    }

    fn latency(&self) -> usize {
        self.engine.as_ref().map_or(0, Engine::latency)
    }

    fn max_feed(&self) -> usize {
        self.max_feed
    }

    fn samples_required(&self) -> usize {
        self.engine
            .as_ref()
            .map_or(0, |e| e.samples_required().min(self.max_feed))
    }

    fn process(&mut self, input: &[&[f32]]) {
        let max_feed = self.max_feed;
        let Some(engine) = self.engine.as_mut() else {
            return;
        };
        // Callers pass one slice per prepared channel; anything else is a programming error.
        if input.len() != self.channels {
            debug_assert!(
                false,
                "process: {} channels, prepared for {}",
                input.len(),
                self.channels
            );
            return;
        }
        let total = input.iter().map(|c| c.len()).min().unwrap_or(0);
        let mut start = 0;
        while start < total {
            let frames = (total - start).min(max_feed);
            engine.process(input, start, frames);
            start += frames;
        }
    }

    fn process_silence(&mut self, frames: usize) {
        let frames = frames.min(self.max_feed);
        if frames == 0 {
            return;
        }
        if let Some(engine) = self.engine.as_mut() {
            engine.process_silence(frames);
        }
    }

    fn available(&self) -> usize {
        self.engine.as_ref().map_or(0, Engine::available)
    }

    fn retrieve(&mut self, output: &mut [&mut [f32]]) -> usize {
        // Same contract as `process`: one slice per prepared channel, anything else is refused.
        if output.len() != self.channels {
            return 0;
        }
        match self.engine.as_mut() {
            Some(engine) => engine.retrieve(output),
            None => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::TAU;

    const SR: u32 = 44_100;
    const TONE_HZ: f64 = 440.0;
    const BLOCK: usize = 256;

    fn sine(frames: usize) -> Vec<f32> {
        (0..frames)
            .map(|i| (0.5 * (TAU * TONE_HZ * i as f64 / SR as f64).sin()) as f32)
            .collect()
    }

    /// Frequency from rising zero crossings over `x`: the same estimator as DeckKeylockTest.cpp.
    fn zero_crossing_hz(x: &[f32]) -> f64 {
        let mut crossings = Vec::new();
        for i in 1..x.len() {
            if x[i - 1] < 0.0 && x[i] >= 0.0 {
                crossings.push((i - 1) as f64 + f64::from(x[i - 1]) / f64::from(x[i - 1] - x[i]));
            }
        }
        if crossings.len() < 2 {
            return 0.0;
        }
        let span = crossings[crossings.len() - 1] - crossings[0];
        f64::from(SR) * (crossings.len() - 1) as f64 / span
    }

    /// Runs `input` through a stretcher with the TimeStretch priming order (start pad, then the
    /// signal, then a flush) and returns the left channel with the start delay removed.
    fn stretch_mono(input: &[f32], time_ratio: f64) -> Vec<f32> {
        let mut st = RubberBandStretcher::new();
        st.prepare(SR, 2, BLOCK).expect("prepare");
        st.set_ratios(time_ratio, 1.0);
        let mut pad = st.start_pad();
        while pad > 0 {
            let n = pad.min(st.max_feed());
            st.process_silence(n);
            pad -= n;
        }
        let mut delay = st.start_delay();
        let mut out = Vec::new();
        let mut buf_l = vec![0.0_f32; BLOCK * 4];
        let mut buf_r = vec![0.0_f32; BLOCK * 4];
        let mut drain = |st: &mut RubberBandStretcher, out: &mut Vec<f32>, delay: &mut usize| {
            while st.available() > 0 {
                let want = st.available().min(buf_l.len());
                let got = {
                    let mut views: [&mut [f32]; 2] = [&mut buf_l[..want], &mut buf_r[..want]];
                    st.retrieve(&mut views)
                };
                let skip = (*delay).min(got);
                *delay -= skip;
                out.extend_from_slice(&buf_l[skip..got]);
            }
        };
        for block in input.chunks(BLOCK) {
            st.process(&[block, block]);
            drain(&mut st, &mut out, &mut delay);
        }
        // Flush: feed silence until the engine has produced the stretched tail.
        let target = (input.len() as f64 * time_ratio).round() as usize;
        let mut guard = 0;
        while out.len() < target && guard < 10_000 {
            st.process_silence(BLOCK);
            drain(&mut st, &mut out, &mut delay);
            guard += 1;
        }
        out.truncate(target);
        out
    }

    #[test]
    fn unprepared_stretcher_is_inert() {
        let mut st = RubberBandStretcher::new();
        assert!(!st.is_prepared());
        st.set_ratios(0.5, 2.0);
        st.process(&[&[0.0; 8], &[0.0; 8]]);
        st.process_silence(64);
        st.reset();
        assert_eq!(st.available(), 0);
        assert_eq!(st.samples_required(), 0);
        assert_eq!(st.retrieve(&mut [&mut [0.0; 4], &mut [0.0; 4]]), 0);
    }

    #[test]
    fn prepare_rejects_bad_arguments_and_stays_unprepared() {
        let mut st = RubberBandStretcher::new();
        assert_eq!(st.prepare(0, 2, BLOCK), Err(StretchError::SampleRate(0)));
        assert_eq!(st.prepare(SR, 0, BLOCK), Err(StretchError::ChannelCount(0)));
        assert_eq!(
            st.prepare(SR, MAX_CHANNELS + 1, BLOCK),
            Err(StretchError::ChannelCount(MAX_CHANNELS + 1))
        );
        assert!(!st.is_prepared());
    }

    #[test]
    fn prepare_sizes_max_feed_like_time_stretch() {
        let mut st = RubberBandStretcher::new();
        st.prepare(SR, 2, 256).expect("prepare");
        assert_eq!(st.max_feed(), 4096);
        st.prepare(SR, 2, 1024).expect("prepare");
        assert_eq!(st.max_feed(), 8192);
    }

    #[test]
    fn ratios_round_trip_through_the_engine() {
        let mut st = RubberBandStretcher::new();
        st.prepare(SR, 2, BLOCK).expect("prepare");
        st.set_ratios(0.8, 1.25);
        assert_eq!(st.time_ratio(), 0.8);
        assert_eq!(st.pitch_scale(), 1.25);
    }

    #[test]
    fn stretched_sine_keeps_pitch_and_length() {
        // 1.5 s of 440 Hz at tempo 1.25 (time ratio 0.8): same pitch, 0.8 times the frames.
        let input = sine(66_150);
        let out = stretch_mono(&input, 0.8);
        let expected = (input.len() as f64 * 0.8) as usize;
        assert!((out.len() as f64 - expected as f64).abs() < expected as f64 * 0.02);
        assert!(out.iter().all(|v| v.is_finite()));
        let hz = zero_crossing_hz(&out[8192..]);
        assert!((hz - TONE_HZ).abs() < TONE_HZ * 0.01, "pitch {hz} Hz");
    }

    #[test]
    fn reset_drops_pending_output() {
        let mut st = RubberBandStretcher::new();
        st.prepare(SR, 2, BLOCK).expect("prepare");
        let input = sine(4096);
        st.process(&[&input, &input]);
        assert!(st.available() > 0);
        st.reset();
        assert_eq!(st.available(), 0);
    }

    #[test]
    fn oversized_input_is_split_across_engine_calls() {
        let mut st = RubberBandStretcher::new();
        st.prepare(SR, 2, BLOCK).expect("prepare");
        let input = sine(st.max_feed() * 3 + 17);
        st.process(&[&input, &input]);
        assert!(st.available() > 0, "engine should produce output");
    }

    #[test]
    fn retrieve_with_the_wrong_channel_count_returns_zero_and_keeps_output() {
        let mut st = RubberBandStretcher::new();
        st.prepare(SR, 2, BLOCK).expect("prepare");
        let input = sine(st.max_feed() * 3 + 17);
        st.process(&[&input, &input]);
        let queued = st.available();
        assert!(queued > 0, "engine should produce output");

        // Three slices for a two-channel engine: must not write or read past the engine's buffers.
        let mut a = vec![0.0_f32; queued];
        let mut b = vec![0.0_f32; queued];
        let mut c = vec![0.0_f32; queued];
        assert_eq!(st.retrieve(&mut [&mut a, &mut b, &mut c]), 0);
        assert!(a.iter().chain(&b).chain(&c).all(|&v| v == 0.0));
        // One slice for a two-channel engine.
        assert_eq!(st.retrieve(&mut [&mut a]), 0);
        assert_eq!(st.available(), queued, "a rejected call consumes nothing");
    }

    #[test]
    fn process_silence_clamps_to_max_feed() {
        let mut st = RubberBandStretcher::new();
        st.prepare(SR, 2, BLOCK).expect("prepare");
        // An absurd request is clamped to one max_feed block rather than overrunning the engine.
        st.process_silence(usize::MAX);
        let mut l = vec![0.0_f32; st.available()];
        let mut r = vec![0.0_f32; st.available()];
        let got = st.retrieve(&mut [&mut l, &mut r]);
        assert!(got > 0);
        assert!(l[..got].iter().all(|v| v.is_finite()));
    }
}

//! The htdemucs pipeline against fake networks: no ONNX Runtime involved. A small segment length
//! keeps the tests fast; the logic is identical at 343 980 samples.

use om_analysis::AnalysisContext;
use om_stems::{
    HtDemucsPipeline, HtDemucsSeparator, ModelContract, SegmentInput, SegmentOutput, SliceSource,
    Spectral, StemError, StemInference, StemKind, StemSeparator, StemSet, StemSetSink,
    StereoSource, EMIT_CHUNK,
};
use std::f32::consts::PI;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

const SEG: usize = 4000;

fn contract() -> ModelContract {
    ModelContract {
        segment_samples: SEG,
        ..ModelContract::htdemucs()
    }
}

/// Splits the mix by fixed gains (in the contract's model order: drums, bass, other, vocals).
/// `wobble` multiplies every stem by a factor that alternates per call.
struct GainFake {
    gains: [f32; 4],
    calls: Arc<AtomicUsize>,
    wobble: f32,
    seen_frames: Arc<Mutex<Vec<usize>>>,
}

impl GainFake {
    fn new(gains: [f32; 4]) -> (Self, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        (
            Self {
                gains,
                calls: calls.clone(),
                wobble: 0.0,
                seen_frames: Arc::default(),
            },
            calls,
        )
    }
}

impl StemInference for GainFake {
    fn run(
        &mut self,
        input: &SegmentInput<'_>,
        output: &mut SegmentOutput,
    ) -> Result<(), StemError> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        self.seen_frames.lock().unwrap().push(input.frames);
        let n = input.frames;
        assert_eq!(input.mix.len(), 2 * n);
        let k = 1.0 + self.wobble * (call % 2) as f32;
        output.stems.clear();
        for g in self.gains {
            for c in 0..2 {
                output
                    .stems
                    .extend(input.mix[c * n..(c + 1) * n].iter().map(|v| v * g * k));
            }
        }
        Ok(())
    }
}

#[derive(Default)]
struct Ctx {
    cancel_after_calls: Option<(usize, Arc<AtomicUsize>)>,
    progress: Mutex<Vec<f32>>,
}

impl AnalysisContext for Ctx {
    fn is_cancelled(&self) -> bool {
        self.cancel_after_calls
            .as_ref()
            .is_some_and(|(n, calls)| calls.load(Ordering::SeqCst) >= *n)
    }
    fn set_progress(&self, fraction: f32) {
        self.progress.lock().unwrap().push(fraction);
    }
}

fn tone(n: usize, freq: f32, rate: f32) -> Vec<f32> {
    (0..n)
        .map(|i| (2.0 * PI * freq * i as f32 / rate).sin() * 0.5)
        .collect()
}

fn run_with(
    fake: GainFake,
    contract: ModelContract,
    left: &[f32],
    right: &[f32],
    rate: f64,
    ctx: &Ctx,
) -> (Result<(), StemError>, StemSetSink) {
    let mut pipeline = HtDemucsPipeline::new(fake, contract).unwrap();
    let mut sink = StemSetSink::new(rate);
    let result = pipeline.run(&mut SliceSource::stereo(left, right, rate), &mut sink, ctx);
    (result, sink)
}

fn max_abs_diff(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len());
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y).abs())
        .fold(0.0, f32::max)
}

fn sum_of_stems(set: &StemSet) -> (Vec<f32>, Vec<f32>) {
    let n = set.stems[&StemKind::Drums].len();
    let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
    for s in set.stems.values() {
        for i in 0..n {
            l[i] += s.left[i];
            r[i] += s.right[i];
        }
    }
    (l, r)
}

#[test]
fn overlap_add_reconstructs_the_mix_and_maps_stem_order() {
    // 3.5 segments plus change: several overlaps and a short last segment.
    let n = SEG * 3 + SEG / 2 + 123;
    let (l, r) = (tone(n, 220.0, 44_100.0), tone(n, 330.0, 44_100.0));
    let (fake, calls) = GainFake::new([0.1, 0.2, 0.3, 0.4]);
    let (res, sink) = run_with(fake, contract(), &l, &r, 44_100.0, &Ctx::default());
    res.unwrap();
    assert!(calls.load(Ordering::SeqCst) >= 5);
    let set = sink.into_set();
    assert!(set.is_complete(n));
    let (sl, sr) = sum_of_stems(&set);
    assert!(
        max_abs_diff(&sl, &l) < 1e-4,
        "left {}",
        max_abs_diff(&sl, &l)
    );
    assert!(max_abs_diff(&sr, &r) < 1e-4);
    // Model order is drums, bass, other, vocals: gains 0.1, 0.2, 0.3, 0.4 (DC offsets are tiny).
    for (kind, g) in [
        (StemKind::Drums, 0.1),
        (StemKind::Bass, 0.2),
        (StemKind::Other, 0.3),
        (StemKind::Vocals, 0.4),
    ] {
        let expect: Vec<f32> = l.iter().map(|v| v * g).collect();
        assert!(
            max_abs_diff(&set.stems[&kind].left, &expect) < 5e-3,
            "{kind}"
        );
    }
}

#[test]
fn segment_joins_are_seamless_even_when_segments_disagree() {
    let n = SEG * 4;
    let l = tone(n, 100.0, 44_100.0);
    let (mut fake, _) = GainFake::new([1.0, 0.0, 0.0, 0.0]);
    fake.wobble = 0.5; // odd segments come out 1.5x louder: a hard join would jump by ~0.25
    let (res, sink) = run_with(fake, contract(), &l, &l, 44_100.0, &Ctx::default());
    res.unwrap();
    let drums = &sink.into_set().stems[&StemKind::Drums].left.clone();
    let worst = drums
        .windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .fold(0.0, f32::max);
    assert!(worst < 0.02, "discontinuity {worst}");
}

#[test]
fn rates_other_than_the_models_round_trip_to_the_input_length() {
    for rate in [48_000.0f64, 22_050.0, 96_000.0] {
        let n = (rate * 0.4) as usize; // several segments after resampling
        let l = tone(n, 440.0, rate as f32);
        let r = tone(n, 660.0, rate as f32);
        let (fake, _) = GainFake::new([0.25; 4]);
        let (res, sink) = run_with(fake, contract(), &l, &r, rate, &Ctx::default());
        res.unwrap();
        let set = sink.into_set();
        assert_eq!(set.sample_rate, rate);
        assert!(set.is_complete(n), "{rate}: length");
        let (sl, _) = sum_of_stems(&set);
        let margin = 1500;
        let err = max_abs_diff(&sl[margin..n - margin], &l[margin..n - margin]);
        assert!(err < 3e-3, "{rate}: error {err}");
    }
}

#[test]
fn progress_is_monotonic_and_ends_at_one() {
    let n = SEG * 5;
    let l = tone(n, 200.0, 44_100.0);
    let (fake, _) = GainFake::new([0.25; 4]);
    let ctx = Ctx::default();
    run_with(fake, contract(), &l, &l, 44_100.0, &ctx)
        .0
        .unwrap();
    let p = ctx.progress.lock().unwrap().clone();
    assert!(p.len() >= 5, "{p:?}");
    assert!(p.windows(2).all(|w| w[0] <= w[1]), "{p:?}");
    assert!(p.iter().all(|v| (0.0..=1.0).contains(v)));
    assert_eq!(*p.last().unwrap(), 1.0);
}

#[test]
fn cancellation_stops_within_one_segment() {
    let n = SEG * 20;
    let l = tone(n, 200.0, 44_100.0);
    let (fake, calls) = GainFake::new([0.25; 4]);
    let ctx = Ctx {
        cancel_after_calls: Some((2, calls.clone())),
        ..Ctx::default()
    };
    let (res, sink) = run_with(fake, contract(), &l, &l, 44_100.0, &ctx);
    assert_eq!(res, Err(StemError::Cancelled));
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "no segment after the cancel"
    );
    // The second segment was computed but not emitted: only the first segment's hop got out.
    let written = sink
        .into_set()
        .stems
        .get(&StemKind::Drums)
        .map_or(0, |b| b.len());
    assert_eq!(written, SEG - SEG / 4);
}

#[test]
fn memory_is_bounded_by_the_segment() {
    // A long clip: every network call sees exactly one segment, and the sink never gets more than
    // EMIT_CHUNK frames at once, however long the input is.
    let n = SEG * 60;
    let l = tone(n, 150.0, 44_100.0);
    let (fake, _) = GainFake::new([0.25; 4]);
    let seen = fake.seen_frames.clone();
    let mut counting = CountingSource {
        inner: SliceSource::stereo(&l, &l, 44_100.0),
        max_request: 0,
    };
    let mut pipeline = HtDemucsPipeline::new(fake, contract()).unwrap();
    let mut sink = StemSetSink::new(44_100.0);
    pipeline
        .run(&mut counting, &mut sink, &Ctx::default())
        .unwrap();
    assert!(seen.lock().unwrap().iter().all(|&f| f == SEG));
    assert!(counting.max_request <= 16_384);
    assert!(sink.max_chunk <= EMIT_CHUNK);

    // At the real segment length the emitted blocks are split into EMIT_CHUNK pieces too.
    let real = ModelContract::htdemucs();
    let n = real.segment_samples + 1000;
    let x = vec![0.1f32; n];
    let (fake, _) = GainFake::new([0.25; 4]);
    let (res, sink) = run_with(fake, real, &x, &x, 44_100.0, &Ctx::default());
    res.unwrap();
    assert!(sink.max_chunk <= EMIT_CHUNK);
    assert!(sink.into_set().is_complete(n));
}

struct CountingSource<'a> {
    inner: SliceSource<'a>,
    max_request: usize,
}

impl StereoSource for CountingSource<'_> {
    fn sample_rate(&self) -> f64 {
        self.inner.sample_rate()
    }
    fn total_frames(&self) -> Option<u64> {
        self.inner.total_frames()
    }
    fn read(&mut self, left: &mut [f32], right: &mut [f32]) -> Result<usize, StemError> {
        self.max_request = self.max_request.max(left.len());
        self.inner.read(left, right)
    }
}

#[test]
fn mono_short_silent_and_empty_inputs() {
    let (fake, _) = GainFake::new([0.25; 4]);
    // Mono: the same signal in both channels comes back as stereo stems.
    let m = tone(1500, 300.0, 44_100.0);
    let mut sink = StemSetSink::new(44_100.0);
    HtDemucsPipeline::new(fake, contract())
        .unwrap()
        .run(
            &mut SliceSource::mono(&m, 44_100.0),
            &mut sink,
            &Ctx::default(),
        )
        .unwrap();
    let set = sink.into_set();
    assert!(set.is_complete(1500), "shorter than one segment");
    let s = &set.stems[&StemKind::Vocals];
    assert_eq!(s.left, s.right);
    let (sl, _) = sum_of_stems(&set);
    assert!(max_abs_diff(&sl, &m) < 1e-4);

    // Silence stays silent (no NaN from the std floor).
    let z = vec![0.0f32; SEG * 2 + 7];
    let (fake, _) = GainFake::new([0.25; 4]);
    let (res, sink) = run_with(fake, contract(), &z, &z, 44_100.0, &Ctx::default());
    res.unwrap();
    let set = sink.into_set();
    assert!(set.is_complete(z.len()));
    assert!(set.stems.values().all(|b| b.left.iter().all(|&v| v == 0.0)));

    // Empty input: nothing written, still reports completion.
    let (fake, calls) = GainFake::new([0.25; 4]);
    let ctx = Ctx::default();
    let (res, sink) = run_with(fake, contract(), &[], &[], 44_100.0, &ctx);
    res.unwrap();
    assert!(sink.into_set().stems.is_empty());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(*ctx.progress.lock().unwrap().last().unwrap(), 1.0);
}

#[test]
fn a_dc_offset_is_kept_and_stems_still_sum_to_the_input() {
    let n = SEG * 2 + 500;
    let l: Vec<f32> = tone(n, 180.0, 44_100.0).iter().map(|v| v + 0.2).collect();
    let (fake, _) = GainFake::new([0.1, 0.2, 0.3, 0.4]);
    let (res, sink) = run_with(fake, contract(), &l, &l, 44_100.0, &Ctx::default());
    res.unwrap();
    let (sl, _) = sum_of_stems(&sink.into_set());
    assert!(max_abs_diff(&sl, &l) < 1e-4);
}

/// Frequency-branch fake: each stem is a quarter of the mix, delivered as a quarter-spectrum plus a
/// time-branch remainder that makes up what the inverse STFT drops (Demucs cuts edge frames, so a
/// spectrum alone is not an exact copy at segment edges). The pipeline must hand over the
/// spectrum, invert `spec_output` and add it to the time branch.
struct SpecFake {
    spectral: Spectral,
}

impl StemInference for SpecFake {
    fn run(
        &mut self,
        input: &SegmentInput<'_>,
        output: &mut SegmentOutput,
    ) -> Result<(), StemError> {
        let n = input.frames;
        let spec = input.spec.expect("contract asks for a spectrum");
        assert_eq!(spec.len(), self.spectral.spec_len(n));
        let quarter: Vec<f32> = spec.iter().map(|v| v * 0.25).collect();
        let (il, ir) = self.spectral.inverse(&quarter, n);
        output.stems.clear();
        let mut all = Vec::new();
        for _ in 0..4 {
            for (c, inv) in [&il, &ir].into_iter().enumerate() {
                output
                    .stems
                    .extend((0..n).map(|i| input.mix[c * n + i] * 0.25 - inv[i]));
            }
            all.extend_from_slice(&quarter);
        }
        output.spec = Some(all);
        Ok(())
    }
}

#[test]
fn the_spectrogram_variant_adds_the_inverted_frequency_branch() {
    let contract = ModelContract {
        segment_samples: 3000,
        n_fft: 512,
        hop: 128,
        spec_input: Some("z".into()),
        spec_output: Some("zout".into()),
        ..ModelContract::htdemucs()
    };
    let n = 3000 * 2 + 400;
    let l = tone(n, 250.0, 44_100.0);
    let r = tone(n, 500.0, 44_100.0);
    let fake = SpecFake {
        spectral: Spectral::new(512, 128),
    };
    let mut sink = StemSetSink::new(44_100.0);
    HtDemucsPipeline::new(fake, contract)
        .unwrap()
        .run(
            &mut SliceSource::stereo(&l, &r, 44_100.0),
            &mut sink,
            &Ctx::default(),
        )
        .unwrap();
    let (sl, sr) = sum_of_stems(&sink.into_set());
    assert!(max_abs_diff(&sl, &l) < 1e-3, "{}", max_abs_diff(&sl, &l));
    assert!(max_abs_diff(&sr, &r) < 1e-3);
}

#[test]
fn separator_wrapper_implements_the_trait() {
    let (fake, _) = GainFake::new([0.25; 4]);
    let sep = HtDemucsSeparator::new("htdemucs-test", 3, fake, contract()).unwrap();
    assert_eq!(sep.id(), "htdemucs-test");
    assert_eq!(sep.version(), 3);
    assert_eq!(sep.native_rate(), 44_100.0);
    let x = tone(5000, 200.0, 44_100.0);
    let mut sink = StemSetSink::new(44_100.0);
    sep.separate_to(
        &mut SliceSource::mono(&x, 44_100.0),
        &mut sink,
        &Ctx::default(),
    )
    .unwrap();
    assert!(sink.into_set().is_complete(5000));
}

#[test]
fn a_wrongly_sized_model_output_is_an_error() {
    struct Bad;
    impl StemInference for Bad {
        fn run(&mut self, _: &SegmentInput<'_>, o: &mut SegmentOutput) -> Result<(), StemError> {
            o.stems = vec![0.0; 5];
            Ok(())
        }
    }
    let x = vec![0.1f32; 100];
    let mut sink = StemSetSink::new(44_100.0);
    let err = HtDemucsPipeline::new(Bad, contract())
        .unwrap()
        .run(
            &mut SliceSource::mono(&x, 44_100.0),
            &mut sink,
            &Ctx::default(),
        )
        .unwrap_err();
    assert!(matches!(err, StemError::Inference(_)));
}

#[test]
fn a_panic_in_one_job_does_not_poison_the_separator_for_later_jobs() {
    struct PanicOnce(Arc<AtomicUsize>);
    impl StemInference for PanicOnce {
        fn run(
            &mut self,
            input: &SegmentInput<'_>,
            o: &mut SegmentOutput,
        ) -> Result<(), StemError> {
            if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
                panic!("network blew up");
            }
            o.stems = vec![0.25; 4 * 2 * input.frames];
            Ok(())
        }
    }
    let calls = Arc::new(AtomicUsize::new(0));
    let sep = HtDemucsSeparator::new("htdemucs-test", 3, PanicOnce(calls), contract()).unwrap();
    let x = tone(3000, 200.0, 44_100.0);
    let mut run = || {
        let mut sink = StemSetSink::new(44_100.0);
        sep.separate_to(
            &mut SliceSource::mono(&x, 44_100.0),
            &mut sink,
            &Ctx::default(),
        )
        .map(|()| sink.into_set().is_complete(3000))
    };
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(&mut run));
    assert!(panicked.is_err(), "the first job panics");
    assert!(
        run().is_err(),
        "the job that finds the poisoned lock fails once"
    );
    assert!(run().unwrap(), "later jobs run");
}

#[test]
fn a_non_finite_model_output_fails_the_job_before_quantising() {
    struct Nan;
    impl StemInference for Nan {
        fn run(
            &mut self,
            input: &SegmentInput<'_>,
            o: &mut SegmentOutput,
        ) -> Result<(), StemError> {
            o.stems = vec![0.1; 8 * input.frames];
            o.stems[3] = f32::NAN;
            Ok(())
        }
    }
    let x = tone(3000, 200.0, 44_100.0);
    let mut sink = StemSetSink::new(44_100.0);
    let err = HtDemucsPipeline::new(Nan, contract())
        .unwrap()
        .run(
            &mut SliceSource::mono(&x, 44_100.0),
            &mut sink,
            &Ctx::default(),
        )
        .unwrap_err();
    match err {
        StemError::Invalid(msg) => assert_eq!(msg, "non-finite model output"),
        other => panic!("expected Invalid, got {other}"),
    }
}

#[test]
fn contract_upper_bounds_are_enforced_and_the_limits_themselves_pass() {
    let ok = ModelContract {
        segment_samples: 4_000_000,
        sample_rate: 192_000,
        ..ModelContract::htdemucs()
    };
    assert!(ok.validate().is_ok());
    let too_long = ModelContract {
        segment_samples: 4_000_001,
        ..ModelContract::htdemucs()
    };
    assert!(too_long.validate().unwrap_err().contains("segmentSamples"));
    let too_fast = ModelContract {
        sample_rate: 192_001,
        ..ModelContract::htdemucs()
    };
    assert!(too_fast.validate().unwrap_err().contains("sampleRate"));
    let too_big_fft = ModelContract {
        n_fft: 32_768,
        ..ModelContract::htdemucs()
    };
    assert!(too_big_fft.validate().unwrap_err().contains("nFft"));
}

//! Rust against Demucs itself. `tests/fixtures/htdemucs_reference.json` is written by
//! `tools/htdemucs_reference.py` from the real htdemucs model; the signal below is the same one.
//!
//! * `spectral_matches_torch` needs no model and runs everywhere: it pins [`Spectral::forward`]
//!   (the STFT the exported graph takes as input) to `model._spec`.
//! * `real_model_matches_demucs` is `#[ignore]`: it needs the exported graph. Export it with
//!   `tools/export_htdemucs.py --out htdemucs.onnx`, then
//!   `OPENMIXER_HTDEMUCS_MODEL=htdemucs.onnx cargo test -p om-stems --features onnx --test htdemucs_reference -- --ignored --nocapture`.

use om_stems::Spectral;
use serde_json::Value;

const SR: f64 = 44_100.0;
const SEG: usize = 343_980;

fn reference() -> Value {
    serde_json::from_str(include_str!("fixtures/htdemucs_reference.json")).expect("fixture parses")
}

/// Same LCG as the Python generator: exact in integer arithmetic.
fn lcg(seed: u32, n: usize) -> Vec<f64> {
    let mut state = seed;
    (0..n)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            f64::from(state) / 4_294_967_296.0 * 2.0 - 1.0
        })
        .collect()
}

fn signal() -> (Vec<f32>, Vec<f32>) {
    let tau = std::f64::consts::TAU;
    let (nl, nr) = (lcg(1, SEG), lcg(2, SEG));
    let t = |i: usize| i as f64 / SR;
    let left = (0..SEG)
        .map(|i| {
            (0.4 * (tau * 110.0 * t(i)).sin() + 0.2 * (tau * 1760.0 * t(i)).sin() + 0.05 * nl[i])
                as f32
        })
        .collect();
    let right = (0..SEG)
        .map(|i| {
            (0.4 * (tau * 110.0 * t(i) + 0.3).sin()
                + 0.2 * (tau * 880.0 * t(i)).sin()
                + 0.05 * nr[i]) as f32
        })
        .collect();
    (left, right)
}

#[test]
fn spectral_matches_torch() {
    let r = reference();
    let (left, right) = signal();
    let spec = Spectral::new(4096, 1024);
    let z = spec.forward(&left, &right);
    let frames = spec.frames(SEG);
    assert_eq!(frames, 336);
    assert_eq!(z.len(), 4 * 2048 * 336);

    let fr: Vec<usize> = r["frames"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as usize)
        .collect();
    let bins: Vec<usize> = r["bins"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as usize)
        .collect();
    let mut worst = 0.0f64;
    let mut peak = 0.0f64;
    for (c, per_bin) in r["spectrum"].as_array().unwrap().iter().enumerate() {
        for (bi, per_frame) in per_bin.as_array().unwrap().iter().enumerate() {
            for (fi, want) in per_frame.as_array().unwrap().iter().enumerate() {
                let want = want.as_f64().unwrap();
                let got = f64::from(z[(c * 2048 + bins[bi]) * frames + fr[fi]]);
                worst = worst.max((got - want).abs());
                peak = peak.max(want.abs());
            }
        }
    }
    assert!(
        peak > 0.1,
        "the fixture should have energy to compare: {peak}"
    );
    eprintln!("spectral: worst abs diff {worst:.2e} against a peak of {peak:.3}");
    assert!(
        worst < 1e-3 * peak,
        "worst abs diff {worst} against a peak of {peak}"
    );
}

/// The `contract` object `tools/export_htdemucs.py` prints must parse to the contract the code runs.
#[test]
fn the_exporters_manifest_declares_the_split_contract() {
    let json = r#"{"id":"htdemucs.onnx","url":"https://example.org/htdemucs.onnx",
        "sha256":"ffef760fbddba22766cf2cf2d864769f903c4bc96bd176704e0f01a92fd69ad8",
        "sizeBytes":174264719,"licenseName":"MIT","licenseText":"text",
        "contract":{"sampleRate":44100,"segmentSamples":343980,"overlap":0.25,"nFft":4096,
        "hop":1024,"mixInput":"mix","specInput":"z","stemsOutput":"stems","specOutput":"zout",
        "stemOrder":["drums","bass","other","vocals"]}}"#;
    let manifest = om_stems::ModelManifest::from_json(json).expect("manifest parses");
    assert_eq!(
        manifest.contract_or_default(),
        om_stems::ModelContract::htdemucs_split()
    );
}

#[cfg(feature = "onnx")]
#[test]
#[ignore = "needs the exported model: set OPENMIXER_HTDEMUCS_MODEL"]
fn real_model_matches_demucs() {
    use om_analysis::NullAnalysisContext;
    use om_stems::{
        HtDemucsPipeline, ModelContract, OrtInference, SliceSource, StemKind, StemSetSink,
    };

    let Some(path) = std::env::var_os("OPENMIXER_HTDEMUCS_MODEL") else {
        eprintln!("OPENMIXER_HTDEMUCS_MODEL is not set; skipping");
        return;
    };
    let contract = ModelContract::htdemucs_split();
    let net = OrtInference::load(std::path::Path::new(&path), contract.clone(), None)
        .expect("the exported graph satisfies the contract");
    let mut pipeline = HtDemucsPipeline::new(net, contract).expect("pipeline");

    let (left, right) = signal();
    let mut sink = StemSetSink::new(SR);
    let started = std::time::Instant::now();
    pipeline
        .run(
            &mut SliceSource::stereo(&left, &right, SR),
            &mut sink,
            &NullAnalysisContext,
        )
        .expect("separates one segment");
    eprintln!(
        "separated {:.1} s of audio in {:?}",
        SEG as f64 / SR,
        started.elapsed()
    );
    let set = sink.into_set();
    assert!(set.is_complete(SEG));

    let r = reference();
    let order = [
        StemKind::Drums,
        StemKind::Bass,
        StemKind::Other,
        StemKind::Vocals,
    ];
    let rms =
        |x: &[f32]| (x.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>() / x.len() as f64).sqrt();
    for (i, kind) in order.iter().enumerate() {
        let stem = &set.stems[kind];
        for (ch, buf) in [(0, &stem.left), (1, &stem.right)] {
            let want = r["stem_rms"][i][ch].as_f64().unwrap();
            let got = rms(buf);
            eprintln!("{kind} ch{ch}: rms {got:.5} (demucs {want:.5})");
            // Loud stems must agree closely; near-silent ones only need to stay near-silent.
            assert!(
                (got - want).abs() <= 0.03 * want.max(0.01),
                "{kind} ch{ch}: rms {got} vs demucs {want}"
            );
        }
    }
}

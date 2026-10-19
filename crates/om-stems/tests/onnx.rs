//! ONNX Runtime backend against a toy model: a 4-way split with fixed gains, hand-encoded as ONNX
//! protobuf below (no `onnx` Python package needed; the file is ~200 bytes and built in the test).
//! Run with `cargo test -p om-stems --features onnx` (the first build downloads ONNX Runtime).
#![cfg(feature = "onnx")]

use om_analysis::NullAnalysisContext;
use om_stems::{HtDemucsPipeline, ModelContract, OrtInference, SliceSource, StemKind, StemSetSink};

// --- minimal protobuf writer -------------------------------------------------------------------

fn varint(mut v: u64, out: &mut Vec<u8>) {
    while v >= 0x80 {
        out.push((v as u8 & 0x7f) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

fn field_varint(num: u32, v: u64, out: &mut Vec<u8>) {
    varint(u64::from(num) << 3, out);
    varint(v, out);
}

fn field_bytes(num: u32, bytes: &[u8], out: &mut Vec<u8>) {
    varint(u64::from(num) << 3 | 2, out);
    varint(bytes.len() as u64, out);
    out.extend_from_slice(bytes);
}

fn msg(f: impl FnOnce(&mut Vec<u8>)) -> Vec<u8> {
    let mut v = Vec::new();
    f(&mut v);
    v
}

fn node(op: &str, inputs: &[&str], output: &str) -> Vec<u8> {
    msg(|m| {
        for i in inputs {
            field_bytes(1, i.as_bytes(), m);
        }
        field_bytes(2, output.as_bytes(), m);
        field_bytes(4, op.as_bytes(), m);
    })
}

fn tensor(name: &str, dims: &[i64], data_type: u64, raw: &[u8]) -> Vec<u8> {
    msg(|m| {
        for d in dims {
            field_varint(1, *d as u64, m);
        }
        field_varint(2, data_type, m);
        field_bytes(8, name.as_bytes(), m);
        field_bytes(9, raw, m);
    })
}

/// `float32` value info with fixed dims and a symbolic last dim `n`.
fn value_info(name: &str, fixed: &[i64]) -> Vec<u8> {
    let dims: Vec<u8> = msg(|shape| {
        for d in fixed {
            field_bytes(1, &msg(|dim| field_varint(1, *d as u64, dim)), shape);
        }
        field_bytes(1, &msg(|dim| field_bytes(2, b"n", dim)), shape);
    });
    let tensor_type = msg(|t| {
        field_varint(1, 1, t); // FLOAT
        field_bytes(2, &dims, t);
    });
    msg(|v| {
        field_bytes(1, name.as_bytes(), v);
        field_bytes(2, &msg(|t| field_bytes(1, &tensor_type, t)), v);
    })
}

/// `stems[1,4,2,n] = Unsqueeze(mix[1,2,n], axis 1) * gains[1,4,1,1]`.
fn toy_model(gains: [f32; 4]) -> Vec<u8> {
    let axes = tensor("axes", &[1], 7, &1i64.to_le_bytes());
    let gain_bytes: Vec<u8> = gains.iter().flat_map(|g| g.to_le_bytes()).collect();
    let gains = tensor("gains", &[1, 4, 1, 1], 1, &gain_bytes);
    let graph = msg(|g| {
        field_bytes(1, &node("Unsqueeze", &["mix", "axes"], "wide"), g);
        field_bytes(1, &node("Mul", &["wide", "gains"], "stems"), g);
        field_bytes(2, b"toy_split", g);
        field_bytes(5, &axes, g);
        field_bytes(5, &gains, g);
        field_bytes(11, &value_info("mix", &[1, 2]), g);
        field_bytes(12, &value_info("stems", &[1, 4, 2]), g);
    });
    msg(|m| {
        field_varint(1, 8, m); // IR version
        field_bytes(8, &msg(|o| field_varint(2, 13, o)), m); // default opset 13
        field_bytes(7, &graph, m);
    })
}

#[test]
fn the_toy_model_splits_with_its_gains_through_the_whole_pipeline() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("toy.onnx");
    let bytes = toy_model([0.1, 0.2, 0.3, 0.4]);
    assert!(bytes.len() < 50_000);
    std::fs::write(&path, bytes).expect("write model");

    let contract = ModelContract {
        segment_samples: 4000,
        ..ModelContract::htdemucs()
    };
    let net = OrtInference::load(&path, contract.clone(), Some(1)).expect("loads");
    let mut pipeline = HtDemucsPipeline::new(net, contract).expect("pipeline");

    let n = 4000 * 2 + 700;
    let tone: Vec<f32> = (0..n)
        .map(|i| (2.0 * std::f32::consts::PI * 300.0 * i as f32 / 48_000.0).sin() * 0.5)
        .collect();
    let mut sink = StemSetSink::new(48_000.0);
    pipeline
        .run(
            &mut SliceSource::mono(&tone, 48_000.0), // 48 kHz: resampled to 44.1 kHz and back
            &mut sink,
            &NullAnalysisContext,
        )
        .expect("separates");
    let set = sink.into_set();
    assert!(set.is_complete(n));
    let margin = 1500;
    for (kind, gain) in [
        (StemKind::Drums, 0.1f32),
        (StemKind::Bass, 0.2),
        (StemKind::Other, 0.3),
        (StemKind::Vocals, 0.4),
    ] {
        let stem = &set.stems[&kind].left;
        let err = (margin..n - margin)
            .map(|i| (stem[i] - tone[i] * gain).abs())
            .fold(0.0f32, f32::max);
        assert!(err < 3e-3, "{kind}: {err}");
    }
}

#[test]
fn a_missing_model_file_is_an_error_not_a_panic() {
    let err = OrtInference::load(
        std::path::Path::new("/nonexistent/model.onnx"),
        ModelContract::htdemucs(),
        None,
    );
    assert!(err.is_err());
}

//! The knob value table the web front end's `captionText` is pinned to: every row is what GPUI's
//! mixer prints under that knob's label for that value (`caption_text` of the readout the control
//! spec carries), so both windows print the same words.

use om_core::ParameterRouter;
use om_engine::DjEngine;
use om_ui::binding::MixerBinding;
use om_ui_bridge::{caption_text, readout_name, slice_controls, ControlSpec};
use std::fmt::Write as _;
use std::sync::Arc;

const PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/ui-bridge/knob-text.txt"
);
const TABLE: &str = include_str!("../../../fixtures/ui-bridge/knob-text.txt");

/// Each knob kind at its default, its ends, and the edges of its special words and rounding:
/// the kill margin, the filter's off band and halves that round away from zero.
const PINNED: [(&str, &[f64]); 6] = [
    ("ch0.trim", &[0.0, 0.25, 0.5, 1.0, 1.4125, 2.0, 4.0]),
    (
        "ch1.eq.high",
        &[-48.0, -47.5, -47.4, -12.5, -0.4, 0.0, 2.5, 6.0],
    ),
    ("ch0.eq.mid", &[-48.0, 0.0, 6.0]),
    ("ch1.eq.low", &[-48.0, -3.5, 0.0, 6.0]),
    (
        "ch0.filter",
        &[
            -1.0, -0.5, -0.125, -0.004, 0.0, 0.004, 0.005, 0.125, 0.995, 1.0,
        ],
    ),
    ("mixer.master", &[0.0, 0.5, 0.7, 1.0, 2.0]),
];

fn specs() -> Vec<ControlSpec> {
    let engine = Arc::new(DjEngine::new(2).0);
    let mixer = MixerBinding::typed(engine, Arc::new(ParameterRouter::new()));
    slice_controls(&mixer, 2)
}

fn table_text() -> String {
    let specs = specs();
    let mut out = String::from(HEADER);
    for (key, values) in PINNED {
        let spec = specs.iter().find(|s| s.key == key).expect(key);
        let readout = spec.readout.expect(key);
        let name = readout_name(readout).expect(key);
        for value in values {
            let text = caption_text(readout, *value);
            let _ = writeln!(out, "{key} {name} {value} {text}");
        }
    }
    out
}

const HEADER: &str = "\
# A knob's value as GPUI's mixer prints it under the label, asserted by both om-ui-bridge
# (tests/knob_text_table.rs, against om_ui::control::Readout through caption_text and the readout
# each control spec carries) and the web front end (src/lib/controls/readout.test.ts).
# Rewrite: cargo test -p om-ui-bridge --test knob_text_table write_table -- --ignored
# <key> <readout> <value> <text, the rest of the line>
";

#[test]
#[ignore = "rewrites the knob text table shared with the TypeScript tests"]
fn write_table() {
    std::fs::write(PATH, table_text()).expect("write the knob text table");
}

#[test]
fn the_table_is_up_to_date() {
    assert_eq!(TABLE, table_text(), "rewrite the table");
}

#[test]
fn the_table_pins_every_slice_knob_kind() {
    let rows = TABLE.lines().filter(|l| !l.starts_with('#')).count();
    assert!(rows >= 12, "the table pins at least twelve values");
    for name in ["gain", "decibels", "filter"] {
        let pinned = format!(" {name} ");
        assert!(TABLE.contains(&pinned), "{name} is not pinned");
    }
}

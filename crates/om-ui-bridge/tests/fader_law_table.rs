//! The fader law table the web front end's `range.ts` interpolation is pinned to: its knot rows
//! must be exactly what `slice_controls` sends, and every value row must hold for
//! `om_ui::fader_law::FaderLaw`, so both windows put the same gain at the same travel.

use om_core::ParameterRouter;
use om_engine::DjEngine;
use om_ui::binding::MixerBinding;
use om_ui::fader_law::FaderLaw;
use om_ui_bridge::{slice_controls, ControlSpec, TravelLaw};
use std::fmt::Write as _;
use std::sync::Arc;

const PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/ui-bridge/fader-law.txt"
);
const TABLE: &str = include_str!("../../../fixtures/ui-bridge/fader-law.txt");

/// The tapered controls and the travel positions pinned for each: ends, knees, knots and points
/// between knots, where the front end interpolates.
const PINNED: [(&str, FaderLaw, [f64; 9]); 2] = [
    (
        "ch0.fader",
        FaderLaw::Channel,
        [0.0, 0.1, 0.2, 0.455, 0.6, 0.8, 0.9, 0.955, 1.0],
    ),
    (
        "mixer.master",
        FaderLaw::Master,
        [0.0, 0.125, 0.25, 0.5, 0.65, 0.75, 0.875, 0.995, 1.0],
    ),
];

fn rows() -> impl Iterator<Item = Vec<&'static str>> {
    let lines = TABLE
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'));
    lines.map(|l| l.split_whitespace().collect())
}

fn number(text: &str) -> f64 {
    text.parse()
        .unwrap_or_else(|_| panic!("not a number: {text}"))
}

fn spec(key: &str) -> ControlSpec {
    let engine = Arc::new(DjEngine::new(2).0);
    let mixer = MixerBinding::typed(engine, Arc::new(ParameterRouter::new()));
    let specs = slice_controls(&mixer, 2);
    specs.into_iter().find(|s| s.key == key).expect(key)
}

fn knots(key: &str) -> Vec<(f64, f64)> {
    match spec(key).law {
        TravelLaw::Knots(k) => k,
        TravelLaw::Linear => panic!("{key} is not tapered"),
    }
}

fn law_of(key: &str) -> FaderLaw {
    PINNED.iter().find(|p| p.0 == key).expect(key).1
}

#[test]
fn the_knot_rows_are_what_the_bridge_sends() {
    let knot_rows: Vec<_> = rows().filter(|r| r[0] == "knots").collect();
    assert_eq!(knot_rows.len(), PINNED.len());
    for row in knot_rows {
        let table: Vec<f64> = row[2..].iter().map(|t| number(t)).collect();
        let sent: Vec<f64> = knots(row[1])
            .into_iter()
            .flat_map(|(p, v)| [p, v])
            .collect();
        assert_eq!(table, sent, "{}: rewrite the table", row[1]);
    }
}

#[test]
fn every_value_row_matches_the_om_ui_law() {
    let values: Vec<_> = rows().filter(|r| r[0] != "knots").collect();
    assert!(values.len() >= 10, "the table pins at least ten positions");
    for row in values {
        let gain = law_of(row[0]).gain_at(number(row[1]) as f32);
        assert!(
            (f64::from(gain) - number(row[2])).abs() < 1e-8,
            "{row:?}: {gain}"
        );
    }
}

fn table_text() -> String {
    let mut out = String::from(HEADER);
    for (key, _, _) in PINNED {
        let points = knots(key).into_iter().map(|(p, v)| format!("{p} {v}"));
        let _ = writeln!(out, "knots {key} {}", points.collect::<Vec<_>>().join(" "));
    }
    out.push_str("# <key> <travel position> <gain>\n");
    for (key, law, positions) in PINNED {
        for pos in positions {
            let _ = writeln!(out, "{key} {pos} {:.9}", law.gain_at(pos as f32));
        }
    }
    out
}

const HEADER: &str = "\
# Fader travel to gain, asserted by both om-ui-bridge (tests/fader_law_table.rs, against
# om_ui::fader_law::FaderLaw and the specs slice_controls sends) and the web front end
# (src/lib/controls/range.test.ts, through its interpolation of the knots).
# Rewrite: cargo test -p om-ui-bridge --test fader_law_table write_table -- --ignored
# knots <key> <position> <gain> ...: the control spec's law, exactly.
";

#[test]
#[ignore = "rewrites the fader law table shared with the TypeScript tests"]
fn write_table() {
    std::fs::write(PATH, table_text()).expect("write the fader law table");
}

#[test]
fn the_table_is_up_to_date() {
    assert_eq!(TABLE, table_text(), "rewrite the table");
}

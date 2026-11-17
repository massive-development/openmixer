//! The meter table the web front end's `meterFraction` and `meterZone` are pinned to: every row
//! must also hold for `om_ui::meter::MeterScale`, so both windows scale a peak the same way.

use om_ui::meter::{MeterScale, MeterZone};
use om_ui::theme::ThemeTokens;

const TABLE: &str = include_str!("../../../fixtures/ui-bridge/meter-table.txt");

fn rows() -> impl Iterator<Item = Vec<&'static str>> {
    let lines = TABLE
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'));
    lines.map(|l| l.split_whitespace().collect())
}

fn number(text: &str) -> f32 {
    text.parse()
        .unwrap_or_else(|_| panic!("not a number: {text}"))
}

fn table_scale() -> MeterScale {
    let row = rows().find(|r| r[0] == "scale").expect("a scale row");
    MeterScale {
        floor_db: number(row[1]),
        mid_db: number(row[2]),
        high_db: number(row[3]),
    }
}

fn zone_name(zone: MeterZone) -> &'static str {
    match zone {
        MeterZone::Low => "low",
        MeterZone::Mid => "mid",
        MeterZone::High => "high",
    }
}

/// `20 log10` of a linear peak; silence is minus infinity, which the scale clamps to its floor.
fn decibels(linear: f32) -> f32 {
    20.0 * linear.log10()
}

#[test]
fn the_table_scale_is_the_builtin_theme() {
    assert_eq!(
        table_scale(),
        MeterScale::from_tokens(&ThemeTokens::builtin_dark())
    );
}

#[test]
fn every_row_matches_the_om_ui_meter_scale() {
    let scale = table_scale();
    let levels: Vec<_> = rows().filter(|r| r[0] != "scale").collect();
    assert!(levels.len() >= 6, "the table pins at least six levels");
    for row in levels {
        let fraction = scale.proportion(decibels(number(row[0])));
        assert!(
            (fraction - number(row[1])).abs() < 1e-5,
            "{row:?}: {fraction}"
        );
        assert_eq!(zone_name(scale.zone(fraction)), row[2], "{row:?}");
    }
}

//! The `controls` command's wire form: each slice control's id and value shape, keyed by its
//! front-end key, camelCase on the wire.

use om_ui_bridge::{ControlKind, ControlSpec};
use serde::Serialize;
use std::collections::HashMap;

/// One control as the front end reads it; values are engine units.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlDto {
    pub id: String,
    pub min: f64,
    pub max: f64,
    #[serde(rename = "default")]
    pub default_value: f64,
    pub detent: Option<f64>,
    /// `toggle`, `trigger` or `continuous`.
    pub kind: &'static str,
    pub label: String,
}

/// The specs keyed by their front-end key.
pub fn control_dtos(specs: Vec<ControlSpec>) -> HashMap<String, ControlDto> {
    specs.into_iter().map(|s| (s.key.clone(), dto(s))).collect()
}

fn dto(s: ControlSpec) -> ControlDto {
    ControlDto {
        id: s.id,
        min: s.min,
        max: s.max,
        default_value: s.default,
        detent: s.detent,
        kind: kind_name(s.kind),
        label: s.label,
    }
}

fn kind_name(kind: ControlKind) -> &'static str {
    match kind {
        ControlKind::Toggle => "toggle",
        ControlKind::Trigger => "trigger",
        ControlKind::Continuous => "continuous",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(kind: ControlKind) -> ControlSpec {
        ControlSpec {
            key: "ch0.trim".into(),
            id: "mixer.ch0.trim".into(),
            min: 0.0,
            max: 4.0,
            default: 1.0,
            detent: Some(1.0),
            kind,
            label: "GAIN".into(),
        }
    }

    #[test]
    fn the_wire_form_names_default_and_the_kind() {
        let map = control_dtos(vec![spec(ControlKind::Continuous)]);
        let json = serde_json::to_string(&map).unwrap();
        for part in [
            "\"ch0.trim\"",
            "\"id\":\"mixer.ch0.trim\"",
            "\"default\":1.0",
            "\"detent\":1.0",
            "\"kind\":\"continuous\"",
            "\"label\":\"GAIN\"",
        ] {
            assert!(json.contains(part), "{part} missing from {json}");
        }
    }

    #[test]
    fn every_kind_has_its_name() {
        let names = [
            ControlKind::Toggle,
            ControlKind::Trigger,
            ControlKind::Continuous,
        ]
        .map(kind_name);
        assert_eq!(names, ["toggle", "trigger", "continuous"]);
    }
}

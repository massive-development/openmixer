//! The `controls` command's wire form: each slice control's id and value shape, keyed by its
//! front-end key, camelCase on the wire.

use om_ui_bridge::{ControlKind, ControlSpec, SnapSpec, TickSpec, TravelLaw};
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
    pub law: LawDto,
    pub ticks: Vec<TickDto>,
    pub snap: Option<SnapDto>,
}

/// `{ "kind": "linear" }` or `{ "kind": "knots", "knots": [[position, value], ...] }`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LawDto {
    Linear,
    Knots { knots: Vec<[f64; 2]> },
}

/// One scale mark: travel position, printed text (empty for none), long mark, 0 dB mark.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TickDto {
    pub pos: f64,
    pub label: String,
    pub major: bool,
    pub unity: bool,
}

/// A dragged knob's pull: values within `width` of `at` become `at`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct SnapDto {
    pub at: f64,
    pub width: f64,
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
        law: law_dto(s.law),
        ticks: s.ticks.into_iter().map(tick_dto).collect(),
        snap: s.snap.map(snap_dto),
    }
}

fn law_dto(law: TravelLaw) -> LawDto {
    match law {
        TravelLaw::Linear => LawDto::Linear,
        TravelLaw::Knots(k) => LawDto::Knots {
            knots: k.into_iter().map(|(p, v)| [p, v]).collect(),
        },
    }
}

fn tick_dto(t: TickSpec) -> TickDto {
    TickDto {
        pos: t.pos,
        label: t.label,
        major: t.major,
        unity: t.unity,
    }
}

fn snap_dto(s: SnapSpec) -> SnapDto {
    SnapDto {
        at: s.at,
        width: s.width,
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
            law: TravelLaw::Linear,
            ticks: Vec::new(),
            snap: Some(SnapSpec {
                at: 1.0,
                width: 0.12,
            }),
        }
    }

    fn json(spec: ControlSpec) -> String {
        serde_json::to_string(&control_dtos(vec![spec])).unwrap()
    }

    #[test]
    fn the_wire_form_names_default_and_the_kind() {
        let json = json(spec(ControlKind::Continuous));
        for part in [
            "\"ch0.trim\"",
            "\"id\":\"mixer.ch0.trim\"",
            "\"default\":1.0",
            "\"detent\":1.0",
            "\"kind\":\"continuous\"",
            "\"label\":\"GAIN\"",
            "\"law\":{\"kind\":\"linear\"}",
            "\"ticks\":[]",
            "\"snap\":{\"at\":1.0,\"width\":0.12}",
        ] {
            assert!(json.contains(part), "{part} missing from {json}");
        }
    }

    #[test]
    fn a_tapered_fader_sends_its_knots_and_marks() {
        let fader = ControlSpec {
            law: TravelLaw::Knots(vec![(0.0, 0.0), (1.0, 1.0)]),
            ticks: vec![TickSpec {
                pos: 1.0,
                label: "0".into(),
                major: true,
                unity: true,
            }],
            snap: None,
            ..spec(ControlKind::Continuous)
        };
        let json = json(fader);
        for part in [
            "\"law\":{\"kind\":\"knots\",\"knots\":[[0.0,0.0],[1.0,1.0]]}",
            "\"ticks\":[{\"pos\":1.0,\"label\":\"0\",\"major\":true,\"unity\":true}]",
            "\"snap\":null",
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

//! Controller-map catalog helpers that need no hardware: a schema check for shipped maps and a
//! pick-by-device-name selector. Both are pure functions over parsed maps and port names.
//!
//! Threads: pure data; usable from any thread.

use crate::editor::find_conflicts;
use crate::map::{MapMode, Mapping, MidiMap};
use crate::output_rule::OutputRule;
use om_core::{ParamId, StateId};
use std::str::FromStr;

/// Words that name the transport, not the device ("Foo MIDI 1", "Foo In").
const NOISE_WORDS: &[&str] = &["midi", "in", "out", "input", "output", "port"];

/// Lowercase port name with ALSA `20:0` addresses and noise words removed, punctuation as spaces.
pub fn normalize_device_name(name: &str) -> String {
    let lower = name.to_lowercase();
    lower
        .split_whitespace()
        .filter(|w| !is_alsa_address(w) && !NOISE_WORDS.contains(w))
        .map(|w| {
            w.chars()
                .map(|c| if c.is_alphanumeric() { c } else { ' ' })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_alsa_address(word: &str) -> bool {
    word.split_once(':').is_some_and(|(a, b)| {
        !a.is_empty()
            && !b.is_empty()
            && a.bytes().all(|c| c.is_ascii_digit())
            && b.bytes().all(|c| c.is_ascii_digit())
    })
}

/// A map offered for selection: an id the caller understands (a file name) and its device hint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapCandidate {
    pub id: String,
    /// The map's `device` field; empty means "never auto-selected".
    pub device: String,
}

/// Result of matching a port name against the candidates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MapChoice {
    Match(String),
    /// Several maps match equally well; the caller asks the user.
    Ambiguous(Vec<String>),
    NoMatch,
}

/// Picks the map whose device hint appears, as whole words, in the port name. The longest hint
/// wins; equally long hints from different maps are ambiguous.
pub fn select_map(port_name: &str, candidates: &[MapCandidate]) -> MapChoice {
    let port = format!(" {} ", normalize_device_name(port_name));
    let mut best = 0;
    let mut ids: Vec<&str> = Vec::new();
    for c in candidates {
        let hint = normalize_device_name(&c.device);
        if hint.is_empty() || !port.contains(&format!(" {hint} ")) {
            continue;
        }
        if hint.len() > best {
            best = hint.len();
            ids.clear();
        }
        if hint.len() == best {
            ids.push(&c.id);
        }
    }
    match ids.as_slice() {
        [] => MapChoice::NoMatch,
        [one] => MapChoice::Match((*one).to_owned()),
        many => MapChoice::Ambiguous(many.iter().map(|s| (*s).to_owned()).collect()),
    }
}

/// Every schema problem in `map`; empty when the map is fit to ship. Parsing already enforces
/// the per-field rules; this adds target ids, duplicate controls and range sanity.
pub fn check_map(map: &MidiMap) -> Vec<String> {
    let mut issues = Vec::new();
    if map.name.trim().is_empty() {
        issues.push("map has no name".to_owned());
    }
    for (i, m) in map.mappings.iter().enumerate() {
        check_target(&m.target, &format!("mapping {i}"), &mut issues);
        check_ranges(i, m, &mut issues);
    }
    for c in find_conflicts(&map.mappings) {
        issues.push(format!(
            "mappings {} and {} both fire on {}",
            c.first, c.second, c.control
        ));
    }
    check_outputs(&map.outputs, &mut issues);
    issues
}

/// Parses `json` and checks it; a parse failure is the single issue.
pub fn check_map_json(json: &str) -> Vec<String> {
    match MidiMap::parse(json) {
        Ok(map) => check_map(&map),
        Err(e) => vec![e.to_string()],
    }
}

fn check_target(target: &str, what: &str, issues: &mut Vec<String>) {
    match ParamId::from_str(target) {
        Ok(ParamId::Ext { .. }) => issues.push(format!("{what}: \"{target}\" is not an engine id")),
        Ok(id) => {
            if let Err(e) = id.validate() {
                issues.push(format!("{what}: {e}"));
            }
        }
        Err(e) => issues.push(format!("{what}: \"{target}\" does not parse ({e})")),
    }
}

fn check_ranges(i: usize, m: &Mapping, issues: &mut Vec<String>) {
    if [m.min, m.max, m.step, m.initial]
        .iter()
        .any(|v| !v.is_finite())
    {
        issues.push(format!("mapping {i}: range values must be finite"));
        return;
    }
    let relative = matches!(m.mode, MapMode::Relative | MapMode::RelativeTwosComplement);
    if relative && m.step <= 0.0 {
        issues.push(format!("mapping {i}: relative step must be positive"));
    }
    let (lo, hi) = (m.min.min(m.max), m.min.max(m.max));
    if m.initial < lo || m.initial > hi {
        issues.push(format!(
            "mapping {i}: initial {} is outside min..max",
            m.initial
        ));
    }
}

fn check_outputs(outputs: &[OutputRule], issues: &mut Vec<String>) {
    for (i, a) in outputs.iter().enumerate() {
        if let Err(e) = StateId::from_str(&a.source) {
            issues.push(format!(
                "output {i} source: \"{}\" does not parse ({e})",
                a.source
            ));
        }
        let twin = outputs
            .iter()
            .skip(i + 1)
            .position(|b| (a.kind, a.channel, a.number) == (b.kind, b.channel, b.number));
        if let Some(offset) = twin {
            issues.push(format!(
                "outputs {i} and {} drive the same LED",
                i + 1 + offset
            ));
        }
    }
}

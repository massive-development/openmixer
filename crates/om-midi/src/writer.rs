//! Map writer: the inverse of [`MidiMap::parse`], always at the current schema version.

use crate::event::MidiKind;
use crate::map::{MapMode, Mapping, MidiMap, MAP_VERSION};
use crate::output_rule::OutputRule;
use serde_json::{json, Map, Value};

impl MidiMap {
    /// Serialises the map at [`MAP_VERSION`]; `parse(to_json())` round-trips.
    pub fn to_json(&self) -> String {
        let mappings: Vec<Value> = self.mappings.iter().map(mapping_to_value).collect();
        let mut root = json!({
            "version": MAP_VERSION,
            "name": self.name,
            "mappings": mappings,
            "outputs": self.outputs.iter().map(OutputRule::to_value).collect::<Vec<Value>>(),
        });
        if !self.device.is_empty() {
            root["device"] = json!(self.device);
        }
        serde_json::to_string_pretty(&root).unwrap_or_default()
    }
}

fn mapping_to_value(m: &Mapping) -> Value {
    let mut o = Map::new();
    o.insert("type".into(), json!(kind_name(m.kind)));
    if m.channel != 0 {
        o.insert("channel".into(), json!(m.channel));
    }
    if m.kind != MidiKind::PitchBend {
        o.insert("number".into(), json!(m.number));
    }
    o.insert("target".into(), json!(m.target));
    o.insert("mode".into(), json!(m.mode.as_str()));
    o.insert("min".into(), json!(m.min));
    o.insert("max".into(), json!(m.max));
    o.insert("step".into(), json!(m.step));
    o.insert("initial".into(), json!(m.initial));
    if m.invert {
        o.insert("invert".into(), json!(true));
    }
    if m.mode == MapMode::Absolute14 {
        o.insert("lsb".into(), json!(m.lsb_number));
    }
    Value::Object(o)
}

fn kind_name(kind: MidiKind) -> &'static str {
    match kind {
        MidiKind::Cc => "cc",
        MidiKind::Note => "note",
        MidiKind::PitchBend => "pitchbend",
    }
}

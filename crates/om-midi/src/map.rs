//! User-editable JSON controller map. Schema is the same as the C++ build:
//!
//! ```json
//! { "name": "My controller", "mappings": [
//!   { "type": "cc", "channel": 1, "number": 7, "target": "mixer.ch0.fader" },
//!   { "type": "cc", "number": 21, "target": "mixer.ch0.eq.low", "mode": "relative",
//!     "step": 0.5, "min": -48, "max": 6, "initial": 0 },
//!   { "type": "note", "number": 36, "target": "deck.0.play", "mode": "toggle" },
//!   { "type": "pitchbend", "target": "deck.0.rate", "min": 0.92, "max": 1.08 } ] }
//! ```
//!
//! `type`: cc|note|pitchbend. `channel`: 1-16, omitted or 0 = any.
//! `mode`: absolute (default), relative (offset 64), relative2c, button (default for notes), toggle,
//! absolute14 (CC pair: `number` is the MSB, `lsb` the LSB controller, default number + 32).
//! The root `version` is 2 (current); a file without it is version 1 and loads unchanged. A version
//! newer than [`MAP_VERSION`] is rejected. The `outputs` array holds the feedback rules
//! ([`OutputRule`]); each one is validated when the map is parsed.

use crate::error::MidiError;
use crate::event::MidiKind;
use crate::output_rule::OutputRule;
use serde_json::Value;

/// Schema version written by [`MidiMap::to_json`].
pub const MAP_VERSION: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapMode {
    Absolute,
    Relative,
    RelativeTwosComplement,
    Button,
    Toggle,
    Absolute14,
}

impl MapMode {
    /// The JSON spelling of the mode.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Absolute => "absolute",
            Self::Relative => "relative",
            Self::RelativeTwosComplement => "relative2c",
            Self::Button => "button",
            Self::Toggle => "toggle",
            Self::Absolute14 => "absolute14",
        }
    }
}

/// One binding from a control to a parameter id.
#[derive(Clone, Debug, PartialEq)]
pub struct Mapping {
    pub kind: MidiKind,
    /// 0 = any channel.
    pub channel: i32,
    pub number: i32,
    pub target: String,
    pub mode: MapMode,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub initial: f64,
    pub invert: bool,
    /// Absolute14 only: the LSB controller number, -1 otherwise.
    pub lsb_number: i32,
}

impl Default for Mapping {
    fn default() -> Self {
        Self {
            kind: MidiKind::Cc,
            channel: 0,
            number: 0,
            target: String::new(),
            mode: MapMode::Absolute,
            min: 0.0,
            max: 1.0,
            step: 0.01,
            initial: 0.0,
            invert: false,
            lsb_number: -1,
        }
    }
}

impl Mapping {
    /// The rules [`MidiMap::parse`] applies, for a mapping built in code.
    pub fn validate(&self) -> Result<(), MidiError> {
        if self.target.is_empty() {
            return Err(invalid("missing target"));
        }
        if !(0..=16).contains(&self.channel) || !(0..=127).contains(&self.number) {
            return Err(invalid("channel must be 0-16 and number 0-127"));
        }
        if self.min == self.max {
            return Err(invalid("min and max must differ"));
        }
        if self.mode == MapMode::Absolute14
            && (self.kind != MidiKind::Cc
                || !(0..=127).contains(&self.lsb_number)
                || self.lsb_number == self.number)
        {
            return Err(invalid("absolute14 needs a cc with a distinct lsb"));
        }
        Ok(())
    }
}

/// A parsed controller map: name, bindings and the raw feedback rules.
#[derive(Clone, Debug, PartialEq)]
pub struct MidiMap {
    pub version: u32,
    pub name: String,
    pub mappings: Vec<Mapping>,
    /// Feedback rules, validated on parse.
    pub outputs: Vec<OutputRule>,
}

impl Default for MidiMap {
    fn default() -> Self {
        Self {
            version: MAP_VERSION,
            name: String::new(),
            mappings: Vec::new(),
            outputs: Vec::new(),
        }
    }
}

impl MidiMap {
    /// Parses a map. Fails without producing a partial map when any mapping is invalid.
    pub fn parse(json: &str) -> Result<Self, MidiError> {
        let root_value: Value =
            serde_json::from_str(json).map_err(|_| invalid("Map is not a JSON object"))?;
        let root = root_value
            .as_object()
            .ok_or_else(|| invalid("Map is not a JSON object"))?;
        let list = root
            .get("mappings")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("Map has no \"mappings\" array"))?;

        let version = match root.get("version") {
            Some(v) => number_of(Some(v)) as i64,
            None => 1,
        };
        if !(1..=i64::from(MAP_VERSION)).contains(&version) {
            return Err(invalid(format!("Map version {version} is not supported")));
        }

        let mappings = list
            .iter()
            .enumerate()
            .map(|(i, v)| {
                parse_mapping(v).map_err(|e| invalid(format!("mapping {i}: {}", e.message())))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let outputs = root
            .get("outputs")
            .and_then(Value::as_array)
            .map(|list| parse_outputs(list))
            .transpose()?
            .unwrap_or_default();

        Ok(Self {
            version: version as u32,
            name: text_of(root.get("name")),
            mappings,
            outputs,
        })
    }
}

/// Parses every feedback rule; the first failure names its index, as the C++ build does.
fn parse_outputs(list: &[Value]) -> Result<Vec<OutputRule>, MidiError> {
    list.iter()
        .enumerate()
        .map(|(i, v)| OutputRule::parse(v).map_err(|e| invalid(format!("output {i}: {e}"))))
        .collect()
}

/// Rule failure while parsing one mapping. Kept apart from `MidiError` so the message stays the
/// bare reason; the caller prefixes the mapping index.
struct RuleError(String);

impl RuleError {
    fn message(&self) -> &str {
        &self.0
    }
}

type RuleResult<T = ()> = Result<T, RuleError>;

fn rule<T>(message: impl Into<String>) -> RuleResult<T> {
    Err(RuleError(message.into()))
}

fn parse_mapping(v: &Value) -> RuleResult<Mapping> {
    let o = v
        .as_object()
        .ok_or_else(|| RuleError("not an object".into()))?;
    let mut m = Mapping::default();
    parse_kind(o, &mut m)?;
    parse_address(o, &mut m)?;
    parse_mode(o, &mut m)?;
    parse_range(o, &mut m)?;
    Ok(m)
}

type Object = serde_json::Map<String, Value>;

fn parse_kind(o: &Object, m: &mut Mapping) -> RuleResult {
    let ty = text_of(o.get("type")).to_lowercase();
    m.kind = match ty.as_str() {
        "cc" => MidiKind::Cc,
        "note" => MidiKind::Note,
        "pitchbend" => MidiKind::PitchBend,
        _ => return rule(format!("unknown type \"{ty}\"")),
    };
    Ok(())
}

fn parse_address(o: &Object, m: &mut Mapping) -> RuleResult {
    m.target = text_of(o.get("target"));
    if m.target.is_empty() {
        return rule("missing target");
    }
    m.channel = number_of(o.get("channel")) as i32;
    if !(0..=16).contains(&m.channel) {
        return rule("channel must be 1-16");
    }
    m.number = if m.kind == MidiKind::PitchBend {
        0
    } else {
        number_of(o.get("number")) as i32
    };
    if !(0..=127).contains(&m.number) {
        return rule("number must be 0-127");
    }
    Ok(())
}

fn parse_mode(o: &Object, m: &mut Mapping) -> RuleResult {
    m.mode = if m.kind == MidiKind::Note {
        MapMode::Button
    } else {
        MapMode::Absolute
    };
    if !o.contains_key("mode") {
        return Ok(());
    }
    let mode = text_of(o.get("mode")).to_lowercase();
    m.mode = match mode.as_str() {
        "absolute" => MapMode::Absolute,
        "relative" => MapMode::Relative,
        "relative2c" => MapMode::RelativeTwosComplement,
        "button" => MapMode::Button,
        "toggle" => MapMode::Toggle,
        "absolute14" => MapMode::Absolute14,
        _ => return rule(format!("unknown mode \"{mode}\"")),
    };
    Ok(())
}

fn parse_range(o: &Object, m: &mut Mapping) -> RuleResult {
    if o.contains_key("min") {
        m.min = number_of(o.get("min"));
    }
    if o.contains_key("max") {
        m.max = number_of(o.get("max"));
    }
    if m.min == m.max {
        return rule("min and max must differ");
    }
    m.step = if o.contains_key("step") {
        number_of(o.get("step"))
    } else {
        (m.max - m.min) / 127.0
    };
    m.initial = if o.contains_key("initial") {
        number_of(o.get("initial"))
    } else {
        m.min
    };
    m.invert = bool_of(o.get("invert"));
    parse_lsb(o, m)
}

fn parse_lsb(o: &Object, m: &mut Mapping) -> RuleResult {
    if m.mode != MapMode::Absolute14 {
        return Ok(());
    }
    if m.kind != MidiKind::Cc {
        return rule("absolute14 needs type cc");
    }
    m.lsb_number = if o.contains_key("lsb") {
        number_of(o.get("lsb")) as i32
    } else {
        m.number + 32
    };
    if !(0..=127).contains(&m.lsb_number) || m.lsb_number == m.number {
        return rule("lsb must be 0-127 and differ from number");
    }
    Ok(())
}

/// Numeric view of a JSON value, the way the C++ `var` conversions read it: numbers as they are,
/// numeric strings parsed, anything else 0.
pub(crate) fn number_of(v: Option<&Value>) -> f64 {
    match v {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(Value::String(s)) => s.trim().parse().unwrap_or(0.0),
        Some(Value::Bool(b)) => f64::from(u8::from(*b)),
        _ => 0.0,
    }
}

/// Text view of a JSON value: strings as they are, missing or null as empty.
pub(crate) fn text_of(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        None | Some(Value::Null) => String::new(),
        Some(other) => other.to_string(),
    }
}

fn bool_of(v: Option<&Value>) -> bool {
    match v {
        Some(Value::Bool(b)) => *b,
        other => number_of(other) != 0.0,
    }
}

pub(crate) fn invalid(message: impl Into<String>) -> MidiError {
    MidiError::Invalid(message.into())
}

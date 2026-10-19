//! One controller feedback rule (an LED or ring driven by engine state). Rules live in the map's
//! `outputs` array. Port of `src/controllers/MidiOutputRule.*`; the schema and its error messages
//! are the C++ ones.
//!
//! ```json
//! { "source": "deck.0.play", "type": "note", "channel": 1, "number": 36, "on": 127, "off": 0 }
//! { "source": "deck.0.cue", "type": "note", "number": 37, "blink": "on", "blinkMs": 250 }
//! { "source": "mixer.master.level", "type": "cc", "number": 8, "mode": "level", "off": 0, "on": 127 }
//! ```
//!
//! `source`: a state id read through a [`om_core::StateReader`]. `type`: note or cc. `channel`:
//! 1-16 (default 1). `mode`: bool (default; state >= threshold is "on") or level (state in
//! [min, max] maps linearly onto off..on). `on`/`off`: 0-127 (defaults 127/0). `blink` (bool mode):
//! "on" blinks while the state is on, "off" while it is off; `blinkMs` is the half period.

use crate::error::MidiError;
use crate::map::{invalid, number_of, text_of};
use crate::message::{control_change, note_off, note_on};
use serde_json::{json, Map, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputKind {
    Note,
    Cc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputMode {
    Bool,
    Level,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blink {
    None,
    WhenOn,
    WhenOff,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OutputRule {
    pub source: String,
    pub kind: OutputKind,
    /// 1-16.
    pub channel: i32,
    /// 0-127.
    pub number: i32,
    pub mode: OutputMode,
    pub on_value: i32,
    pub off_value: i32,
    pub threshold: f64,
    pub min: f64,
    pub max: f64,
    pub blink: Blink,
    pub blink_ms: i32,
}

impl Default for OutputRule {
    fn default() -> Self {
        Self {
            source: String::new(),
            kind: OutputKind::Note,
            channel: 1,
            number: 0,
            mode: OutputMode::Bool,
            on_value: 127,
            off_value: 0,
            threshold: 0.5,
            min: 0.0,
            max: 1.0,
            blink: Blink::None,
            blink_ms: 250,
        }
    }
}

type Object = Map<String, Value>;

impl OutputRule {
    /// Parses and validates one rule. Fails, without a partial rule, for an invalid one.
    pub fn parse(v: &Value) -> Result<Self, MidiError> {
        let o = v.as_object().ok_or_else(|| invalid("not an object"))?;
        let mut rule = Self::default();
        parse_fields(o, &mut rule)?;
        parse_behaviour(o, &mut rule)?;
        Ok(rule)
    }

    /// The canonical JSON form. `parse(to_value())` gives the same rule for any parsed rule.
    pub fn to_value(&self) -> Value {
        let kind = match self.kind {
            OutputKind::Cc => "cc",
            OutputKind::Note => "note",
        };
        let mut o = Map::new();
        o.insert("source".into(), json!(self.source));
        o.insert("type".into(), json!(kind));
        o.insert("channel".into(), json!(self.channel));
        o.insert("number".into(), json!(self.number));
        o.insert("on".into(), json!(self.on_value));
        o.insert("off".into(), json!(self.off_value));
        if self.mode == OutputMode::Level {
            o.insert("mode".into(), json!("level"));
            o.insert("min".into(), json!(self.min));
            o.insert("max".into(), json!(self.max));
            return Value::Object(o);
        }
        o.insert("threshold".into(), json!(self.threshold));
        match self.blink {
            Blink::None => {}
            Blink::WhenOn => {
                o.insert("blink".into(), json!("on"));
                o.insert("blinkMs".into(), json!(self.blink_ms));
            }
            Blink::WhenOff => {
                o.insert("blink".into(), json!("off"));
                o.insert("blinkMs".into(), json!(self.blink_ms));
            }
        }
        Value::Object(o)
    }

    /// Controller value (0-127) for `state` at `now_ms` (any monotonic clock, milliseconds).
    pub fn evaluate(&self, state: f64, now_ms: f64) -> i32 {
        if self.mode == OutputMode::Level {
            let norm = ((state - self.min) / (self.max - self.min)).clamp(0.0, 1.0);
            let value =
                f64::from(self.off_value) + norm * f64::from(self.on_value - self.off_value);
            return value.round() as i32;
        }
        let on = state >= self.threshold;
        let blinking = match self.blink {
            Blink::WhenOn => on,
            Blink::WhenOff => !on,
            Blink::None => false,
        };
        if !blinking {
            return if on { self.on_value } else { self.off_value };
        }
        // Lit on even half periods, so every blinking rule shares one phase.
        let phase = (now_ms / f64::from(self.blink_ms)).floor() as i64;
        if phase % 2 == 0 {
            self.on_value
        } else {
            self.off_value
        }
    }

    /// The raw MIDI message for a controller value. Note rules send note-off for 0.
    pub fn to_message(&self, value: i32) -> Vec<u8> {
        let v = value.clamp(0, 127) as u8;
        let channel = self.channel.clamp(1, 16) as u8;
        match self.kind {
            OutputKind::Cc => control_change(channel, self.number as u8, v),
            OutputKind::Note if v > 0 => note_on(channel, self.number as u8, v),
            OutputKind::Note => note_off(channel, self.number as u8),
        }
    }
}

fn parse_fields(o: &Object, r: &mut OutputRule) -> Result<(), MidiError> {
    r.source = text_of(o.get("source"));
    if r.source.is_empty() {
        return Err(invalid("missing source"));
    }
    let ty = text_of(o.get("type")).to_lowercase();
    r.kind = match ty.as_str() {
        "note" => OutputKind::Note,
        "cc" => OutputKind::Cc,
        _ => return Err(invalid("output type must be note or cc")),
    };
    r.channel = if o.contains_key("channel") {
        number_of(o.get("channel")) as i32
    } else {
        1
    };
    r.number = number_of(o.get("number")) as i32;
    if o.contains_key("on") {
        r.on_value = number_of(o.get("on")) as i32;
    }
    if o.contains_key("off") {
        r.off_value = number_of(o.get("off")) as i32;
    }
    let in_range = |v: i32, lo: i32, hi: i32| (lo..=hi).contains(&v);
    if !in_range(r.channel, 1, 16)
        || !in_range(r.number, 0, 127)
        || !in_range(r.on_value, 0, 127)
        || !in_range(r.off_value, 0, 127)
    {
        return Err(invalid("channel must be 1-16, number/on/off 0-127"));
    }
    Ok(())
}

fn parse_behaviour(o: &Object, r: &mut OutputRule) -> Result<(), MidiError> {
    let mode = text_of(o.get("mode")).to_lowercase();
    if mode == "level" {
        r.mode = OutputMode::Level;
    } else if !mode.is_empty() && mode != "bool" {
        return Err(invalid(format!("unknown output mode \"{mode}\"")));
    }

    let blink = text_of(o.get("blink")).to_lowercase();
    match blink.as_str() {
        "on" => r.blink = Blink::WhenOn,
        "off" => r.blink = Blink::WhenOff,
        "" => {}
        _ => return Err(invalid("blink must be \"on\" or \"off\"")),
    }

    if o.contains_key("threshold") {
        r.threshold = number_of(o.get("threshold"));
    }
    if o.contains_key("min") {
        r.min = number_of(o.get("min"));
    }
    if o.contains_key("max") {
        r.max = number_of(o.get("max"));
    }
    if o.contains_key("blinkMs") {
        r.blink_ms = number_of(o.get("blinkMs")) as i32;
    }
    if r.min == r.max || r.blink_ms < 1 {
        return Err(invalid("min and max must differ, blinkMs must be positive"));
    }
    Ok(())
}

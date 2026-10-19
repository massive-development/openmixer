//! Recorded MIDI stream: `{"name": ..., "events": [{"time": seconds, "bytes": "B0 07 64"}, ...]}`.
//! Replayed headlessly through a [`MidiMapper`] to test mappings without hardware.

use crate::error::MidiError;
use crate::mapper::MidiMapper;
use crate::message::message_length;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

#[derive(Clone, Debug, PartialEq)]
pub struct TimedMidi {
    /// Seconds from the start of the trace.
    pub time: f64,
    /// Raw message bytes, status first.
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MidiTrace {
    pub name: String,
    pub events: Vec<TimedMidi>,
}

impl MidiTrace {
    pub fn add(&mut self, time: f64, bytes: Vec<u8>) {
        self.events.push(TimedMidi { time, bytes });
    }

    pub fn from_json(json: &str) -> Result<Self, MidiError> {
        let root_value: Value =
            serde_json::from_str(json).map_err(|_| invalid("Trace is not a JSON object"))?;
        let root = root_value
            .as_object()
            .ok_or_else(|| invalid("Trace is not a JSON object"))?;
        let list = root
            .get("events")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("Trace has no \"events\" array"))?;

        let events = list
            .iter()
            .enumerate()
            .map(|(i, v)| parse_event(v).map_err(|e| invalid(format!("event {i}: {e}"))))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            name: root
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            events,
        })
    }

    pub fn from_file(path: &Path) -> Result<Self, MidiError> {
        if !path.is_file() {
            return Err(MidiError::NotFound(path.to_path_buf()));
        }
        Self::from_json(&fs::read_to_string(path)?)
    }

    pub fn to_json(&self) -> String {
        let events: Vec<Value> = self
            .events
            .iter()
            .map(|e| {
                let hex: Vec<String> = e.bytes.iter().map(|b| format!("{b:02X}")).collect();
                json!({ "time": e.time, "bytes": hex.join(" ") })
            })
            .collect();
        serde_json::to_string_pretty(&json!({ "name": self.name, "events": events }))
            .unwrap_or_default()
    }
}

/// Parses a hex dump such as `"B0 07 7F"` (whitespace optional). `None` when it is empty or not
/// hex.
pub fn parse_hex(text: &str) -> Option<Vec<u8>> {
    let digits: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if digits.is_empty() || !digits.len().is_multiple_of(2) || !digits.is_ascii() {
        return None;
    }
    digits
        .as_bytes()
        .chunks(2)
        .map(|pair| {
            let s = std::str::from_utf8(pair).ok()?;
            u8::from_str_radix(s, 16).ok()
        })
        .collect()
}

/// Why one trace event is rejected; `from_json` prefixes the event index.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TraceError {
    #[error("not an object")]
    NotObject,
    #[error("missing or invalid bytes")]
    BadBytes,
    #[error("first byte is not a status byte")]
    NotStatus,
    #[error("wrong message length")]
    WrongLength,
}

fn parse_event(v: &Value) -> Result<TimedMidi, TraceError> {
    let o = v.as_object().ok_or(TraceError::NotObject)?;
    let bytes = o
        .get("bytes")
        .and_then(Value::as_str)
        .and_then(parse_hex)
        .ok_or(TraceError::BadBytes)?;

    let status = bytes[0];
    if status & 0x80 == 0 {
        return Err(TraceError::NotStatus);
    }
    // Sysex length is variable, so only fixed-length messages are length-checked.
    if let Some(len) = message_length(status) {
        if len != bytes.len() {
            return Err(TraceError::WrongLength);
        }
    }
    let time = o.get("time").and_then(Value::as_f64).unwrap_or(0.0);
    Ok(TimedMidi { time, bytes })
}

fn invalid(message: impl Into<String>) -> MidiError {
    MidiError::Invalid(message.into())
}

/// Replays as fast as possible; returns the total number of parameter updates.
pub fn replay_trace(trace: &MidiTrace, mapper: &MidiMapper) -> usize {
    trace
        .events
        .iter()
        .map(|e| mapper.handle_message(&e.bytes))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reason(json: &str) -> String {
        match MidiTrace::from_json(json) {
            Err(MidiError::Invalid(message)) => message,
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn each_bad_event_names_its_index_and_rule() {
        let event = |body: &str| format!(r#"{{"events":[{{"time":0,{body}}}]}}"#);
        assert_eq!(reason(r#"{"events":[1]}"#), "event 0: not an object");
        assert_eq!(
            reason(&event(r#""bytes":"zz""#)),
            "event 0: missing or invalid bytes"
        );
        assert_eq!(
            reason(&event(r#""bytes":"07 7F""#)),
            "event 0: first byte is not a status byte"
        );
        assert_eq!(
            reason(&event(r#""bytes":"B0 07""#)),
            "event 0: wrong message length"
        );
    }

    #[test]
    fn a_valid_event_parses() {
        let json = r#"{"events":[{"time":1.5,"bytes":"B0 07 7F"}]}"#;
        let trace = MidiTrace::from_json(json).unwrap();
        assert_eq!(
            trace.events,
            vec![TimedMidi {
                time: 1.5,
                bytes: vec![0xB0, 0x07, 0x7F]
            }]
        );
    }
}

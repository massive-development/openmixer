//! `--ui-drive-out=<path>`: what an outside driver (`tools/ui-drive.py`) needs to press the window's
//! controls and read back what happened. The front end sends the controls' rectangles once its
//! layout settles (the `ui_layout` command, on the main thread); the control thread rewrites the
//! engine's state to `<path>.state` at 10 Hz. Nothing here runs without the flag.

use crate::controls::ControlDto;
use om_core::{ChannelSnapshot, DeckSnapshot, EngineSnapshot};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Why a layout map or a state file was not written.
#[derive(Debug, thiserror::Error)]
pub enum DriveError {
    #[error("malformed layout map: {0}")]
    Malformed(#[from] serde_json::Error),
    #[error("layout map field {0} is not a finite number")]
    NonFinite(String),
    #[error("layout map field {0} is negative")]
    Negative(String),
    #[error("layout map has an empty control key")]
    EmptyKey,
    #[error("could not write {path}: {source}")]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// The two files a drive run writes: the layout map at the named path, the state beside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriveOut {
    pub layout: PathBuf,
    pub state: PathBuf,
}

impl DriveOut {
    /// The files for `--ui-drive-out=<path>`: `<path>` and `<path>.state`.
    pub fn new(path: PathBuf) -> Self {
        let mut state = OsString::from(path.as_os_str());
        state.push(".state");
        Self {
            layout: path,
            state: PathBuf::from(state),
        }
    }
}

/// One control's box in CSS px, relative to the window's content area.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// The content area's size in CSS px.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Viewport {
    pub width: f64,
    pub height: f64,
}

/// `LayoutMap` in `web/src/lib/drive.ts`: every control with a `data-control` key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LayoutMap {
    pub device_pixel_ratio: f64,
    pub viewport: Viewport,
    pub controls: BTreeMap<String, ControlRect>,
}

/// The layout file: the front end's map and every control's spec (the `controls` command's), so a
/// driver takes ranges, defaults and laws from the shell rather than its own copy.
#[derive(Debug, Serialize)]
pub struct DriveLayout<'a> {
    #[serde(flatten)]
    pub layout: &'a LayoutMap,
    pub specs: &'a HashMap<String, ControlDto>,
}

/// The map in `value`, refused when a field is missing, extra, mistyped, not finite or negative.
pub fn parse_layout(value: serde_json::Value) -> Result<LayoutMap, DriveError> {
    let map: LayoutMap = serde_json::from_value(value)?;
    size("devicePixelRatio", map.device_pixel_ratio)?;
    size("viewport.width", map.viewport.width)?;
    size("viewport.height", map.viewport.height)?;
    map.controls
        .iter()
        .try_for_each(|(key, rect)| check_rect(key, rect))?;
    Ok(map)
}

fn check_rect(key: &str, rect: &ControlRect) -> Result<(), DriveError> {
    if key.is_empty() {
        return Err(DriveError::EmptyKey);
    }
    finite(&format!("{key}.x"), rect.x)?;
    finite(&format!("{key}.y"), rect.y)?;
    size(&format!("{key}.width"), rect.width)?;
    size(&format!("{key}.height"), rect.height)
}

// A position may be negative (scrolled or off the left edge); it must be a number.
fn finite(field: &str, value: f64) -> Result<(), DriveError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(DriveError::NonFinite(field.to_owned()))
    }
}

fn size(field: &str, value: f64) -> Result<(), DriveError> {
    finite(field, value)?;
    if value < 0.0 {
        return Err(DriveError::Negative(field.to_owned()));
    }
    Ok(())
}

/// One deck as the driver reads it; `bpm` is the deck's beat grid tempo, null without a grid.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DeckState {
    pub loaded: bool,
    pub playing: bool,
    pub sync: bool,
    pub looping: bool,
    pub position_frames: i64,
    pub length_frames: i64,
    pub file_sample_rate: f64,
    pub rate: f64,
    pub loop_start: i64,
    pub loop_end: i64,
    pub hotcues: Vec<Option<i64>>,
    pub bpm: Option<f64>,
}

/// One channel strip as the driver reads it; `eq` is low, mid, high.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChannelState {
    pub fader: f32,
    pub trim: f32,
    pub filter: f32,
    pub eq: [f32; 3],
}

/// The engine as `<path>.state` holds it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DriveState {
    /// `EngineSnapshot::built`: a newer file has a larger number.
    pub built: u64,
    pub decks: Vec<DeckState>,
    pub channels: Vec<ChannelState>,
    pub crossfader: f32,
    pub master_gain: f32,
}

/// The state of `snapshot`; `bpms[d]` is deck `d`'s grid tempo (missing reads as none).
pub fn drive_state(snapshot: &EngineSnapshot, bpms: &[Option<f64>]) -> DriveState {
    let decks = snapshot.decks.iter().enumerate();
    DriveState {
        built: snapshot.built,
        decks: decks
            .map(|(d, deck)| deck_state(deck, bpms.get(d).copied().flatten()))
            .collect(),
        channels: snapshot.mixer.channels.iter().map(channel_state).collect(),
        crossfader: snapshot.mixer.crossfader,
        master_gain: snapshot.mixer.master_gain,
    }
}

fn deck_state(deck: &DeckSnapshot, bpm: Option<f64>) -> DeckState {
    DeckState {
        loaded: deck.loaded,
        playing: deck.playing,
        sync: deck.sync,
        looping: deck.looping,
        position_frames: deck.position_frames,
        length_frames: deck.length_frames,
        file_sample_rate: deck.file_sample_rate,
        rate: deck.rate,
        loop_start: deck.loop_start,
        loop_end: deck.loop_end,
        hotcues: deck.hotcues.to_vec(),
        bpm,
    }
}

fn channel_state(channel: &ChannelSnapshot) -> ChannelState {
    ChannelState {
        fader: channel.fader,
        trim: channel.trim,
        filter: channel.filter,
        eq: channel.eq,
    }
}

/// Writes `value` as JSON to `path` through a temporary file and a rename, so a reader never
/// sees half a file.
pub fn write_json_atomic(path: &Path, value: &impl Serialize) -> Result<(), DriveError> {
    let write_error = |source| DriveError::Write {
        path: path.to_path_buf(),
        source,
    };
    let json = serde_json::to_vec(value)?;
    let mut temp = OsString::from(path.as_os_str());
    temp.push(".tmp");
    std::fs::write(&temp, json).map_err(write_error)?;
    std::fs::rename(&temp, path).map_err(write_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn layout() -> serde_json::Value {
        json!({
            "devicePixelRatio": 1.0,
            "viewport": { "width": 1600.0, "height": 780.0 },
            "controls": {
                "deck0.play": { "x": 10.0, "y": 700.0, "width": 80.0, "height": 40.0 },
                "deck0.lane": { "x": -0.5, "y": 150.0, "width": 1600.0, "height": 90.0 }
            }
        })
    }

    #[test]
    fn a_full_layout_parses() {
        let map = parse_layout(layout()).unwrap();
        assert_eq!(map.controls.len(), 2);
        assert_eq!(map.controls["deck0.play"].width, 80.0);
        assert_eq!(map.viewport.height, 780.0);
    }

    #[test]
    fn a_malformed_layout_is_an_error_not_a_panic() {
        for bad in [
            json!("not an object"),
            json!(null),
            json!({ "devicePixelRatio": 1.0 }),
            json!({ "devicePixelRatio": "1", "viewport": { "width": 1, "height": 1 }, "controls": {} }),
        ] {
            assert!(matches!(parse_layout(bad), Err(DriveError::Malformed(_))));
        }
        let mut extra = layout();
        extra["extra"] = json!(1);
        assert!(matches!(parse_layout(extra), Err(DriveError::Malformed(_))));
        let mut rect = layout();
        rect["controls"]["deck0.play"]["depth"] = json!(1);
        assert!(matches!(parse_layout(rect), Err(DriveError::Malformed(_))));
    }

    #[test]
    fn a_non_finite_number_sent_as_null_is_refused() {
        // `JSON.stringify` turns NaN and Infinity into null.
        let mut value = layout();
        value["controls"]["deck0.play"]["x"] = serde_json::Value::Null;
        assert!(matches!(parse_layout(value), Err(DriveError::Malformed(_))));
    }

    #[test]
    fn a_non_finite_number_is_refused() {
        assert!(matches!(
            finite("x", f64::NAN),
            Err(DriveError::NonFinite(_))
        ));
        let rect = ControlRect {
            x: 0.0,
            y: f64::INFINITY,
            width: 1.0,
            height: 1.0,
        };
        assert!(matches!(
            check_rect("k", &rect),
            Err(DriveError::NonFinite(f)) if f == "k.y"
        ));
    }

    #[test]
    fn a_negative_size_or_scale_is_refused() {
        let mut value = layout();
        value["controls"]["deck0.play"]["height"] = json!(-1.0);
        assert!(
            matches!(parse_layout(value), Err(DriveError::Negative(f)) if f == "deck0.play.height")
        );
        let mut value = layout();
        value["viewport"]["width"] = json!(-1.0);
        assert!(matches!(parse_layout(value), Err(DriveError::Negative(_))));
        let mut value = layout();
        value["devicePixelRatio"] = json!(-2.0);
        assert!(matches!(parse_layout(value), Err(DriveError::Negative(_))));
    }

    #[test]
    fn an_empty_key_is_refused() {
        let mut value = layout();
        value["controls"][""] = json!({ "x": 0, "y": 0, "width": 1, "height": 1 });
        assert!(matches!(parse_layout(value), Err(DriveError::EmptyKey)));
    }

    #[test]
    fn the_layout_file_carries_the_map_and_the_specs() {
        let map = parse_layout(layout()).unwrap();
        let specs = HashMap::new();
        let value = serde_json::to_value(DriveLayout {
            layout: &map,
            specs: &specs,
        })
        .unwrap();
        assert_eq!(value["devicePixelRatio"], json!(1.0));
        assert_eq!(value["controls"]["deck0.play"]["x"], json!(10.0));
        assert_eq!(value["specs"], json!({}));
    }

    #[test]
    fn the_state_file_sits_beside_the_layout_map() {
        let out = DriveOut::new(PathBuf::from("/x/drive.json"));
        assert_eq!(out.layout, PathBuf::from("/x/drive.json"));
        assert_eq!(out.state, PathBuf::from("/x/drive.json.state"));
    }

    fn snapshot() -> EngineSnapshot {
        let (engine, _audio) = om_engine::DjEngine::new(2);
        let mut snapshot = engine.snapshot();
        let deck = &mut snapshot.decks[0];
        deck.playing = true;
        deck.loop_start = 100;
        deck.loop_end = 200;
        deck.hotcues[2] = Some(48_000);
        snapshot.mixer.channels[1].eq = [-3.0, 0.0, 6.0];
        snapshot.mixer.crossfader = 0.25;
        snapshot
    }

    #[test]
    fn the_state_serialises_every_field_the_driver_reads() {
        let state = drive_state(&snapshot(), &[Some(124.0)]);
        let value = serde_json::to_value(&state).unwrap();
        let deck = &value["decks"][0];
        for field in [
            "loaded",
            "playing",
            "sync",
            "looping",
            "position_frames",
            "length_frames",
            "file_sample_rate",
            "rate",
            "loop_start",
            "loop_end",
            "hotcues",
            "bpm",
        ] {
            assert!(deck.get(field).is_some(), "deck field {field}");
        }
        assert_eq!(deck["playing"], json!(true));
        assert_eq!(deck["hotcues"][2], json!(48_000));
        assert_eq!(deck["hotcues"][0], serde_json::Value::Null);
        assert_eq!(deck["bpm"], json!(124.0));
        assert_eq!(value["decks"][1]["bpm"], serde_json::Value::Null);
        assert_eq!(value["channels"][1]["eq"], json!([-3.0, 0.0, 6.0]));
        for field in ["fader", "trim", "filter", "eq"] {
            assert!(
                value["channels"][0].get(field).is_some(),
                "channel field {field}"
            );
        }
        assert_eq!(value["crossfader"], json!(0.25));
        assert!(value.get("master_gain").is_some());
    }

    #[test]
    fn the_state_is_written_whole_and_replaced() {
        let dir = std::env::temp_dir().join(format!("om-ui-tauri-drive-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("drive.json.state");
        write_json_atomic(&path, &drive_state(&snapshot(), &[])).unwrap();
        write_json_atomic(&path, &json!({ "second": true })).unwrap();
        let back: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(back, json!({ "second": true }));
        assert!(!dir.join("drive.json.state.tmp").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_write_into_a_missing_folder_is_an_error() {
        let path = std::env::temp_dir()
            .join("om-ui-tauri-no-such-dir-xyz")
            .join("s.json");
        assert!(matches!(
            write_json_atomic(&path, &json!(1)),
            Err(DriveError::Write { .. })
        ));
    }
}

//! Editable beat grid: tempo markers, each starting a constant-tempo segment. Positions are source
//! frames at the track's own sample rate; beat 0 (and every multiple of `beats_per_bar`) is a
//! downbeat. Edits mark the grid manual so re-analysis can leave user corrections alone. Port of
//! `src/analysis/BeatGridModel.{h,cpp}` and `AnalysedBeatGrid.h`.

use om_engine::grid::BeatGrid;
use serde_json::{json, Map, Value};
use std::sync::Arc;

/// Version written to and required from the JSON form.
const FORMAT_VERSION: f64 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TempoMarker {
    pub frame: f64,
    pub beat: f64,
    pub bpm: f64,
}

fn valid_bpm(bpm: f64) -> bool {
    bpm.is_finite() && bpm > 1.0 && bpm < 1000.0
}

/// Value type; not thread-safe in the sense that edits need `&mut`. Copy it, or hand an
/// [`AnalysedBeatGrid`] snapshot to the engine.
#[derive(Debug, Clone, PartialEq)]
pub struct BeatGridModel {
    markers: Vec<TempoMarker>,
    sample_rate: f64,
    beats_per_bar: i32,
    manual: bool,
}

impl Default for BeatGridModel {
    fn default() -> Self {
        Self {
            markers: Vec::new(),
            sample_rate: 0.0,
            beats_per_bar: 4,
            manual: false,
        }
    }
}

impl BeatGridModel {
    /// One marker at `downbeat_frame` (beat 0) for constant tempo `bpm`. Empty when `bpm` or the
    /// rate is unusable.
    pub fn constant(bpm: f64, sample_rate: f64, downbeat_frame: f64, beats_per_bar: i32) -> Self {
        let mut grid = Self {
            sample_rate,
            beats_per_bar: beats_per_bar.max(1),
            ..Self::default()
        };
        if valid_bpm(bpm) && sample_rate > 0.0 {
            grid.markers.push(TempoMarker {
                frame: downbeat_frame,
                beat: 0.0,
                bpm,
            });
        }
        grid
    }

    pub fn is_valid(&self) -> bool {
        !self.markers.is_empty() && self.sample_rate > 0.0
    }

    pub fn is_manual(&self) -> bool {
        self.manual
    }

    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    pub fn beats_per_bar(&self) -> i32 {
        self.beats_per_bar
    }

    pub fn markers(&self) -> &[TempoMarker] {
        &self.markers
    }

    /// Tempo of the first marker, or 0 when empty.
    pub fn bpm(&self) -> f64 {
        self.markers.first().map_or(0.0, |m| m.bpm)
    }

    pub fn bpm_at_frame(&self, frame: f64) -> f64 {
        if self.markers.is_empty() {
            0.0
        } else {
            self.segment_for_frame(frame).bpm
        }
    }

    pub fn beat_at_frame(&self, frame: f64) -> f64 {
        if !self.is_valid() {
            return 0.0;
        }
        let m = *self.segment_for_frame(frame);
        m.beat + (frame - m.frame) / self.frames_per_beat(&m)
    }

    pub fn frame_at_beat(&self, beat: f64) -> f64 {
        if !self.is_valid() {
            return 0.0;
        }
        let m = *self.segment_for_beat(beat);
        m.frame + (beat - m.beat) * self.frames_per_beat(&m)
    }

    /// Keeps the first marker and drops tempo changes.
    pub fn set_bpm(&mut self, bpm: f64) {
        if !self.is_valid() || !valid_bpm(bpm) {
            return;
        }
        self.markers.truncate(1);
        self.markers[0].bpm = bpm;
        self.manual = true;
    }

    /// Moves the whole grid by `frames`.
    pub fn shift(&mut self, frames: f64) {
        for m in &mut self.markers {
            m.frame += frames;
        }
        self.manual = true;
    }

    /// The beat nearest `frame` becomes a downbeat.
    pub fn set_downbeat_near(&mut self, frame: f64) {
        if !self.is_valid() {
            return;
        }
        let bar = f64::from(self.beats_per_bar);
        let nearest = self.beat_at_frame(frame).round();
        let offset = nearest - bar * (nearest / bar).floor();
        for m in &mut self.markers {
            m.beat -= offset;
        }
        self.manual = true;
    }

    /// Starts a new tempo segment at `frame`, or replaces the tempo of a marker already there.
    /// Beats stay continuous across the change.
    pub fn insert_tempo_change(&mut self, frame: f64, bpm: f64) {
        if !self.is_valid() || !valid_bpm(bpm) {
            return;
        }
        let at = self.markers.partition_point(|m| m.frame < frame);
        if at < self.markers.len() && self.markers[at].frame == frame {
            self.markers[at].bpm = bpm;
        } else {
            // Beat is taken from the grid before the insert.
            let beat = self.beat_at_frame(frame);
            self.markers.insert(at, TempoMarker { frame, beat, bpm });
        }

        for i in 1..self.markers.len() {
            let prev = self.markers[i - 1];
            self.markers[i].beat =
                prev.beat + (self.markers[i].frame - prev.frame) / self.frames_per_beat(&prev);
        }
        self.manual = true;
    }

    pub fn set_beats_per_bar(&mut self, beats: i32) {
        self.beats_per_bar = beats.max(1);
        self.manual = true;
    }

    pub fn set_manual(&mut self, is_manual: bool) {
        self.manual = is_manual;
    }

    /// JSON form. Field names match the C++ `BeatGridModel::toVar`, so cached payloads stay readable.
    pub fn to_json(&self) -> Value {
        let markers: Vec<Value> = self
            .markers
            .iter()
            .map(|m| json!({ "frame": m.frame, "beat": m.beat, "bpm": m.bpm }))
            .collect();
        let mut obj = Map::new();
        obj.insert("version".into(), json!(FORMAT_VERSION));
        obj.insert("sampleRate".into(), json!(self.sample_rate));
        obj.insert("beatsPerBar".into(), json!(self.beats_per_bar));
        obj.insert("manual".into(), json!(self.manual));
        obj.insert("markers".into(), Value::Array(markers));
        Value::Object(obj)
    }

    /// Parses and validates the JSON form. `None` for a wrong version, no markers, an invalid
    /// marker, markers out of frame order, or a non-positive sample rate.
    pub fn from_json(v: &Value) -> Option<Self> {
        let list = v.get("markers")?.as_array()?;
        if v.get("version").and_then(Value::as_f64) != Some(FORMAT_VERSION) || list.is_empty() {
            return None;
        }

        let sample_rate = v.get("sampleRate").and_then(Value::as_f64).unwrap_or(0.0);
        let beats_per_bar =
            (v.get("beatsPerBar").and_then(Value::as_f64).unwrap_or(0.0) as i32).max(1);
        let manual = v.get("manual").and_then(Value::as_bool).unwrap_or(false);

        let mut grid = Self {
            markers: Vec::with_capacity(list.len()),
            sample_rate,
            beats_per_bar,
            manual,
        };
        for item in list {
            let m = marker_from_json(item)?;
            if grid
                .markers
                .last()
                .is_some_and(|last| m.frame <= last.frame)
            {
                return None;
            }
            grid.markers.push(m);
        }
        (grid.sample_rate > 0.0).then_some(grid)
    }

    fn frames_per_beat(&self, m: &TempoMarker) -> f64 {
        self.sample_rate * 60.0 / m.bpm
    }

    /// Last marker whose frame is at or before `frame`; the first marker before the start.
    /// Requires a non-empty grid.
    fn segment_for_frame(&self, frame: f64) -> &TempoMarker {
        let at = self.markers.partition_point(|m| m.frame <= frame);
        &self.markers[at.saturating_sub(1)]
    }

    fn segment_for_beat(&self, beat: f64) -> &TempoMarker {
        let at = self.markers.partition_point(|m| m.beat <= beat);
        &self.markers[at.saturating_sub(1)]
    }
}

fn marker_from_json(v: &Value) -> Option<TempoMarker> {
    let frame = v.get("frame")?.as_f64()?;
    let beat = v.get("beat")?.as_f64()?;
    let bpm = v.get("bpm")?.as_f64()?;
    (valid_bpm(bpm) && frame.is_finite() && beat.is_finite()).then_some(TempoMarker {
        frame,
        beat,
        bpm,
    })
}

/// Immutable [`BeatGridModel`] snapshot exposed through the engine's [`BeatGrid`] interface, so
/// sync and quantize can use analysed or user-corrected grids. Thread-safe because it never changes.
#[derive(Debug, Clone, PartialEq)]
pub struct AnalysedBeatGrid {
    model: BeatGridModel,
}

impl AnalysedBeatGrid {
    pub fn new(snapshot: BeatGridModel) -> Self {
        Self { model: snapshot }
    }

    pub fn model(&self) -> &BeatGridModel {
        &self.model
    }
}

impl BeatGrid for AnalysedBeatGrid {
    fn bpm(&self) -> f64 {
        self.model.bpm()
    }

    fn beats_per_bar(&self) -> i32 {
        self.model.beats_per_bar()
    }

    fn beat_at_frame(&self, frame: i64) -> f64 {
        self.model.beat_at_frame(frame as f64)
    }

    fn frame_at_beat(&self, beat: f64) -> i64 {
        self.model.frame_at_beat(beat).round() as i64
    }
}

/// `None` for an invalid model, so callers can hand the result straight to `Sync::set_grid`.
pub fn make_engine_grid(model: &BeatGridModel) -> Option<Arc<dyn BeatGrid>> {
    model
        .is_valid()
        .then(|| Arc::new(AnalysedBeatGrid::new(model.clone())) as Arc<dyn BeatGrid>)
}

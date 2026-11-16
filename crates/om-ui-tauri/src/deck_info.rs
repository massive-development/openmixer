//! The deck data the front end asks for: a deck's track info and its packed waveform peaks. Built
//! on Tauri's IPC threads from the shared deck state and the engine's control side.

use crate::decks::{DeckView, Decks};
use om_engine::grid::BeatGrid;
use om_engine::DjEngine;
use om_ui_bridge::{pack_peaks, peak_levels, PeakLevel};
use serde::Serialize;

/// A deck's track as the front end shows it; camelCase on the wire.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckInfo {
    pub title: String,
    pub artist: String,
    pub bpm: Option<f64>,
    pub key: Option<String>,
    /// Source frame of the grid's beat 0, a downbeat.
    pub first_beat_frame: Option<i64>,
    pub sample_rate: f64,
    pub num_samples: i64,
    /// False for a colourless silhouette (all height in the low byte).
    pub spectral: bool,
    pub levels: Vec<PeakLevelDto>,
    pub hotcues: Vec<Option<i64>>,
    pub generation: u64,
}

/// One level's place in the `deck_peaks` bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeakLevelDto {
    pub samples_per_point: u32,
    pub len: u32,
}

impl From<PeakLevel> for PeakLevelDto {
    fn from(level: PeakLevel) -> Self {
        Self {
            samples_per_point: level.samples_per_point,
            len: level.len,
        }
    }
}

/// Deck `deck`'s info, or why there is none.
pub(crate) fn deck_info(decks: &Decks, engine: &DjEngine, deck: usize) -> Result<DeckInfo, String> {
    let view = read(decks, engine, deck)?;
    let grid = engine.deck_control(deck).grid();
    let hotcues = engine
        .deck_snapshot(deck)
        .map(|s| s.hotcues.to_vec())
        .unwrap_or_default();
    Ok(info_of(view, grid.as_deref(), hotcues))
}

/// Deck `deck`'s packed peaks; empty while it has no waveform.
pub(crate) fn deck_peaks(decks: &Decks, engine: &DjEngine, deck: usize) -> Result<Vec<u8>, String> {
    let view = read(decks, engine, deck)?;
    Ok(view
        .waveform
        .map(|w| pack_peaks(&w).bytes)
        .unwrap_or_default())
}

fn read(decks: &Decks, engine: &DjEngine, deck: usize) -> Result<DeckView, String> {
    let known = deck < engine.num_decks();
    known
        .then(|| decks.read(deck))
        .flatten()
        .ok_or_else(|| format!("no deck {deck}"))
}

// A beat grid on the deck takes precedence for the BPM, as in the GPUI header.
fn info_of(view: DeckView, grid: Option<&dyn BeatGrid>, hotcues: Vec<Option<i64>>) -> DeckInfo {
    let track = view.track;
    let waveform = view.waveform.as_deref();
    DeckInfo {
        bpm: grid
            .map(BeatGrid::bpm)
            .or((track.bpm > 0.0).then_some(track.bpm)),
        key: (!track.key.is_empty()).then_some(track.key),
        first_beat_frame: grid.map(|g| g.frame_at_beat(0.0)),
        title: track.title,
        artist: track.artist,
        sample_rate: waveform.map_or(0.0, |w| w.sample_rate()),
        num_samples: waveform.map_or(0, |w| w.num_samples()),
        spectral: waveform.is_some_and(|w| w.has_spectrum()),
        levels: waveform
            .map(peak_levels)
            .unwrap_or_default()
            .into_iter()
            .map(PeakLevelDto::from)
            .collect(),
        hotcues,
        generation: view.generation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use om_analysis::{WaveformData, WaveformLevel, WaveformPoint};
    use om_engine::grid::FixedTempoGrid;
    use om_ui::widgets::deck::TrackInfo;
    use std::sync::Arc;

    fn waveform() -> Arc<WaveformData> {
        let base = WaveformLevel {
            samples_per_point: 256,
            points: vec![WaveformPoint::default(); 10],
        };
        Arc::new(WaveformData::new(48_000.0, 2560, base))
    }

    fn view() -> DeckView {
        DeckView {
            generation: 3,
            track: TrackInfo {
                title: "Loop".into(),
                artist: "Gen".into(),
                key: "Am".into(),
                camelot: "8A".into(),
                bpm: 124.0,
            },
            waveform: Some(waveform()),
        }
    }

    #[test]
    fn a_loaded_deck_reports_track_grid_and_levels() {
        let grid = FixedTempoGrid::new(126.0, 48_000.0, 1_000);
        let info = info_of(view(), Some(&grid), vec![Some(5), None]);
        assert_eq!((info.title.as_str(), info.artist.as_str()), ("Loop", "Gen"));
        assert_eq!(info.bpm, Some(126.0), "the grid wins");
        assert_eq!(info.key.as_deref(), Some("Am"));
        assert_eq!(info.first_beat_frame, Some(1_000));
        assert_eq!(
            (info.sample_rate, info.num_samples, info.spectral),
            (48_000.0, 2560, true)
        );
        assert_eq!(
            info.levels,
            vec![PeakLevelDto {
                samples_per_point: 256,
                len: 10
            }]
        );
        assert_eq!((info.hotcues, info.generation), (vec![Some(5), None], 3));
    }

    #[test]
    fn without_a_grid_the_track_bpm_is_used_and_unknowns_are_none() {
        let info = info_of(view(), None, Vec::new());
        assert_eq!((info.bpm, info.first_beat_frame), (Some(124.0), None));
        let empty = info_of(DeckView::default(), None, Vec::new());
        assert_eq!((empty.bpm, empty.key, empty.levels.len()), (None, None, 0));
        assert!(!empty.spectral);
    }

    #[test]
    fn the_wire_names_are_camel_case() {
        let json = serde_json::to_string(&info_of(view(), None, vec![None])).unwrap();
        for name in [
            "firstBeatFrame",
            "sampleRate",
            "numSamples",
            "samplesPerPoint",
            "hotcues",
        ] {
            assert!(json.contains(name), "{name} missing from {json}");
        }
    }

    #[test]
    fn an_out_of_range_deck_is_an_error_not_a_panic() {
        let engine = DjEngine::new(2).0;
        let decks = Decks::new(&engine);
        assert_eq!(deck_info(&decks, &engine, 2), Err("no deck 2".to_owned()));
        assert!(deck_peaks(&decks, &engine, usize::MAX).is_err());
        assert_eq!(deck_peaks(&decks, &engine, 0), Ok(Vec::new()));
        let cues = om_core::param_id::HOTCUES_PER_DECK as usize;
        assert_eq!(
            deck_info(&decks, &engine, 0).map(|i| i.hotcues.len()),
            Ok(cues)
        );
    }
}

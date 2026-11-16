//! The deck data the front end asks for: a deck's track info and its packed waveform peaks. Built
//! on Tauri's IPC threads from the shared deck state and the engine's control side.

use crate::decks::{DeckView, Decks};
use om_core::ParameterTarget;
use om_engine::grid::BeatGrid;
use om_engine::DjEngine;
use om_ui::binding::DeckBinding;
use om_ui::widgets::deck::TrackHeader;
use om_ui_bridge::{pack_peaks, peak_levels, PeakLevel};
use serde::Serialize;
use std::sync::Arc;

/// A deck's track as the front end shows it; camelCase on the wire.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckInfo {
    pub title: String,
    /// The title as GPUI's deck header prints it: the track's, or its empty-deck words.
    pub title_text: String,
    /// The key as GPUI's deck header prints it (its default notation), or `--`.
    pub key_text: String,
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
    /// Counts every change to the deck: track, waveform, hot cues.
    pub generation: u64,
    /// Counts waveform changes only; the peaks need fetching again only when it moves.
    pub peaks_generation: u64,
}

/// Bytes before the peaks in a `deck_peaks` answer: the peaks generation, u64 little-endian.
pub const PEAKS_HEADER_LEN: usize = 8;

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
pub(crate) fn deck_info(
    decks: &Decks,
    engine: &Arc<DjEngine>,
    deck: usize,
) -> Result<DeckInfo, String> {
    let view = read(decks, engine, deck)?;
    let grid = engine.deck_control(deck).grid();
    let hotcues = engine
        .deck_snapshot(deck)
        .map(|s| s.hotcues.to_vec())
        .unwrap_or_default();
    let words = header_words(engine, deck, &view);
    Ok(DeckInfo {
        title_text: words.0,
        key_text: words.1,
        ..info_of(view, grid.as_deref(), hotcues)
    })
}

/// The title and key GPUI's header model prints for this track.
fn header_words(engine: &Arc<DjEngine>, deck: usize, view: &DeckView) -> (String, String) {
    let binding = DeckBinding::new(Arc::clone(engine), Arc::new(NoWrites), deck);
    let mut header = TrackHeader::new(binding);
    header.set_track(view.track.clone());
    let text = header.text();
    (text.title, text.key)
}

/// The header model only reads; a write it might attempt goes nowhere.
struct NoWrites;

impl ParameterTarget for NoWrites {
    fn set_parameter(&self, _id: &str, _value: f64) -> bool {
        false
    }
}

/// Deck `deck`'s packed peaks after their generation (read with them, so a late answer is
/// recognised); no peak bytes while it has no waveform.
pub(crate) fn deck_peaks(decks: &Decks, engine: &DjEngine, deck: usize) -> Result<Vec<u8>, String> {
    let view = read(decks, engine, deck)?;
    let bytes = view.waveform.map(|w| pack_peaks(&w).bytes);
    Ok(tagged(view.peaks_generation, &bytes.unwrap_or_default()))
}

fn tagged(peaks_generation: u64, bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(PEAKS_HEADER_LEN + bytes.len());
    out.extend_from_slice(&peaks_generation.to_le_bytes());
    out.extend_from_slice(bytes);
    out
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
        title_text: String::new(),
        key_text: String::new(),
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
        peaks_generation: view.peaks_generation,
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
            peaks_generation: 2,
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
        assert_eq!(info.peaks_generation, 2);
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
            "peaksGeneration",
        ] {
            assert!(json.contains(name), "{name} missing from {json}");
        }
    }

    #[test]
    fn an_out_of_range_deck_is_an_error_not_a_panic() {
        let engine = Arc::new(DjEngine::new(2).0);
        let decks = Decks::new(&engine);
        assert_eq!(deck_info(&decks, &engine, 2), Err("no deck 2".to_owned()));
        assert!(deck_peaks(&decks, &engine, usize::MAX).is_err());
        assert_eq!(
            deck_peaks(&decks, &engine, 0),
            Ok(vec![0; PEAKS_HEADER_LEN])
        );
        let cues = om_core::param_id::HOTCUES_PER_DECK as usize;
        assert_eq!(
            deck_info(&decks, &engine, 0).map(|i| i.hotcues.len()),
            Ok(cues)
        );
    }

    #[test]
    fn peaks_carry_their_generation_first() {
        let out = tagged(0x0102, &[7, 8, 9, 10]);
        assert_eq!(out.len(), PEAKS_HEADER_LEN + 4);
        assert_eq!(u64::from_le_bytes(out[..8].try_into().unwrap()), 0x0102);
        assert_eq!(&out[8..], &[7, 8, 9, 10]);
    }

    #[test]
    fn the_header_words_are_gpui_s() {
        let engine = Arc::new(DjEngine::new(1).0);
        let (title, key) = header_words(&engine, 0, &view());
        assert_eq!((title.as_str(), key.as_str()), ("Loop", "8A"));
        let (empty, none) = header_words(&engine, 0, &DeckView::default());
        assert!(
            !empty.is_empty(),
            "an empty deck prints GPUI's words, not nothing"
        );
        assert_eq!(none, "--");
    }
}

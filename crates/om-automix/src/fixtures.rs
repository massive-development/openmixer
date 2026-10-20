//! Shared builders for the unit tests of this crate.

use crate::types::{DeckInfo, FixedGrid, Section, SectionKind};
use std::sync::Arc;

pub const LENGTH_BEATS: f64 = 480.0;

pub fn sections() -> Vec<Section> {
    let s = |a, b, kind, energy| Section {
        start_beat: a,
        end_beat: b,
        kind,
        energy,
    };
    vec![
        s(0.0, 32.0, SectionKind::Intro, 0.3),
        s(32.0, 400.0, SectionKind::Main, 0.9),
        s(400.0, LENGTH_BEATS, SectionKind::Outro, 0.3),
    ]
}

/// A track on `deck` at `bpm` with grid, sections (outro from beat 400), mix points 0 and 400.
pub fn info(deck: u8, bpm: f64, camelot: &str, stems: bool) -> DeckInfo {
    DeckInfo {
        deck,
        grid: Some(Arc::new(FixedGrid::new(bpm, 44_100.0, 0))),
        camelot: camelot.to_owned(),
        sections: sections(),
        mix_out: Some(400.0),
        mix_in: Some(0.0),
        has_stems: stems,
        length_beats: LENGTH_BEATS,
        bpm,
        loaded_track_id: None,
    }
}

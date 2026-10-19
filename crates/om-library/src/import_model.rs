//! Importer-neutral view of a foreign library: what an importer parses and the writer merges.
//! Plain values only, so importers stay free of database code and are testable on their own.
//! Port of `src/library/ImportModel.h`.

use crate::track_record::TrackRecord;

/// Hotcue or saved loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CueKind {
    #[default]
    Cue,
    Loop,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportedCue {
    pub kind: CueKind,
    /// Hotcue slot 0.., or -1 for a memory cue or loop.
    pub num: i32,
    pub start_sec: f64,
    /// Loops only.
    pub end_sec: f64,
    pub label: String,
    pub colour_argb: u32,
}

impl Default for ImportedCue {
    fn default() -> Self {
        Self {
            kind: CueKind::Cue,
            num: -1,
            start_sec: 0.0,
            end_sec: 0.0,
            label: String::new(),
            colour_argb: 0,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ImportedTrack {
    /// Id the source's playlists use to refer to this track.
    pub key: String,
    /// `record.path` is the decoded local path.
    pub record: TrackRecord,
    /// 0 = unknown (the writer assumes 44100).
    pub sample_rate: f64,
    pub cues: Vec<ImportedCue>,
}

#[derive(Debug, Clone, Default)]
pub struct ImportedPlaylist {
    pub name: String,
    pub is_folder: bool,
    /// Folders only.
    pub children: Vec<ImportedPlaylist>,
    /// Playlists only, in order.
    pub track_keys: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ImportedLibrary {
    pub tracks: Vec<ImportedTrack>,
    /// Top level.
    pub playlists: Vec<ImportedPlaylist>,
}

/// Outcome counts. Skipped = entries that cannot map to a local file; errors = malformed data or
/// a failed database write (which rolls the whole import back and restores the counts from before).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportReport {
    pub tracks_added: usize,
    pub tracks_merged: usize,
    pub tracks_skipped: usize,
    pub cues_imported: usize,
    pub playlists_created: usize,
    pub entries_added: usize,
    pub entries_skipped: usize,
    pub errors: usize,
    /// First few problems, human readable.
    pub messages: Vec<String>,
}

impl ImportReport {
    const MAX_MESSAGES: usize = 20;

    /// Records a problem; a bad export could otherwise flood the report.
    pub fn note(&mut self, message: impl Into<String>) {
        if self.messages.len() < Self::MAX_MESSAGES {
            self.messages.push(message.into());
        }
    }
}

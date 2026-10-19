//! Plain data for one library track row. Value type; safe to copy between threads.

/// One row of the `tracks` table.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrackRecord {
    /// 0 until inserted.
    pub id: i64,
    /// Unique; absolute file path.
    pub path: String,
    /// SHA-256 of the file bytes; matches the analysis cache key.
    pub content_hash: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub year: i32,
    pub duration_sec: f64,
    /// Unset until analysed.
    pub bpm: Option<f64>,
    /// Musical key, for example "Am".
    pub key: String,
    /// Camelot code, for example "8A".
    pub camelot: String,
    /// 0..=5.
    pub rating: i32,
    pub play_count: i32,
    /// 0 on insert means "now".
    pub date_added_ms: i64,
    pub last_played_ms: Option<i64>,
    pub comment: String,
}

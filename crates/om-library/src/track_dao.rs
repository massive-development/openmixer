//! Track table access. Non-owning view of a `Database`: the database must outlive the DAO and the
//! DAO is used from the thread that owns that database.

use crate::database::{Database, Transaction};
use crate::error::{LibraryError, Result};
use crate::track_key::set_key;
use crate::track_record::TrackRecord;
use om_analysis::MusicalKey;
use rusqlite::{params, Params, Row};

/// Column list for `SELECT <SELECT_COLUMNS> FROM tracks`; shared with the search engine.
pub const SELECT_COLUMNS: &str =
    "id, path, content_hash, title, artist, album, genre, year, duration_sec, \
     bpm, musical_key, camelot, rating, play_count, date_added_ms, last_played_ms, comment";

/// Row mapper for a row selected with [`SELECT_COLUMNS`].
pub fn read_row(row: &Row<'_>) -> rusqlite::Result<TrackRecord> {
    Ok(TrackRecord {
        id: row.get(0)?,
        path: row.get(1)?,
        content_hash: row.get(2)?,
        title: row.get(3)?,
        artist: row.get(4)?,
        album: row.get(5)?,
        genre: row.get(6)?,
        year: row.get(7)?,
        duration_sec: row.get(8)?,
        bpm: row.get(9)?,
        key: row.get(10)?,
        camelot: row.get(11)?,
        rating: row.get(12)?,
        play_count: row.get(13)?,
        date_added_ms: row.get(14)?,
        last_played_ms: row.get(15)?,
        comment: row.get(16)?,
    })
}

/// Track reads and writes.
pub struct TrackDao<'a> {
    db: &'a Database,
}

impl<'a> TrackDao<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Fails if the path already exists. Fills `date_added_ms` with now when it is 0.
    pub fn insert(&self, track: &TrackRecord) -> Result<i64> {
        self.db.connection().execute(
            "INSERT INTO tracks (path, content_hash, title, artist, album, genre, year, duration_sec, bpm, \
             musical_key, camelot, rating, play_count, date_added_ms, last_played_ms, comment) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16);",
            params![
                track.path,
                track.content_hash,
                track.title,
                track.artist,
                track.album,
                track.genre,
                track.year,
                track.duration_sec,
                track.bpm,
                track.key,
                track.camelot,
                track.rating,
                track.play_count,
                added_or_now(track),
                track.last_played_ms,
                track.comment,
            ],
        )?;
        Ok(self.db.last_insert_rowid())
    }

    /// Insert or refresh by path. File-derived fields are overwritten; id, rating, play count, last
    /// played and date added are kept. Returns the row id.
    pub fn upsert_by_path(&self, track: &TrackRecord) -> Result<i64> {
        let tx = Transaction::begin(self.db)?;
        // A failed lookup propagates before anything is written.
        let id = match self.find_by_path(&track.path)? {
            Some(existing) => {
                let merged = TrackRecord {
                    id: existing.id,
                    ..track.clone()
                };
                self.update(&merged)?;
                existing.id
            }
            None => self.insert(track)?,
        };
        tx.commit()?;
        Ok(id)
    }

    /// Rewrites the file-derived fields of an existing row (by `id`).
    pub fn update(&self, t: &TrackRecord) -> Result<()> {
        self.run_checked(
            "UPDATE tracks SET content_hash=?2, title=?3, artist=?4, album=?5, genre=?6, year=?7, \
             duration_sec=?8, bpm=?9, musical_key=?10, camelot=?11, comment=?12 WHERE id=?1;",
            params![
                t.id,
                t.content_hash,
                t.title,
                t.artist,
                t.album,
                t.genre,
                t.year,
                t.duration_sec,
                t.bpm,
                t.key,
                t.camelot,
                t.comment,
            ],
        )
    }

    pub fn remove(&self, id: i64) -> Result<()> {
        self.run_checked("DELETE FROM tracks WHERE id=?1;", params![id])
    }

    pub fn find_by_id(&self, id: i64) -> Result<Option<TrackRecord>> {
        self.first("WHERE id = ?1", params![id])
    }

    pub fn find_by_path(&self, path: &str) -> Result<Option<TrackRecord>> {
        self.first("WHERE path = ?1", params![path])
    }

    /// Every track with this content hash, ordered by id.
    pub fn find_by_content_hash(&self, hash: &str) -> Result<Vec<TrackRecord>> {
        self.query("WHERE content_hash = ?1 ORDER BY id", params![hash])
    }

    /// Every track, ordered by id.
    pub fn all(&self) -> Result<Vec<TrackRecord>> {
        self.query("ORDER BY id", [])
    }

    pub fn count(&self) -> Result<i64> {
        Ok(self
            .db
            .connection()
            .query_row("SELECT COUNT(*) FROM tracks;", [], |row| row.get(0))?)
    }

    /// Narrow writer so analysis does not rewrite whole rows.
    pub fn set_analysis(&self, id: i64, bpm: f64, key: &str, camelot: &str) -> Result<()> {
        self.run_checked(
            "UPDATE tracks SET bpm=?2, musical_key=?3, camelot=?4 WHERE id=?1;",
            params![id, bpm, key, camelot],
        )
    }

    /// Writes only the values that are set, leaving the rest of the row alone. `COALESCE` keeps an
    /// unset column's value in one atomic statement.
    pub fn update_analysis(
        &self,
        id: i64,
        bpm: Option<f64>,
        key: Option<&MusicalKey>,
    ) -> Result<()> {
        let (name, camelot) = match key {
            Some(k) => {
                let mut scratch = TrackRecord::default();
                set_key(&mut scratch, k);
                (Some(scratch.key), Some(scratch.camelot))
            }
            None => (None, None),
        };
        self.run_checked(
            "UPDATE tracks SET bpm=COALESCE(?2, bpm), musical_key=COALESCE(?3, musical_key), \
             camelot=COALESCE(?4, camelot) WHERE id=?1;",
            params![id, bpm, name, camelot],
        )
    }

    pub fn set_rating(&self, id: i64, rating: i32) -> Result<()> {
        self.run_checked(
            "UPDATE tracks SET rating=?2 WHERE id=?1;",
            params![id, rating],
        )
    }

    /// Records one play: appends a `play_history` row and bumps the play count and last-played
    /// time on the track. `deck` is the deck it played on, if known. Atomic; an unknown track
    /// changes nothing.
    pub fn record_play(&self, track_id: i64, deck: Option<u8>, played_ms: i64) -> Result<()> {
        let tx = Transaction::begin(self.db)?;
        self.run_checked(
            "UPDATE tracks SET play_count = play_count + 1, last_played_ms=?2 WHERE id=?1;",
            params![track_id, played_ms],
        )?;
        self.db.connection().execute(
            "INSERT INTO play_history (track_id, played_ms, deck) VALUES (?1, ?2, ?3);",
            params![track_id, played_ms, deck.map(i64::from)],
        )?;
        tx.commit()
    }

    /// The most recent plays across the library, newest first.
    pub fn recent_plays(&self, limit: usize) -> Result<Vec<PlayEntry>> {
        self.db.query_rows(
            "SELECT id, track_id, played_ms, deck FROM play_history \
             ORDER BY played_ms DESC, id DESC LIMIT ?1;",
            params![i64::try_from(limit).unwrap_or(i64::MAX)],
            read_play,
        )
    }

    /// Every recorded play of one track, newest first.
    pub fn plays_for(&self, track_id: i64) -> Result<Vec<PlayEntry>> {
        self.db.query_rows(
            "SELECT id, track_id, played_ms, deck FROM play_history WHERE track_id = ?1 \
             ORDER BY played_ms DESC, id DESC;",
            params![track_id],
            read_play,
        )
    }

    /// Absolute play history, for library import. `record_play` is the playback increment.
    pub fn set_play_stats(
        &self,
        id: i64,
        play_count: i32,
        last_played_ms: Option<i64>,
    ) -> Result<()> {
        self.run_checked(
            "UPDATE tracks SET play_count=?2, last_played_ms=?3 WHERE id=?1;",
            params![id, play_count, last_played_ms],
        )
    }

    fn run_checked(&self, sql: &str, params: impl Params) -> Result<()> {
        let changed = self.db.connection().execute(sql, params)?;
        if changed > 0 {
            Ok(())
        } else {
            Err(LibraryError::NotFound("no such track"))
        }
    }

    fn query(&self, tail: &str, params: impl Params) -> Result<Vec<TrackRecord>> {
        self.db.query_rows(
            &format!("SELECT {SELECT_COLUMNS} FROM tracks {tail};"),
            params,
            read_row,
        )
    }

    fn first(&self, tail: &str, params: impl Params) -> Result<Option<TrackRecord>> {
        Ok(self.query(tail, params)?.into_iter().next())
    }
}

/// One row of `play_history`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayEntry {
    pub id: i64,
    pub track_id: i64,
    pub played_ms: i64,
    /// Deck the track played on, when it was known when the play was recorded.
    pub deck: Option<u8>,
}

fn read_play(row: &Row<'_>) -> rusqlite::Result<PlayEntry> {
    Ok(PlayEntry {
        id: row.get(0)?,
        track_id: row.get(1)?,
        played_ms: row.get(2)?,
        deck: row
            .get::<_, Option<i64>>(3)?
            .and_then(|d| u8::try_from(d).ok()),
    })
}

fn added_or_now(t: &TrackRecord) -> i64 {
    if t.date_added_ms != 0 {
        t.date_added_ms
    } else {
        now_ms()
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

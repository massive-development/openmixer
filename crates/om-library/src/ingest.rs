//! Scans a folder tree for audio files and keeps the tracks table in step with it: new files are
//! added, changed files (mtime or size) re-read, unchanged files skipped, vanished files flagged
//! missing (rows and user data are kept, so a returning file restores its rating and cues).
//! Runs on the thread that owns the `Library` it is given.

use crate::artwork_dao::{ArtworkRecord, MAX_ARTWORK_BYTES};
use crate::database::Transaction;
use crate::error::{LibraryError, Result};
use crate::library::Library;
use crate::metadata::{Artwork, MetadataReaderRegistry, TrackMetadata};
use crate::track_record::TrackRecord;
use std::collections::HashSet;
use std::fs;
use std::ops::AddAssign;
use std::path::{Path, PathBuf, MAIN_SEPARATOR};
use std::time::{SystemTime, UNIX_EPOCH};

/// Counts for one scan. Filled in even when the scan is stopped or fails part way.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IngestSummary {
    pub scanned: usize,
    pub added: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub restored: usize,
    pub missing: usize,
    pub failed: usize,
}

impl AddAssign for IngestSummary {
    fn add_assign(&mut self, o: Self) {
        self.scanned += o.scanned;
        self.added += o.added;
        self.updated += o.updated;
        self.unchanged += o.unchanged;
        self.restored += o.restored;
        self.missing += o.missing;
        self.failed += o.failed;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Added,
    Updated,
    Unchanged,
    Restored,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileStamp {
    mtime_ms: i64,
    size_bytes: i64,
}

/// Folder ingest bound to one library and one reader registry.
pub struct Ingestor<'a> {
    library: &'a Library,
    readers: &'a MetadataReaderRegistry,
}

impl<'a> Ingestor<'a> {
    pub fn new(library: &'a Library, readers: &'a MetadataReaderRegistry) -> Self {
        Self { library, readers }
    }

    /// Ingests every supported file under `folder`, then flags tracks under it that were not seen.
    /// Fails if `folder` is not a directory (nothing is marked missing then: an unmounted drive is
    /// not a deleted library), or if `should_stop` returns true between files (no missing flags).
    /// `should_stop` is polled before each file.
    pub fn ingest_folder(
        &self,
        folder: &Path,
        summary: &mut IngestSummary,
        should_stop: &dyn Fn() -> bool,
    ) -> Result<()> {
        if !folder.is_dir() {
            return Err(LibraryError::Ingest(format!(
                "not a folder: {}",
                folder.display()
            )));
        }
        let base = full_path(folder);
        let mut seen: HashSet<String> = HashSet::new();
        let mut pending: Vec<PathBuf> = vec![base.clone()];
        while let Some(dir) = pending.pop() {
            let Ok(entries) = fs::read_dir(&dir) else {
                continue; // unreadable subfolder: skipped
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                if kind.is_dir() {
                    pending.push(path);
                    continue;
                }
                // Symlinked files count; symlinked folders are not followed (no cycles).
                let is_file = if kind.is_symlink() {
                    fs::metadata(&path).is_ok_and(|m| m.is_file())
                } else {
                    kind.is_file()
                };
                if !is_file {
                    continue;
                }
                if should_stop() {
                    return Err(LibraryError::Ingest("stopped".into()));
                }
                if !self.readers.supports(&path) {
                    continue;
                }
                seen.insert(path_text(&path));
                summary.scanned += 1;
                match self.ingest_file(&path) {
                    Outcome::Added => summary.added += 1,
                    Outcome::Updated => summary.updated += 1,
                    Outcome::Unchanged => summary.unchanged += 1,
                    Outcome::Restored => summary.restored += 1,
                    // Stays "seen": a transient failure must not flag the file missing.
                    Outcome::Failed => summary.failed += 1,
                }
            }
        }
        summary.missing += self.mark_missing(&base, &seen, should_stop);
        Ok(())
    }

    fn ingest_file(&self, file: &Path) -> Outcome {
        let tracks = self.library.tracks();
        let trackfiles = self.library.track_files();
        // A failed lookup is not "new file": it would duplicate or clobber rows.
        let Ok(existing) = tracks.find_by_path(&path_text(file)) else {
            return Outcome::Failed;
        };
        let state = match &existing {
            Some(track) => match trackfiles.find(track.id) {
                Ok(state) => state,
                Err(_) => return Outcome::Failed,
            },
            None => None,
        };
        // why: stamped before hashing, so an edit during the read is caught by the next scan
        let Some(stamp) = stamp_of(file) else {
            return Outcome::Failed;
        };
        let unchanged =
            state.filter(|s| s.mtime_ms == stamp.mtime_ms && s.size_bytes == stamp.size_bytes);
        match (unchanged, existing) {
            (Some(state), Some(_)) if !state.missing => Outcome::Unchanged,
            (Some(_), Some(track)) => match trackfiles.set_missing(track.id, false) {
                Ok(()) => Outcome::Restored,
                Err(_) => Outcome::Failed,
            },
            (_, existing) => self.refresh(file, existing, stamp),
        }
    }

    fn refresh(&self, file: &Path, existing: Option<TrackRecord>, stamp: FileStamp) -> Outcome {
        let hash = match om_analysis::hash_file_content(file) {
            Some(hash) if !hash.is_empty() => hash,
            _ => return Outcome::Failed,
        };
        let Some(reader) = self.readers.find(file) else {
            return Outcome::Failed;
        };
        let mut meta = TrackMetadata::default();
        if reader.read(file, &mut meta).is_err() {
            return Outcome::Failed;
        }

        let mut record = to_record(file, &meta, hash.clone());
        if let Some(old) = &existing {
            merge_fill_only(old, file, &mut record);
            if analysis_still_valid(old, &hash) {
                carry_analysis(old, &mut record);
            }
        }

        match self.commit_record(&record, stamp, meta.artwork.as_ref()) {
            Ok(()) if existing.is_some() => Outcome::Updated,
            Ok(()) => Outcome::Added,
            Err(_) => Outcome::Failed,
        }
    }

    /// Upserts the row, its file state and its cover in one transaction, so a failure leaves none
    /// of them behind. A cover over [`MAX_ARTWORK_BYTES`] is skipped, and any cover stored from an
    /// earlier read is removed, since the file no longer matches it.
    fn commit_record(
        &self,
        record: &TrackRecord,
        stamp: FileStamp,
        artwork: Option<&Artwork>,
    ) -> Result<()> {
        let tx = Transaction::begin(self.library.database())?;
        let id = self.library.tracks().upsert_by_path(record)?;
        self.library
            .track_files()
            .set(id, stamp.mtime_ms, stamp.size_bytes)?;
        self.store_artwork(id, &record.path, artwork)?;
        tx.commit()
    }

    fn store_artwork(&self, id: i64, path: &str, artwork: Option<&Artwork>) -> Result<()> {
        let dao = self.library.artwork();
        let Some(art) = artwork else {
            return dao.delete(id);
        };
        if art.data.len() > MAX_ARTWORK_BYTES {
            tracing::debug!(
                path,
                bytes = art.data.len(),
                limit = MAX_ARTWORK_BYTES,
                "cover art too large to store; skipped"
            );
            return dao.delete(id);
        }
        dao.set(id, &ArtworkRecord::new(art.mime_type.clone(), art.data.clone()))
    }

    /// Flags tracks under `folder` that were not seen in this pass. A partial pass (stopped, or a
    /// failed listing) flags nothing. Returns how many rows were newly flagged.
    fn mark_missing(
        &self,
        folder: &Path,
        seen: &HashSet<String>,
        should_stop: &dyn Fn() -> bool,
    ) -> usize {
        let trackfiles = self.library.track_files();
        let Ok(entries) = trackfiles.under(&folder_prefix(folder)) else {
            return 0;
        };
        let mut flagged = 0;
        for entry in entries {
            if should_stop() {
                break; // a partial scan must not flag files it never got to
            }
            if !entry.missing
                && !seen.contains(&entry.path)
                && trackfiles.set_missing(entry.track_id, true).is_ok()
            {
                flagged += 1;
            }
        }
        flagged
    }
}

/// Absolute, normalised form of `path` (no trailing separator, no `.` components).
pub(crate) fn full_path(path: &Path) -> PathBuf {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    absolute.components().collect()
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Folder path with a trailing separator, so "/music" does not match "/music2/...".
fn folder_prefix(folder: &Path) -> String {
    let mut text = path_text(&full_path(folder));
    if !text.ends_with(MAIN_SEPARATOR) {
        text.push(MAIN_SEPARATOR);
    }
    text
}

fn stamp_of(file: &Path) -> Option<FileStamp> {
    let meta = fs::metadata(file).ok()?;
    Some(FileStamp {
        // why: a missing modification time counts as 0
        mtime_ms: meta.modified().map_or(0, system_time_ms),
        size_bytes: i64::try_from(meta.len()).unwrap_or(i64::MAX),
    })
}

fn system_time_ms(time: SystemTime) -> i64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_millis()).unwrap_or(i64::MAX),
        Err(e) => -i64::try_from(e.duration().as_millis()).unwrap_or(i64::MAX),
    }
}

fn file_stem(file: &Path) -> String {
    file.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn to_record(file: &Path, meta: &TrackMetadata, hash: String) -> TrackRecord {
    TrackRecord {
        path: path_text(file),
        content_hash: hash,
        title: meta.title.clone(),
        artist: meta.artist.clone(),
        album: meta.album.clone(),
        genre: meta.genre.clone(),
        comment: meta.comment.clone(),
        year: meta.year,
        duration_sec: meta.duration_sec,
        ..Default::default()
    }
}

fn carry_analysis(from: &TrackRecord, to: &mut TrackRecord) {
    to.bpm = from.bpm;
    to.key = from.key.clone();
    to.camelot = from.camelot.clone();
}

fn keep_if_empty(read: &mut String, old: &str) {
    if read.is_empty() {
        *read = old.to_owned();
    }
}

/// why: file tags are often sparser than imported data (filename-only title, no artist), so never
/// blank or downgrade it.
fn merge_fill_only(old: &TrackRecord, file: &Path, rec: &mut TrackRecord) {
    if !old.title.is_empty() && rec.title == file_stem(file) {
        rec.title = old.title.clone();
    }
    keep_if_empty(&mut rec.title, &old.title);
    keep_if_empty(&mut rec.artist, &old.artist);
    keep_if_empty(&mut rec.album, &old.album);
    keep_if_empty(&mut rec.genre, &old.genre);
    keep_if_empty(&mut rec.comment, &old.comment);
    if rec.year == 0 {
        rec.year = old.year;
    }
    if rec.duration_sec <= 0.0 {
        rec.duration_sec = old.duration_sec;
    }
}

/// why: imported rows have no hash yet; their analysis is still the user's, and identical bytes
/// keep theirs too.
fn analysis_still_valid(old: &TrackRecord, hash: &str) -> bool {
    old.content_hash.is_empty() || old.content_hash == hash
}

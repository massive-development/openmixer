//! Where the browser table gets its rows: a counted, pageable list of tracks. [`SearchTrackSource`]
//! runs the query syntax against the library, scoped to a playlist or to everything;
//! [`PrepQueueSource`] lists the prep queue in order.

use std::sync::Arc;

use om_library::{Database, Library, PrepEntry, SearchEngine, SearchQuery, TrackRecord};

/// A counted, pageable list of tracks.
pub trait TrackSource {
    fn count(&mut self) -> usize;
    /// The rows at `offset`; an `Err` carries the reason the page could not be read.
    fn fetch(&mut self, offset: usize, limit: usize) -> Result<Vec<TrackRecord>, String>;
    /// Why the last count failed, or the query text was rejected; empty when fine.
    fn error(&self) -> String {
        String::new()
    }
}

/// Query-backed rows. The engine owns its database handle, so the source can outlive the call
/// that made it.
pub struct SearchTrackSource {
    engine: SearchEngine<Arc<Database>>,
    query: SearchQuery,
    last_error: String,
}

impl SearchTrackSource {
    /// `scope` is a fragment that always applies (`""` = all tracks, `playlist:#3`); `text` is the
    /// user's search box. A non-empty `sort_field` replaces any `sort:` token in the text.
    pub fn new(
        engine: SearchEngine<Arc<Database>>,
        scope: &str,
        text: &str,
        sort_field: &str,
        descending: bool,
    ) -> Self {
        let parsed = engine.parse(&format!("{scope} {text}"));
        let mut query = parsed.query;
        if !sort_field.is_empty() {
            query.sort_field = sort_field.to_owned();
            query.descending = descending;
        }
        Self {
            engine,
            query,
            last_error: parsed.errors.join("; "),
        }
    }

    pub fn query(&self) -> &SearchQuery {
        &self.query
    }
}

impl TrackSource for SearchTrackSource {
    fn count(&mut self) -> usize {
        match self.engine.count(&self.query) {
            Ok(n) => usize::try_from(n.max(0)).unwrap_or(0),
            Err(error) => {
                self.last_error = error.to_string();
                0
            }
        }
    }

    fn fetch(&mut self, offset: usize, limit: usize) -> Result<Vec<TrackRecord>, String> {
        let mut page = self.query.clone();
        page.offset = u32::try_from(offset).unwrap_or(u32::MAX);
        page.limit = Some(u32::try_from(limit).unwrap_or(u32::MAX));
        self.engine.search(&page).map_err(|error| error.to_string())
    }

    fn error(&self) -> String {
        self.last_error.clone()
    }
}

/// The prep queue in queue order. The entries are the snapshot the browser took when it built the
/// source; the browser rebuilds the source whenever the queue changes.
pub struct PrepQueueSource {
    library: Arc<Library>,
    entries: Vec<PrepEntry>,
}

impl PrepQueueSource {
    pub fn new(library: Arc<Library>, entries: Vec<PrepEntry>) -> Self {
        Self { library, entries }
    }
}

impl TrackSource for PrepQueueSource {
    fn count(&mut self) -> usize {
        self.entries.len()
    }

    fn fetch(&mut self, offset: usize, limit: usize) -> Result<Vec<TrackRecord>, String> {
        let end = self.entries.len().min(offset.saturating_add(limit));
        Ok(self
            .entries
            .get(offset.min(end)..end)
            .unwrap_or_default()
            .iter()
            .filter_map(|entry| {
                self.library
                    .tracks()
                    .find_by_id(entry.track_id)
                    .ok()
                    .flatten()
            })
            .collect())
    }
}

/// A playlist in its own order, one row per entry (a track added twice shows twice). The ids are
/// the snapshot the browser took when it built the source.
pub struct PlaylistSource {
    library: Arc<Library>,
    track_ids: Vec<i64>,
}

impl PlaylistSource {
    pub fn new(library: Arc<Library>, track_ids: Vec<i64>) -> Self {
        Self { library, track_ids }
    }
}

impl TrackSource for PlaylistSource {
    fn count(&mut self) -> usize {
        self.track_ids.len()
    }

    fn fetch(&mut self, offset: usize, limit: usize) -> Result<Vec<TrackRecord>, String> {
        let end = self.track_ids.len().min(offset.saturating_add(limit));
        self.track_ids
            .get(offset.min(end)..end)
            .unwrap_or_default()
            .iter()
            .map(|&id| {
                self.library
                    .tracks()
                    .find_by_id(id)
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| format!("track {id} is missing"))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Counting {
        total: usize,
        fetches: usize,
    }

    impl TrackSource for Counting {
        fn count(&mut self) -> usize {
            self.total
        }

        fn fetch(&mut self, offset: usize, limit: usize) -> Result<Vec<TrackRecord>, String> {
            self.fetches += 1;
            Ok((offset..self.total.min(offset + limit))
                .map(|i| TrackRecord {
                    id: i as i64 + 1,
                    title: format!("T{i}"),
                    ..TrackRecord::default()
                })
                .collect())
        }
    }

    #[test]
    fn a_counting_source_pages_by_offset_and_limit() {
        let mut source = Counting {
            total: 250,
            fetches: 0,
        };
        assert_eq!(source.count(), 250);
        let page = source.fetch(200, 100).expect("counting source never fails");
        assert_eq!(page.len(), 50);
        assert_eq!(page[49].title, "T249");
        assert_eq!(source.fetches, 1);
    }
}

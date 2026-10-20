//! The automix queue over the library (A3): the next track from a playlist in order, or from a
//! search/crate filter shuffled, skipping what played recently and what is not analysed yet. The
//! rules live in `om_automix::queue::select_next`; this file feeds them from the database.
//!
//! The queue owns its own `Library` handle (open it read-only on the library file) so the control
//! thread never shares a connection with the UI. A track counts as recent when it is among the
//! last `recent_count` plays in `play_history` **and** was played within `recent_hours`.
//!
//! Threads: the thread that creates it (a `Library` is not `Send`): build it on the control thread.

use om_automix::queue::{
    select_next, AutomixQueue, Order, QueueConfig, QueueRng, QueueTrack, SkipReason, SplitMix64,
};
use om_library::{Library, SearchEngine, TrackRecord};
use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

/// Where the next track comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueSource {
    /// Play a playlist (or crate) in its order, wrapping at the end.
    Playlist(i64),
    /// Shuffle the results of a library search (the current filter).
    Search(String),
}

type GridCheck = Box<dyn Fn(&TrackRecord) -> bool>;
type Clock = Box<dyn Fn() -> i64>;

pub struct LibraryQueue {
    library: Library,
    source: QueueSource,
    cfg: QueueConfig,
    rng: Box<dyn QueueRng>,
    clock: Clock,
    grid_check: GridCheck,
    /// Position in the playlist of the last pick.
    cursor: usize,
    skips: Vec<(i64, SkipReason)>,
    last_error: Option<String>,
}

impl LibraryQueue {
    /// A queue over `library` with a seeded RNG and the wall clock. The beat grid counts as
    /// available when the track has a BPM (the analysis writes both together); replace the test
    /// with [`LibraryQueue::with_grid_check`] to ask the analysis cache.
    pub fn new(library: Library, source: QueueSource, cfg: QueueConfig, seed: u64) -> Self {
        Self {
            library,
            source,
            cfg,
            rng: Box::new(SplitMix64(seed)),
            clock: Box::new(system_now_ms),
            grid_check: Box::new(|t| t.bpm.is_some()),
            cursor: 0,
            skips: Vec::new(),
            last_error: None,
        }
    }

    pub fn with_clock(mut self, clock: impl Fn() -> i64 + 'static) -> Self {
        self.clock = Box::new(clock);
        self
    }

    pub fn with_rng(mut self, rng: impl QueueRng + 'static) -> Self {
        self.rng = Box::new(rng);
        self
    }

    pub fn with_grid_check(mut self, check: impl Fn(&TrackRecord) -> bool + 'static) -> Self {
        self.grid_check = Box::new(check);
        self
    }

    pub fn set_source(&mut self, source: QueueSource) {
        self.source = source;
        self.cursor = 0;
    }

    /// The database error of the last call, if it failed.
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    fn candidates(&self) -> Result<Vec<TrackRecord>, String> {
        match &self.source {
            QueueSource::Playlist(id) => {
                let entries = self
                    .library
                    .playlists()
                    .entries(*id)
                    .map_err(|e| e.to_string())?;
                let mut tracks = Vec::with_capacity(entries.len());
                for entry in entries {
                    if let Some(t) = self
                        .library
                        .tracks()
                        .find_by_id(entry.track_id)
                        .map_err(|e| e.to_string())?
                    {
                        tracks.push(t);
                    }
                }
                Ok(tracks)
            }
            QueueSource::Search(text) => SearchEngine::new(self.library.database())
                .search_text(text)
                .map_err(|e| e.to_string()),
        }
    }

    fn recent_ids(&self) -> Result<HashSet<i64>, String> {
        let plays = self
            .library
            .tracks()
            .recent_plays(self.cfg.recent_count)
            .map_err(|e| e.to_string())?;
        let oldest = (self.clock)() - (self.cfg.recent_hours * 3_600_000.0) as i64;
        Ok(plays
            .into_iter()
            .filter(|p| p.played_ms >= oldest)
            .map(|p| p.track_id)
            .collect())
    }

    fn queue_track(&self, t: &TrackRecord) -> QueueTrack {
        QueueTrack {
            id: t.id,
            title: t.title.clone(),
            artist: t.artist.clone(),
            path: t.path.clone(),
            bpm: t.bpm,
            camelot: t.camelot.clone(),
            has_grid: (self.grid_check)(t),
        }
    }

    fn pick(&mut self, current: Option<&QueueTrack>) -> Result<Option<QueueTrack>, String> {
        let records = self.candidates()?;
        let recent = self.recent_ids()?;
        let mut tracks: Vec<QueueTrack> = records.iter().map(|t| self.queue_track(t)).collect();
        let in_order = matches!(self.source, QueueSource::Playlist(_));
        if in_order && !tracks.is_empty() {
            // Start after the last pick and wrap.
            let start = self.cursor % tracks.len();
            tracks.rotate_left(start);
        }
        let cfg = QueueConfig {
            order: if in_order {
                Order::InOrder
            } else {
                Order::Shuffle
            },
            ..self.cfg
        };
        let selection = select_next(&tracks, current, &recent, &cfg, self.rng.as_mut());
        self.skips = selection.skipped;
        if in_order {
            if let Some(pick) = &selection.pick {
                if let Some(at) = tracks.iter().position(|t| t.id == pick.id) {
                    self.cursor = (self.cursor % tracks.len() + at + 1) % tracks.len();
                }
            }
        }
        Ok(selection.pick)
    }
}

impl AutomixQueue for LibraryQueue {
    fn next_track(&mut self, current: Option<&QueueTrack>) -> Option<QueueTrack> {
        match self.pick(current) {
            Ok(pick) => {
                self.last_error = None;
                pick
            }
            Err(error) => {
                self.skips.clear();
                self.last_error = Some(error);
                None
            }
        }
    }

    fn last_skips(&self) -> Vec<(i64, SkipReason)> {
        self.skips.clone()
    }
}

fn system_now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

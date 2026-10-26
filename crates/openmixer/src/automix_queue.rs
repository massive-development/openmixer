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
    in_tempo_gate, select_next, AutomixQueue, Order, QueueConfig, QueueRng, QueueTrack, SkipReason,
    SplitMix64,
};
use om_library::{Library, PrepStatus, SearchEngine, TrackRecord};
use om_ui::library::SearchFeed;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// Where the next track comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueSource {
    /// Play a playlist (or crate) in its order, wrapping at the end.
    Playlist(i64),
    /// Shuffle the results of a fixed library search.
    Search(String),
    /// Shuffle the results of the library browser's search as it is now: the browser publishes its
    /// text to the queue's search feed, and each pick reads it.
    CurrentSearch,
}

/// The settings the window changes while automix runs. The host reads them before each pick, so
/// a change in the Automix page applies to the next track without a restart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueSettings {
    pub source: QueueSource,
    /// Prefer key-compatible, tempo-close tracks (shuffled sources only).
    pub smart: bool,
}

pub type SharedQueueSettings = Arc<Mutex<QueueSettings>>;

type GridCheck = Box<dyn Fn(&TrackRecord) -> bool>;

/// Most beat-grid checks one shuffled pick may make (each check can hit the analysis cache).
const GRID_CHECK_CAP: usize = 50;
/// Times one unanalysed track is put on the prep queue before automix gives up on it.
const MAX_ANALYSIS_SUBMISSIONS: u32 = 3;
type Clock = Box<dyn Fn() -> i64>;

pub struct LibraryQueue {
    library: Arc<Library>,
    source: QueueSource,
    cfg: QueueConfig,
    rng: Box<dyn QueueRng>,
    clock: Clock,
    grid_check: GridCheck,
    /// The browser's search text, read by [`QueueSource::CurrentSearch`]. Empty when unset.
    search_feed: SearchFeed,
    /// Position in the playlist of the last pick.
    cursor: usize,
    skips: Vec<(i64, SkipReason)>,
    last_error: Option<String>,
    /// How often each unanalysed track went onto the prep queue this session.
    submissions: HashMap<i64, u32>,
    /// Skipped as unanalysed by the last call and still worth waiting for.
    pending: Vec<i64>,
    /// Skipped as unanalysed by the last call, given up on after [`MAX_ANALYSIS_SUBMISSIONS`].
    given_up: Vec<i64>,
}

impl LibraryQueue {
    /// A queue over `library` with a seeded RNG and the wall clock. Without
    /// [`LibraryQueue::with_grid_check`] a track counts as having a grid when it has a BPM; the app
    /// asks the analysis cache instead, which is the rule that matters.
    pub fn new(library: Library, source: QueueSource, cfg: QueueConfig, seed: u64) -> Self {
        // The queue stays on the window thread with the browser: the Arc is for sharing one
        // connection on that thread, not across threads.
        #[allow(clippy::arc_with_non_send_sync)]
        let library = Arc::new(library);
        Self::shared(library, source, cfg, seed)
    }

    /// A queue over a library the app shares with its browser (the same connection pool).
    pub fn shared(library: Arc<Library>, source: QueueSource, cfg: QueueConfig, seed: u64) -> Self {
        Self {
            library,
            source,
            cfg,
            rng: Box::new(SplitMix64(seed)),
            clock: Box::new(system_now_ms),
            grid_check: Box::new(|t| t.bpm.is_some()),
            search_feed: SearchFeed::default(),
            cursor: 0,
            skips: Vec::new(),
            last_error: None,
            submissions: HashMap::new(),
            pending: Vec::new(),
            given_up: Vec::new(),
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

    /// Reads [`QueueSource::CurrentSearch`] from `feed` (the library browser publishes there).
    pub fn with_search_feed(mut self, feed: SearchFeed) -> Self {
        self.search_feed = feed;
        self
    }

    pub fn set_source(&mut self, source: QueueSource) {
        if self.source != source {
            self.source = source;
            self.cursor = 0;
        }
    }

    /// The tempo pre-filter for the next picks (see [`QueueConfig::tempo_gate`]).
    pub fn set_tempo_gate(&mut self, gate: Option<f64>) {
        self.cfg.tempo_gate = gate;
    }

    pub fn set_smart(&mut self, smart: bool) {
        self.cfg.smart = smart;
    }

    /// Takes the window's queue settings (source and smart picks) for the next pick.
    pub fn apply(&mut self, settings: &QueueSettings) {
        self.set_source(settings.source.clone());
        self.set_smart(settings.smart);
    }

    pub fn source(&self) -> &QueueSource {
        &self.source
    }

    /// The database error of the last call, if it failed.
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Tracks the last call skipped as unanalysed that analysis may still deliver.
    pub fn pending_analysis(&self) -> &[i64] {
        &self.pending
    }

    /// Puts the unanalysed tracks the last selection skipped on the prep queue (the persisted
    /// queue the app's `PrepQueue` pumps). A track already waiting there is left alone; one whose
    /// entry finished (or vanished) without making it analysed is submitted again, up to
    /// [`MAX_ANALYSIS_SUBMISSIONS`] times, then given up on.
    fn request_analysis(&mut self, ids: &[i64]) {
        self.pending.clear();
        self.given_up.clear();
        let dao = self.library.prep_queue();
        for &id in ids {
            let count = self.submissions.get(&id).copied().unwrap_or(0);
            if count >= MAX_ANALYSIS_SUBMISSIONS {
                self.given_up.push(id);
                continue;
            }
            self.pending.push(id);
            let waiting = matches!(
                dao.find(id),
                Ok(Some(e)) if matches!(e.status, PrepStatus::Queued | PrepStatus::Running)
            );
            if !waiting && dao.enqueue(id).is_ok() {
                self.submissions.insert(id, count + 1);
            }
        }
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
            QueueSource::CurrentSearch => {
                let text = self
                    .search_feed
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone();
                SearchEngine::new(self.library.database())
                    .search_text(&text)
                    .map_err(|e| e.to_string())
            }
        }
    }

    /// The last play time of each track in the recent window: the newest `count` plays, within the
    /// history hours.
    fn recent_plays(&self, count: usize) -> Result<HashMap<i64, i64>, String> {
        let plays = self
            .library
            .tracks()
            .recent_plays(count)
            .map_err(|e| e.to_string())?;
        let oldest = (self.clock)() - (self.cfg.recent_hours * 3_600_000.0) as i64;
        let mut last: HashMap<i64, i64> = HashMap::new();
        for p in plays.into_iter().filter(|p| p.played_ms >= oldest) {
            let at = last.entry(p.track_id).or_insert(p.played_ms);
            *at = (*at).max(p.played_ms);
        }
        Ok(last)
    }

    fn queue_track(&self, t: &TrackRecord) -> QueueTrack {
        to_queue_track(t, (self.grid_check)(t))
    }

    /// Shuffles all records first, then asks for the beat grid lazily: the walk stops at the first
    /// analysed candidate (or after [`GRID_CHECK_CAP`] checks). Smart picks keep checking up to the
    /// cap so they have a pool to choose from. Recent tracks are checked too, outside the cap.
    fn shuffled_candidates(
        &mut self,
        mut records: Vec<TrackRecord>,
        current: Option<&QueueTrack>,
        recent: &HashSet<i64>,
        smart: bool,
    ) -> Vec<QueueTrack> {
        for i in (1..records.len()).rev() {
            let j = (self.rng.next_u64() % (i as u64 + 1)) as usize;
            records.swap(i, j);
        }
        let mut out = Vec::with_capacity(records.len());
        let mut checks = 0;
        let mut found = false;
        let gate = self.cfg.tempo_gate;
        for t in &records {
            let skipped = recent.contains(&t.id) || current.is_some_and(|c| c.id == t.id);
            if skipped {
                // Not counted against the cap: the window is small, and relaxing the recent filter
                // needs to know which recent tracks are analysed.
                out.push(to_queue_track(t, (self.grid_check)(t)));
                continue;
            }
            if checks == GRID_CHECK_CAP || (found && !smart) {
                break;
            }
            checks += 1;
            let qt = to_queue_track(t, (self.grid_check)(t));
            // A track outside the tempo window does not end the walk: the walk looks for one that fits.
            found |= qt.analysed() && in_tempo_gate(current, &qt, gate);
            out.push(qt);
        }
        out
    }

    fn pick(&mut self, current: Option<&QueueTrack>) -> Result<Option<QueueTrack>, String> {
        let records = self.candidates()?;
        // The recent window never covers more than half the pool, so a short pool still has tracks
        // to choose from.
        let window = self.cfg.recent_count.min(records.len() / 2);
        let last_played = self.recent_plays(window)?;
        let recent: HashSet<i64> = last_played.keys().copied().collect();
        let in_order = matches!(self.source, QueueSource::Playlist(_));
        let tracks: Vec<QueueTrack> = if in_order {
            let mut tracks: Vec<QueueTrack> = records.iter().map(|t| self.queue_track(t)).collect();
            if !tracks.is_empty() {
                // Start after the last pick and wrap.
                let start = self.cursor % tracks.len();
                tracks.rotate_left(start);
            }
            tracks
        } else {
            let smart = self.cfg.smart;
            self.shuffled_candidates(records, current, &recent, smart)
        };
        let cfg = QueueConfig {
            order: if in_order {
                Order::InOrder
            } else {
                Order::Shuffle
            },
            ..self.cfg
        };
        let mut selection = select_next(&tracks, current, &recent, &cfg, self.rng.as_mut());
        if selection.pick.is_none() {
            // Nothing eligible: relax the recent filter for the least recently played half of the
            // recent, analysed candidates.
            let mut older: Vec<&QueueTrack> = tracks
                .iter()
                .filter(|t| t.analysed() && recent.contains(&t.id))
                .filter(|t| current.is_none_or(|c| c.id != t.id))
                .collect();
            older.sort_by_key(|t| last_played.get(&t.id).copied().unwrap_or(i64::MIN));
            older.truncate(older.len().div_ceil(2));
            if !older.is_empty() {
                let relaxed: HashSet<i64> = recent
                    .iter()
                    .copied()
                    .filter(|id| !older.iter().any(|t| t.id == *id))
                    .collect();
                selection = select_next(&tracks, current, &relaxed, &cfg, self.rng.as_mut());
            }
        }
        self.skips = selection.skipped;
        self.request_analysis(&selection.needs_analysis);
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
                self.last_error = (!self.given_up.is_empty()).then(|| {
                    let ids: Vec<String> = self.given_up.iter().map(i64::to_string).collect();
                    format!(
                        "gave up analysing after {MAX_ANALYSIS_SUBMISSIONS} tries: track {}",
                        ids.join(", ")
                    )
                });
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

fn to_queue_track(t: &TrackRecord, has_grid: bool) -> QueueTrack {
    QueueTrack {
        id: t.id,
        title: t.title.clone(),
        artist: t.artist.clone(),
        path: t.path.clone(),
        bpm: t.bpm,
        camelot: t.camelot.clone(),
        has_grid,
    }
}

fn system_now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

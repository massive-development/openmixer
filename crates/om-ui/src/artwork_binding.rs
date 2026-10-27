//! Cover art for the UI. The decision of what to show lives here; the gpui layer only draws it.
//!
//! [`artwork_for`] turns a track's stored cover into an [`ArtworkView`]: no cover, a failing
//! library, or bytes whose header is not a readable PNG or JPEG all give [`ArtworkView::None`],
//! never a panic. A small LRU cache ([`ArtworkCache`], 16 tracks) answers a repeat without a second
//! query. [`DeckArtwork`] keeps one view per deck and loads covers on a worker thread, so the UI
//! thread never reads the library for art (the same split as `TrackLoader`).

use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;

use om_library::{image_size, ArtworkRecord, Library, LibraryError, OpenMode};

/// Covers kept by the cache.
pub const CACHE_CAPACITY: usize = 16;

/// Where covers come from. The library implements it; tests use a stub.
pub trait ArtworkSource {
    fn artwork(&self, track_id: i64) -> Result<Option<ArtworkRecord>, LibraryError>;
}

impl ArtworkSource for Library {
    fn artwork(&self, track_id: i64) -> Result<Option<ArtworkRecord>, LibraryError> {
        self.artwork().get(track_id)
    }
}

/// What a deck or detail view shows for a track's cover.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ArtworkView {
    /// No cover, or one that cannot be shown: the view draws its placeholder.
    #[default]
    None,
    Image {
        mime: String,
        data: Arc<[u8]>,
        width: u32,
        height: u32,
    },
}

impl ArtworkView {
    /// True for PNG data, false for JPEG (the only formats a view is made from).
    pub fn is_png(&self) -> bool {
        matches!(self, Self::Image { data, .. } if data.starts_with(&[0x89, b'P', b'N', b'G']))
    }
}

/// Least-recently-used cache of [`ArtworkView`]s by track id, [`CACHE_CAPACITY`] entries. A track
/// with no cover is cached too, so it is not queried again.
#[derive(Debug, Default)]
pub struct ArtworkCache {
    /// Most recently used last.
    entries: VecDeque<(i64, ArtworkView)>,
}

impl ArtworkCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// The view for `track_id`, from the cache or, on a miss, one query of `lib`.
    pub fn artwork_for(&mut self, lib: &impl ArtworkSource, track_id: i64) -> ArtworkView {
        if let Some(at) = self.entries.iter().position(|(id, _)| *id == track_id) {
            if let Some(entry) = self.entries.remove(at) {
                let view = entry.1.clone();
                self.entries.push_back(entry);
                return view;
            }
        }
        let view = load(lib, track_id);
        if self.entries.len() >= CACHE_CAPACITY {
            self.entries.pop_front();
        }
        self.entries.push_back((track_id, view.clone()));
        view
    }
}

fn load(lib: &impl ArtworkSource, track_id: i64) -> ArtworkView {
    match lib.artwork(track_id) {
        Ok(Some(record)) => view_of(record),
        Ok(None) => ArtworkView::None,
        Err(error) => {
            eprintln!("library: could not read the cover of track {track_id}: {error}");
            ArtworkView::None
        }
    }
}

/// The view of a stored cover. The size is read from the data itself, not from the row, so a row
/// that disagrees with its bytes cannot give a wrong size. Data with no readable header is `None`.
fn view_of(record: ArtworkRecord) -> ArtworkView {
    match image_size(&record.data) {
        Some((width, height)) if width > 0 && height > 0 => ArtworkView::Image {
            mime: record.mime,
            data: record.data.into(),
            width,
            height,
        },
        _ => ArtworkView::None,
    }
}

thread_local! {
    static CACHE: RefCell<ArtworkCache> = RefCell::new(ArtworkCache::new());
}

/// The cover of `track_id` through a per-thread [`ArtworkCache`].
pub fn artwork_for(lib: &impl ArtworkSource, track_id: i64) -> ArtworkView {
    CACHE.with(|cache| cache.borrow_mut().artwork_for(lib, track_id))
}

/// Reads covers on a worker thread. The source is built on the worker (a library connection is
/// not `Sync`), requests go in by track id and views come back through [`Self::poll`].
pub struct ArtworkLoader {
    requests: Sender<i64>,
    results: Receiver<(i64, ArtworkView)>,
}

impl ArtworkLoader {
    /// Starts the worker. `open` runs on it; `None` means no source, so every request answers
    /// [`ArtworkView::None`].
    pub fn spawn<S, F>(open: F) -> Self
    where
        S: ArtworkSource,
        F: FnOnce() -> Option<S> + Send + 'static,
    {
        let (requests, wanted) = channel::<i64>();
        let (done, results) = channel();
        thread::spawn(move || {
            let source = open();
            let mut cache = ArtworkCache::new();
            for track_id in wanted {
                let view = match &source {
                    Some(source) => cache.artwork_for(source, track_id),
                    None => ArtworkView::None,
                };
                if done.send((track_id, view)).is_err() {
                    break;
                }
            }
        });
        Self { requests, results }
    }

    /// A loader reading the library file at `db_path` read-only. An in-memory library has no
    /// second connection to read, so it shows no covers.
    pub fn for_library_file(db_path: &str) -> Self {
        let path = db_path.to_owned();
        Self::spawn(move || {
            if path == crate::library::MEMORY_PATH {
                return None;
            }
            Library::open(&path, OpenMode::ReadOnly, &[]).ok()
        })
    }

    /// Asks for the cover of `track_id`. Never blocks.
    pub fn request(&self, track_id: i64) {
        // A dead worker just leaves the deck on its placeholder.
        let _ = self.requests.send(track_id);
    }

    /// Covers finished since the last call.
    pub fn poll(&self) -> Vec<(i64, ArtworkView)> {
        self.results.try_iter().collect()
    }
}

/// The cover shown on each deck. [`Self::follow`] is called with the track loaded in the deck
/// (every tick is fine); the cover arrives later through [`Self::pump`].
pub struct DeckArtwork {
    loader: ArtworkLoader,
    wanted: Vec<Option<i64>>,
    shown: Vec<ArtworkView>,
}

impl DeckArtwork {
    pub fn new(loader: ArtworkLoader, decks: usize) -> Self {
        Self {
            loader,
            wanted: vec![None; decks],
            shown: vec![ArtworkView::None; decks],
        }
    }

    /// Points `deck` at `track_id`. A change clears the old cover at once and requests the new one.
    pub fn follow(&mut self, deck: usize, track_id: Option<i64>) {
        let Some(wanted) = self.wanted.get_mut(deck) else {
            return;
        };
        if *wanted == track_id {
            return;
        }
        *wanted = track_id;
        self.shown[deck] = ArtworkView::None;
        if let Some(id) = track_id {
            self.loader.request(id);
        }
    }

    /// Takes finished covers. Returns true when any deck's view changed.
    pub fn pump(&mut self) -> bool {
        let mut changed = false;
        for (id, view) in self.loader.poll() {
            for (deck, wanted) in self.wanted.iter().enumerate() {
                if *wanted == Some(id) && self.shown[deck] != view {
                    self.shown[deck] = view.clone();
                    changed = true;
                }
            }
        }
        changed
    }

    /// What `deck` shows now; [`ArtworkView::None`] until its cover has loaded.
    pub fn view(&self, deck: usize) -> &ArtworkView {
        const NONE: &ArtworkView = &ArtworkView::None;
        self.shown.get(deck).unwrap_or(NONE)
    }
}

//! Cover art for the UI: a missing or undecodable cover is `None`, a PNG reports its size, and the
//! cache answers a repeat without asking the library again.

use std::cell::Cell;

use om_library::{ArtworkRecord, LibraryError};
use om_ui::artwork_binding::{artwork_for, ArtworkLoader, ArtworkSource, ArtworkView, DeckArtwork};

struct Stub {
    record: Option<ArtworkRecord>,
    calls: Cell<usize>,
}

impl Stub {
    fn new(record: Option<ArtworkRecord>) -> Self {
        Self {
            record,
            calls: Cell::new(0),
        }
    }
}

impl ArtworkSource for Stub {
    fn artwork(&self, _track_id: i64) -> Result<Option<ArtworkRecord>, LibraryError> {
        self.calls.set(self.calls.get() + 1);
        Ok(self.record.clone())
    }
}

/// A PNG signature and IHDR chunk for a `w` x `h` image: all `image_size` reads.
fn png_header(w: u32, h: u32) -> Vec<u8> {
    let mut data = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];
    data.extend_from_slice(b"IHDR");
    data.extend_from_slice(&w.to_be_bytes());
    data.extend_from_slice(&h.to_be_bytes());
    data
}

#[test]
fn missing_artwork_is_none() {
    let lib = Stub::new(None);
    assert_eq!(artwork_for(&lib, 1), ArtworkView::None);
}

#[test]
fn garbage_bytes_are_none_not_a_panic() {
    let lib = Stub::new(Some(ArtworkRecord::new("image/png", vec![0, 1, 2])));
    assert_eq!(artwork_for(&lib, 2), ArtworkView::None);
}

#[test]
fn a_failing_library_is_none() {
    struct Broken;
    impl ArtworkSource for Broken {
        fn artwork(&self, _: i64) -> Result<Option<ArtworkRecord>, LibraryError> {
            Err(LibraryError::Setup("boom".into()))
        }
    }
    assert_eq!(artwork_for(&Broken, 3), ArtworkView::None);
}

#[test]
fn valid_png_reports_size() {
    let lib = Stub::new(Some(ArtworkRecord::new("image/png", png_header(300, 200))));
    match artwork_for(&lib, 4) {
        ArtworkView::Image {
            mime,
            width,
            height,
            data,
        } => {
            assert_eq!((mime.as_str(), width, height), ("image/png", 300, 200));
            assert_eq!(data.len(), 24);
        }
        ArtworkView::None => panic!("expected an image"),
    }
}

#[test]
fn cache_serves_second_call_without_a_query() {
    let lib = Stub::new(Some(ArtworkRecord::new("image/png", png_header(8, 8))));
    let first = artwork_for(&lib, 5);
    let second = artwork_for(&lib, 5);
    assert_eq!(first, second);
    assert_eq!(lib.calls.get(), 1);
}

#[test]
fn a_missing_cover_is_cached_too() {
    let lib = Stub::new(None);
    artwork_for(&lib, 6);
    artwork_for(&lib, 6);
    assert_eq!(lib.calls.get(), 1);
}

#[test]
fn cache_holds_sixteen_tracks() {
    let lib = Stub::new(None);
    for id in 100..117 {
        artwork_for(&lib, id);
    }
    assert_eq!(lib.calls.get(), 17);
    // 101..=116 are the 16 most recent: still cached; 100 was evicted.
    artwork_for(&lib, 116);
    assert_eq!(lib.calls.get(), 17);
    artwork_for(&lib, 100);
    assert_eq!(lib.calls.get(), 18);
}

struct Shared(Option<ArtworkRecord>);

impl ArtworkSource for Shared {
    fn artwork(&self, _: i64) -> Result<Option<ArtworkRecord>, LibraryError> {
        Ok(self.0.clone())
    }
}

fn wait_for_cover(decks: &mut DeckArtwork) {
    for _ in 0..500 {
        if decks.pump() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[test]
fn a_deck_gets_its_cover_from_the_worker_and_clears_on_change() {
    let record = ArtworkRecord::new("image/png", png_header(4, 4));
    let loader = ArtworkLoader::spawn(move || Some(Shared(Some(record))));
    let mut decks = DeckArtwork::new(loader, 2);
    assert_eq!(decks.view(0), &ArtworkView::None);
    decks.follow(0, Some(7));
    // Loaded off this thread: the view stays empty until the worker answers.
    assert_eq!(decks.view(0), &ArtworkView::None);
    wait_for_cover(&mut decks);
    assert!(matches!(decks.view(0), ArtworkView::Image { width: 4, .. }));
    assert_eq!(decks.view(1), &ArtworkView::None);
    decks.follow(0, None);
    assert_eq!(decks.view(0), &ArtworkView::None);
}

#[test]
fn a_loader_without_a_source_answers_none() {
    let loader = ArtworkLoader::spawn(|| None::<Shared>);
    let mut decks = DeckArtwork::new(loader, 1);
    decks.follow(0, Some(1));
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert!(!decks.pump());
    assert_eq!(decks.view(0), &ArtworkView::None);
}

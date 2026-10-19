//! Cover art (migration 6): the artwork DAO, header sizes, and what ingest stores from a tag.

mod common;

use common::{save_tag, stored, tag_of, touch, write_mp3, Tags, TempDir};
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::tag::TagType;
use om_library::{
    core_migrations, image_size, ArtworkRecord, Ingestor, Library, MetadataReaderRegistry,
    OpenMode, TrackRecord, MAX_ARTWORK_BYTES,
};
use std::path::Path;

/// The classic 1x1 PNG.
const PNG_1X1: &[u8] = &[
    0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, b'I', b'H', b'D', b'R',
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, b'I', b'D', b'A', b'T', 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, b'I', b'E', b'N', b'D', 0xAE,
    0x42, 0x60, 0x82,
];

fn memory_library() -> Library {
    Library::open(":memory:", OpenMode::ReadWrite, core_migrations()).expect("open :memory:")
}

fn add_track(lib: &Library, path: &str) -> i64 {
    lib.tracks()
        .insert(&TrackRecord {
            path: path.into(),
            ..TrackRecord::default()
        })
        .expect("insert")
}

/// An MP3 whose front cover is `cover` (or no picture at all).
fn mp3_with_cover(path: &Path, frames: usize, cover: Option<(&[u8], MimeType)>) {
    let file = write_mp3(path, frames, &Tags::night_drive());
    if let Some((bytes, mime)) = cover {
        let mut tag = tag_of(TagType::Id3v2, &Tags::night_drive());
        tag.push_picture(
            Picture::unchecked(bytes.to_vec())
                .pic_type(PictureType::CoverFront)
                .mime_type(mime)
                .build(),
        );
        save_tag(&file, &tag);
    }
}

#[test]
fn dao_round_trips_replaces_and_deletes() {
    let lib = memory_library();
    let id = add_track(&lib, "/m/a.mp3");
    assert!(lib.artwork().get(id).unwrap().is_none());

    lib.artwork()
        .set(id, &ArtworkRecord::new("image/png", PNG_1X1.to_vec()))
        .unwrap();
    let got = lib.artwork().get(id).unwrap().expect("stored");
    assert_eq!(got.mime, "image/png");
    assert_eq!((got.width, got.height), (Some(1), Some(1)));
    assert_eq!(got.data, PNG_1X1);

    lib.artwork()
        .set(id, &ArtworkRecord::new("image/jpeg", vec![1, 2, 3]))
        .unwrap();
    let replaced = lib.artwork().get(id).unwrap().unwrap();
    assert_eq!(replaced.mime, "image/jpeg");
    assert_eq!((replaced.width, replaced.height), (None, None));
    assert_eq!(replaced.data, vec![1, 2, 3]);

    lib.artwork().delete(id).unwrap();
    assert!(lib.artwork().get(id).unwrap().is_none());
    lib.artwork().delete(id).expect("deleting nothing is fine");
}

#[test]
fn artwork_needs_a_track_and_goes_with_it() {
    let lib = memory_library();
    assert!(lib
        .artwork()
        .set(999, &ArtworkRecord::new("image/png", PNG_1X1.to_vec()))
        .is_err());

    let id = add_track(&lib, "/m/a.mp3");
    lib.artwork()
        .set(id, &ArtworkRecord::new("image/png", PNG_1X1.to_vec()))
        .unwrap();
    lib.tracks().remove(id).unwrap();
    let rows: i64 = lib
        .database()
        .connection()
        .query_row("SELECT COUNT(*) FROM artwork;", [], |row| row.get(0))
        .unwrap();
    assert_eq!(rows, 0);
}

#[test]
fn header_sizes_come_from_png_and_jpeg_headers_only() {
    assert_eq!(image_size(PNG_1X1), Some((1, 1)));

    // SOI, an APP0 segment, then a baseline SOF0 segment: 2 px wide, 3 px high.
    let jpeg = [
        0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00, 0xFF, 0xC0, 0x00, 0x08, 0x08, 0x00, 0x03,
        0x00, 0x02, 0x01,
    ];
    assert_eq!(image_size(&jpeg), Some((2, 3)));

    assert_eq!(image_size(b"GIF89a...."), None, "other formats are not sized");
    assert_eq!(image_size(&PNG_1X1[..20]), None, "a truncated header");
    assert_eq!(image_size(&[0xFF, 0xD8, 0xFF]), None);
}

#[test]
fn ingest_stores_the_front_cover_from_the_tag() {
    let lib = memory_library();
    let readers = MetadataReaderRegistry::with_defaults();
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("art.mp3");
    mp3_with_cover(&file, 10, Some((PNG_1X1, MimeType::Png)));

    let mut summary = om_library::IngestSummary::default();
    Ingestor::new(&lib, &readers)
        .ingest_folder(tmp.path(), &mut summary, &|| false)
        .unwrap();
    assert_eq!(summary.added, 1);

    let id = lib
        .tracks()
        .find_by_path(&stored(&file))
        .unwrap()
        .unwrap()
        .id;
    let art = lib.artwork().get(id).unwrap().expect("cover stored");
    assert_eq!(art.mime, "image/png");
    assert_eq!((art.width, art.height), (Some(1), Some(1)));
    assert_eq!(art.data, PNG_1X1);
}

#[test]
fn a_changed_file_updates_its_cover_and_a_removed_one_clears_it() {
    let lib = memory_library();
    let readers = MetadataReaderRegistry::with_defaults();
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("art.mp3");
    mp3_with_cover(&file, 10, Some((PNG_1X1, MimeType::Png)));
    touch(&file, 1_600_000_000_000);
    let scan = |lib: &Library| {
        let mut summary = om_library::IngestSummary::default();
        Ingestor::new(lib, &readers)
            .ingest_folder(tmp.path(), &mut summary, &|| false)
            .unwrap();
        summary
    };
    scan(&lib);
    let id = lib.tracks().find_by_path(&stored(&file)).unwrap().unwrap().id;
    assert!(lib.artwork().get(id).unwrap().is_some());

    // Same bytes and mtime: nothing is re-read, so the cover stays.
    assert_eq!(scan(&lib).unchanged, 1);

    // A new cover (a JPEG header) on a touched file replaces the old one.
    let jpeg = [0xFF, 0xD8, 0xFF, 0xC0, 0x00, 0x08, 0x08, 0x00, 0x05, 0x00, 0x07, 0x01];
    mp3_with_cover(&file, 10, Some((&jpeg, MimeType::Jpeg)));
    touch(&file, 1_700_000_000_000);
    assert_eq!(scan(&lib).updated, 1);
    let art = lib.artwork().get(id).unwrap().unwrap();
    assert_eq!(art.mime, "image/jpeg");
    assert_eq!((art.width, art.height), (Some(7), Some(5)));

    // The cover is gone from the file: the stored copy goes with it.
    mp3_with_cover(&file, 10, None);
    touch(&file, 1_800_000_000_000);
    assert_eq!(scan(&lib).updated, 1);
    assert!(lib.artwork().get(id).unwrap().is_none());
}

#[test]
fn an_oversized_cover_is_skipped_not_stored() {
    let lib = memory_library();
    let readers = MetadataReaderRegistry::with_defaults();
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("big.mp3");
    let mut big = PNG_1X1.to_vec();
    big.resize(MAX_ARTWORK_BYTES + 1, 0);
    mp3_with_cover(&file, 10, Some((&big, MimeType::Png)));

    let mut summary = om_library::IngestSummary::default();
    Ingestor::new(&lib, &readers)
        .ingest_folder(tmp.path(), &mut summary, &|| false)
        .unwrap();
    assert_eq!(summary.added, 1, "the track itself is still added");
    let id = lib.tracks().find_by_path(&stored(&file)).unwrap().unwrap().id;
    assert!(lib.artwork().get(id).unwrap().is_none());
}

#[test]
fn a_cover_at_exactly_the_limit_is_stored() {
    let lib = memory_library();
    let readers = MetadataReaderRegistry::with_defaults();
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("edge.mp3");
    let mut edge = PNG_1X1.to_vec();
    edge.resize(MAX_ARTWORK_BYTES, 0);
    mp3_with_cover(&file, 10, Some((&edge, MimeType::Png)));

    let mut summary = om_library::IngestSummary::default();
    Ingestor::new(&lib, &readers)
        .ingest_folder(tmp.path(), &mut summary, &|| false)
        .unwrap();
    let id = lib.tracks().find_by_path(&stored(&file)).unwrap().unwrap().id;
    assert_eq!(lib.artwork().get(id).unwrap().unwrap().data.len(), MAX_ARTWORK_BYTES);
}

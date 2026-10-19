//! Port of tests/unit/LibraryIngestTest.cpp, MetadataReaderTest group, plus lofty coverage for the
//! formats JUCE did not tag (MP3, FLAC, OGG, M4A) and cover art.

mod common;

use common::{
    save_tag, tag_of, write_flac, write_m4a, write_mp3, write_ogg, write_wav, Tags, TempDir,
};
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::prelude::*;
use lofty::tag::TagType;
use om_library::{MetadataReader, MetadataReaderRegistry, TrackMetadata};
use std::path::Path;

fn read(path: &Path) -> TrackMetadata {
    let mut meta = TrackMetadata::default();
    let reader = MetadataReaderRegistry::with_defaults();
    reader
        .find(path)
        .expect("reader claims the file")
        .read(path, &mut meta)
        .expect("readable");
    meta
}

#[test]
fn lofty_reader_reads_riff_info_and_duration() {
    let tmp = TempDir::new().unwrap();
    let f = write_wav(&tmp.path().join("a.wav"), 2.0, &Tags::night_drive(), 440.0);
    let m = read(&f);
    assert_eq!(m.title, "Night Drive");
    assert_eq!(m.artist, "The Artist");
    assert_eq!(m.album, "The Album");
    assert_eq!(m.genre, "House");
    assert_eq!(m.year, 2019);
    assert_eq!(m.comment, "nice");
    assert!(
        (m.duration_sec - 2.0).abs() < 0.01,
        "duration {}",
        m.duration_sec
    );
}

#[test]
fn untagged_file_falls_back_to_file_name_title() {
    let tmp = TempDir::new().unwrap();
    let f = write_wav(
        &tmp.path().join("Some Song.wav"),
        0.5,
        &Tags::default(),
        440.0,
    );
    let m = read(&f);
    assert_eq!(m.title, "Some Song");
    assert!(m.artist.is_empty());
}

#[test]
fn corrupt_file_fails() {
    let tmp = TempDir::new().unwrap();
    let f = tmp.path().join("bad.wav");
    std::fs::write(&f, "this is not audio").unwrap();
    let mut meta = TrackMetadata::default();
    assert!(MetadataReaderRegistry::with_defaults()
        .find(&f)
        .expect("claimed by extension")
        .read(&f, &mut meta)
        .is_err());
}

struct FakeReader;

impl MetadataReader for FakeReader {
    fn name(&self) -> &str {
        "fake"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["wav", "xyz"]
    }
    fn read(&self, _: &Path, out: &mut TrackMetadata) -> om_library::Result<()> {
        out.title = "fake".into();
        Ok(())
    }
}

#[test]
fn registry_later_reader_wins_and_lists_extensions() {
    let named = |n: &str| std::path::PathBuf::from("/music").join(n);
    let mut registry = MetadataReaderRegistry::with_defaults();
    assert!(registry.supported_extensions().iter().any(|e| e == "wav"));
    assert!(!registry.supports(&named("song.xyz")));
    assert!(!registry.supports(&named("notes.txt")));

    registry.add(FakeReader);
    assert_eq!(registry.find(&named("a.WAV")).unwrap().name(), "fake"); // case-insensitive
    assert!(registry.supports(&named("song.xyz")));
}

#[test]
fn lofty_reader_reads_id3_tags_and_duration_for_mp3() {
    let tmp = TempDir::new().unwrap();
    // 100 frames of 1152 samples at 44.1 kHz: 2.612 s
    let f = write_mp3(&tmp.path().join("m.mp3"), 100, &Tags::night_drive());
    let m = read(&f);
    assert_eq!(m.title, "Night Drive");
    assert_eq!(m.artist, "The Artist");
    assert_eq!(m.album, "The Album");
    assert_eq!(m.genre, "House");
    assert_eq!(m.year, 2019);
    assert_eq!(m.comment, "nice");
    assert!(
        (m.duration_sec - 2.612).abs() < 0.1,
        "duration {}",
        m.duration_sec
    );
}

#[test]
fn lofty_reader_reads_vorbis_comments_and_duration_for_flac() {
    let tmp = TempDir::new().unwrap();
    let f = write_flac(&tmp.path().join("f.flac"), 3, &Tags::night_drive());
    let m = read(&f);
    assert_eq!(m.title, "Night Drive");
    assert_eq!(m.artist, "The Artist");
    assert_eq!(m.album, "The Album");
    assert_eq!(m.year, 2019);
    assert!(
        (m.duration_sec - 3.0).abs() < 0.01,
        "duration {}",
        m.duration_sec
    );
}

#[test]
fn lofty_reader_reads_vorbis_comments_and_duration_for_ogg() {
    let tmp = TempDir::new().unwrap();
    let f = write_ogg(&tmp.path().join("o.ogg"), 2, &Tags::night_drive());
    let m = read(&f);
    assert_eq!(m.title, "Night Drive");
    assert_eq!(m.artist, "The Artist");
    assert_eq!(m.genre, "House");
    assert!(
        (m.duration_sec - 2.0).abs() < 0.01,
        "duration {}",
        m.duration_sec
    );
}

#[test]
fn lofty_reader_reads_itunes_atoms_and_duration_for_m4a() {
    let tmp = TempDir::new().unwrap();
    let f = write_m4a(&tmp.path().join("m.m4a"), 4, &Tags::night_drive());
    let m = read(&f);
    assert_eq!(m.title, "Night Drive");
    assert_eq!(m.artist, "The Artist");
    assert_eq!(m.album, "The Album");
    assert_eq!(m.genre, "House");
    assert!(
        (m.duration_sec - 4.0).abs() < 0.01,
        "duration {}",
        m.duration_sec
    );
}

#[test]
fn cover_art_is_read_from_the_front_cover_picture() {
    let tmp = TempDir::new().unwrap();
    let f = write_mp3(&tmp.path().join("art.mp3"), 10, &Tags::night_drive());
    let jpeg = vec![0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3, 4, 0xFF, 0xD9];
    let mut tag = tag_of(TagType::Id3v2, &Tags::night_drive());
    tag.push_picture(
        Picture::unchecked(jpeg.clone())
            .pic_type(PictureType::CoverFront)
            .mime_type(MimeType::Jpeg)
            .build(),
    );
    save_tag(&f, &tag);

    let m = read(&f);
    let art = m.artwork.expect("cover art read");
    assert_eq!(art.mime_type, "image/jpeg");
    assert_eq!(art.data, jpeg);
}

#[test]
fn untagged_formats_report_no_artwork() {
    let tmp = TempDir::new().unwrap();
    let f = write_wav(&tmp.path().join("plain.wav"), 0.5, &Tags::default(), 440.0);
    assert!(read(&f).artwork.is_none());
    // lofty's own view agrees: the WAV has no pictures
    assert!(lofty::read_from_path(&f)
        .unwrap()
        .primary_tag()
        .map_or(true, |t| t.pictures().is_empty()));
}

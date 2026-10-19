//! Shared fixtures for the ingest and metadata tests: generated audio files with tags, built from
//! bytes so no binary fixtures are checked in. Port of tests/unit/LibraryIngestTestUtil.h.
//!
//! WAV (with RIFF INFO), FLAC, MP3, OGG Vorbis and M4A are hand-built minimal containers. Tags for
//! ID3v2, Vorbis comments and iTunes atoms are then written with lofty's own writer.

#![allow(dead_code)] // each test binary uses a different subset

use lofty::config::WriteOptions;
use lofty::prelude::*;
use lofty::tag::{Tag, TagType};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub use tempfile::TempDir;

/// The sample rate used by every generated file.
pub const RATE: u32 = 8000;

/// Text for each tag, keyed by the metadata a test wants to set.
#[derive(Debug, Clone, Default)]
pub struct Tags {
    pub title: Option<&'static str>,
    pub artist: Option<&'static str>,
    pub album: Option<&'static str>,
    pub genre: Option<&'static str>,
    pub year: Option<&'static str>,
    pub comment: Option<&'static str>,
}

impl Tags {
    /// The tag set used by the C++ `tags()` helper.
    pub fn night_drive() -> Self {
        Self {
            title: Some("Night Drive"),
            artist: Some("The Artist"),
            album: Some("The Album"),
            genre: Some("House"),
            year: Some("2019-05-01"),
            comment: Some("nice"),
        }
    }
}

fn create_parent(path: &Path) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create fixture folder");
    }
}

/// Sine samples as 16-bit little-endian PCM, mono, at `RATE`.
fn sine_pcm(seconds: f64, freq: f32) -> Vec<u8> {
    let n = (seconds * f64::from(RATE)) as usize;
    let mut out = Vec::with_capacity(n * 2);
    for i in 0..n {
        let phase = 2.0 * std::f32::consts::PI * freq * i as f32 / RATE as f32;
        let sample = (0.5 * phase.sin() * f32::from(i16::MAX)) as i16;
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

fn riff_chunk(out: &mut Vec<u8>, id: &[u8; 4], payload: &[u8]) {
    out.extend_from_slice(id);
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    if payload.len() % 2 == 1 {
        out.push(0); // RIFF chunks are padded to even length
    }
}

/// `LIST`/`INFO` chunk body for the given four-character keys.
fn riff_info(entries: &[(&[u8; 4], &str)]) -> Vec<u8> {
    let mut body = b"INFO".to_vec();
    for (key, text) in entries {
        let mut value = text.as_bytes().to_vec();
        value.push(0);
        riff_chunk(&mut body, key, &value);
    }
    body
}

/// Short sine WAV, 16-bit mono at `RATE`, with optional RIFF INFO tags (INAM, IART, IPRD, IGNR,
/// ICRD, ICMT). `seconds` sets the duration, `freq` the content. Overwrites an existing file.
pub fn write_wav(path: &Path, seconds: f64, tags: &Tags, freq: f32) -> PathBuf {
    create_parent(path);
    let pcm = sine_pcm(seconds, freq);

    let mut fmt = Vec::new();
    fmt.extend_from_slice(&1u16.to_le_bytes()); // PCM
    fmt.extend_from_slice(&1u16.to_le_bytes()); // mono
    fmt.extend_from_slice(&RATE.to_le_bytes());
    fmt.extend_from_slice(&(RATE * 2).to_le_bytes()); // byte rate
    fmt.extend_from_slice(&2u16.to_le_bytes()); // block align
    fmt.extend_from_slice(&16u16.to_le_bytes()); // bits per sample

    let mut body = b"WAVE".to_vec();
    riff_chunk(&mut body, b"fmt ", &fmt);
    riff_chunk(&mut body, b"data", &pcm);

    let info_entries: Vec<(&[u8; 4], &str)> = [
        (b"INAM", tags.title),
        (b"IART", tags.artist),
        (b"IPRD", tags.album),
        (b"IGNR", tags.genre),
        (b"ICRD", tags.year),
        (b"ICMT", tags.comment),
    ]
    .into_iter()
    .filter_map(|(key, value)| value.map(|v| (key, v)))
    .collect();
    if !info_entries.is_empty() {
        riff_chunk(&mut body, b"LIST", &riff_info(&info_entries));
    }

    let mut file = b"RIFF".to_vec();
    file.extend_from_slice(&(body.len() as u32).to_le_bytes());
    file.extend_from_slice(&body);
    fs::write(path, file).expect("write wav");
    path.to_path_buf()
}

/// Writes `tag` into the file at `path` with lofty's writer.
pub fn save_tag(path: &Path, tag: &Tag) {
    tag.save_to_path(path, WriteOptions::default())
        .expect("write tags");
}

/// Fills a lofty tag from `tags` using the generic item keys each format maps.
pub fn tag_of(kind: TagType, tags: &Tags) -> Tag {
    let mut tag = Tag::new(kind);
    if let Some(v) = tags.title {
        tag.insert_text(ItemKey::TrackTitle, v.to_owned());
    }
    if let Some(v) = tags.artist {
        tag.insert_text(ItemKey::TrackArtist, v.to_owned());
    }
    if let Some(v) = tags.album {
        tag.insert_text(ItemKey::AlbumTitle, v.to_owned());
    }
    if let Some(v) = tags.genre {
        tag.insert_text(ItemKey::Genre, v.to_owned());
    }
    if let Some(v) = tags.year {
        tag.insert_text(ItemKey::RecordingDate, v.to_owned());
    }
    if let Some(v) = tags.comment {
        tag.insert_text(ItemKey::Comment, v.to_owned());
    }
    tag
}

/// MPEG-1 Layer III frames at 128 kbps, 44.1 kHz (417 bytes each, 1152 samples, silent bodies).
/// `frames` sets the length: duration is `frames * 1152 / 44100` seconds.
pub fn write_mp3(path: &Path, frames: usize, tags: &Tags) -> PathBuf {
    create_parent(path);
    let mut bytes = Vec::with_capacity(frames * 417);
    for _ in 0..frames {
        bytes.extend_from_slice(&[0xFF, 0xFB, 0x90, 0x00]); // sync, MPEG-1 L3, 128k, 44.1k
        bytes.resize(bytes.len() + 413, 0);
    }
    fs::write(path, bytes).expect("write mp3");
    save_tag(path, &tag_of(TagType::Id3v2, tags));
    path.to_path_buf()
}

/// FLAC with a STREAMINFO block (mono, 16-bit, 44.1 kHz) and no audio frames, plus Vorbis
/// comments. `seconds` sets the duration through the stream's total sample count.
pub fn write_flac(path: &Path, seconds: u32, tags: &Tags) -> PathBuf {
    create_parent(path);
    const SAMPLE_RATE: u64 = 44_100;
    let total = u64::from(seconds) * SAMPLE_RATE;

    let mut info = Vec::with_capacity(34);
    info.extend_from_slice(&4096u16.to_be_bytes()); // min block size
    info.extend_from_slice(&4096u16.to_be_bytes()); // max block size
    info.extend_from_slice(&[0, 0, 0, 0, 0, 0]); // frame sizes unknown
    // sample rate (20 bits), channels-1 (3), bits-1 (5), total samples (36)
    let packed = (SAMPLE_RATE << 44) | (15u64 << 36) | total;
    info.extend_from_slice(&packed.to_be_bytes());
    info.extend_from_slice(&[0u8; 16]); // MD5 unknown

    let mut bytes = b"fLaC".to_vec();
    bytes.push(0x80); // last-metadata-block flag, block type 0 (STREAMINFO)
    bytes.extend_from_slice(&[0x00, 0x00, 0x22]); // length 34
    bytes.extend_from_slice(&info);
    fs::write(path, bytes).expect("write flac");
    save_tag(path, &tag_of(TagType::VorbisComments, tags));
    path.to_path_buf()
}

const OGG_CRC_POLY: u32 = 0x04c1_1db7;

fn ogg_crc(bytes: &[u8]) -> u32 {
    let mut crc = 0u32;
    for &byte in bytes {
        crc ^= u32::from(byte) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 {
                (crc << 1) ^ OGG_CRC_POLY
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// One Ogg page holding one packet (which may be empty).
fn ogg_page(out: &mut Vec<u8>, flags: u8, granule: i64, serial: u32, seq: u32, packet: &[u8]) {
    let mut lacing = Vec::new();
    let mut left = packet.len();
    while left >= 255 {
        lacing.push(255u8);
        left -= 255;
    }
    lacing.push(left as u8);

    let mut page = b"OggS".to_vec();
    page.push(0); // stream structure version
    page.push(flags);
    page.extend_from_slice(&granule.to_le_bytes());
    page.extend_from_slice(&serial.to_le_bytes());
    page.extend_from_slice(&seq.to_le_bytes());
    page.extend_from_slice(&[0, 0, 0, 0]); // CRC placeholder
    page.push(lacing.len() as u8);
    page.extend_from_slice(&lacing);
    page.extend_from_slice(packet);
    let crc = ogg_crc(&page);
    page[22..26].copy_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&page);
}

/// Ogg Vorbis (mono, 44.1 kHz) with a valid identification, comment and setup header and no audio
/// packets, then Vorbis comments written with lofty. `seconds` sets the duration through the last
/// page's granule position.
pub fn write_ogg(path: &Path, seconds: u32, tags: &Tags) -> PathBuf {
    create_parent(path);
    const SAMPLE_RATE: u32 = 44_100;
    let serial = 0x0BAD_F00D;

    let mut ident = vec![1u8];
    ident.extend_from_slice(b"vorbis");
    ident.extend_from_slice(&0u32.to_le_bytes()); // version
    ident.push(1); // channels
    ident.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    ident.extend_from_slice(&0i32.to_le_bytes()); // max bitrate
    ident.extend_from_slice(&128_000i32.to_le_bytes()); // nominal bitrate
    ident.extend_from_slice(&0i32.to_le_bytes()); // min bitrate
    ident.push(0xB8); // block sizes 256 / 2048
    ident.push(1); // framing

    let mut comments = vec![3u8];
    comments.extend_from_slice(b"vorbis");
    let vendor = b"openmixer-test";
    comments.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
    comments.extend_from_slice(vendor);
    comments.extend_from_slice(&0u32.to_le_bytes()); // no comments; lofty writes them below
    comments.push(1); // framing

    let mut setup = vec![5u8];
    setup.extend_from_slice(b"vorbis");
    setup.extend_from_slice(&[0u8; 16]);

    let mut bytes = Vec::new();
    ogg_page(&mut bytes, 0x02, 0, serial, 0, &ident);
    ogg_page(&mut bytes, 0x00, 0, serial, 1, &comments);
    ogg_page(&mut bytes, 0x00, 0, serial, 2, &setup);
    let end = i64::from(seconds) * i64::from(SAMPLE_RATE);
    ogg_page(&mut bytes, 0x04, end, serial, 3, &[]);
    fs::write(path, bytes).expect("write ogg");
    save_tag(path, &tag_of(TagType::VorbisComments, tags));
    path.to_path_buf()
}

fn mp4_box(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut out = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
    out.extend_from_slice(kind);
    out.extend_from_slice(payload);
    out
}

/// Minimal M4A: `ftyp` plus a `moov` with a 1000-unit timescale and `seconds * 1000` duration.
/// Carries no audio track; lofty reads its tags and duration from the movie header.
pub fn write_m4a(path: &Path, seconds: u32, tags: &Tags) -> PathBuf {
    create_parent(path);
    let mut ftyp = b"M4A ".to_vec();
    ftyp.extend_from_slice(&0u32.to_be_bytes());
    ftyp.extend_from_slice(b"M4A isom");

    let mut mvhd = vec![0u8; 4]; // version 0, flags 0
    mvhd.extend_from_slice(&[0u8; 8]); // creation and modification time
    mvhd.extend_from_slice(&1000u32.to_be_bytes()); // timescale
    mvhd.extend_from_slice(&(seconds * 1000).to_be_bytes()); // duration
    mvhd.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // rate 1.0
    mvhd.extend_from_slice(&0x0100u16.to_be_bytes()); // volume 1.0
    mvhd.extend_from_slice(&[0u8; 10]); // reserved
    for word in [0x0001_0000u32, 0, 0, 0, 0x0001_0000, 0, 0, 0, 0x4000_0000] {
        mvhd.extend_from_slice(&word.to_be_bytes()); // unity matrix
    }
    mvhd.extend_from_slice(&[0u8; 24]); // pre-defined
    mvhd.extend_from_slice(&2u32.to_be_bytes()); // next track id

    let mut file = mp4_box(b"ftyp", &ftyp);
    file.extend_from_slice(&mp4_box(b"moov", &mp4_box(b"mvhd", &mvhd)));
    fs::write(path, file).expect("write m4a");
    save_tag(path, &tag_of(TagType::Mp4Ilst, tags));
    path.to_path_buf()
}

/// Sets the file's modification time, as the C++ `touch` helper does.
pub fn touch(path: &Path, ms: u64) {
    let when = UNIX_EPOCH + Duration::from_millis(ms);
    fs::OpenOptions::new()
        .write(true)
        .open(path)
        .and_then(|f| f.set_modified(when))
        .expect("set mtime");
}

/// A path inside `dir`, with the same spelling the library stores for it.
pub fn in_dir(dir: &Path, name: &str) -> PathBuf {
    dir.join(name)
}

/// Stored-path spelling of a file (what the library keys its rows by).
pub fn stored(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Reads a tag set back through the lofty writer's own reader, for fixture self-checks.
pub fn reread(path: &Path) -> lofty::file::TaggedFile {
    lofty::read_from_path(path).expect("fixture is readable")
}

/// Keeps `SystemTime` in the import list used by `touch` callers.
pub fn epoch_ms(ms: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_millis(ms)
}

//! Tag and duration reading for ingest. Port of `src/library/MetadataReader.*` and
//! `JuceMetadataReader.*`. The default reader uses lofty instead of JUCE's format manager: tags for
//! MP3 (ID3v2), FLAC and OGG (Vorbis comments), M4A/MP4 (iTunes atoms), and WAV/AIFF (RIFF INFO),
//! plus duration and the front cover where the container has one.
//! Called from the ingest thread only.

use crate::error::{LibraryError, Result};
use lofty::file::{AudioFile, TaggedFile, TaggedFileExt};
use lofty::picture::PictureType;
use lofty::tag::{ItemKey, Tag};
use std::path::Path;

/// Cover art found in the file's tags. Read but not stored yet (no artwork table in the schema).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Artwork {
    pub mime_type: String,
    pub data: Vec<u8>,
}

/// What a reader can tell about a file; empty/zero means "not present in the file".
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrackMetadata {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub comment: String,
    pub year: i32,
    pub duration_sec: f64,
    pub artwork: Option<Artwork>,
}

/// Tag-reading extension point. Readers claim lower-case extensions without the dot.
pub trait MetadataReader: Send {
    fn name(&self) -> &str;
    fn extensions(&self) -> &'static [&'static str];
    /// Fails when the file cannot be decoded.
    fn read(&self, path: &Path, out: &mut TrackMetadata) -> Result<()>;
}

/// Later registrations win for an extension, so a better reader can override a default.
#[derive(Default)]
pub struct MetadataReaderRegistry {
    readers: Vec<Box<dyn MetadataReader>>,
}

impl MetadataReaderRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registry preloaded with the built-in lofty reader.
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.add(LoftyMetadataReader);
        registry
    }

    pub fn add<R: MetadataReader + 'static>(&mut self, reader: R) {
        self.readers.push(Box::new(reader));
    }

    /// The newest reader that claims the file's extension (case-insensitive).
    pub fn find(&self, path: &Path) -> Option<&dyn MetadataReader> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        self.readers
            .iter()
            .rev()
            .find(|r| r.extensions().contains(&ext.as_str()))
            .map(|r| r.as_ref())
    }

    pub fn supports(&self, path: &Path) -> bool {
        self.find(path).is_some()
    }

    /// Every extension any reader claims, without duplicates, in registration order.
    pub fn supported_extensions(&self) -> Vec<String> {
        let mut all: Vec<String> = Vec::new();
        for reader in &self.readers {
            for ext in reader.extensions() {
                if !all.iter().any(|e| e == ext) {
                    all.push((*ext).to_owned());
                }
            }
        }
        all
    }
}

/// The built-in reader: lofty probes the container from its bytes, so the extension only decides
/// whether the file is offered to it.
pub struct LoftyMetadataReader;

const LOFTY_EXTENSIONS: &[&str] = &[
    "wav", "aif", "aiff", "flac", "ogg", "oga", "opus", "mp3", "m4a", "m4b", "mp4", "aac",
];

impl MetadataReader for LoftyMetadataReader {
    fn name(&self) -> &str {
        "lofty"
    }

    fn extensions(&self) -> &'static [&'static str] {
        LOFTY_EXTENSIONS
    }

    fn read(&self, path: &Path, out: &mut TrackMetadata) -> Result<()> {
        let tagged = lofty::read_from_path(path).map_err(|e| {
            LibraryError::Metadata(format!(
                "not a decodable audio file: {}: {e}",
                file_name(path)
            ))
        })?;
        *out = read_tagged(&tagged);
        if out.title.is_empty() {
            // why: library rows need something to show
            out.title = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
        }
        Ok(())
    }
}

fn read_tagged(tagged: &TaggedFile) -> TrackMetadata {
    let tags = tagged.tags();
    TrackMetadata {
        title: first_text(tags, &[ItemKey::TrackTitle]),
        artist: first_text(tags, &[ItemKey::TrackArtist]),
        album: first_text(tags, &[ItemKey::AlbumTitle]),
        genre: first_text(tags, &[ItemKey::Genre]),
        comment: first_text(tags, &[ItemKey::Comment]),
        year: leading_year(&first_text(tags, &[ItemKey::RecordingDate, ItemKey::Year])),
        duration_sec: tagged.properties().duration().as_secs_f64(),
        artwork: cover_art(tags),
    }
}

/// First non-blank text for the first key that has one. Keys are tried in order, tags in file order.
fn first_text(tags: &[Tag], keys: &[ItemKey]) -> String {
    for key in keys {
        for tag in tags {
            if let Some(text) = tag.get_string(*key).map(str::trim) {
                if !text.is_empty() {
                    return text.to_owned();
                }
            }
        }
    }
    String::new()
}

/// Four leading digits as a year (1000..=9999), otherwise 0. "2019-05-01" gives 2019.
fn leading_year(date: &str) -> i32 {
    let head: String = date.chars().take(4).collect();
    head.parse::<i32>()
        .ok()
        .filter(|y| (1000..=9999).contains(y))
        .unwrap_or(0)
}

fn cover_art(tags: &[Tag]) -> Option<Artwork> {
    for tag in tags {
        let picture = tag
            .get_picture_type(PictureType::CoverFront)
            .or_else(|| tag.pictures().first());
        if let Some(picture) = picture {
            if picture.data().is_empty() {
                continue;
            }
            return Some(Artwork {
                mime_type: picture
                    .mime_type()
                    .map_or_else(|| "application/octet-stream".to_owned(), |m| m.as_str().to_owned()),
                data: picture.data().to_vec(),
            });
        }
    }
    None
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leading_year_accepts_four_digit_prefixes_only() {
        assert_eq!(leading_year("2019-05-01"), 2019);
        assert_eq!(leading_year("1999"), 1999);
        assert_eq!(leading_year("19-5"), 0);
        assert_eq!(leading_year("0999"), 0);
        assert_eq!(leading_year(""), 0);
    }

    #[test]
    fn registry_prefers_newest_reader_and_lists_extensions_once() {
        struct Named(&'static str);
        impl MetadataReader for Named {
            fn name(&self) -> &str {
                self.0
            }
            fn extensions(&self) -> &'static [&'static str] {
                &["wav"]
            }
            fn read(&self, _: &Path, _: &mut TrackMetadata) -> Result<()> {
                Ok(())
            }
        }
        let mut registry = MetadataReaderRegistry::with_defaults();
        registry.add(Named("override"));
        assert_eq!(
            registry.find(Path::new("/m/a.WAV")).map(|r| r.name()),
            Some("override")
        );
        let exts = registry.supported_extensions();
        assert_eq!(exts.iter().filter(|e| *e == "wav").count(), 1);
    }
}

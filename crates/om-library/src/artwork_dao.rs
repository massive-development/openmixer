//! Cover art per track (migration 6). Thumbnail-sized only: ingest stores the front cover exactly as
//! the file's tag holds it (no re-encoding) and skips covers over [`MAX_ARTWORK_BYTES`]. Same
//! threading rules as `TrackDao`.

use crate::database::Database;
use crate::error::Result;
use rusqlite::{params, OptionalExtension};

/// Largest cover ingest stores. Larger covers are skipped, not shrunk.
pub const MAX_ARTWORK_BYTES: usize = 512 * 1024;

/// One stored cover.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtworkRecord {
    /// The image type as the tag declared it, for example "image/jpeg".
    pub mime: String,
    /// Pixel size, read from the image header when the format is PNG or JPEG.
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub data: Vec<u8>,
}

impl ArtworkRecord {
    /// A record for `data`, with the size read from its header.
    pub fn new(mime: impl Into<String>, data: Vec<u8>) -> Self {
        let (width, height) = match image_size(&data) {
            Some((w, h)) => (Some(w), Some(h)),
            None => (None, None),
        };
        Self {
            mime: mime.into(),
            width,
            height,
            data,
        }
    }
}

/// Pixel size of a PNG or JPEG, from its header only. `None` for other formats or a damaged header.
pub fn image_size(data: &[u8]) -> Option<(u32, u32)> {
    const PNG_MAGIC: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if data.starts_with(PNG_MAGIC) {
        // IHDR follows the signature: length (4), "IHDR" (4), width (4), height (4).
        if data.get(12..16)? != b"IHDR" {
            return None;
        }
        let width = u32::from_be_bytes(data.get(16..20)?.try_into().ok()?);
        let height = u32::from_be_bytes(data.get(20..24)?.try_into().ok()?);
        return Some((width, height));
    }
    if data.starts_with(&[0xFF, 0xD8]) {
        return jpeg_size(data);
    }
    None
}

/// Walks JPEG markers to the first start-of-frame segment, which carries the size.
fn jpeg_size(data: &[u8]) -> Option<(u32, u32)> {
    let mut pos = 2;
    loop {
        // Skip to the next marker; fill bytes (0xFF) may precede the marker byte.
        while *data.get(pos)? == 0xFF {
            pos += 1;
        }
        let marker = *data.get(pos)?;
        pos += 1;
        // Standalone markers carry no length: SOI, EOI, TEM and the restart markers.
        if marker == 0xD8 || marker == 0xD9 || marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
            continue;
        }
        let length = usize::from(u16::from_be_bytes(data.get(pos..pos + 2)?.try_into().ok()?));
        // SOF0..SOF15, except DHT (C4), JPG (C8) and DAC (CC), which share the range.
        let is_frame = (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC);
        if is_frame {
            // Segment: length (2), precision (1), height (2), width (2).
            let height = u16::from_be_bytes(data.get(pos + 3..pos + 5)?.try_into().ok()?);
            let width = u16::from_be_bytes(data.get(pos + 5..pos + 7)?.try_into().ok()?);
            return Some((u32::from(width), u32::from(height)));
        }
        pos += length;
    }
}

/// Cover art reads and writes.
pub struct ArtworkDao<'a> {
    db: &'a Database,
}

impl<'a> ArtworkDao<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// The track's cover, if it has one.
    pub fn get(&self, track_id: i64) -> Result<Option<ArtworkRecord>> {
        Ok(self
            .db
            .connection()
            .query_row(
                "SELECT mime, width, height, data FROM artwork WHERE track_id = ?1;",
                params![track_id],
                |row| {
                    Ok(ArtworkRecord {
                        mime: row.get(0)?,
                        width: row.get::<_, Option<i64>>(1)?.and_then(|v| u32::try_from(v).ok()),
                        height: row.get::<_, Option<i64>>(2)?.and_then(|v| u32::try_from(v).ok()),
                        data: row.get(3)?,
                    })
                },
            )
            .optional()?)
    }

    /// Stores the cover for the track, replacing any previous one. Fails for an unknown track.
    pub fn set(&self, track_id: i64, art: &ArtworkRecord) -> Result<()> {
        self.db.connection().execute(
            "INSERT OR REPLACE INTO artwork (track_id, mime, width, height, data) \
             VALUES (?1, ?2, ?3, ?4, ?5);",
            params![
                track_id,
                art.mime,
                art.width.map(i64::from),
                art.height.map(i64::from),
                art.data,
            ],
        )?;
        Ok(())
    }

    /// Removes the track's cover. Removing a track that has none is not an error.
    pub fn delete(&self, track_id: i64) -> Result<()> {
        self.db
            .connection()
            .execute("DELETE FROM artwork WHERE track_id = ?1;", params![track_id])?;
        Ok(())
    }
}

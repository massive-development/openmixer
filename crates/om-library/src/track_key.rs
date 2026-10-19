//! Keeps a track's key and Camelot code consistent; shared by importers, merge and prep results.

use crate::track_record::TrackRecord;
use om_analysis::MusicalKey;

/// Sets `key` (canonical name) and `camelot` from a parsed key.
pub fn set_key(track: &mut TrackRecord, key: &MusicalKey) {
    track.key = key.name();
    track.camelot = key.camelot();
}

/// Derives the canonical key and Camelot code from whichever of the two is present (name or
/// Camelot code). Unparseable text leaves the track untouched.
pub fn normalise_key(track: &mut TrackRecord) {
    let source = if track.key.trim().is_empty() {
        &track.camelot
    } else {
        &track.key
    };
    if let Some(key) = parse(source) {
        set_key(track, &key);
    }
}

fn parse(text: &str) -> Option<MusicalKey> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    MusicalKey::from_name(trimmed).or_else(|| MusicalKey::from_camelot(trimmed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalise_fills_camelot_from_name_and_name_from_camelot() {
        let mut t = TrackRecord {
            key: "Am".into(),
            ..Default::default()
        };
        normalise_key(&mut t);
        assert_eq!(t.camelot, "8A");

        let mut t = TrackRecord {
            camelot: "8A".into(),
            ..Default::default()
        };
        normalise_key(&mut t);
        assert_eq!(t.key, "Am");
    }

    #[test]
    fn unparseable_key_is_left_alone() {
        let mut t = TrackRecord {
            key: "not a key".into(),
            camelot: "8A".into(),
            ..Default::default()
        };
        normalise_key(&mut t);
        assert_eq!(t.key, "not a key");
        assert_eq!(t.camelot, "8A");
    }
}

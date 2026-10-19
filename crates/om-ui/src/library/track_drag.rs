//! Drag-and-drop payload for library tracks. The browser table describes a row with [`describe`];
//! a deck widget accepts it with [`parse`]. A plain string, so plugin widgets can start or accept the
//! same drag. Port of `src/ui/TrackDrag.h`.

/// Prefix that marks a library-track drag.
pub const PREFIX: &str = "openmixer.track:";

/// The drag description for one track.
pub fn describe(track_id: i64) -> String {
    format!("{PREFIX}{track_id}")
}

/// The track id in a drag description; `None` for anything else.
pub fn parse(description: &str) -> Option<i64> {
    let rest = description.strip_prefix(PREFIX)?;
    if rest.is_empty() {
        return None;
    }
    // JUCE's getLargeIntValue reads a leading integer and stops at the first non-digit.
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit() || *c == '-').collect();
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describe_and_parse_round_trip() {
        assert_eq!(parse(&describe(42)), Some(42));
        assert_eq!(parse("not a track"), None);
        assert_eq!(parse(PREFIX), None);
    }
}

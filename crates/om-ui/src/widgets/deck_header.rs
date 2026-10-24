//! Pure display logic of the deck header: which key notation to show, the time-remaining warning,
//! loop-size labels and the colours and sizes the header uses. The gpui layer only places what this
//! module decides.
//!
//! The sizes are local constants (px) that can later move into theme tokens; the colours are the
//! status colours of the header (sync on, time running out) and do not follow the deck accent.

use super::deck::TrackInfo;
use crate::theme::Colour;

/// Remaining time at or below which the countdown turns red, in seconds.
pub const REMAINING_WARN_SECS: f64 = 30.0;

/// Time-remaining numeral: the largest number in the header.
pub const TIME_REMAINING_SIZE: f32 = 38.0;
/// Effective BPM numeral.
pub const BPM_SIZE: f32 = 28.0;
/// Key numeral.
pub const KEY_SIZE: f32 = 28.0;
/// Elapsed time, title and artist (secondary text).
pub const ELAPSED_SIZE: f32 = 14.0;
pub const TITLE_SIZE: f32 = 16.0;
pub const ARTIST_SIZE: f32 = 13.0;
/// Smallest text anywhere on a deck: captions, chips, tab labels.
pub const MIN_TEXT_SIZE: f32 = 12.0;
/// Height of the stage header band and of a deck card's own header.
pub const HEADER_HEIGHT: i32 = 60;
/// The deck letter badge is a square this wide.
pub const BADGE_SIZE: i32 = 28;

/// Fill of the SYNC button and chip while sync is on.
pub const SYNC_ON: Colour = Colour::from_argb(0xFF19_D3A2);
/// Countdown under [`REMAINING_WARN_SECS`].
pub const WARN_RED: Colour = Colour::from_argb(0xFFFF_3B4E);
/// Text on [`SYNC_ON`] and on the cue palette: near-black for contrast on the bright fills.
pub const ON_BRIGHT: Colour = Colour::from_argb(0xFF0B_0F14);

/// Type sizes of the header numerals for a header `width` px wide. The full sizes need about 520 px;
/// narrower headers (the 1024 px window gives each deck about 370) step down so the title keeps
/// room, but never below the 22 px numeral and 28 px countdown.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeaderMetrics {
    pub time: f32,
    pub number: f32,
}

pub fn header_metrics(width: i32) -> HeaderMetrics {
    if width >= 520 {
        HeaderMetrics {
            time: TIME_REMAINING_SIZE,
            number: BPM_SIZE,
        }
    } else if width >= 430 {
        HeaderMetrics {
            time: 32.0,
            number: 24.0,
        }
    } else {
        HeaderMetrics {
            time: 28.0,
            number: 22.0,
        }
    }
}

/// Letter badge of a deck: A, B, C, D, ... so the identity is not carried by colour alone.
pub fn deck_letter(index: usize) -> char {
    char::from(b'A' + (index % 26) as u8)
}

/// Which spelling of the musical key the header shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeyNotation {
    /// Camelot wheel code, `8A`.
    #[default]
    Camelot,
    /// Musical name, `Am`.
    Musical,
}

/// The key in the chosen notation only. If the track lacks that spelling, the other one is shown
/// rather than nothing; `--` when the key is unknown.
pub fn format_key(info: &TrackInfo, notation: KeyNotation) -> String {
    let (first, second) = match notation {
        KeyNotation::Camelot => (&info.camelot, &info.key),
        KeyNotation::Musical => (&info.key, &info.camelot),
    };
    [first, second]
        .into_iter()
        .find(|s| !s.is_empty())
        .cloned()
        .unwrap_or_else(|| "--".to_owned())
}

/// Whether the countdown shows the warning colour: a track is loaded (`length_secs > 0`) and no
/// more than [`REMAINING_WARN_SECS`] are left.
pub fn remaining_warning(remaining_secs: f64, length_secs: f64) -> bool {
    length_secs > 0.0 && remaining_secs <= REMAINING_WARN_SECS
}

/// A loop length in beats as the button shows it: `1/4`, `1/2`, `1`, `8`; one decimal otherwise.
/// Empty for no loop.
pub fn loop_label(beats: f64) -> String {
    if beats <= 0.0 {
        return String::new();
    }
    let near = |target: f64| (beats - target).abs() < 0.01;
    if near(0.25) {
        "1/4".to_owned()
    } else if near(0.5) {
        "1/2".to_owned()
    } else if (beats - beats.round()).abs() < 0.01 {
        format!("{}", beats.round() as i64)
    } else {
        format!("{beats:.1}")
    }
}

/// Eight cue colours, hot to cool, bright enough for [`ON_BRIGHT`] text.
pub const CUE_PALETTE: [Colour; 8] = [
    Colour::from_argb(0xFFFF_5468), // red
    Colour::from_argb(0xFFFF_9248), // orange
    Colour::from_argb(0xFFFF_D84A), // yellow
    Colour::from_argb(0xFF7B_E05A), // green
    Colour::from_argb(0xFF2E_D8C0), // teal
    Colour::from_argb(0xFF4F_B4FF), // blue
    Colour::from_argb(0xFFA7_88FF), // violet
    Colour::from_argb(0xFFFF_6FC4), // pink
];

#[cfg(test)]
mod tests {
    use super::*;

    fn info(key: &str, camelot: &str) -> TrackInfo {
        TrackInfo {
            key: key.into(),
            camelot: camelot.into(),
            ..Default::default()
        }
    }

    #[test]
    fn key_shows_one_notation_with_a_fallback() {
        let both = info("Am", "8A");
        assert_eq!(format_key(&both, KeyNotation::Camelot), "8A");
        assert_eq!(format_key(&both, KeyNotation::Musical), "Am");
        assert_eq!(format_key(&info("Am", ""), KeyNotation::Camelot), "Am");
        assert_eq!(format_key(&info("", "8A"), KeyNotation::Musical), "8A");
        assert_eq!(format_key(&info("", ""), KeyNotation::Camelot), "--");
    }

    #[test]
    fn the_countdown_warns_at_thirty_seconds_and_only_with_a_track() {
        assert!(!remaining_warning(30.5, 200.0));
        assert!(remaining_warning(30.0, 200.0));
        assert!(remaining_warning(0.0, 200.0));
        assert!(!remaining_warning(0.0, 0.0), "no track, no warning");
    }

    #[test]
    fn loop_sizes_read_as_short_labels() {
        assert_eq!(loop_label(0.0), "");
        assert_eq!(loop_label(0.25), "1/4");
        assert_eq!(loop_label(0.5), "1/2");
        assert_eq!(loop_label(1.0), "1");
        assert_eq!(loop_label(8.004), "8");
        assert_eq!(loop_label(32.0), "32");
        assert_eq!(loop_label(1.5), "1.5");
    }

    #[test]
    fn the_countdown_is_always_the_largest_number() {
        for width in [300, 370, 450, 520, 900] {
            let m = header_metrics(width);
            assert!(m.time > m.number, "{width}: {m:?}");
            assert!(m.number > ELAPSED_SIZE);
        }
        assert_eq!(header_metrics(900).time, TIME_REMAINING_SIZE);
    }

    #[test]
    fn deck_letters_and_cue_palette() {
        assert_eq!(
            (0..4).map(deck_letter).collect::<String>(),
            "ABCD".to_owned()
        );
        let mut seen = CUE_PALETTE.to_vec();
        seen.dedup();
        assert_eq!(seen.len(), 8, "eight distinct cue colours");
    }
}

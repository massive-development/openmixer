//! Plain inputs of the planner: a beat-grid view, Camelot keys, sections and the per-deck
//! [`DeckInfo`].
//!
//! `om-automix` depends on `om-core` only. The engine's `BeatGrid` and the analysis crate's
//! sections and keys live in crates that pull in the engine, so the host (the app) adapts them to
//! these small types: [`GridView`] has the same four methods as `om_engine::grid::BeatGrid`, and a
//! forwarding impl is a few lines. That keeps the dependency direction clean (`openmixer` ->
//! `om-automix` -> `om-core`) and lets every test here run on synthetic grids.
//!
//! Threads: pure data; any thread.

use std::sync::Arc;

/// The beat clock of a track. Same contract as the engine's `BeatGrid`: beats count from the grid
/// origin (beat 0 is a downbeat; negative beats are before it), frames are source frames.
pub trait GridView: Send + Sync {
    fn bpm(&self) -> f64;

    fn beats_per_bar(&self) -> i32 {
        4
    }

    fn beat_at_frame(&self, frame: i64) -> f64;

    fn frame_at_beat(&self, beat: f64) -> i64;
}

/// A constant-tempo grid: `bpm`, source sample rate and the frame of beat 0. Used by tests and as
/// the fallback for a track with a tempo but no beat map.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FixedGrid {
    bpm: f64,
    frames_per_beat: f64,
    origin: i64,
}

impl FixedGrid {
    pub fn new(bpm: f64, sample_rate: f64, first_beat_frame: i64) -> Self {
        Self {
            bpm,
            frames_per_beat: sample_rate * 60.0 / bpm,
            origin: first_beat_frame,
        }
    }
}

impl GridView for FixedGrid {
    fn bpm(&self) -> f64 {
        self.bpm
    }

    fn beat_at_frame(&self, frame: i64) -> f64 {
        (frame - self.origin) as f64 / self.frames_per_beat
    }

    fn frame_at_beat(&self, beat: f64) -> i64 {
        self.origin + (beat * self.frames_per_beat).round() as i64
    }
}

/// A Camelot wheel position: 1 to 12 plus minor (`A`) or major (`B`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Camelot {
    pub number: u8,
    pub minor: bool,
}

impl Camelot {
    /// Parses `"8A"` / `"12b"` (surrounding space ignored). `None` for anything else.
    pub fn parse(code: &str) -> Option<Self> {
        let code = code.trim();
        let letter = code.chars().last()?.to_ascii_uppercase();
        let digits = code.get(..code.len().checked_sub(1)?)?;
        let minor = match letter {
            'A' => true,
            'B' => false,
            _ => return None,
        };
        if digits.is_empty() || digits.len() > 2 || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let number: u8 = digits.parse().ok()?;
        (1..=12).contains(&number).then_some(Self { number, minor })
    }

    /// Steps around the wheel, 0 to 6.
    pub fn wheel_distance(self, other: Self) -> u8 {
        let d = (i16::from(self.number) - i16::from(other.number)).rem_euclid(12) as u8;
        d.min(12 - d)
    }

    /// The usual harmonic moves: same slot, one step either way in the same mode, or the relative
    /// key (same number, other mode). Same rule as `om_analysis::is_harmonically_compatible`
    /// (the openmixer tests check the two agree).
    pub fn compatible(self, other: Self) -> bool {
        if self.minor == other.minor {
            self.wheel_distance(other) <= 1
        } else {
            self.number == other.number
        }
    }
}

/// What a section is, from the analysis label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SectionKind {
    Intro,
    Main,
    Breakdown,
    Outro,
}

impl SectionKind {
    /// From the analysis label (`"intro"`, `"main"`, `"breakdown"`, `"outro"`); unknown is `Main`.
    pub fn from_label(label: &str) -> Self {
        match label.trim().to_ascii_lowercase().as_str() {
            "intro" => Self::Intro,
            "breakdown" => Self::Breakdown,
            "outro" => Self::Outro,
            _ => Self::Main,
        }
    }
}

/// One section of a track in beats on its own grid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Section {
    pub start_beat: f64,
    pub end_beat: f64,
    pub kind: SectionKind,
    /// 0 to 1 mean loudness.
    pub energy: f32,
}

impl Section {
    pub fn length_beats(&self) -> f64 {
        (self.end_beat - self.start_beat).max(0.0)
    }
}

/// Everything the planner needs to know about one loaded track.
#[derive(Clone)]
pub struct DeckInfo {
    /// The engine deck the track is on.
    pub deck: u8,
    /// `None` when the track has no beat grid; planning then fails with `NoGrid`.
    pub grid: Option<Arc<dyn GridView>>,
    /// Camelot code (`"8A"`); empty or unparsable means unknown.
    pub camelot: String,
    pub sections: Vec<Section>,
    /// Suggested mix-out point (start of the outro), in beats.
    pub mix_out: Option<f64>,
    /// Suggested mix-in point (where the track is cued when it is the incoming one), in beats.
    pub mix_in: Option<f64>,
    /// A complete stem set is loaded on the deck.
    pub has_stems: bool,
    /// Track length in beats on its grid.
    pub length_beats: f64,
    /// Track tempo in BPM (at rate 1.0).
    pub bpm: f64,
    /// The library id of the track on the deck; `None` when the host does not know it. Used to tell
    /// a track automix loaded from one the user loaded over it.
    pub loaded_track_id: Option<i64>,
}

impl std::fmt::Debug for DeckInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeckInfo")
            .field("deck", &self.deck)
            .field("has_grid", &self.grid.is_some())
            .field("camelot", &self.camelot)
            .field("sections", &self.sections.len())
            .field("mix_out", &self.mix_out)
            .field("mix_in", &self.mix_in)
            .field("has_stems", &self.has_stems)
            .field("length_beats", &self.length_beats)
            .field("bpm", &self.bpm)
            .finish()
    }
}

impl DeckInfo {
    /// The parsed key, when known.
    pub fn key(&self) -> Option<Camelot> {
        Camelot::parse(&self.camelot)
    }

    pub fn beats_per_bar(&self) -> f64 {
        f64::from(self.grid.as_ref().map_or(4, |g| g.beats_per_bar()).max(1))
    }

    /// The first section of `kind` that starts at or after `beat`.
    pub fn section_after(&self, kind: SectionKind, beat: f64) -> Option<&Section> {
        self.sections
            .iter()
            .filter(|s| s.kind == kind && s.start_beat >= beat)
            .min_by(|a, b| a.start_beat.total_cmp(&b.start_beat))
    }

    /// The last section of `kind`.
    pub fn last_section(&self, kind: SectionKind) -> Option<&Section> {
        self.sections
            .iter()
            .filter(|s| s.kind == kind)
            .max_by(|a, b| a.start_beat.total_cmp(&b.start_beat))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(code: &str) -> Camelot {
        Camelot::parse(code).unwrap()
    }

    #[test]
    fn parses_camelot_codes() {
        assert_eq!(
            c("8A"),
            Camelot {
                number: 8,
                minor: true
            }
        );
        assert_eq!(
            c(" 12b "),
            Camelot {
                number: 12,
                minor: false
            }
        );
        for bad in ["", "A", "0A", "13B", "8", "8C", "108A", "x8A", "-1A"] {
            assert_eq!(Camelot::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn compatibility_follows_the_wheel() {
        assert!(c("8A").compatible(c("8A")));
        assert!(c("8A").compatible(c("9A")));
        assert!(c("8A").compatible(c("7A")));
        assert!(c("1A").compatible(c("12A")), "the wheel wraps");
        assert!(c("8A").compatible(c("8B")), "relative key");
        assert!(!c("8A").compatible(c("9B")));
        assert!(!c("8A").compatible(c("10A")));
        assert!(!c("8B").compatible(c("3B")));
        assert_eq!(c("1A").wheel_distance(c("7B")), 6);
    }

    #[test]
    fn fixed_grid_round_trips_beats_and_frames() {
        let g = FixedGrid::new(120.0, 44_100.0, 1000);
        assert_eq!(g.frame_at_beat(0.0), 1000);
        assert!((g.beat_at_frame(1000 + 22_050) - 1.0).abs() < 1e-12);
        assert_eq!(g.frame_at_beat(g.beat_at_frame(123_456)), 123_456);
    }

    #[test]
    fn section_labels_map_to_kinds() {
        assert_eq!(SectionKind::from_label("Intro"), SectionKind::Intro);
        assert_eq!(SectionKind::from_label("outro"), SectionKind::Outro);
        assert_eq!(SectionKind::from_label("breakdown"), SectionKind::Breakdown);
        assert_eq!(SectionKind::from_label("whatever"), SectionKind::Main);
    }
}

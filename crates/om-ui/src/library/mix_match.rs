//! How well a library track mixes with the track that is playing: the key relation on the Camelot
//! wheel and the tempo distance. Pure functions; the table model feeds them the reference track and
//! the gpui front end tints rows by the result.

use om_analysis::MusicalKey;

/// Largest tempo gap, in percent, that still counts as an easy blend without a big pitch move.
pub const BPM_TOLERANCE_PERCENT: f64 = 6.0;

/// How a track's key relates to the reference key on the Camelot wheel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyRelation {
    /// The same code (8A and 8A).
    Same,
    /// One step either way in the same mode (8A and 9A or 7A).
    Adjacent,
    /// The same number, other mode (8A and 8B).
    Relative,
    /// Anything else.
    Clash,
}

impl KeyRelation {
    pub fn is_compatible(self) -> bool {
        self != KeyRelation::Clash
    }
}

/// The relation of two keys, or `None` when either is not a Camelot code.
pub fn key_relation(reference: &str, other: &str) -> Option<KeyRelation> {
    let a = MusicalKey::from_camelot(reference)?;
    let b = MusicalKey::from_camelot(other)?;
    let step = (a.camelot_number() - b.camelot_number()).rem_euclid(12);
    Some(if a.minor != b.minor {
        if step == 0 {
            KeyRelation::Relative
        } else {
            KeyRelation::Clash
        }
    } else {
        match step {
            0 => KeyRelation::Same,
            1 | 11 => KeyRelation::Adjacent,
            _ => KeyRelation::Clash,
        }
    })
}

/// Tempo distance in percent of the reference. A track at half or double the reference tempo
/// counts as a match (it mixes on every other beat), so the smallest of the three is returned.
/// `None` when either tempo is unknown or not positive.
pub fn bpm_distance_percent(reference: f64, other: f64) -> Option<f64> {
    if !(reference.is_finite() && other.is_finite()) || reference <= 0.0 || other <= 0.0 {
        return None;
    }
    let percent = |target: f64| (other - target).abs() / target * 100.0;
    Some(
        percent(reference)
            .min(percent(reference * 2.0))
            .min(percent(reference / 2.0)),
    )
}

/// How strongly a row stands out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum MixLevel {
    #[default]
    None,
    /// The key mixes but the tempo is far off or unknown.
    Key,
    /// The key mixes and the tempo is within [`BPM_TOLERANCE_PERCENT`].
    Strong,
}

/// The reference track's key and tempo.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MixReference {
    pub camelot: String,
    pub bpm: Option<f64>,
}

/// The result for one track.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MixMatch {
    pub key: Option<KeyRelation>,
    pub bpm_percent: Option<f64>,
    pub level: MixLevel,
}

/// Scores `camelot` and `bpm` against the reference. Without a reference key the level is `None`:
/// tempo alone does not mark a track.
pub fn score(reference: &MixReference, camelot: &str, bpm: Option<f64>) -> MixMatch {
    let key = key_relation(&reference.camelot, camelot);
    let bpm_percent = reference
        .bpm
        .zip(bpm)
        .and_then(|(r, o)| bpm_distance_percent(r, o));
    let level = match key {
        Some(relation) if relation.is_compatible() => {
            if bpm_percent.is_some_and(|p| p <= BPM_TOLERANCE_PERCENT) {
                MixLevel::Strong
            } else {
                MixLevel::Key
            }
        }
        _ => MixLevel::None,
    };
    MixMatch {
        key,
        bpm_percent,
        level,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_relations_follow_the_wheel() {
        assert_eq!(key_relation("8A", "8A"), Some(KeyRelation::Same));
        assert_eq!(key_relation("8A", "9A"), Some(KeyRelation::Adjacent));
        assert_eq!(key_relation("8A", "7A"), Some(KeyRelation::Adjacent));
        assert_eq!(key_relation("12B", "1B"), Some(KeyRelation::Adjacent));
        assert_eq!(key_relation("1A", "12A"), Some(KeyRelation::Adjacent));
        assert_eq!(key_relation("8A", "8B"), Some(KeyRelation::Relative));
        assert_eq!(key_relation("8A", "9B"), Some(KeyRelation::Clash));
        assert_eq!(key_relation("8A", "10A"), Some(KeyRelation::Clash));
        assert_eq!(key_relation("8a", " 8b "), Some(KeyRelation::Relative));
        assert_eq!(key_relation("8A", ""), None);
        assert_eq!(key_relation("", "8A"), None);
        assert_eq!(key_relation("13A", "8A"), None);
    }

    #[test]
    fn relation_agrees_with_the_analysis_rule() {
        for a in 1..=12 {
            for b in 1..=12 {
                for (ma, mb) in [("A", "A"), ("A", "B"), ("B", "A"), ("B", "B")] {
                    let (ca, cb) = (format!("{a}{ma}"), format!("{b}{mb}"));
                    let expected = om_analysis::is_harmonically_compatible(
                        &MusicalKey::from_camelot(&ca).unwrap(),
                        &MusicalKey::from_camelot(&cb).unwrap(),
                    );
                    assert_eq!(key_relation(&ca, &cb).unwrap().is_compatible(), expected);
                }
            }
        }
    }

    #[test]
    fn tempo_distance_counts_half_and_double_time() {
        assert_eq!(bpm_distance_percent(120.0, 120.0), Some(0.0));
        let near = bpm_distance_percent(120.0, 126.0).unwrap();
        assert!((near - 5.0).abs() < 1e-9);
        assert!(bpm_distance_percent(128.0, 64.0).unwrap() < 1e-9);
        assert!(bpm_distance_percent(64.0, 128.0).unwrap() < 1e-9);
        assert!(bpm_distance_percent(120.0, 90.0).unwrap() > 20.0);
        assert_eq!(bpm_distance_percent(0.0, 120.0), None);
        assert_eq!(bpm_distance_percent(120.0, f64::NAN), None);
    }

    fn reference() -> MixReference {
        MixReference {
            camelot: "8A".into(),
            bpm: Some(124.0),
        }
    }

    #[test]
    fn strong_needs_key_and_tempo() {
        assert_eq!(
            score(&reference(), "9A", Some(126.0)).level,
            MixLevel::Strong
        );
        assert_eq!(
            score(&reference(), "8B", Some(62.0)).level,
            MixLevel::Strong
        );
        assert_eq!(score(&reference(), "9A", Some(140.0)).level, MixLevel::Key);
        assert_eq!(score(&reference(), "9A", None).level, MixLevel::Key);
        assert_eq!(score(&reference(), "3B", Some(124.0)).level, MixLevel::None);
        assert_eq!(score(&reference(), "", Some(124.0)).level, MixLevel::None);
    }

    #[test]
    fn no_reference_key_marks_nothing() {
        let tempo_only = MixReference {
            camelot: String::new(),
            bpm: Some(124.0),
        };
        assert_eq!(score(&tempo_only, "8A", Some(124.0)).level, MixLevel::None);
        assert_eq!(
            score(&MixReference::default(), "8A", None).level,
            MixLevel::None
        );
    }

    #[test]
    fn unknown_reference_tempo_falls_back_to_key() {
        let key_only = MixReference {
            camelot: "8A".into(),
            bpm: None,
        };
        assert_eq!(score(&key_only, "8A", Some(100.0)).level, MixLevel::Key);
    }
}

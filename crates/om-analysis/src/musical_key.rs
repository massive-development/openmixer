//! Musical key with note names and Camelot wheel notation (1-12 plus A minor / B major). Pitch
//! classes: 0 = C ... 11 = B.

const SHARP_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];
/// C major is 8B; A minor is 8A.
const CAMELOT_OF_C: i32 = 8;

fn wrap12(v: i32) -> i32 {
    v.rem_euclid(12)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MusicalKey {
    pub tonic: i32,
    pub minor: bool,
}

impl MusicalKey {
    pub fn new(tonic: i32, minor: bool) -> Self {
        Self { tonic, minor }
    }

    /// "C", "F#m".
    pub fn name(&self) -> String {
        format!(
            "{}{}",
            SHARP_NAMES[wrap12(self.tonic) as usize],
            if self.minor { "m" } else { "" }
        )
    }

    /// Wheel number 1..=12.
    pub fn camelot_number(&self) -> i32 {
        let fifths_from_c = wrap12(self.relative_major_tonic() * 7);
        wrap12(CAMELOT_OF_C - 1 + fifths_from_c) + 1
    }

    /// "8B".
    pub fn camelot(&self) -> String {
        format!(
            "{}{}",
            self.camelot_number(),
            if self.minor { "A" } else { "B" }
        )
    }

    /// The relative key: same Camelot number, other mode.
    pub fn relative(&self) -> MusicalKey {
        if self.minor {
            MusicalKey::new(wrap12(self.tonic + 3), false)
        } else {
            MusicalKey::new(wrap12(self.tonic + 9), true)
        }
    }

    /// Parses "8A" / "8B" (case-insensitive, surrounding whitespace ignored).
    pub fn from_camelot(code: &str) -> Option<MusicalKey> {
        let trimmed = code.trim().to_ascii_uppercase();
        let letter = trimmed.chars().last()?;
        let digits = &trimmed[..trimmed.len() - letter.len_utf8()];
        if letter != 'A' && letter != 'B' {
            return None;
        }
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let number: i64 = digits.parse().ok()?;
        if !(1..=12).contains(&number) {
            return None;
        }

        let fifths_from_c = wrap12(number as i32 - CAMELOT_OF_C);
        // 7 is its own inverse mod 12.
        let major = MusicalKey::new(wrap12(fifths_from_c * 7), false);
        Some(if letter == 'B' {
            major
        } else {
            major.relative()
        })
    }

    /// Parses "C", "F#m", "Bbm".
    pub fn from_name(name: &str) -> Option<MusicalKey> {
        let mut text = name.trim();
        let is_minor = text.ends_with('m') && !text.to_ascii_lowercase().ends_with("maj");
        if is_minor {
            text = &text[..text.len() - 1];
        }
        pitch_class_of(text).map(|pc| MusicalKey::new(pc, is_minor))
    }

    fn relative_major_tonic(&self) -> i32 {
        if self.minor {
            wrap12(self.tonic + 3)
        } else {
            self.tonic
        }
    }
}

fn pitch_class_of(note: &str) -> Option<i32> {
    if let Some(pc) = SHARP_NAMES
        .iter()
        .position(|name| name.eq_ignore_ascii_case(note))
    {
        return Some(pc as i32);
    }
    let mut chars = note.chars();
    let (first, second) = (chars.next()?, chars.next());
    if chars.next().is_none() && matches!(second, Some('b' | 'B')) {
        let natural = pitch_class_of(&first.to_string())?;
        return Some(wrap12(natural - 1));
    }
    None
}

/// Same slot, plus or minus one on the wheel, or the relative key: the usual harmonic-mixing moves.
pub fn is_harmonically_compatible(a: &MusicalKey, b: &MusicalKey) -> bool {
    let distance = wrap12(a.camelot_number() - b.camelot_number());
    if a.minor == b.minor {
        distance == 0 || distance == 1 || distance == 11
    } else {
        distance == 0
    }
}

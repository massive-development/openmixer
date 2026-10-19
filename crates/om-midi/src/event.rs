/// What a normalised event carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MidiKind {
    Cc,
    Note,
    PitchBend,
}

/// Normalised MIDI event (CC, note, pitch bend) extracted from a raw message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MidiEvent {
    pub kind: MidiKind,
    /// 1-16.
    pub channel: i32,
    /// Controller or note number; 0 for pitch bend.
    pub number: i32,
    /// 0-127, or 0-16383 for pitch bend; note-off is 0.
    pub value: i32,
    pub note_on: bool,
}

impl MidiEvent {
    /// Normalises one complete channel message. `None` for message types the mapper does not handle
    /// (and for truncated messages).
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let status = *bytes.first()?;
        if !(0x80..=0xEF).contains(&status) {
            return None;
        }
        let &[_, d1, d2, ..] = bytes else {
            return None;
        };
        let channel = i32::from(status & 0x0F) + 1;
        let (d1, d2) = (i32::from(d1), i32::from(d2));
        match status & 0xF0 {
            0xB0 => Some(Self {
                kind: MidiKind::Cc,
                channel,
                number: d1,
                value: d2,
                note_on: false,
            }),
            0x90 if d2 != 0 => Some(Self {
                kind: MidiKind::Note,
                channel,
                number: d1,
                value: d2,
                note_on: true,
            }),
            0x80 | 0x90 => Some(Self {
                kind: MidiKind::Note,
                channel,
                number: d1,
                value: 0,
                note_on: false,
            }),
            0xE0 => Some(Self {
                kind: MidiKind::PitchBend,
                channel,
                number: 0,
                value: d1 | (d2 << 7),
                note_on: false,
            }),
            _ => None,
        }
    }
}

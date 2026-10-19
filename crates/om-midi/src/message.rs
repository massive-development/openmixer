//! Raw MIDI message builders and framing rules. Messages are plain byte vectors: status first.

/// Control change on `channel` (1-16).
pub fn control_change(channel: u8, controller: u8, value: u8) -> Vec<u8> {
    vec![0xB0 | channel_nibble(channel), controller, value]
}

/// Note on; velocity 0 is sent as a note on, as the sender chose it.
pub fn note_on(channel: u8, note: u8, velocity: u8) -> Vec<u8> {
    vec![0x90 | channel_nibble(channel), note, velocity]
}

/// Note off with velocity 0 (status 0x80).
pub fn note_off(channel: u8, note: u8) -> Vec<u8> {
    vec![0x80 | channel_nibble(channel), note, 0]
}

/// Pitch bend; `value` is 14 bits, 8192 is centre.
pub fn pitch_bend(channel: u8, value: u16) -> Vec<u8> {
    let v = value & 0x3FFF;
    vec![
        0xE0 | channel_nibble(channel),
        (v & 0x7F) as u8,
        (v >> 7) as u8,
    ]
}

/// Program change (two bytes; the mapper ignores it).
pub fn program_change(channel: u8, program: u8) -> Vec<u8> {
    vec![0xC0 | channel_nibble(channel), program]
}

/// Total length of a message from its status byte, or `None` for sysex, whose length is variable.
pub fn message_length(status: u8) -> Option<usize> {
    match status & 0xF0 {
        0x80 | 0x90 | 0xA0 | 0xB0 | 0xE0 => Some(3),
        0xC0 | 0xD0 => Some(2),
        _ => match status {
            0xF0 => None,
            0xF1 | 0xF3 => Some(2),
            0xF2 => Some(3),
            _ => Some(1),
        },
    }
}

fn channel_nibble(channel: u8) -> u8 {
    channel.clamp(1, 16) - 1
}

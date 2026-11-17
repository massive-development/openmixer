//! `EngineSnapshot` to a little-endian binary packet for the 30 Hz front-end stream.
//! Runs on the stream thread, never the audio thread.

use om_core::{ChannelSnapshot, DeckSnapshot, EngineSnapshot, MixerSnapshot};

/// `OMS1`: lets the front end reject a packet from another protocol.
pub const PACKET_MAGIC: u32 = 0x4F4D_5331;
/// Bumped whenever the layout changes.
pub const PACKET_VERSION: u16 = 2;

const HEADER_LEN: usize = 24;
const DECK_LEN: usize = 56;
const CHANNEL_LEN: usize = 32;
const MASTER_LEN: usize = 16;
const DECK_PAD: [u8; 7] = [0; 7];
/// Largest deck or channel count the one-byte header fields hold.
const MAX_COUNT: usize = u8::MAX as usize;

/// Bytes in a packet for `decks` decks and `channels` mixer channels.
pub fn packet_len(decks: usize, channels: usize) -> usize {
    HEADER_LEN + decks * DECK_LEN + channels * CHANNEL_LEN + MASTER_LEN
}

/// Clears `out` and writes `snap` into it; `age_ms` is the time from snapshot pull to send.
pub fn pack_snapshot(snap: &EngineSnapshot, age_ms: f32, out: &mut Vec<u8>) {
    out.clear();
    // The body holds no more than the saturated header counts say.
    let decks = &snap.decks[..snap.decks.len().min(MAX_COUNT)];
    let channels = &snap.mixer.channels[..snap.mixer.channels.len().min(MAX_COUNT)];
    out.reserve(packet_len(decks.len(), channels.len()));
    put_header(snap, age_ms, out);
    decks.iter().for_each(|d| put_deck(d, out));
    channels.iter().for_each(|c| put_channel(c, out));
    put_master(&snap.mixer, out);
}

fn put_header(snap: &EngineSnapshot, age_ms: f32, out: &mut Vec<u8>) {
    out.extend_from_slice(&PACKET_MAGIC.to_le_bytes());
    out.extend_from_slice(&PACKET_VERSION.to_le_bytes());
    out.push(count_u8(snap.decks.len()));
    out.push(count_u8(snap.mixer.channels.len()));
    out.extend_from_slice(&snap.ticks.to_le_bytes());
    out.extend_from_slice(&age_ms.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
}

// The header holds counts in one byte; no engine has more than 255 decks or channels.
fn count_u8(n: usize) -> u8 {
    u8::try_from(n).unwrap_or(u8::MAX)
}

fn put_deck(deck: &DeckSnapshot, out: &mut Vec<u8>) {
    out.extend_from_slice(&deck.position_frames.to_le_bytes());
    out.extend_from_slice(&deck.length_frames.to_le_bytes());
    out.extend_from_slice(&deck.rate.to_le_bytes());
    out.extend_from_slice(&deck.file_sample_rate.to_le_bytes());
    out.push(deck_flags(deck));
    out.extend_from_slice(&DECK_PAD);
    // The front end wraps its interpolated position inside the loop between packets.
    out.extend_from_slice(&deck.loop_start.to_le_bytes());
    out.extend_from_slice(&deck.loop_end.to_le_bytes());
}

fn deck_flags(deck: &DeckSnapshot) -> u8 {
    [
        deck.loaded,
        deck.playing,
        deck.looping,
        deck.sync,
        deck.keylock,
    ]
    .iter()
    .enumerate()
    .fold(0, |acc, (bit, &on)| acc | (u8::from(on) << bit))
}

fn put_channel(ch: &ChannelSnapshot, out: &mut Vec<u8>) {
    let values = [ch.peak_left, ch.peak_right, ch.fader, ch.trim, ch.filter];
    put_f32s(values.iter().chain(ch.eq.iter()), out);
}

fn put_master(mixer: &MixerSnapshot, out: &mut Vec<u8>) {
    let values = [
        mixer.master_peak_left,
        mixer.master_peak_right,
        mixer.master_gain,
        mixer.crossfader,
    ];
    put_f32s(values.iter(), out);
}

fn put_f32s<'a>(values: impl Iterator<Item = &'a f32>, out: &mut Vec<u8>) {
    values.for_each(|v| out.extend_from_slice(&v.to_le_bytes()));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> EngineSnapshot {
        let deck0 = DeckSnapshot {
            loaded: true,
            playing: true,
            position_frames: 441_000,
            length_frames: 11_025_000,
            rate: 1.02,
            file_sample_rate: 44_100.0,
            looping: true,
            loop_start: 440_000,
            loop_end: 450_000,
            ..DeckSnapshot::default()
        };
        let ch0 = ChannelSnapshot {
            peak_left: 0.5,
            peak_right: 0.25,
            fader: 0.85,
            ..ChannelSnapshot::default()
        };
        EngineSnapshot {
            ticks: 7,
            built: 1,
            decks: vec![deck0, DeckSnapshot::default()],
            mixer: MixerSnapshot {
                channels: vec![ch0, ChannelSnapshot::default()],
                crossfader: 0.5,
                master_peak_left: 0.75,
                master_peak_right: 0.6,
                ..MixerSnapshot::default()
            },
            stems: Vec::new(),
            fx: Default::default(),
            sampler: Default::default(),
            automix: Default::default(),
            record: Default::default(),
        }
    }

    #[test]
    fn packet_len_two_decks_two_channels() {
        assert_eq!(packet_len(2, 2), 216);
    }

    #[test]
    fn header_fields() {
        let mut out = Vec::new();
        pack_snapshot(&fixture(), 2.5, &mut out);
        assert_eq!(out.len(), 216);
        assert_eq!(
            u32::from_le_bytes(out[0..4].try_into().unwrap()),
            PACKET_MAGIC
        );
        assert_eq!(u16::from_le_bytes(out[4..6].try_into().unwrap()), 2);
        assert_eq!((out[6], out[7]), (2, 2));
        assert_eq!(f32::from_le_bytes(out[16..20].try_into().unwrap()), 2.5);
    }

    #[test]
    fn deck_block() {
        let mut out = Vec::new();
        pack_snapshot(&fixture(), 0.0, &mut out);
        assert_eq!(i64::from_le_bytes(out[24..32].try_into().unwrap()), 441_000);
        assert_eq!(
            i64::from_le_bytes(out[32..40].try_into().unwrap()),
            11_025_000
        );
        assert_eq!(f64::from_le_bytes(out[40..48].try_into().unwrap()), 1.02);
        assert_eq!(out[56], 0b0000_0111); // loaded, playing and looping
        assert_eq!(i64::from_le_bytes(out[64..72].try_into().unwrap()), 440_000);
        assert_eq!(i64::from_le_bytes(out[72..80].try_into().unwrap()), 450_000);
        assert_eq!(i64::from_le_bytes(out[120..128].try_into().unwrap()), 0); // deck 1
    }

    #[test]
    fn channel_and_master_blocks() {
        let mut out = Vec::new();
        pack_snapshot(&fixture(), 0.0, &mut out);
        let f = |o: usize| f32::from_le_bytes(out[o..o + 4].try_into().unwrap());
        assert_eq!((f(136), f(140), f(144)), (0.5, 0.25, 0.85)); // channel 0: peaks, fader
        assert_eq!((f(200), f(204), f(212)), (0.75, 0.6, 0.5)); // master peaks, crossfader
    }

    #[test]
    fn reuses_the_buffer() {
        let mut out = vec![9u8; 500];
        pack_snapshot(&fixture(), 0.0, &mut out);
        assert_eq!(out.len(), 216);
    }

    #[test]
    fn counts_and_body_agree_when_saturated() {
        let mut snap = fixture();
        snap.mixer.channels = vec![ChannelSnapshot::default(); 256];
        let mut out = Vec::new();
        pack_snapshot(&snap, 0.0, &mut out);
        assert_eq!(out.len(), packet_len(snap.decks.len(), 255));
        assert_eq!(out[7], 255);
    }

    #[test]
    fn matches_golden_file() {
        let mut out = Vec::new();
        pack_snapshot(&fixture(), 2.5, &mut out);
        assert_eq!(
            out,
            include_bytes!("../../../fixtures/ui-bridge/snapshot-v2.bin")
        );
    }

    #[test]
    #[ignore = "rewrites the golden packet shared with the TypeScript tests"]
    fn write_golden() {
        let mut out = Vec::new();
        pack_snapshot(&fixture(), 2.5, &mut out);
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/ui-bridge/snapshot-v2.bin"
        );
        std::fs::write(path, out).unwrap();
    }
}

//! WAV fixtures. Written by hand (RIFF, `fmt `, `data`) so the writers stay independent of the
//! decoders under test. Every writer panics on I/O failure: fixtures are test preconditions.

use std::path::Path;

/// Format tag for IEEE float samples.
const FORMAT_FLOAT: u16 = 3;
/// Format tag for integer PCM samples.
const FORMAT_PCM: u16 = 1;

fn header(format: u16, channels: u16, sample_rate: u32, bits: u16, data_len: u32) -> Vec<u8> {
    let block_align = channels * (bits / 8);
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&format.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&(sample_rate * u32::from(block_align)).to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&bits.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    out
}

/// Writes interleaved 32-bit float samples as a WAV file.
pub fn write_float_wav(path: &Path, channels: u16, sample_rate: u32, samples: &[f32]) {
    let data_len = (samples.len() * 4) as u32;
    let mut bytes = header(FORMAT_FLOAT, channels, sample_rate, 32, data_len);
    for s in samples {
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(path, bytes).expect("write float wav");
}

/// Writes interleaved 16-bit PCM samples as a WAV file.
pub fn write_pcm16_wav(path: &Path, channels: u16, sample_rate: u32, samples: &[i16]) {
    let data_len = (samples.len() * 2) as u32;
    let mut bytes = header(FORMAT_PCM, channels, sample_rate, 16, data_len);
    for s in samples {
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(path, bytes).expect("write pcm16 wav");
}

/// A decoded WAV file: format, and the samples as interleaved `f32` (16-bit PCM is scaled to
/// `[-1, 1)`).
#[derive(Debug, Clone, PartialEq)]
pub struct WavData {
    pub channels: u16,
    pub sample_rate: u32,
    pub samples: Vec<f32>,
}

/// Reads a WAV file written by this module (float32 or PCM16, canonical 44-byte header).
///
/// # Panics
/// Panics on any other layout; this reader only has to understand the fixtures in this crate.
pub fn read_wav(path: &Path) -> WavData {
    let bytes = std::fs::read(path).expect("read wav");
    assert_eq!(&bytes[0..4], b"RIFF", "not a RIFF file");
    assert_eq!(&bytes[8..16], b"WAVEfmt ", "unexpected header layout");
    let format = u16::from_le_bytes([bytes[20], bytes[21]]);
    let channels = u16::from_le_bytes([bytes[22], bytes[23]]);
    let sample_rate = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]);
    let bits = u16::from_le_bytes([bytes[34], bytes[35]]);
    assert_eq!(&bytes[36..40], b"data", "unexpected header layout");
    let data_len = u32::from_le_bytes([bytes[40], bytes[41], bytes[42], bytes[43]]) as usize;
    let data = &bytes[44..44 + data_len];
    let samples = match (format, bits) {
        (FORMAT_FLOAT, 32) => data
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect(),
        (FORMAT_PCM, 16) => data
            .chunks_exact(2)
            .map(|c| f32::from(i16::from_le_bytes([c[0], c[1]])) / 32_768.0)
            .collect(),
        other => panic!("unsupported WAV format {other:?}"),
    };
    WavData {
        channels,
        sample_rate,
        samples,
    }
}

/// Interleaves equal-length channels: `[L0, L1, ...]`, `[R0, R1, ...]` becomes `[L0, R0, L1, R1,
/// ...]`.
///
/// # Panics
/// Panics if the channels differ in length.
pub fn interleave(channels: &[&[f32]]) -> Vec<f32> {
    let frames = channels.first().map_or(0, |c| c.len());
    assert!(
        channels.iter().all(|c| c.len() == frames),
        "channels must have equal length"
    );
    let mut out = Vec::with_capacity(frames * channels.len());
    for i in 0..frames {
        for ch in channels {
            out.push(ch[i]);
        }
    }
    out
}

/// Splits interleaved samples into `channels` equal-length vectors.
///
/// # Panics
/// Panics if the sample count is not a multiple of `channels`.
pub fn deinterleave(samples: &[f32], channels: usize) -> Vec<Vec<f32>> {
    assert!(
        channels > 0 && samples.len().is_multiple_of(channels),
        "ragged input"
    );
    let mut out = vec![Vec::with_capacity(samples.len() / channels); channels];
    for frame in samples.chunks_exact(channels) {
        for (ch, &s) in out.iter_mut().zip(frame) {
            ch.push(s);
        }
    }
    out
}

//! A minimal streaming WAV codec for the stem cache: 16-bit PCM, two channels, one rate. Written
//! and read only by this crate, so it handles exactly one header layout (a canonical 44-byte
//! header). Samples are quantised with a round-to-nearest of `x * 32767` after clamping to
//! `[-1, 1]`, so a read-back differs from the input by at most half a step (about 1.6e-5).

use std::fs::File;
use std::io::{self, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;

pub const HEADER_LEN: u64 = 44;
const BYTES_PER_FRAME: u64 = 4;
const FULL_SCALE: f32 = 32767.0;

/// The header of a stereo 16-bit PCM file with `frames` frames.
fn header(rate: u32, frames: u64) -> io::Result<[u8; HEADER_LEN as usize]> {
    let data_bytes = frames
        .checked_mul(BYTES_PER_FRAME)
        .filter(|&n| n <= u64::from(u32::MAX - 36))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "stem is too long for a WAV file",
            )
        })? as u32;
    let mut h = [0u8; HEADER_LEN as usize];
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&(36 + data_bytes).to_le_bytes());
    h[8..12].copy_from_slice(b"WAVE");
    h[12..16].copy_from_slice(b"fmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes());
    h[20..22].copy_from_slice(&1u16.to_le_bytes()); // PCM
    h[22..24].copy_from_slice(&2u16.to_le_bytes()); // channels
    h[24..28].copy_from_slice(&rate.to_le_bytes());
    h[28..32].copy_from_slice(&(rate * BYTES_PER_FRAME as u32).to_le_bytes());
    h[32..34].copy_from_slice(&(BYTES_PER_FRAME as u16).to_le_bytes());
    h[34..36].copy_from_slice(&16u16.to_le_bytes()); // bits per sample
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&data_bytes.to_le_bytes());
    Ok(h)
}

/// Float to 16-bit: clamp, scale, round to nearest. NaN becomes silence.
pub fn quantise(x: f32) -> i16 {
    (x.clamp(-1.0, 1.0) * FULL_SCALE).round() as i16
}

/// 16-bit to float, the inverse of [`quantise`] up to the rounding step.
pub fn dequantise(s: i16) -> f32 {
    f32::from(s) / FULL_SCALE
}

/// Streams stereo frames into one file. The header's sizes are written as zero first and patched
/// by [`WavWriter::finish`].
pub struct WavWriter {
    out: BufWriter<File>,
    frames: u64,
    rate: u32,
    scratch: Vec<u8>,
}

impl WavWriter {
    pub fn create(path: &Path, rate: u32) -> io::Result<Self> {
        let mut file = File::create(path)?;
        file.write_all(&header(rate, 0)?)?;
        Ok(Self {
            out: BufWriter::new(file),
            frames: 0,
            rate,
            scratch: Vec::new(),
        })
    }

    pub fn write(&mut self, left: &[f32], right: &[f32]) -> io::Result<()> {
        if left.len() != right.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "channel length mismatch",
            ));
        }
        // Check the size before writing anything: a too-long stem must not leave a silently
        // truncated header behind.
        header(self.rate, self.frames + left.len() as u64)?;
        self.scratch.clear();
        for (l, r) in left.iter().zip(right) {
            self.scratch.extend_from_slice(&quantise(*l).to_le_bytes());
            self.scratch.extend_from_slice(&quantise(*r).to_le_bytes());
        }
        self.out.write_all(&self.scratch)?;
        self.frames += left.len() as u64;
        Ok(())
    }

    /// Patches the header with the final size, flushes and closes the file. Returns the frames.
    pub fn finish(self) -> io::Result<u64> {
        let Self {
            out, frames, rate, ..
        } = self;
        let mut file = out.into_inner().map_err(|e| e.into_error())?;
        file.seek(SeekFrom::Start(0))?;
        file.write_all(&header(rate, frames)?)?;
        file.sync_data()?;
        Ok(frames)
    }
}

/// Random access to the frames of one file.
pub struct WavReader {
    file: File,
    rate: u32,
    frames: u64,
}

impl WavReader {
    pub fn open(path: &Path) -> io::Result<Self> {
        let mut file = File::open(path)?;
        let mut h = [0u8; HEADER_LEN as usize];
        file.read_exact(&mut h)?;
        let valid = &h[0..4] == b"RIFF"
            && &h[8..12] == b"WAVE"
            && &h[36..40] == b"data"
            && u16::from_le_bytes([h[20], h[21]]) == 1
            && u16::from_le_bytes([h[22], h[23]]) == 2
            && u16::from_le_bytes([h[34], h[35]]) == 16;
        if !valid {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "not a 16-bit stereo PCM WAV written by OpenMixer",
            ));
        }
        let rate = u32::from_le_bytes([h[24], h[25], h[26], h[27]]);
        let data = u32::from_le_bytes([h[40], h[41], h[42], h[43]]);
        Ok(Self {
            file,
            rate,
            frames: u64::from(data) / BYTES_PER_FRAME,
        })
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// Reads up to `left.len()` frames from `start`; returns the frames read (0 at the end).
    pub fn read(&mut self, start: u64, left: &mut [f32], right: &mut [f32]) -> io::Result<usize> {
        let n = left
            .len()
            .min(right.len())
            .min(self.frames.saturating_sub(start) as usize);
        if n == 0 {
            return Ok(0);
        }
        self.file
            .seek(SeekFrom::Start(HEADER_LEN + start * BYTES_PER_FRAME))?;
        let mut raw = vec![0u8; n * BYTES_PER_FRAME as usize];
        self.file.read_exact(&mut raw)?;
        for (i, frame) in raw.chunks_exact(4).enumerate() {
            left[i] = dequantise(i16::from_le_bytes([frame[0], frame[1]]));
            right[i] = dequantise(i16::from_le_bytes([frame[2], frame[3]]));
        }
        Ok(n)
    }
}

//! Streaming RIFF/WAVE writer for the recorder: stereo, 16/24-bit PCM or 32-bit float, canonical
//! 44-byte header. The sizes in the header are rewritten on [`WavOut::patch`], so a file that was
//! never finished (a crash) is playable up to the last patch.

use super::RecordFormat;
use std::fs::{File, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub(super) const HEADER_LEN: u64 = 44;

impl RecordFormat {
    pub(super) fn bytes_per_sample(self) -> u64 {
        match self {
            Self::Pcm16 => 2,
            Self::Pcm24 => 3,
            Self::Float32 => 4,
        }
    }

    /// Bytes of one stereo frame.
    pub fn frame_bytes(self) -> u64 {
        self.bytes_per_sample() * 2
    }

    fn encode(self, x: f32, out: &mut Vec<u8>) {
        match self {
            Self::Pcm16 => {
                let v = (x.clamp(-1.0, 1.0) * 32767.0).round() as i16;
                out.extend_from_slice(&v.to_le_bytes());
            }
            Self::Pcm24 => {
                let v = (x.clamp(-1.0, 1.0) * 8_388_607.0).round() as i32;
                out.extend_from_slice(&v.to_le_bytes()[..3]);
            }
            Self::Float32 => {
                let x = if x.is_finite() { x } else { 0.0 };
                out.extend_from_slice(&x.to_le_bytes());
            }
        }
    }
}

fn header(format: RecordFormat, rate: u32, data_bytes: u32) -> [u8; HEADER_LEN as usize] {
    let (tag, bits) = match format {
        RecordFormat::Pcm16 => (1u16, 16u16),
        RecordFormat::Pcm24 => (1, 24),
        RecordFormat::Float32 => (3, 32),
    };
    let align = format.frame_bytes() as u16;
    let mut h = [0u8; HEADER_LEN as usize];
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&data_bytes.saturating_add(36).to_le_bytes());
    h[8..12].copy_from_slice(b"WAVE");
    h[12..16].copy_from_slice(b"fmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes());
    h[20..22].copy_from_slice(&tag.to_le_bytes());
    h[22..24].copy_from_slice(&2u16.to_le_bytes());
    h[24..28].copy_from_slice(&rate.to_le_bytes());
    h[28..32].copy_from_slice(&(rate * u32::from(align)).to_le_bytes());
    h[32..34].copy_from_slice(&align.to_le_bytes());
    h[34..36].copy_from_slice(&bits.to_le_bytes());
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&data_bytes.to_le_bytes());
    h
}

pub(super) struct WavOut {
    file: File,
    pub(super) path: PathBuf,
    format: RecordFormat,
    rate: u32,
    buf: Vec<u8>,
    pub(super) data_bytes: u64,
}

impl WavOut {
    /// Creates `path`, failing if it exists (a recording never overwrites).
    pub(super) fn create(path: &Path, format: RecordFormat, rate: u32) -> io::Result<Self> {
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
        file.write_all(&header(format, rate, 0))?;
        Ok(Self {
            file,
            path: path.to_path_buf(),
            format,
            rate,
            buf: Vec::with_capacity(1 << 16),
            data_bytes: 0,
        })
    }

    /// Appends interleaved L/R samples (an even count).
    pub(super) fn write(&mut self, samples: &[f32]) -> io::Result<()> {
        for &s in samples {
            self.format.encode(s, &mut self.buf);
        }
        self.data_bytes += samples.len() as u64 * self.format.bytes_per_sample();
        if self.buf.len() >= 1 << 16 {
            self.flush()?;
        }
        Ok(())
    }

    pub(super) fn flush(&mut self) -> io::Result<()> {
        if !self.buf.is_empty() {
            self.file.write_all(&self.buf)?;
            self.buf.clear();
        }
        Ok(())
    }

    /// Flushes and rewrites the sizes so the file on disk is valid right now.
    pub(super) fn patch(&mut self) -> io::Result<()> {
        self.flush()?;
        let data = u32::try_from(self.data_bytes).unwrap_or(u32::MAX);
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&header(self.format, self.rate, data))?;
        self.file.seek(SeekFrom::End(0))?;
        Ok(())
    }

    pub(super) fn finish(mut self) -> io::Result<()> {
        self.patch()?;
        self.file.sync_all()
    }
}

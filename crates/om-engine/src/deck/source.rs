//! File access for decks: probe and open a track with symphonia, and read stereo frames at any
//! source position. Runs on the decoder thread (and on the control thread at load time), never on
//! the audio thread.

use std::fs::File;
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{Decoder, DecoderOptions};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::{Time, TimeBase};

/// Why a file could not become a deck source.
#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("cannot open {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("unsupported or unreadable audio: {0}")]
    Unsupported(String),
}

/// Describes a probed file. `length_frames` is exact: when the container does not state it, the
/// file is decoded once up front to count frames.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrackInfo {
    pub sample_rate: f64,
    pub length_frames: i64,
    pub channels: usize,
}

/// Stereo source frames read from a file, decoded on demand.
pub struct SourceReader {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_id: u32,
    time_base: Option<TimeBase>,
    info: TrackInfo,
    /// Source frame index of the next frame the decoder will produce.
    pos: i64,
    /// Interleaved samples of the current packet and the frame cursor into them.
    samples: Option<SampleBuffer<f32>>,
    buffered_frames: usize,
    cursor: usize,
    /// Set when the stream ended or failed; reads past this point return silence.
    exhausted: bool,
    /// Sticky: a packet failed to read or decode, or the channel layout changed. Reads past such a
    /// point are silence, so a caller that needs every frame must check this.
    damaged: bool,
}

impl SourceReader {
    /// Probes `path` and prepares a reader positioned at frame 0.
    pub fn open(path: &Path) -> Result<Self, SourceError> {
        let file = File::open(path).map_err(|source| SourceError::Io {
            path: path.display().to_string(),
            source,
        })?;
        let stream = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }
        let probed = symphonia::default::get_probe()
            .format(
                &hint,
                stream,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .map_err(|e| SourceError::Unsupported(e.to_string()))?;
        let format = probed.format;
        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL)
            .ok_or_else(|| SourceError::Unsupported("no decodable audio track".into()))?;
        let track_id = track.id;
        let params = track.codec_params.clone();
        let sample_rate = params
            .sample_rate
            .ok_or_else(|| SourceError::Unsupported("unknown sample rate".into()))?;
        let channels = params.channels.map_or(0, |c| c.count());
        if channels == 0 {
            return Err(SourceError::Unsupported("no channels".into()));
        }
        let decoder = symphonia::default::get_codecs()
            .make(&params, &DecoderOptions::default())
            .map_err(|e| SourceError::Unsupported(e.to_string()))?;
        let time_base = params.time_base;
        let mut reader = Self {
            format,
            decoder,
            track_id,
            time_base,
            info: TrackInfo {
                sample_rate: f64::from(sample_rate),
                length_frames: 0,
                channels,
            },
            pos: 0,
            samples: None,
            buffered_frames: 0,
            cursor: 0,
            exhausted: false,
            damaged: false,
        };
        reader.info.length_frames = match params.n_frames {
            Some(n) => i64::try_from(n).unwrap_or(i64::MAX),
            None => reader.count_frames(),
        };
        reader.rewind();
        Ok(reader)
    }

    pub fn info(&self) -> TrackInfo {
        self.info
    }

    /// True once a packet failed to read or decode (as opposed to the stream simply ending). A
    /// plain end of file does not set it.
    pub fn damaged(&self) -> bool {
        self.damaged
    }

    /// Fills `dst` with stereo frames starting at source frame `from`. Mono files are duplicated
    /// to both sides; files with more than two channels contribute their first two. Frames past
    /// the end (or that fail to decode) are silence. Returns how many frames came from the decoder;
    /// the rest of `dst` is silence and must not be mistaken for audio.
    pub fn read_stereo(&mut self, from: i64, dst: &mut [[f32; 2]]) -> usize {
        let from = from.max(0);
        if from != self.pos {
            self.seek_to(from);
        }
        let mut written = 0;
        while written < dst.len() {
            if self.cursor >= self.buffered_frames && !self.next_packet() {
                break;
            }
            let take = (dst.len() - written).min(self.buffered_frames - self.cursor);
            let ch = self.info.channels;
            let Some(buffer) = self.samples.as_ref() else {
                break;
            };
            let src = &buffer.samples()[self.cursor * ch..(self.cursor + take) * ch];
            for (out, frame) in dst[written..written + take]
                .iter_mut()
                .zip(src.chunks_exact(ch))
            {
                *out = [frame[0], if ch > 1 { frame[1] } else { frame[0] }];
            }
            self.cursor += take;
            written += take;
            self.pos += take as i64;
        }
        for out in &mut dst[written..] {
            *out = [0.0, 0.0];
        }
        written
    }

    /// Repositions the decoder so the next frame produced is `frame`.
    fn seek_to(&mut self, frame: i64) {
        let sr = self.info.sample_rate;
        let to = match self.time_base {
            Some(tb) => {
                let ts = tb.calc_timestamp(Time::from(frame as f64 / sr));
                SeekTo::TimeStamp {
                    ts,
                    track_id: self.track_id,
                }
            }
            None => SeekTo::TimeStamp {
                ts: frame as u64,
                track_id: self.track_id,
            },
        };
        self.decoder.reset();
        self.buffered_frames = 0;
        self.cursor = 0;
        self.exhausted = false;
        match self.format.seek(SeekMode::Accurate, to) {
            Ok(seeked) => {
                self.pos = match self.time_base {
                    Some(tb) => {
                        let t = tb.calc_time(seeked.actual_ts);
                        ((t.seconds as f64 + t.frac) * sr).round() as i64
                    }
                    None => seeked.actual_ts as i64,
                };
            }
            Err(_) => {
                self.exhausted = true;
                self.pos = frame;
                return;
            }
        }
        // Accurate seeks land on a packet boundary at or before the target: discard the rest.
        while self.pos < frame {
            if self.cursor >= self.buffered_frames && !self.next_packet() {
                break;
            }
            let skip = ((frame - self.pos) as usize).min(self.buffered_frames - self.cursor);
            self.cursor += skip;
            self.pos += skip as i64;
        }
    }

    /// Decodes packets until one yields frames. Returns false at end of stream.
    fn next_packet(&mut self) -> bool {
        if self.exhausted {
            return false;
        }
        loop {
            let packet = match self.format.next_packet() {
                Ok(p) => p,
                Err(SymError::IoError(_)) => {
                    self.exhausted = true;
                    return false;
                }
                Err(e) => {
                    tracing::warn!(error = %e, "deck source: packet read failed");
                    self.damaged = true;
                    self.exhausted = true;
                    return false;
                }
            };
            if packet.track_id() != self.track_id {
                continue;
            }
            match self.decoder.decode(&packet) {
                Ok(decoded) => {
                    let channels = decoded.spec().channels.count();
                    if channels != self.info.channels {
                        // Channel layout changed mid-stream: treat as end of usable audio.
                        self.damaged = true;
                        self.exhausted = true;
                        return false;
                    }
                    let needed = decoded.frames() * channels;
                    if self.samples.as_ref().is_none_or(|b| b.capacity() < needed) {
                        self.samples = Some(SampleBuffer::new(
                            decoded.capacity() as u64,
                            *decoded.spec(),
                        ));
                    }
                    let Some(buffer) = self.samples.as_mut() else {
                        return false;
                    };
                    buffer.copy_interleaved_ref(decoded);
                    self.buffered_frames = buffer.len() / channels;
                    self.cursor = 0;
                    if self.buffered_frames > 0 {
                        return true;
                    }
                }
                Err(SymError::DecodeError(msg)) => {
                    tracing::debug!(error = msg, "deck source: skipped undecodable packet");
                    self.damaged = true;
                }
                Err(_) => {
                    self.damaged = true;
                    self.exhausted = true;
                    return false;
                }
            }
        }
    }

    /// Resets the reader to frame 0 and clears any buffered packet.
    fn rewind(&mut self) {
        self.seek_to(0);
    }

    /// Counts frames by decoding the whole stream. Used only when the container has no frame count.
    fn count_frames(&mut self) -> i64 {
        self.rewind();
        let mut total = 0_i64;
        while self.next_packet() {
            total += self.buffered_frames as i64;
            self.cursor = self.buffered_frames;
        }
        total
    }
}

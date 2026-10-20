//! Stem mode sources: four stereo files (drums, bass, vocals, other) read in lockstep, one
//! eight-float frame per source frame. Runs on the decoder thread (and on the control thread when
//! `load_stems` validates the files), never on the audio thread.

use super::source::{SourceError, SourceReader, TrackInfo};
use om_dsp::{STEM_COUNT, STEM_FRAME_FLOATS};
use std::path::Path;

/// File names inside a stem directory, in ring order: drums, bass, vocals, other.
pub const STEM_FILES: [&str; STEM_COUNT] = ["drums.wav", "bass.wav", "vocals.wav", "other.wav"];

/// How many frames a stem may differ from the track before `load_stems` refuses it.
pub const LENGTH_TOLERANCE_FRAMES: i64 = 16;

/// Why a stem set could not be attached to a deck.
#[derive(Debug, thiserror::Error)]
pub enum DeckError {
    #[error("no track is loaded")]
    NotLoaded,
    #[error("stem file missing: {0}")]
    Missing(String),
    #[error("stem {name}: {source}")]
    Source {
        name: &'static str,
        #[source]
        source: SourceError,
    },
    #[error("stem {name} is {found} Hz but the track is {expected} Hz")]
    RateMismatch {
        name: &'static str,
        found: f64,
        expected: f64,
    },
    #[error("stem {name} has {found} frames but the track has {expected}")]
    LengthMismatch {
        name: &'static str,
        found: i64,
        expected: i64,
    },
}

/// The four stem readers and a scratch buffer for lockstep reads.
pub(super) struct StemReaders {
    readers: [SourceReader; STEM_COUNT],
    scratch: Vec<[f32; 2]>,
}

impl StemReaders {
    /// Opens `dir/{drums,bass,vocals,other}.wav` and checks each against the loaded track.
    pub(super) fn open(dir: &Path, track: TrackInfo, scratch_frames: usize) -> Result<Self, DeckError> {
        let mut opened = Vec::with_capacity(STEM_COUNT);
        for name in STEM_FILES {
            let path = dir.join(name);
            if !path.is_file() {
                return Err(DeckError::Missing(path.display().to_string()));
            }
            let reader = SourceReader::open(&path).map_err(|source| DeckError::Source { name, source })?;
            let info = reader.info();
            if (info.sample_rate - track.sample_rate).abs() > 0.5 {
                return Err(DeckError::RateMismatch {
                    name,
                    found: info.sample_rate,
                    expected: track.sample_rate,
                });
            }
            if (info.length_frames - track.length_frames).abs() > LENGTH_TOLERANCE_FRAMES {
                return Err(DeckError::LengthMismatch {
                    name,
                    found: info.length_frames,
                    expected: track.length_frames,
                });
            }
            opened.push(reader);
        }
        let Ok(readers) = <[SourceReader; STEM_COUNT]>::try_from(opened) else {
            unreachable!("exactly {STEM_COUNT} stem files were opened");
        };
        Ok(Self {
            readers,
            scratch: vec![[0.0; 2]; scratch_frames.max(1)],
        })
    }

    /// Fills `dst` with frames starting at source frame `from`. A stem that ends early reads as
    /// silence. Returns the frame count the longest stem actually decoded.
    pub(super) fn read_frames(&mut self, from: i64, dst: &mut [[f32; STEM_FRAME_FLOATS]]) -> usize {
        let mut decoded = 0;
        let step = self.scratch.len();
        let mut done = 0;
        while done < dst.len() {
            let n = (dst.len() - done).min(step);
            for (stem, reader) in self.readers.iter_mut().enumerate() {
                let got = reader.read_stereo(from + done as i64, &mut self.scratch[..n]);
                decoded = decoded.max(done + got);
                for (frame, sample) in dst[done..done + n].iter_mut().zip(&self.scratch[..n]) {
                    frame[stem * 2] = sample[0];
                    frame[stem * 2 + 1] = sample[1];
                }
            }
            done += n;
        }
        decoded
    }
}

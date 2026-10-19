//! Decoded mono audio handed to analysers.

/// Mono samples in `[-1, 1]` at `sample_rate`. Immutable once a job starts.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioClip {
    pub samples: Vec<f32>,
    pub sample_rate: f64,
}

impl AudioClip {
    pub fn new(samples: Vec<f32>, sample_rate: f64) -> Self {
        Self {
            samples,
            sample_rate,
        }
    }

    pub fn duration_seconds(&self) -> f64 {
        if self.sample_rate > 0.0 {
            self.samples.len() as f64 / self.sample_rate
        } else {
            0.0
        }
    }
}

impl Default for AudioClip {
    fn default() -> Self {
        Self {
            samples: Vec::new(),
            sample_rate: 44_100.0,
        }
    }
}

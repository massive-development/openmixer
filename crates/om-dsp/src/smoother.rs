//! One-pole parameter smoother for control values that must glide instead of stepping.

/// One-pole smoother; a zero time constant makes it follow the target instantly.
#[derive(Debug, Clone, Copy)]
pub struct Smoother {
    coeff: f32,
    value: f32,
}

impl Default for Smoother {
    fn default() -> Self {
        Self {
            coeff: 1.0,
            value: 0.0,
        }
    }
}

impl Smoother {
    pub fn set_time(&mut self, sample_rate: f64, seconds: f64) {
        self.coeff = if seconds <= 0.0 {
            1.0
        } else {
            (1.0 - (-1.0 / (seconds * sample_rate)).exp()) as f32
        };
    }

    pub fn snap(&mut self, v: f32) {
        self.value = v;
    }

    pub fn current(&self) -> f32 {
        self.value
    }

    pub fn step(&mut self, target: f32) -> f32 {
        self.value += self.coeff * (target - self.value);
        if (target - self.value).abs() < 1.0e-6 {
            self.value = target;
        }
        self.value
    }
}

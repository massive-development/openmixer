//! Value state of the on-screen controls: a ranged slider or knob that snaps to its interval and
//! clamps to its range, and the readout format each knob shows. Port of the value handling of
//! `juce::Slider` and `LabeledKnob` as the widgets use them.

use crate::format;

/// A value in `min..=max`, snapped to `interval` (0 = continuous). `default` is what a double-click
/// restores.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ranged {
    pub min: f64,
    pub max: f64,
    pub interval: f64,
    pub default: f64,
    value: f64,
}

impl Ranged {
    /// A control at its default.
    pub fn new(min: f64, max: f64, interval: f64, default: f64) -> Self {
        let mut r = Self {
            min,
            max,
            interval,
            default,
            value: default,
        };
        r.value = r.snap(default);
        r
    }

    pub fn value(&self) -> f64 {
        self.value
    }

    /// Changes the range and re-fits the current value inside it.
    pub fn set_range(&mut self, min: f64, max: f64, interval: f64) {
        self.min = min;
        self.max = max;
        self.interval = interval;
        self.value = self.snap(self.value);
    }

    /// Stores `value` (snapped and clamped) and returns the stored value.
    pub fn set(&mut self, value: f64) -> f64 {
        self.value = self.snap(value);
        self.value
    }

    /// Restores the default, as a double-click does.
    pub fn reset(&mut self) -> f64 {
        self.set(self.default)
    }

    /// `value` snapped to the interval and clamped to the range.
    pub fn snap(&self, value: f64) -> f64 {
        let stepped = if self.interval > 0.0 {
            self.min + ((value - self.min) / self.interval).round() * self.interval
        } else {
            value
        };
        stepped.clamp(self.min, self.max)
    }
}

/// How a knob's value reads on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Readout {
    /// Linear gain as `+3 dB`; zero reads `-inf`.
    Gain,
    /// Decibels as `-12 dB`.
    Decibels,
    /// Sweep filter, `LPF 50%`.
    Filter,
    /// `75%` for 0..1.
    Percent,
    /// Whole number.
    Whole,
    /// Two decimals.
    Fixed2,
    /// Pitch fader: percentage around 1.0.
    Rate,
}

impl Readout {
    pub fn text(self, value: f64) -> String {
        match self {
            Readout::Gain => {
                if value <= 0.0 {
                    "-inf".to_owned()
                } else {
                    format!("{} dB", format::round_half_away(20.0 * value.log10()))
                }
            }
            Readout::Decibels => format!("{} dB", format::round_half_away(value)),
            Readout::Filter => format::filter(value),
            Readout::Percent => format!("{}%", format::round_half_away(value * 100.0)),
            Readout::Whole => format!("{}", format::round_half_away(value)),
            Readout::Fixed2 => format!("{value:.2}"),
            Readout::Rate => format::rate(value),
        }
    }
}

/// A labelled knob: a ranged value plus how it reads.
#[derive(Debug, Clone, PartialEq)]
pub struct Knob {
    pub label: String,
    pub readout: Readout,
    pub range: Ranged,
}

impl Knob {
    pub fn new(
        label: impl Into<String>,
        readout: Readout,
        min: f64,
        max: f64,
        default: f64,
    ) -> Self {
        Self {
            label: label.into(),
            readout,
            range: Ranged::new(min, max, 0.0, default),
        }
    }

    pub fn value(&self) -> f64 {
        self.range.value()
    }

    /// Stores a value without sending it anywhere (mirroring the engine).
    pub fn set_quiet(&mut self, value: f64) {
        self.range.set(value);
    }

    pub fn text(&self) -> String {
        self.readout.text(self.value())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_snap_to_the_interval_and_clamp() {
        let mut r = Ranged::new(0.92, 1.08, 0.0005, 1.0);
        assert!((r.set(1.05) - 1.05).abs() < 1e-9);
        assert!((r.set(1.5) - 1.08).abs() < 1e-9, "clamps to the range");
        assert!((r.set(0.9) - 0.92).abs() < 1e-9);
        assert!(
            (r.set(1.00026) - 1.0005).abs() < 1e-9,
            "snaps to the nearest step"
        );
    }

    #[test]
    fn narrowing_the_range_refits_the_value() {
        let mut r = Ranged::new(0.0, 2.0, 0.001, 1.0);
        r.set(1.4);
        r.set_range(0.84, 1.16, 0.001);
        assert!((r.value() - 1.16).abs() < 1e-9);
    }

    #[test]
    fn readouts_match_the_value_formats() {
        assert_eq!(Readout::Gain.text(0.0), "-inf");
        assert_eq!(Readout::Gain.text(2.0), "6 dB");
        assert_eq!(Readout::Percent.text(0.35), "35%");
        assert_eq!(Readout::Fixed2.text(0.5), "0.50");
        assert_eq!(Readout::Whole.text(120.4), "120");
    }
}

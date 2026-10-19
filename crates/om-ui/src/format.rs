//! Readouts for control values: what a knob means, not its raw position.

/// Fixed-point text with an explicit `+` for positive values and never `-0.0`.
pub fn signed_fixed(value: f64, decimals: usize) -> String {
    let text = format!("{value:.decimals$}");
    let shown: f64 = text.parse().unwrap_or(0.0);
    if shown == 0.0 {
        return format!("{:.decimals$}", 0.0);
    }
    if shown > 0.0 {
        format!("+{text}")
    } else {
        text
    }
}

/// Decibels with one decimal and a sign, e.g. `+3.0 dB`.
pub fn decibels(db: f64) -> String {
    format!("{} dB", signed_fixed(db, 1))
}

/// Linear gain shown as dB; silence reads as `-inf dB`.
pub fn gain_as_decibels(linear: f64) -> String {
    if linear <= 0.0 {
        "-inf dB".to_owned()
    } else {
        decibels(20.0 * linear.log10())
    }
}

/// Sweep filter: -1 low-pass .. 0 off .. +1 high-pass.
pub fn filter(value: f64) -> String {
    if value.abs() < 0.005 {
        return "Off".to_owned();
    }
    let side = if value < 0.0 { "LPF" } else { "HPF" };
    format!("{side} {}%", round_half_away(value.abs() * 100.0))
}

/// Playback rate (1.0 = original tempo) as a pitch-fader percentage.
pub fn rate(rate: f64) -> String {
    format!("{}%", signed_fixed((rate - 1.0) * 100.0, 1))
}

/// Rounds half away from zero.
pub fn round_half_away(v: f64) -> i64 {
    v.round() as i64
}

/// `m:ss`, optionally with tenths. Tenths truncate (a running clock never runs ahead); whole
/// seconds round (a duration reads as stored). Negative times read as zero.
pub fn clock(seconds: f64, with_tenths: bool) -> String {
    let s = seconds.max(0.0);
    let whole = if with_tenths {
        s as i64
    } else {
        round_half_away(s)
    };
    let mut text = format!("{}:{:02}", whole / 60, whole % 60);
    if with_tenths {
        text.push_str(&format!(".{}", ((s - whole as f64) * 10.0) as i64));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decibels_carry_a_sign_and_never_show_negative_zero() {
        assert_eq!(decibels(3.0), "+3.0 dB");
        assert_eq!(decibels(-12.04), "-12.0 dB");
        assert_eq!(decibels(0.0), "0.0 dB");
        assert_eq!(decibels(-0.01), "0.0 dB");
    }

    #[test]
    fn gain_reads_as_decibels_with_silence_as_minus_infinity() {
        assert_eq!(gain_as_decibels(1.0), "0.0 dB");
        assert_eq!(gain_as_decibels(2.0), "+6.0 dB");
        assert_eq!(gain_as_decibels(0.0), "-inf dB");
    }

    #[test]
    fn filter_names_the_side_of_the_sweep() {
        assert_eq!(filter(0.0), "Off");
        assert_eq!(filter(0.001), "Off");
        assert_eq!(filter(-0.5), "LPF 50%");
        assert_eq!(filter(1.0), "HPF 100%");
    }

    #[test]
    fn rate_is_a_percentage_around_original_tempo() {
        assert_eq!(rate(1.0), "0.0%");
        assert_eq!(rate(1.025), "+2.5%");
        assert_eq!(rate(0.92), "-8.0%");
    }

    #[test]
    fn clock_truncates_tenths_and_rounds_whole_seconds() {
        assert_eq!(clock(245.4, false), "4:05");
        assert_eq!(clock(245.6, false), "4:06");
        assert_eq!(clock(65.34, true), "1:05.3");
        assert_eq!(clock(-3.0, true), "0:00.0");
        assert_eq!(clock(12.0, true), "0:12.0");
    }
}

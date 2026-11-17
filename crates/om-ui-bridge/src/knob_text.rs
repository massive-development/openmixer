//! The value a knob prints under its label, as GPUI's mixer prints it, and the readout names the
//! web front end formats by. The front end's port (`web/src/lib/controls/readout.ts`) is pinned to
//! `caption_text` by `fixtures/ui-bridge/knob-text.txt`.

use om_ui::control::Readout;

/// What GPUI prints under a knob's label for `value`: the knob's readout without its unit, which
/// the label names (`om-ui-gpui/src/mixer_view.rs`, `caption_value`).
pub fn caption_text(readout: Readout, value: f64) -> String {
    let text = readout.text(value);
    text.trim_end_matches(" dB")
        .trim_end_matches(" %")
        .to_owned()
}

/// The name the front end formats `readout` by; `None` for a readout no slice knob uses, which
/// the front end does not port.
pub fn readout_name(readout: Readout) -> Option<&'static str> {
    match readout {
        Readout::Gain => Some("gain"),
        Readout::Decibels => Some("decibels"),
        Readout::Filter => Some("filter"),
        Readout::Percent | Readout::Whole | Readout::Fixed2 | Readout::Rate => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_caption_drops_the_unit_the_label_names() {
        assert_eq!(caption_text(Readout::Gain, 1.0), "0");
        assert_eq!(caption_text(Readout::Gain, 2.0), "+6");
        assert_eq!(caption_text(Readout::Gain, 0.0), "-inf");
        assert_eq!(caption_text(Readout::Decibels, -12.4), "-12");
        assert_eq!(caption_text(Readout::Decibels, -48.0), "KILL");
        assert_eq!(caption_text(Readout::Filter, 0.0), "Off");
        assert_eq!(caption_text(Readout::Filter, -0.5), "LPF 50%");
    }

    #[test]
    fn only_the_slice_readouts_have_names() {
        assert_eq!(readout_name(Readout::Gain), Some("gain"));
        assert_eq!(readout_name(Readout::Decibels), Some("decibels"));
        assert_eq!(readout_name(Readout::Filter), Some("filter"));
        assert_eq!(readout_name(Readout::Percent), None);
    }
}

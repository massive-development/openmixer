//! Every user-facing name for a control, notice or figure that the views would otherwise build from
//! a router id or a CLI flag. Pure strings: the stored ids (`deck.0.play`) never change on disk, and
//! only this module turns them into words ("Deck 1 Play"). Numbers in ids are the router's
//! zero-based indices and are shown one-based.

use crate::settings::AudioStats;
use om_core::ParamId;
use std::str::FromStr;

/// A notice the window shows in the header chip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Notice {
    /// `--mock-device`: the silent demo output.
    Demo,
    /// The audio device could not be opened; the app runs silent.
    NoDevice,
}

/// The text of a header notice.
pub fn notice_text(notice: Notice) -> &'static str {
    match notice {
        Notice::Demo => "Demo mode: no audio output. Choose an output in Settings.",
        Notice::NoDevice => "No audio device: running silent.",
    }
}

/// A readable parameter name: `deck.0.play` -> "Deck 1 Play", `mixer.ch0.trim` -> "Channel 1 Gain".
/// An id that is not a known one is returned as it is written.
pub fn param_label(id: &str) -> String {
    if id == "router.unknown" {
        return "Unrouted messages".to_owned();
    }
    let normal = normalise(id);
    if ParamId::from_str(&normal).is_err() {
        return id.to_owned();
    }
    match normal.as_str() {
        "mixer.cue.level" => return "Headphone Volume".to_owned(),
        "mixer.cue.mix" => return "Headphone Mix".to_owned(),
        _ => {}
    }
    let segments: Vec<&str> = normal.split('.').collect();
    if let ["sync", deck, "sync"] = segments[..] {
        if let Ok(n) = deck.parse::<u32>() {
            return format!("Deck {} Sync", n + 1);
        }
    }
    let mut words: Vec<String> = Vec::new();
    for (i, segment) in segments.iter().enumerate() {
        if (i == 0 && *segment == "mixer" && segments.len() > 1) || *segment == "eq" {
            continue;
        }
        words.push(word(segment));
    }
    words.join(" ")
}

/// Alternative spellings of the same id: a numbered EQ band (0 high, 1 mid, 2 low, as the strip
/// draws them) and the `sync.enabled.<deck>` form.
fn normalise(id: &str) -> String {
    let mut parts: Vec<String> = id.split('.').map(str::to_owned).collect();
    let n = parts.len();
    if n >= 2 && parts[n - 2] == "eq" {
        let band = match parts[n - 1].as_str() {
            "0" => Some("high"),
            "1" => Some("mid"),
            "2" => Some("low"),
            _ => None,
        };
        if let Some(band) = band {
            parts[n - 1] = band.to_owned();
        }
    }
    if n == 3 && parts[0] == "sync" && parts[1] == "enabled" {
        let deck = parts[2].clone();
        parts = vec!["sync".to_owned(), deck, "sync".to_owned()];
    }
    parts.join(".")
}

fn word(segment: &str) -> String {
    if let Ok(n) = segment.parse::<u32>() {
        return (n + 1).to_string();
    }
    for (prefix, name) in [("ch", "Channel"), ("pad", "Pad")] {
        if let Some(n) = segment
            .strip_prefix(prefix)
            .and_then(|r| r.parse::<u32>().ok())
        {
            return format!("{name} {}", n + 1);
        }
    }
    match segment {
        "trim" => "Gain".to_owned(),
        "rate" => "Tempo".to_owned(),
        "pfl" => "Cue".to_owned(),
        "xfader" => "Crossfader assign".to_owned(),
        "keylock" => "Key lock".to_owned(),
        other => capitalize(other),
    }
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// "Outputs 1-2" for the output pair at index `pair`.
pub fn output_pair_label(pair: usize) -> String {
    format!("Outputs {}-{}", 2 * pair + 1, 2 * pair + 2)
}

/// The readout under the audio controls for a running device.
pub fn audio_stats_text(stats: &AudioStats) -> String {
    format!(
        "Latency {:.1} ms, round trip {:.1} ms at {} Hz / {} samples\nCPU {:.0}% (peak {:.2} ms of {:.2} ms), dropouts {}",
        stats.output_latency_ms,
        stats.round_trip_latency_ms,
        stats.sample_rate.round() as i64,
        stats.buffer_size,
        stats.cpu_load * 100.0,
        stats.max_callback_us / 1000.0,
        stats.budget_us / 1000.0,
        stats.xrun_count,
    )
}

/// The header's latency indicator: "4 ms  0 dropouts" (a single dropout reads "1 dropout").
pub fn latency_text(latency_ms: f64, dropouts: u64) -> String {
    let noun = if dropouts == 1 { "dropout" } else { "dropouts" };
    format!("{} ms  {dropouts} {noun}", latency_ms.round() as i64)
}

/// The captions of a channel strip's controls, in strip order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelLabels {
    pub gain: &'static str,
    pub high: &'static str,
    pub mid: &'static str,
    pub low: &'static str,
    pub filter: &'static str,
    pub cue: &'static str,
}

/// One name per strip control: the same word in every view.
pub const fn channel_labels() -> ChannelLabels {
    ChannelLabels {
        gain: "GAIN",
        high: "HIGH",
        mid: "MID",
        low: "LOW",
        filter: "FILTER",
        cue: "CUE",
    }
}

/// A loop length in beats as the pads show it: `0.25` -> "1/4", `0.5` -> "1/2", `4.0` -> "4".
pub fn loop_label(beats: f32) -> String {
    if (beats - 0.25).abs() < 1e-4 {
        "1/4".to_owned()
    } else if (beats - 0.5).abs() < 1e-4 {
        "1/2".to_owned()
    } else {
        format!("{}", beats.round() as i64)
    }
}

/// Caption over the loop length buttons.
pub const LOOP_CAPTION: &str = "LOOP (beats)";
/// Library action: deletes the selected playlist.
pub const DELETE_PLAYLIST: &str = "Delete playlist";
/// Library action: takes the selected tracks out of the playlist, keeping them in the library.
pub const REMOVE_FROM_PLAYLIST: &str = "Remove from playlist";
/// Library action: separates the selected tracks into stems.
pub const SPLIT_STEMS: &str = "Split stems";
/// Library action: stops the stem separation queue.
pub const CANCEL_STEMS: &str = "Cancel stems";
/// The library table's duration column.
pub const LENGTH_COLUMN: &str = "Length";

/// "Load to Deck 1" for the deck at index `deck`.
pub fn load_to_deck(deck: usize) -> String {
    format!("Load to Deck {}", deck + 1)
}

/// The library status line: "3 tracks", or "3 of 120" when a filter hides some.
pub fn status_count(total: usize, shown: usize) -> String {
    if shown == total {
        match total {
            1 => "1 track".to_owned(),
            n => format!("{n} tracks"),
        }
    } else {
        format!("{shown} of {total}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params_read_one_based_and_in_words() {
        for (id, label) in [
            ("deck.0.play", "Deck 1 Play"),
            ("mixer.ch0.trim", "Channel 1 Gain"),
            ("mixer.ch1.eq.2", "Channel 2 Low"),
            ("mixer.ch1.eq.low", "Channel 2 Low"),
            ("mixer.ch0.fader", "Channel 1 Fader"),
            ("sync.enabled.0", "Deck 1 Sync"),
            ("sync.0.sync", "Deck 1 Sync"),
            ("deck.0.rate", "Deck 1 Tempo"),
            ("router.unknown", "Unrouted messages"),
            ("mixer.master", "Master"),
            ("sampler.pad2.trigger", "Sampler Pad 3 Trigger"),
            ("nonsense..id", "nonsense..id"),
        ] {
            assert_eq!(param_label(id), label, "{id}");
        }
    }

    #[test]
    fn output_pairs_and_notices() {
        assert_eq!(output_pair_label(0), "Outputs 1-2");
        assert_eq!(output_pair_label(1), "Outputs 3-4");
        assert_eq!(
            notice_text(Notice::Demo),
            "Demo mode: no audio output. Choose an output in Settings."
        );
        assert_eq!(
            notice_text(Notice::NoDevice),
            "No audio device: running silent."
        );
    }

    #[test]
    fn no_label_leaks_a_flag_or_an_id() {
        let mut texts = vec![
            notice_text(Notice::Demo).to_owned(),
            notice_text(Notice::NoDevice).to_owned(),
            output_pair_label(3),
            latency_text(4.2, 0),
            load_to_deck(1),
        ];
        for id in [
            "deck.0.play",
            "deck.1.loop.reloop",
            "mixer.ch0.trim",
            "mixer.ch2.eq.high",
            "mixer.cue.mix",
            "sync.enabled.1",
        ] {
            texts.push(param_label(id));
        }
        for text in texts {
            for bad in ["--", "deck.", "mixer.", ".ch"] {
                assert!(!text.contains(bad), "{text:?} contains {bad:?}");
            }
        }
    }

    #[test]
    fn audio_stats_name_latency_and_dropouts() {
        let stats = AudioStats {
            output_latency_ms: 4.2,
            round_trip_latency_ms: 9.1,
            sample_rate: 48_000.0,
            buffer_size: 256,
            cpu_load: 0.25,
            max_callback_us: 1_500.0,
            budget_us: 5_333.0,
            xrun_count: 2,
        };
        assert_eq!(
            audio_stats_text(&stats),
            "Latency 4.2 ms, round trip 9.1 ms at 48000 Hz / 256 samples\nCPU 25% (peak 1.50 ms of 5.33 ms), dropouts 2"
        );
        assert_eq!(latency_text(4.2, 0), "4 ms  0 dropouts");
        assert_eq!(latency_text(4.2, 1), "4 ms  1 dropout");
    }

    #[test]
    fn strip_loop_and_library_names() {
        let c = channel_labels();
        assert_eq!(
            [c.gain, c.high, c.mid, c.low, c.filter, c.cue],
            ["GAIN", "HIGH", "MID", "LOW", "FILTER", "CUE"]
        );
        assert_eq!(loop_label(0.25), "1/4");
        assert_eq!(loop_label(0.5), "1/2");
        assert_eq!(loop_label(8.0), "8");
        assert_eq!(DELETE_PLAYLIST, "Delete playlist");
        assert_eq!(REMOVE_FROM_PLAYLIST, "Remove from playlist");
        assert_eq!(load_to_deck(0), "Load to Deck 1");
        assert_eq!(SPLIT_STEMS, "Split stems");
        assert_eq!(LENGTH_COLUMN, "Length");
        assert_eq!(status_count(3, 3), "3 tracks");
        assert_eq!(status_count(120, 3), "3 of 120");
    }
}

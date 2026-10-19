//! The engine as a [`StateReader`]: the ids a controller map's feedback rules can read. Port of
//! `src/app/EngineStateReader.*`. Every read is a control-side getter (an atomic or a short
//! uncontended lock), so nothing here touches the audio thread.
//!
//! ```text
//! deck.<N>.play | loaded | keylock | loop | cue (loaded and stopped) | sync | sync.master
//! deck.<N>.slip | quantize | rate | position (0-1)
//! deck.<N>.hotcue<K>.set (1 when the cue exists)
//! mixer.ch<N>.pfl | fader | trim | filter | level (decaying peak)
//! mixer.crossfader | mixer.master (gain) | mixer.master.level (decaying peak, louder side)
//! ```

use om_core::StateReader;
use om_engine::DjEngine;
use std::sync::Arc;

pub struct EngineStateReader {
    engine: Arc<DjEngine>,
}

impl EngineStateReader {
    pub fn new(engine: Arc<DjEngine>) -> Self {
        Self { engine }
    }
}

/// A decimal index: non-empty, digits only.
fn digits(s: &str) -> Option<usize> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

fn flag(on: bool) -> f64 {
    if on {
        1.0
    } else {
        0.0
    }
}

impl StateReader for EngineStateReader {
    fn read_state(&self, id: &str) -> Option<f64> {
        let parts: Vec<&str> = id.split('.').collect();
        if parts.len() < 2 {
            return None;
        }
        match parts[0] {
            "deck" => self.read_deck(&parts),
            "mixer" => self.read_mixer(&parts),
            _ => None,
        }
    }
}

impl EngineStateReader {
    fn read_deck(&self, parts: &[&str]) -> Option<f64> {
        if parts.len() < 3 {
            return None;
        }
        let index = digits(parts[1])?;
        if index >= self.engine.num_decks() {
            return None;
        }
        let deck = self.engine.deck(index);
        let control = self.engine.deck_control(index);
        let sync = self.engine.sync();
        let what = parts[2..].join(".");
        Some(match what.as_str() {
            "play" => flag(deck.is_playing()),
            "loaded" => flag(deck.is_loaded()),
            "keylock" => flag(deck.is_keylock()),
            "loop" => flag(deck.is_looping()),
            "cue" => flag(deck.is_loaded() && !deck.is_playing()),
            "sync" => flag(sync.is_enabled(index)),
            "sync.master" => flag(sync.leader() == Some(index)),
            "slip" => flag(control.is_slipping()),
            "quantize" => flag(control.is_quantize()),
            "rate" => deck.rate(),
            "position" => {
                let length = deck.length_frames();
                if length > 0 {
                    deck.position_frames() as f64 / length as f64
                } else {
                    0.0
                }
            }
            other => {
                let cue = other
                    .strip_suffix(".set")
                    .and_then(|name| name.strip_prefix("hotcue"))
                    .and_then(digits)?;
                flag(deck.hotcue(cue).is_some())
            }
        })
    }

    fn read_mixer(&self, parts: &[&str]) -> Option<f64> {
        let mixer = self.engine.mixer();
        if let Some(channel) = parts[1].strip_prefix("ch").and_then(digits) {
            if channel >= mixer.num_channels() || parts.len() != 3 {
                return None;
            }
            return Some(match parts[2] {
                "pfl" => flag(mixer.pfl(channel)),
                "fader" => f64::from(mixer.fader(channel)),
                "trim" => f64::from(mixer.trim(channel)),
                "filter" => f64::from(mixer.filter(channel)),
                "level" => f64::from(mixer.channel_peak(channel)),
                _ => return None,
            });
        }
        Some(match parts[1..].join(".").as_str() {
            "crossfader" => f64::from(mixer.crossfader()),
            "master" => f64::from(mixer.master_gain()),
            "master.level" => f64::from(mixer.master_peak_left().max(mixer.master_peak_right())),
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Port of EngineStateReaderTest.
    #[test]
    fn reports_deck_and_mixer_state_by_id_and_rejects_unknown_ids() {
        let (engine, _audio) = DjEngine::new(2);
        let engine = Arc::new(engine);
        let reader = EngineStateReader::new(Arc::clone(&engine));
        assert_eq!(reader.read_state("deck.0.play"), Some(0.0));
        engine.deck(0).play();
        assert_eq!(reader.read_state("deck.0.play"), Some(1.0));

        engine.mixer().set_pfl(1, true);
        assert_eq!(reader.read_state("mixer.ch1.pfl"), Some(1.0));
        engine.deck_control(0).set_slip(true);
        assert_eq!(reader.read_state("deck.0.slip"), Some(1.0));
        engine.deck(1).set_hotcue(2, 1000);
        assert_eq!(reader.read_state("deck.1.hotcue2.set"), Some(1.0));
        assert_eq!(reader.read_state("deck.1.hotcue3.set"), Some(0.0));
        engine.sync().set_leader(Some(1));
        assert_eq!(reader.read_state("deck.1.sync.master"), Some(1.0));
        for id in [
            "mixer.master.level",
            "mixer.crossfader",
            "mixer.ch0.level",
            "deck.0.cue",
            "deck.0.loop",
            "deck.0.position",
        ] {
            assert!(reader.read_state(id).is_some(), "{id}");
        }

        for id in [
            "",
            "deck",
            "deck.99.play",
            "deck.0.bogus",
            "deck.x.play",
            "deck..play",
            "mixer.ch99.pfl",
            "mixer.ch0.bogus",
            "mixer.ch0",
            "nope.0.play",
            "deck.0.hotcuex.set",
        ] {
            assert_eq!(reader.read_state(id), None, "{id}");
        }
    }
}

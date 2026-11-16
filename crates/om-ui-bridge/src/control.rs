//! Front-end control writes: an id string and a value, validated, then dispatched to the router.
//! Runs on a control thread (the IPC command handler), never the audio thread.

use crate::BridgeError;
use om_core::param_id::{
    ChannelParam, DeckParam, EqBand, MixerParam, HOTCUES_PER_DECK, MAX_CHANNELS, MAX_DECKS,
};
use om_core::{ParamId, ParameterRouter};
use om_ui::widgets::loops::{loop_param, SIZES};

/// Parses `id`, checks `value`, and dispatches once. Nothing is dispatched on an error.
pub fn dispatch_control(router: &ParameterRouter, id: &str, value: f64) -> Result<(), BridgeError> {
    if !value.is_finite() {
        return Err(BridgeError::NonFinite);
    }
    let parsed = parse_control(id)?;
    if router.dispatch(&parsed, value) {
        Ok(())
    } else {
        Err(BridgeError::Rejected(id.to_owned()))
    }
}

fn parse_control(id: &str) -> Result<ParamId, BridgeError> {
    let unknown = || BridgeError::UnknownControl(id.to_owned());
    let parsed: ParamId = id.parse().map_err(|_| unknown())?;
    parsed.validate().map_err(|_| unknown())?;
    Ok(parsed)
}

/// Stable front-end keys paired with canonical ids, for every control the decks slice uses.
/// Indices past the engine's limits are dropped rather than producing ids that cannot validate.
pub fn slice_control_ids(decks: u8, channels: u8) -> Vec<(String, String)> {
    slice_params(decks, channels)
        .into_iter()
        .map(|(key, id)| (key, id.to_string()))
        .collect()
}

/// [`slice_control_ids`] with the ids still typed, for the range lookup.
pub(crate) fn slice_params(decks: u8, channels: u8) -> Vec<(String, ParamId)> {
    let decks = (0..decks.min(MAX_DECKS)).flat_map(deck_controls);
    let channels = (0..channels.min(MAX_CHANNELS)).flat_map(channel_controls);
    decks.chain(channels).chain(master_controls()).collect()
}

fn master_controls() -> impl Iterator<Item = (String, ParamId)> {
    [
        ("mixer.crossfader", MixerParam::Crossfader),
        ("mixer.master", MixerParam::Master),
    ]
    .into_iter()
    .map(|(key, p)| (key.to_owned(), ParamId::Mixer(p)))
}

const DECK_CONTROLS: [(&str, DeckParam); 4] = [
    ("play", DeckParam::Play),
    ("sync", DeckParam::Sync),
    ("tempo", DeckParam::Rate),
    ("seek", DeckParam::Seek),
];

const CHANNEL_CONTROLS: [(&str, ChannelParam); 6] = [
    ("fader", ChannelParam::Fader),
    ("trim", ChannelParam::Trim),
    ("filter", ChannelParam::Filter),
    ("eq.low", ChannelParam::Eq(EqBand::Low)),
    ("eq.mid", ChannelParam::Eq(EqBand::Mid)),
    ("eq.high", ChannelParam::Eq(EqBand::High)),
];

/// Transport, hot cues (`hotcue<i>`, `hotcue<i>.clear`) and loops (`loop.<beats>`, `loop.exit`).
fn deck_controls(deck: u8) -> impl Iterator<Item = (String, ParamId)> {
    let transport = DECK_CONTROLS.into_iter().map(|(k, p)| (k.to_owned(), p));
    let all = transport.chain(hotcue_controls()).chain(loop_controls());
    all.map(move |(key, p)| (format!("deck{deck}.{key}"), ParamId::Deck { deck, p }))
}

fn hotcue_controls() -> impl Iterator<Item = (String, DeckParam)> {
    (0..HOTCUES_PER_DECK).flat_map(|index| {
        let set = (
            format!("hotcue{index}"),
            DeckParam::Hotcue {
                index,
                clear: false,
            },
        );
        let clear = (
            format!("hotcue{index}.clear"),
            DeckParam::Hotcue { index, clear: true },
        );
        [set, clear]
    })
}

// The sizes GPUI's loop row offers (`om_ui::widgets::loops::SIZES`), keyed by their beats.
fn loop_controls() -> impl Iterator<Item = (String, DeckParam)> {
    let sizes = SIZES
        .into_iter()
        .map(|beats| (format!("loop.{beats}"), loop_param(beats)));
    sizes.chain([("loop.exit".to_owned(), DeckParam::LoopExit)])
}

fn channel_controls(ch: u8) -> impl Iterator<Item = (String, ParamId)> {
    CHANNEL_CONTROLS.into_iter().map(move |(key, p)| {
        let id = ParamId::Mixer(MixerParam::Channel { ch, p });
        (format!("ch{ch}.{key}"), id)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use om_core::{Namespace, ParamTarget};
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};

    type Seen = Arc<Mutex<Vec<(String, f64)>>>;

    /// Accepts every deck id and records it, as the router's own tests do.
    struct Recorder(Seen);

    impl ParamTarget for Recorder {
        fn set_param(&self, id: &ParamId, value: f64) -> bool {
            self.0.lock().unwrap().push((id.to_string(), value));
            true
        }
    }

    fn router_with_recorder() -> (ParameterRouter, Seen) {
        let seen = Seen::default();
        let router = ParameterRouter::new();
        router.register_typed(Namespace::Deck, Arc::new(Recorder(seen.clone())));
        (router, seen)
    }

    fn id_for(key: &str) -> String {
        let ids = slice_control_ids(2, 2);
        let found = ids.into_iter().find(|(k, _)| k == key);
        found.map(|(_, id)| id).unwrap()
    }

    #[test]
    fn known_id_reaches_the_target() {
        let (router, seen) = router_with_recorder();
        let deck0_play = id_for("deck0.play");
        dispatch_control(&router, &deck0_play, 1.0).unwrap();
        assert_eq!(seen.lock().unwrap().as_slice(), &[(deck0_play, 1.0)]);
    }

    #[test]
    fn unknown_id_is_rejected() {
        let (router, seen) = router_with_recorder();
        for id in ["deck.0.not_a_control", "mixer.ch9.fader", "", "deck..play"] {
            let result = dispatch_control(&router, id, 1.0);
            assert!(
                matches!(result, Err(BridgeError::UnknownControl(_))),
                "{id}"
            );
        }
        // A well-formed id outside the engine namespaces is a plugin id; no plugin owns it here.
        let ext = dispatch_control(&router, "not.a.real.id", 1.0);
        assert!(matches!(ext, Err(BridgeError::Rejected(_))));
        assert!(seen.lock().unwrap().is_empty());
    }

    #[test]
    fn non_finite_value_is_rejected() {
        let (router, seen) = router_with_recorder();
        let deck0_play = id_for("deck0.play");
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let result = dispatch_control(&router, &deck0_play, value);
            assert!(matches!(result, Err(BridgeError::NonFinite)));
        }
        assert!(seen.lock().unwrap().is_empty());
    }

    #[test]
    fn unregistered_namespace_is_rejected() {
        let router = ParameterRouter::new();
        let result = dispatch_control(&router, &id_for("deck0.play"), 1.0);
        assert!(matches!(result, Err(BridgeError::Rejected(_))));
    }

    #[test]
    fn slice_ids_round_trip() {
        let ids = slice_control_ids(2, 2);
        for (key, id) in &ids {
            let parsed: ParamId = id.parse().unwrap();
            assert_eq!(&parsed.to_string(), id, "{key}");
            parsed.validate().unwrap();
        }
        let keys: HashSet<&str> = ids.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys.len(), ids.len());
    }

    #[test]
    fn slice_ids_cover_the_slice() {
        let ids = slice_control_ids(2, 2);
        let per_deck = 4 + 2 * usize::from(HOTCUES_PER_DECK) + SIZES.len() + 1;
        assert_eq!(ids.len(), 2 * per_deck + 2 * 6 + 2);
        assert_eq!(id_for("deck1.tempo"), "deck.1.rate");
        assert_eq!(id_for("deck0.hotcue3"), "deck.0.hotcue3");
        assert_eq!(id_for("deck0.hotcue3.clear"), "deck.0.hotcue3.clear");
        assert_eq!(id_for("deck1.loop.0.25"), "deck.1.loop.quantized.0.25");
        assert_eq!(id_for("deck1.loop.4"), "deck.1.loop.quantized.4");
        assert_eq!(id_for("deck0.loop.exit"), "deck.0.loop.exit");
        assert_eq!(id_for("ch0.eq.low"), "mixer.ch0.eq.low");
        assert_eq!(id_for("mixer.crossfader"), "mixer.crossfader");
    }
}

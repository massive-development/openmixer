//! What the widget models need: read access to the engine, and a write path that goes only through
//! the string-id control surface. Ids are built from [`ParamId`], so the strings a widget sends are
//! the canonical ids the router parses back to the same typed id. UI and MIDI stay equivalent.

use om_core::param_id::{ChannelParam, DeckParam, FxParam, FxRack, MixerParam};
use om_core::{ParamId, ParameterTarget};
use om_dsp::{CrossfaderAssign, CrossfaderCurve};
use om_engine::deck::Deck;
use om_engine::{DeckControl, DjEngine, FxRouting, MixerControl, Sync};
use std::sync::Arc;

/// Routes one deck's parameters. Cheap to clone.
#[derive(Clone)]
pub struct DeckBinding {
    pub engine: Arc<DjEngine>,
    pub target: Arc<dyn ParameterTarget>,
    pub index: usize,
}

impl DeckBinding {
    pub fn new(engine: Arc<DjEngine>, target: Arc<dyn ParameterTarget>, index: usize) -> Self {
        Self {
            engine,
            target,
            index,
        }
    }

    pub fn deck(&self) -> &Deck {
        self.engine.deck(self.index)
    }

    pub fn control(&self) -> &DeckControl {
        self.engine.deck_control(self.index)
    }

    pub fn sync(&self) -> &Sync {
        self.engine.sync()
    }

    /// The typed id of one of this deck's parameters. The engine has at most `MAX_DECKS` decks, so
    /// the index always fits.
    pub fn id(&self, p: DeckParam) -> ParamId {
        ParamId::Deck {
            deck: self.index as u8,
            p,
        }
    }

    /// Sends one of this deck's parameters. `false` when the router does not know the id.
    pub fn send(&self, p: DeckParam, value: f64) -> bool {
        self.target.set_parameter(&self.id(p).to_string(), value)
    }
}

/// Routes the mixer and FX parameters. Cheap to clone.
#[derive(Clone)]
pub struct MixerBinding {
    pub engine: Arc<DjEngine>,
    pub target: Arc<dyn ParameterTarget>,
}

impl MixerBinding {
    pub fn new(engine: Arc<DjEngine>, target: Arc<dyn ParameterTarget>) -> Self {
        Self { engine, target }
    }

    pub fn mixer(&self) -> &MixerControl {
        self.engine.mixer()
    }

    pub fn fx(&self) -> &FxRouting {
        self.engine.fx()
    }

    pub fn num_channels(&self) -> usize {
        self.mixer().num_channels()
    }

    /// Sends one mixer parameter (`mixer.*`).
    pub fn send(&self, p: MixerParam, value: f64) -> bool {
        self.target
            .set_parameter(&ParamId::Mixer(p).to_string(), value)
    }

    /// Sends one parameter of an FX rack or unit (`fx.*`).
    pub fn send_fx(&self, rack: FxRack, unit: Option<u16>, p: FxParam, value: f64) -> bool {
        self.target
            .set_parameter(&ParamId::Fx { rack, unit, p }.to_string(), value)
    }

    /// The typed id of one channel's parameter.
    pub fn channel(channel: usize, p: ChannelParam) -> MixerParam {
        MixerParam::Channel {
            ch: channel as u8,
            p,
        }
    }
}

/// Where a channel's meters get their peaks. The engine's [`MixerControl`] implements it; tests
/// supply synthetic levels.
pub trait LevelSource {
    /// `(left, right)` linear peak of one channel.
    fn channel_peaks(&self, channel: usize) -> (f32, f32);
    /// `(left, right)` linear peak of the master bus.
    fn master_peaks(&self) -> (f32, f32);
}

impl LevelSource for MixerControl {
    fn channel_peaks(&self, channel: usize) -> (f32, f32) {
        (
            self.channel_peak_left(channel),
            self.channel_peak_right(channel),
        )
    }

    fn master_peaks(&self) -> (f32, f32) {
        (self.master_peak_left(), self.master_peak_right())
    }
}

/// Engine index of a crossfader assignment (`mixer.ch<N>.xfader` value).
pub fn assign_index(assign: CrossfaderAssign) -> usize {
    match assign {
        CrossfaderAssign::Thru => 0,
        CrossfaderAssign::Left => 1,
        CrossfaderAssign::Right => 2,
    }
}

/// Crossfader assignment for an index, or `None` outside 0..=2.
pub fn assign_from_index(index: usize) -> Option<CrossfaderAssign> {
    match index {
        0 => Some(CrossfaderAssign::Thru),
        1 => Some(CrossfaderAssign::Left),
        2 => Some(CrossfaderAssign::Right),
        _ => None,
    }
}

/// Engine index of a crossfader curve (`mixer.crossfader.curve` value).
pub fn curve_index(curve: CrossfaderCurve) -> usize {
    match curve {
        CrossfaderCurve::Linear => 0,
        CrossfaderCurve::ConstantPower => 1,
    }
}

/// Crossfader curve for an index, or `None` outside 0..=1.
pub fn curve_from_index(index: usize) -> Option<CrossfaderCurve> {
    match index {
        0 => Some(CrossfaderCurve::Linear),
        1 => Some(CrossfaderCurve::ConstantPower),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use om_core::param_id::{EqBand, HOTCUES_PER_DECK, MAX_CHANNELS, MAX_DECKS};
    use om_engine::DjEngine;
    use std::str::FromStr;

    /// Every deck, mixer and FX id a binding can build parses back to the same typed id.
    #[test]
    fn every_id_built_by_the_bindings_parses_back_to_the_same_param_id() {
        let mut ids = Vec::new();
        for deck in 0..MAX_DECKS {
            let p_list = [
                DeckParam::Play,
                DeckParam::Rate,
                DeckParam::Keylock,
                DeckParam::Seek,
                DeckParam::LoopExit,
                DeckParam::LoopReloop,
                DeckParam::LoopQuantized(0.25),
                DeckParam::LoopQuantized(4.0),
                DeckParam::Sync,
                DeckParam::SyncMaster,
                DeckParam::Slip,
                DeckParam::Quantize,
            ];
            for p in p_list {
                ids.push(ParamId::Deck { deck, p });
            }
            for index in 0..HOTCUES_PER_DECK {
                for clear in [false, true] {
                    ids.push(ParamId::Deck {
                        deck,
                        p: DeckParam::Hotcue { index, clear },
                    });
                }
            }
        }
        for ch in 0..MAX_CHANNELS {
            for p in [
                ChannelParam::Trim,
                ChannelParam::Fader,
                ChannelParam::Filter,
                ChannelParam::Pfl,
                ChannelParam::Xfader,
                ChannelParam::Eq(EqBand::Low),
                ChannelParam::Eq(EqBand::Mid),
                ChannelParam::Eq(EqBand::High),
            ] {
                ids.push(ParamId::Mixer(MixerParam::Channel { ch, p }));
            }
        }
        for p in [
            MixerParam::Crossfader,
            MixerParam::CrossfaderCurve,
            MixerParam::Master,
            MixerParam::CueLevel,
            MixerParam::CueMix,
        ] {
            ids.push(ParamId::Mixer(p));
        }
        for rack in [FxRack::Master, FxRack::Channel(1)] {
            ids.push(ParamId::Fx {
                rack,
                unit: None,
                p: FxParam::Bpm,
            });
            ids.push(ParamId::Fx {
                rack,
                unit: Some(2),
                p: FxParam::Enable,
            });
            ids.push(ParamId::Fx {
                rack,
                unit: Some(2),
                p: FxParam::Mix,
            });
            ids.push(ParamId::Fx {
                rack,
                unit: Some(2),
                p: FxParam::Effect("feedback".to_owned()),
            });
        }

        for id in &ids {
            let text = id.to_string();
            assert_eq!(ParamId::from_str(&text).as_ref(), Ok(id), "{text}");
        }
    }

    /// Ids a binding sends reach the engine through the router, not just the parser.
    #[test]
    fn a_binding_send_reaches_the_engine_under_its_deck_index() {
        let (engine, _audio) = DjEngine::new(2);
        let engine = Arc::new(engine);
        let router = Arc::new(om_core::ParameterRouter::new());
        engine.register(&router);
        let binding = DeckBinding::new(Arc::clone(&engine), router.clone(), 1);
        assert_eq!(binding.id(DeckParam::Sync).to_string(), "deck.1.sync");
        assert!(binding.send(DeckParam::Sync, 1.0));
        assert!(engine.sync().is_enabled(1));
    }
}

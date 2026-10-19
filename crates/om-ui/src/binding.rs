//! What the widget models need: read access to the engine, and a write path. Writes go to the
//! router as typed [`Command::Param`]s when the binding has a [`CommandSink`] (the production
//! window). Without one, a binding writes through the string façade, which is what the widget
//! tests' recording rig uses. Both paths deliver the same [`ParamId`], so they stay equivalent.

use om_core::param_id::{ChannelParam, DeckParam, FxParam, FxRack, MixerParam};
use om_core::{
    Command, CommandSink, DeckSnapshot, ParamError, ParamId, ParameterRouter, ParameterTarget,
};
use om_dsp::{CrossfaderAssign, CrossfaderCurve};
use om_engine::deck::Deck;
use om_engine::{DeckControl, DjEngine, FxRouting, MixerControl, Sync};
use std::sync::Arc;

/// Delivers one id: a typed command when a sink is present, else the string target. An id that
/// does not validate is never sent.
fn deliver(
    sink: Option<&Arc<dyn CommandSink>>,
    target: &Arc<dyn ParameterTarget>,
    id: ParamId,
    value: f64,
) -> bool {
    match sink {
        Some(sink) => Command::param(id, value).is_ok_and(|cmd| sink.send(cmd).is_ok()),
        None => id.validate().is_ok() && target.set_parameter(&id.to_string(), value),
    }
}

/// Routes one deck's parameters. Cheap to clone.
#[derive(Clone)]
pub struct DeckBinding {
    pub engine: Arc<DjEngine>,
    /// Typed writes. `None` falls back to `target`.
    pub sink: Option<Arc<dyn CommandSink>>,
    /// String writes, used when there is no sink.
    pub target: Arc<dyn ParameterTarget>,
    pub index: usize,
}

impl DeckBinding {
    /// A binding that writes through the string façade (test rigs).
    pub fn new(engine: Arc<DjEngine>, target: Arc<dyn ParameterTarget>, index: usize) -> Self {
        Self {
            engine,
            sink: None,
            target,
            index,
        }
    }

    /// A binding that sends typed commands to `router` (the production window).
    pub fn typed(engine: Arc<DjEngine>, router: Arc<ParameterRouter>, index: usize) -> Self {
        Self {
            engine,
            sink: Some(router.clone()),
            target: router,
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

    /// This deck's state, copied out. Display reads use this instead of the getters: one copy
    /// gives a consistent set of fields for a frame, and it does not allocate.
    pub fn state(&self) -> DeckSnapshot {
        self.engine.deck_snapshot(self.index)
    }

    /// The typed id of one of this deck's parameters. Fails when the index does not fit a deck id.
    pub fn id(&self, p: DeckParam) -> Result<ParamId, ParamError> {
        let deck = u8::try_from(self.index).map_err(|_| ParamError::OutOfRange {
            id: format!("deck.{}", self.index),
            what: "deck",
            max: u16::from(om_core::param_id::MAX_DECKS) - 1,
        })?;
        let id = ParamId::Deck { deck, p };
        id.validate()?;
        Ok(id)
    }

    /// Sends one of this deck's parameters. `false` when the id is invalid or the router does not
    /// handle it.
    pub fn send(&self, p: DeckParam, value: f64) -> bool {
        match self.id(p) {
            Ok(id) => deliver(self.sink.as_ref(), &self.target, id, value),
            Err(_) => false,
        }
    }
}

/// Routes the mixer and FX parameters. Cheap to clone.
#[derive(Clone)]
pub struct MixerBinding {
    pub engine: Arc<DjEngine>,
    /// Typed writes. `None` falls back to `target`.
    pub sink: Option<Arc<dyn CommandSink>>,
    /// String writes, used when there is no sink.
    pub target: Arc<dyn ParameterTarget>,
}

impl MixerBinding {
    /// A binding that writes through the string façade (test rigs).
    pub fn new(engine: Arc<DjEngine>, target: Arc<dyn ParameterTarget>) -> Self {
        Self {
            engine,
            sink: None,
            target,
        }
    }

    /// A binding that sends typed commands to `router` (the production window).
    pub fn typed(engine: Arc<DjEngine>, router: Arc<ParameterRouter>) -> Self {
        Self {
            engine,
            sink: Some(router.clone()),
            target: router,
        }
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
        deliver(self.sink.as_ref(), &self.target, ParamId::Mixer(p), value)
    }

    /// Sends one parameter of an FX rack or unit (`fx.*`).
    pub fn send_fx(&self, rack: FxRack, unit: Option<u16>, p: FxParam, value: f64) -> bool {
        deliver(
            self.sink.as_ref(),
            &self.target,
            ParamId::Fx { rack, unit, p },
            value,
        )
    }

    /// Sends one channel parameter (`mixer.ch<c>.*`). `false` when `channel` does not fit a channel
    /// id or the router does not handle it.
    pub fn send_channel(&self, channel: usize, p: ChannelParam, value: f64) -> bool {
        match u8::try_from(channel) {
            Ok(ch) => deliver(
                self.sink.as_ref(),
                &self.target,
                ParamId::Mixer(MixerParam::Channel { ch, p }),
                value,
            ),
            Err(_) => false,
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

    /// The display read path: a binding's state matches the engine getters it replaces.
    #[test]
    fn deck_state_matches_the_getters_it_replaces() {
        let (engine, _audio) = DjEngine::new(2);
        let engine = Arc::new(engine);
        let router = Arc::new(om_core::ParameterRouter::new());
        engine.register(&router);
        let binding = DeckBinding::new(Arc::clone(&engine), router.clone(), 1);
        engine.deck(1).play();
        engine.deck(1).set_keylock(true);
        engine.deck(1).set_hotcue(4, 900);
        engine.deck_control(1).set_quantize(false);
        let state = binding.state();
        assert!(state.playing && state.keylock && !state.quantize && !state.loop_valid());
        assert_eq!(state.rate, engine.deck(1).rate());
        assert_eq!(state.hotcues[4], engine.deck(1).hotcue(4));
        assert_eq!(state.hotcues[3], None);
        assert_eq!(state.sync, engine.sync().is_enabled(1));
        assert_eq!(state.slip, engine.deck_control(1).is_slipping());
    }

    /// Ids a binding sends reach the engine through the router, not just the parser.
    #[test]
    fn a_binding_send_reaches_the_engine_under_its_deck_index() {
        let (engine, _audio) = DjEngine::new(2);
        let engine = Arc::new(engine);
        let router = Arc::new(om_core::ParameterRouter::new());
        engine.register(&router);
        let binding = DeckBinding::new(Arc::clone(&engine), router.clone(), 1);
        assert_eq!(
            binding.id(DeckParam::Sync).unwrap().to_string(),
            "deck.1.sync"
        );
        assert!(binding.send(DeckParam::Sync, 1.0));
        assert!(engine.sync().is_enabled(1));
    }

    /// Forwards every typed command to the router and keeps a copy of it.
    struct RecordingSink {
        router: Arc<ParameterRouter>,
        commands: std::sync::Mutex<Vec<Command>>,
    }

    impl CommandSink for RecordingSink {
        fn send(&self, cmd: Command) -> Result<(), om_core::CommandError> {
            self.commands.lock().unwrap().push(cmd.clone());
            self.router.send(cmd)
        }
    }

    /// Fails the test if a write reaches the string path.
    struct NoText;

    impl ParameterTarget for NoText {
        fn set_parameter(&self, id: &str, _value: f64) -> bool {
            panic!("a typed binding wrote text {id:?}");
        }
    }

    /// A typed binding sends `Command::Param` with the parsed id and never formats a string.
    #[test]
    fn a_typed_binding_sends_param_commands_without_text() {
        let (engine, _audio) = DjEngine::new(2);
        let engine = Arc::new(engine);
        let router = Arc::new(om_core::ParameterRouter::new());
        engine.register(&router);
        let sink = Arc::new(RecordingSink {
            router: Arc::clone(&router),
            commands: std::sync::Mutex::new(Vec::new()),
        });
        let deck = DeckBinding {
            engine: Arc::clone(&engine),
            sink: Some(sink.clone()),
            target: Arc::new(NoText),
            index: 1,
        };
        assert!(deck.send(DeckParam::Sync, 1.0));
        assert!(engine.sync().is_enabled(1));

        let mixer = MixerBinding {
            engine: Arc::clone(&engine),
            sink: Some(sink.clone()),
            target: Arc::new(NoText),
        };
        assert!(mixer.send_channel(0, ChannelParam::Fader, 0.5));
        assert!(mixer.send(MixerParam::Master, 0.25));

        let sent = sink.commands.lock().unwrap().clone();
        assert_eq!(
            sent,
            vec![
                Command::Param {
                    id: ParamId::Deck {
                        deck: 1,
                        p: DeckParam::Sync
                    },
                    value: 1.0,
                },
                Command::Param {
                    id: ParamId::Mixer(MixerParam::Channel {
                        ch: 0,
                        p: ChannelParam::Fader
                    }),
                    value: 0.5,
                },
                Command::Param {
                    id: ParamId::Mixer(MixerParam::Master),
                    value: 0.25,
                },
            ]
        );
    }

    /// A typed write the router does not handle reports `false`, as the string path does.
    #[test]
    fn a_typed_write_nobody_handles_reports_false() {
        let (engine, _audio) = DjEngine::new(2);
        let engine = Arc::new(engine);
        // No engine targets registered: every typed id is rejected.
        let router = Arc::new(om_core::ParameterRouter::new());
        let deck = DeckBinding::typed(Arc::clone(&engine), router, 0);
        assert!(!deck.send(DeckParam::Play, 1.0));
    }

    /// Ids that do not validate never reach the sink or the string target.
    #[test]
    fn a_binding_refuses_ids_that_do_not_validate() {
        let (engine, _audio) = DjEngine::new(2);
        let engine = Arc::new(engine);
        let router = Arc::new(om_core::ParameterRouter::new());
        engine.register(&router);
        let sink = Arc::new(RecordingSink {
            router: Arc::clone(&router),
            commands: std::sync::Mutex::new(Vec::new()),
        });
        let mixer = MixerBinding {
            engine: Arc::clone(&engine),
            sink: Some(sink.clone()),
            target: Arc::new(NoText),
        };
        assert!(!mixer.send_fx(FxRack::Master, Some(1000), FxParam::Enable, 1.0));
        assert!(!mixer.send_fx(FxRack::Master, None, FxParam::Mix, 1.0));
        assert!(!mixer.send_channel(4, ChannelParam::Fader, 1.0));
        let deck = DeckBinding {
            engine: Arc::clone(&engine),
            sink: Some(sink.clone()),
            target: Arc::new(NoText),
            index: 4,
        };
        assert!(!deck.send(DeckParam::Play, 1.0));
        assert!(sink.commands.lock().unwrap().is_empty());

        // The string path validates too: a hand-built id is refused before it is printed.
        let text_only = MixerBinding::new(Arc::clone(&engine), Arc::new(Silent));
        assert!(!text_only.send_fx(FxRack::Master, None, FxParam::Mix, 1.0));
    }

    /// Accepts every write and reports it handled; used where a refusal must come before it.
    struct Silent;

    impl ParameterTarget for Silent {
        fn set_parameter(&self, _id: &str, _value: f64) -> bool {
            true
        }
    }
}

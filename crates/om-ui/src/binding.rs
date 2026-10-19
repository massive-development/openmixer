//! What the widget models need: read access to the engine, and a write path that goes only through
//! the string-id control surface (`deck.<N>.<id>`, `mixer.<id>`, `fx.<id>`), so UI and MIDI stay
//! equivalent.

use om_core::ParameterTarget;
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

    /// Sends `deck.<index>.<id>`. `false` when the router does not know the id.
    pub fn send(&self, id: &str, value: f64) -> bool {
        self.target
            .set_parameter(&format!("deck.{}.{id}", self.index), value)
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

    /// Sends `mixer.<id>`.
    pub fn send(&self, id: &str, value: f64) -> bool {
        self.target.set_parameter(&format!("mixer.{id}"), value)
    }

    /// Sends `fx.<id>`.
    pub fn send_fx(&self, id: &str, value: f64) -> bool {
        self.target.set_parameter(&format!("fx.{id}"), value)
    }

    /// `ch<N>.<id>` for one channel.
    pub fn channel_id(channel: usize, id: &str) -> String {
        format!("ch{channel}.{id}")
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

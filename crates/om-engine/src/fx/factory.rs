//! Name to effect-creator registry; built-ins are pre-registered and plugins may add more.
//! Control thread only.

use super::effect::Effect;
use super::{
    crush::CrushFx, echo::EchoFx, flanger::FlangerFx, gate::GateFx, phaser::PhaserFx,
    reverb::ReverbFx, roll::RollFx, tape_stop::TapeStopFx,
};
use std::collections::BTreeMap;

pub type Creator = Box<dyn Fn() -> Box<dyn Effect> + Send + Sync>;

const BUILT_IN_NAMES: [&str; 8] = [
    "echo", "reverb", "flanger", "phaser", "crush", "gate", "roll", "tapestop",
];

#[derive(Default)]
pub struct FxFactory {
    creators: BTreeMap<String, Creator>,
}

impl FxFactory {
    /// Factory with the eight built-in effects registered.
    pub fn with_built_ins() -> Self {
        let mut f = Self::default();
        f.register_effect("echo", Box::new(|| Box::new(EchoFx::default())));
        f.register_effect("reverb", Box::new(|| Box::new(ReverbFx::default())));
        f.register_effect("flanger", Box::new(|| Box::new(FlangerFx::default())));
        f.register_effect("phaser", Box::new(|| Box::new(PhaserFx::default())));
        f.register_effect("crush", Box::new(|| Box::new(CrushFx::default())));
        f.register_effect("gate", Box::new(|| Box::new(GateFx::default())));
        f.register_effect("roll", Box::new(|| Box::new(RollFx::default())));
        f.register_effect("tapestop", Box::new(|| Box::new(TapeStopFx::default())));
        f
    }

    /// Built-in names in their default rack order.
    pub fn builtin_names() -> &'static [&'static str] {
        &BUILT_IN_NAMES
    }

    /// Returns `false` if the name is empty or already taken.
    pub fn register_effect(&mut self, name: &str, creator: Creator) -> bool {
        if name.is_empty() || self.creators.contains_key(name) {
            return false;
        }
        self.creators.insert(name.to_owned(), creator);
        true
    }

    pub fn create(&self, name: &str) -> Option<Box<dyn Effect>> {
        self.creators.get(name).map(|make| make())
    }

    /// Sorted registry names.
    pub fn names(&self) -> Vec<String> {
        self.creators.keys().cloned().collect()
    }
}

//! FX rack: the `Effect` trait, the eight built-in effects, the per-slot `FxUnit`, the `FxRack`
//! series chain, the name registry and the built-in presets.

pub mod crush;
pub mod delay_line;
pub mod echo;
pub mod effect;
pub mod factory;
pub mod flanger;
pub mod gate;
pub mod math;
pub mod phaser;
pub mod presets;
pub mod rack;
pub mod reverb;
pub mod reverb_parts;
pub mod roll;
pub mod tape_stop;
pub mod unit;
pub mod wipe;

pub use crush::CrushFx;
pub use delay_line::DelayLine;
pub use echo::EchoFx;
pub use effect::{Effect, FxContext, ParamControl, ParamSet, ParamSpec, MAX_BPM, MIN_BPM};
pub use factory::FxFactory;
pub use flanger::FlangerFx;
pub use gate::GateFx;
pub use phaser::PhaserFx;
pub use presets::{apply_preset, builtin_presets, find_preset, FxPreset};
pub use rack::{FxRack, FxRackControl};
pub use reverb::ReverbFx;
pub use roll::RollFx;
pub use tape_stop::TapeStopFx;
pub use unit::{FxUnit, FxUnitControl};

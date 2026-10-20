#![forbid(unsafe_code)]

//! Automix helpers. Pure control-side code: nothing here touches the audio thread or the engine's
//! internals. Plans are values; a runner applies them through a [`om_core::CommandSink`] on the
//! control thread.
//!
//! - [`stem_fade`]: stem ramps (volume, filter, mute) and the standard transition moves built from
//!   them (bass swap, vocals out, drums-only bridge, filter sweep).

pub mod stem_fade;

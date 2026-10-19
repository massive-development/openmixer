//! Parameter cells written by the control thread and read by the audio thread, plus the decaying
//! meters the audio thread writes. Shared by `Arc` between [`super::Mixer`] and [`super::MixerControl`].

use om_core::atomic::AtomicF32;
use om_dsp::{CrossfaderAssign, PeakMeter, ThreeBandEq};
use std::sync::atomic::{AtomicBool, AtomicU8};

pub const MAX_CHANNELS: usize = 4;
/// Master and cue level ceiling.
pub const MAX_GAIN: f32 = 2.0;
/// Trim ceiling.
pub const MAX_TRIM: f32 = 4.0;

pub struct ChannelShared {
    pub trim: AtomicF32,
    pub fader: AtomicF32,
    pub pfl: AtomicBool,
    pub assign: AtomicU8,
    pub eq: [AtomicF32; 3],
    pub filter: AtomicF32,
    pub meter_left: PeakMeter,
    pub meter_right: PeakMeter,
}

impl ChannelShared {
    fn new() -> Self {
        Self {
            trim: AtomicF32::new(1.0),
            fader: AtomicF32::new(1.0),
            pfl: AtomicBool::new(false),
            assign: AtomicU8::new(assign_to_u8(CrossfaderAssign::Thru)),
            eq: std::array::from_fn(|_| AtomicF32::new(0.0)),
            filter: AtomicF32::new(0.0),
            meter_left: PeakMeter::default(),
            meter_right: PeakMeter::default(),
        }
    }
}

pub struct MixerShared {
    pub channels: Vec<ChannelShared>,
    pub crossfader: AtomicF32,
    pub constant_power: AtomicBool,
    pub master_gain: AtomicF32,
    pub cue_level: AtomicF32,
    pub cue_mix: AtomicF32,
    pub master_peak_left: PeakMeter,
    pub master_peak_right: PeakMeter,
}

impl MixerShared {
    pub fn new(num_channels: usize) -> Self {
        Self {
            channels: (0..num_channels).map(|_| ChannelShared::new()).collect(),
            // Crossfader defaults (centre, constant power) and unity master.
            crossfader: AtomicF32::new(0.5),
            constant_power: AtomicBool::new(true),
            master_gain: AtomicF32::new(1.0),
            cue_level: AtomicF32::new(1.0),
            cue_mix: AtomicF32::new(0.0),
            master_peak_left: PeakMeter::default(),
            master_peak_right: PeakMeter::default(),
        }
    }
}

pub fn assign_to_u8(assign: CrossfaderAssign) -> u8 {
    match assign {
        CrossfaderAssign::Thru => 0,
        CrossfaderAssign::Left => 1,
        CrossfaderAssign::Right => 2,
    }
}

pub fn assign_from_u8(value: u8) -> CrossfaderAssign {
    match value {
        1 => CrossfaderAssign::Left,
        2 => CrossfaderAssign::Right,
        _ => CrossfaderAssign::Thru,
    }
}

/// EQ band gain limits, shared with the DSP unit.
pub const MIN_EQ_DB: f32 = ThreeBandEq::MIN_DB;
pub const MAX_EQ_DB: f32 = ThreeBandEq::MAX_DB;

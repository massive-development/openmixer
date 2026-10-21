//! Reads the value of a control id out of an [`EngineSnapshot`], for the ids automix drives or
//! watches. `None` for ids the snapshot does not carry (FX racks, sync leader, trigger-style ids).

use om_core::param_id::{ChannelParam, DeckParam, EqBand, MixerParam};
use om_core::{EngineSnapshot, ParamId};

fn flag(on: bool) -> f64 {
    if on {
        1.0
    } else {
        0.0
    }
}

/// The current value of `id`, in the units the id takes.
pub fn read_param(snap: &EngineSnapshot, id: &ParamId) -> Option<f64> {
    match id {
        ParamId::Mixer(MixerParam::Crossfader) => Some(f64::from(snap.mixer.crossfader)),
        ParamId::Mixer(MixerParam::Master) => Some(f64::from(snap.mixer.master_gain)),
        ParamId::Mixer(MixerParam::Channel { ch, p }) => {
            let c = snap.mixer.channels.get(usize::from(*ch))?;
            match p {
                ChannelParam::Trim => Some(f64::from(c.trim)),
                ChannelParam::Fader => Some(f64::from(c.fader)),
                ChannelParam::Filter => Some(f64::from(c.filter)),
                ChannelParam::Pfl => Some(flag(c.pfl)),
                ChannelParam::Eq(band) => Some(f64::from(
                    c.eq[match band {
                        EqBand::Low => 0,
                        EqBand::Mid => 1,
                        EqBand::High => 2,
                    }],
                )),
                ChannelParam::Xfader => Some(f64::from(c.assign.index())),
            }
        }
        ParamId::Deck { deck, p } => {
            let d = snap.decks.get(usize::from(*deck))?;
            match p {
                DeckParam::Play => Some(flag(d.playing)),
                DeckParam::Sync => Some(flag(d.sync)),
                DeckParam::SyncMaster => Some(flag(d.sync_master)),
                DeckParam::Rate => Some(d.rate),
                DeckParam::Keylock => Some(flag(d.keylock)),
                DeckParam::Slip => Some(flag(d.slip)),
                DeckParam::Quantize => Some(flag(d.quantize)),
                _ => None,
            }
        }
        ParamId::Stem { deck, stem, p } => snap
            .stems
            .get(usize::from(*deck))
            .and_then(|s| s.get(usize::from(*stem)))
            .map(|s| s.read(p)),
        _ => None,
    }
}

/// A blank snapshot for `decks` decks and as many mixer channels, every deck loaded and stopped
/// with no stems. For tests of this crate and of hosts that fake an engine.
pub fn blank_snapshot(decks: usize) -> EngineSnapshot {
    use om_core::{
        AutomixSnapshot, ChannelSnapshot, DeckSnapshot, MixerSnapshot, SamplerSnapshot,
        StemSnapshot,
    };
    EngineSnapshot {
        ticks: 0,
        built: 1,
        decks: (0..decks)
            .map(|_| DeckSnapshot {
                loaded: true,
                rate: 1.0,
                file_sample_rate: 44_100.0,
                length_frames: 44_100 * 600,
                ..DeckSnapshot::default()
            })
            .collect(),
        mixer: MixerSnapshot {
            channels: vec![
                ChannelSnapshot {
                    trim: 1.0,
                    fader: 1.0,
                    ..ChannelSnapshot::default()
                };
                decks
            ],
            crossfader: 0.0,
            master_gain: 1.0,
            ..MixerSnapshot::default()
        },
        stems: (0..decks).map(|_| [StemSnapshot::default(); 4]).collect(),
        sampler: SamplerSnapshot::default(),
        automix: AutomixSnapshot::default(),
    }
}

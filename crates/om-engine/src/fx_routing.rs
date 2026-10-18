//! FX racks for the signal path: one per mixer channel plus one on master, each inserted at slot 0
//! of its chain. One `ParameterTarget` (router prefix "fx") reaches all of them.
//!
//! Ids: `master.<rack id>` and `ch<N>.<rack id>`, where `<rack id>` is an `FxRack` id ("bpm",
//! "fx<N>.enable", "fx<N>.mix", "fx<N>.<effect param>"), e.g. "master.fx1.mix", "ch0.fx2.enable".
//!
//! Not yet ported: `followDecks`, which needs `Deck` and `Sync` (Task 4).

use crate::fx::{FxRack, FxRackControl};
use crate::mixer::MixerControl;
use om_core::ParameterTarget;

pub const INSERT_SLOT: usize = 0;

pub struct FxRouting {
    channel_racks: Vec<FxRackControl>,
    master_rack: FxRackControl,
}

impl FxRouting {
    /// Creates one rack per channel and one for master, and inserts each into `mixer`. Control thread.
    pub fn new(mixer: &MixerControl) -> Self {
        let mut channel_racks = Vec::with_capacity(mixer.num_channels());
        for ch in 0..mixer.num_channels() {
            let rack = FxRack::default();
            channel_racks.push(rack.control());
            mixer.set_channel_insert(ch, INSERT_SLOT, Some(Box::new(rack)));
        }
        let rack = FxRack::default();
        let master_rack = rack.control();
        mixer.set_master_insert(INSERT_SLOT, Some(Box::new(rack)));
        Self {
            channel_racks,
            master_rack,
        }
    }

    pub fn channel_rack(&self, ch: usize) -> Option<&FxRackControl> {
        self.channel_racks.get(ch)
    }

    pub fn master_rack(&self) -> &FxRackControl {
        &self.master_rack
    }

    fn find_rack(&self, name: &str) -> Option<&FxRackControl> {
        if name == "master" {
            return Some(&self.master_rack);
        }
        let digits = name.strip_prefix("ch")?;
        if digits.is_empty() || digits.len() > 3 || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        self.channel_racks.get(digits.parse::<usize>().ok()?)
    }
}

impl ParameterTarget for FxRouting {
    fn set_parameter(&self, id: &str, value: f64) -> bool {
        let Some((name, rest)) = id.split_once('.') else {
            return false;
        };
        if name.is_empty() {
            return false;
        }
        self.find_rack(name)
            .is_some_and(|rack| rack.set_parameter(rest, value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mixer::Mixer;

    #[test]
    fn ids_reach_the_channel_and_master_racks_and_bad_prefixes_are_rejected() {
        let (_mixer, ctl) = Mixer::new(2);
        let routing = FxRouting::new(&ctl);
        assert!(routing.set_parameter("ch0.fx1.mix", 0.4));
        assert_eq!(routing.channel_rack(0).unwrap().num_units(), 8);
        assert_eq!(routing.channel_rack(0).unwrap().unit(1).unwrap().mix(), 0.4);
        assert!(routing.set_parameter("master.fx2.enable", 1.0));
        assert!(routing.master_rack().unit(2).unwrap().is_enabled());
        assert!(
            !routing.set_parameter("ch2.fx1.mix", 0.4),
            "channel out of range"
        );
        assert!(!routing.set_parameter("chx.fx1.mix", 0.4));
        assert!(!routing.set_parameter(".fx1.mix", 0.4));
        assert!(!routing.set_parameter("master", 0.4));
    }
}

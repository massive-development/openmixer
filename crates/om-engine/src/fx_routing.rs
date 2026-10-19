//! FX racks for the signal path: one per mixer channel plus one on master, each inserted at slot 0
//! of its chain. One `ParameterTarget` (router prefix "fx") reaches all of them.
//!
//! Ids: `master.<rack id>` and `ch<N>.<rack id>`, where `<rack id>` is an `FxRack` id ("bpm",
//! "fx<N>.enable", "fx<N>.mix", "fx<N>.<effect param>"), e.g. "master.fx1.mix", "ch0.fx2.enable".
//! Port of `src/engine/FxRouting.*`.

use crate::deck::Deck;
use crate::fx::{FxRack, FxRackControl};
use crate::mixer::MixerControl;
use crate::sync::Sync;
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

    /// Feeds each rack the tempo of its deck (grid bpm x rate). Master follows the sync leader,
    /// else the first deck that has a grid. Racks without a grid keep their last tempo. A deck
    /// with a grid overwrites a manually set `bpm` on every call. Control thread, about 30 Hz.
    pub fn follow_decks(&self, decks: &[Option<&Deck>], sync: &Sync) {
        let count = decks.len();
        for (d, rack) in self.channel_racks.iter().enumerate().take(count) {
            if let (Some(grid), Some(Some(deck))) = (sync.grid(d), decks.get(d)) {
                rack.set_tempo_from_grid(grid.as_ref(), deck.rate());
            }
        }
        if let Some(source) = master_source_deck(sync, count) {
            if let (Some(grid), Some(Some(deck))) = (sync.grid(source), decks.get(source)) {
                self.master_rack
                    .set_tempo_from_grid(grid.as_ref(), deck.rate());
            }
        }
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

/// Deck whose grid drives master: the sync leader when it has a grid, else the first deck with one.
fn master_source_deck(sync: &Sync, num_decks: usize) -> Option<usize> {
    if let Some(leader) = sync.leader() {
        if leader < num_decks && sync.grid(leader).is_some() {
            return Some(leader);
        }
    }
    (0..num_decks).find(|&d| sync.grid(d).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::FixedTempoGrid;
    use crate::mixer::Mixer;
    use std::sync::Arc;

    const RATE: f64 = 44_100.0;

    #[test]
    fn follow_decks_matches_engine_wiring_tempo_numbers() {
        let (_mixer, ctl) = Mixer::new(2);
        let routing = FxRouting::new(&ctl);
        let sync = Sync::new(2);
        sync.set_grid(0, Some(Arc::new(FixedTempoGrid::new(120.0, RATE, 0))));
        sync.set_grid(1, Some(Arc::new(FixedTempoGrid::new(100.0, RATE, 0))));
        let (deck0, _a0) = Deck::new(1 << 12);
        let (deck1, _a1) = Deck::new(1 << 12);
        deck0.set_rate(1.1);
        let decks = [Some(&deck0), Some(&deck1)];

        routing.follow_decks(&decks, &sync);
        let tempo = |rack: &FxRackControl| rack.unit(0).unwrap().tempo();
        assert!((tempo(routing.channel_rack(0).unwrap()) - 132.0).abs() < 1e-9);
        assert!((tempo(routing.channel_rack(1).unwrap()) - 100.0).abs() < 1e-9);
        assert!(
            (tempo(routing.master_rack()) - 132.0).abs() < 1e-9,
            "master follows first deck with a grid"
        );

        sync.set_leader(Some(1));
        routing.follow_decks(&decks, &sync);
        assert!(
            (tempo(routing.master_rack()) - 100.0).abs() < 1e-9,
            "master follows the sync leader"
        );
    }

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

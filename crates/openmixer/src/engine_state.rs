//! The engine as a [`StateReader`]: the ids a controller map's feedback rules can read. A read
//! parses the id into a [`StateId`] and looks it up in a fresh [`EngineSnapshot`], so the values
//! are the same ones the UI reads. Snapshots are built on the control thread, never the audio one.
//!
//! ```text
//! deck.<N>.play | loaded | keylock | loop | cue (loaded and stopped) | sync | sync.master
//! deck.<N>.slip | quantize | rate | position (0-1)
//! deck.<N>.hotcue<K>.set (1 when the cue exists)
//! mixer.ch<N>.pfl | fader | trim | filter | level (decaying peak)
//! mixer.crossfader | mixer.master (gain) | mixer.master.level (decaying peak, louder side)
//! ```

use om_core::{StateId, StateKey, StateReader};
use om_engine::DjEngine;
use std::sync::Arc;

pub struct EngineStateReader {
    engine: Arc<DjEngine>,
}

impl EngineStateReader {
    pub fn new(engine: Arc<DjEngine>) -> Self {
        Self { engine }
    }
}

impl StateReader for EngineStateReader {
    fn read_state(&self, id: &str) -> Option<f64> {
        let id: StateId = id.parse().ok()?;
        self.engine.snapshot().read(&id)
    }

    /// One snapshot for the whole pass: every rule reads the same capture, and the ids were parsed
    /// when the keys were made.
    fn read_states(&self, keys: &[StateKey], out: &mut [Option<f64>]) {
        if keys.is_empty() {
            return;
        }
        let snapshot = self.engine.snapshot();
        for (key, slot) in keys.iter().zip(out.iter_mut()) {
            *slot = key.state_id().and_then(|id| snapshot.read(id));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use om_core::StateId;

    /// The same read through the string adapter and through a snapshot; unknown ids read `None`.
    #[test]
    fn reports_deck_and_mixer_state_by_id_and_rejects_unknown_ids() {
        let (engine, _audio) = DjEngine::new(2);
        let engine = Arc::new(engine);
        let reader = EngineStateReader::new(Arc::clone(&engine));
        assert_eq!(reader.read_state("deck.0.play"), Some(0.0));
        engine.deck(0).play();
        assert_eq!(reader.read_state("deck.0.play"), Some(1.0));

        engine.mixer().set_pfl(1, true);
        assert_eq!(reader.read_state("mixer.ch1.pfl"), Some(1.0));
        engine.deck_control(0).set_slip(true);
        assert_eq!(reader.read_state("deck.0.slip"), Some(1.0));
        engine.deck(1).set_hotcue(2, 1000);
        assert_eq!(reader.read_state("deck.1.hotcue2.set"), Some(1.0));
        assert_eq!(reader.read_state("deck.1.hotcue3.set"), Some(0.0));
        engine.sync().set_leader(Some(1));
        assert_eq!(reader.read_state("deck.1.sync.master"), Some(1.0));
        for id in [
            "mixer.master.level",
            "mixer.crossfader",
            "mixer.ch0.level",
            "deck.0.cue",
            "deck.0.loop",
            "deck.0.position",
        ] {
            assert!(reader.read_state(id).is_some(), "{id}");
        }

        for id in [
            "",
            "deck",
            "deck.99.play",
            "deck.0.bogus",
            "deck.x.play",
            "deck..play",
            "mixer.ch99.pfl",
            "mixer.ch0.bogus",
            "mixer.ch0",
            "nope.0.play",
            "deck.0.hotcuex.set",
        ] {
            assert_eq!(reader.read_state(id), None, "{id}");
        }
    }

    /// A feedback pass over many rules takes one snapshot, not one per rule.
    #[test]
    fn a_pass_over_many_keys_builds_one_snapshot() {
        let (engine, _audio) = DjEngine::new(2);
        let engine = Arc::new(engine);
        let reader = EngineStateReader::new(Arc::clone(&engine));
        let keys: Vec<StateKey> = [
            "deck.0.play",
            "deck.1.play",
            "deck.0.hotcue1.set",
            "mixer.ch0.level",
            "mixer.ch1.fader",
            "mixer.master.level",
            "plugin.thing",
        ]
        .iter()
        .map(|id| StateKey::new(id))
        .collect();
        let mut out = vec![None; keys.len()];

        let before = engine.snapshots_built();
        reader.read_states(&keys, &mut out);
        assert_eq!(engine.snapshots_built() - before, 1);
        assert_eq!(out[0], Some(0.0));
        assert!(out[3].is_some());
        assert_eq!(out[6], None, "an id that is not engine state reads None");

        let before = engine.snapshots_built();
        reader.read_states(&[], &mut []);
        assert_eq!(engine.snapshots_built() - before, 0, "no keys, no snapshot");
    }

    /// Snapshot form of the same cases: parsed ids read from one snapshot.
    #[test]
    fn a_snapshot_answers_the_same_ids_as_the_string_reader() {
        let (engine, _audio) = DjEngine::new(2);
        engine.deck(0).play();
        engine.mixer().set_pfl(1, true);
        engine.deck(1).set_hotcue(2, 1000);
        engine.sync().set_leader(Some(1));
        let snap = engine.snapshot();
        let reader = EngineStateReader::new(Arc::new(engine));
        for id in [
            "deck.0.play",
            "deck.0.loaded",
            "deck.0.cue",
            "deck.0.position",
            "mixer.ch1.pfl",
            "deck.1.hotcue2.set",
            "deck.1.sync.master",
            "mixer.master.level",
            "deck.0.bogus",
        ] {
            let parsed: Option<StateId> = id.parse().ok();
            let from_snapshot = parsed.and_then(|p| snap.read(&p));
            assert_eq!(from_snapshot, reader.read_state(id), "{id}");
        }
        assert_eq!(snap.read(&"deck.0.play".parse().unwrap()), Some(1.0));
    }
}

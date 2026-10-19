//! The engine's control clock runs in the composition, not in a window: sync rate changes land
//! with no UI and no audio rendering.

use om_core::ParameterTarget;
use om_engine::grid::{BeatGrid, FixedTempoGrid};
use openmixer::composition::AppComposition;
use std::sync::Arc;
use std::time::Duration;

#[test]
fn sync_follows_the_leader_without_a_ui_or_rendering() {
    let scratch = tempfile::tempdir().expect("tempdir");
    let c = AppComposition::new(&scratch.path().join("prefs.json"), None);
    let rate = 48_000.0;
    let leader: Arc<dyn BeatGrid> = Arc::new(FixedTempoGrid::new(120.0, rate, 0));
    let follower: Arc<dyn BeatGrid> = Arc::new(FixedTempoGrid::new(100.0, rate, 0));
    c.engine.set_grid(0, Some(leader));
    c.engine.set_grid(1, Some(follower));
    assert!(c.router.set_parameter("deck.0.sync.master", 1.0));
    assert!(c.router.set_parameter("deck.1.sync", 1.0));
    c.engine.deck(0).play();
    c.engine.deck(1).play();

    // Nothing calls update_control here: the control thread must bring the follower to 120 bpm.
    om_testing::assert_eventually!(
        Duration::from_secs(5),
        || (c.engine.deck(1).rate() - 1.2).abs() <= 1e-9,
        "the follower never synced; rate {}",
        c.engine.deck(1).rate()
    );
    assert_eq!(c.engine.deck(0).rate(), 1.0);
}

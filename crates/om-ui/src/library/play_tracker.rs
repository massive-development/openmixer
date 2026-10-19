//! Decides when a deck's track counts as played: 30 s of playing time on the deck since the track
//! was loaded. Control thread only; it reads deck snapshots and never touches the audio thread.

use std::time::Duration;

use om_core::DeckSnapshot;

/// Playing time after which a load counts as one play.
pub const PLAY_THRESHOLD: Duration = Duration::from_secs(30);

/// Largest slice of time credited for one observation. A stalled UI (a dragged window, a debugger)
/// must not credit the whole stall as playing time.
pub const MAX_STEP: Duration = Duration::from_secs(1);

/// Per-deck play detection. [`Self::reset`] on every load; [`Self::observe`] once per tick.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PlayTracker {
    track_id: Option<i64>,
    playing_for: Duration,
    recorded: bool,
}

impl PlayTracker {
    /// Starts counting for a freshly loaded track. `None` stops counting.
    pub fn reset(&mut self, track_id: Option<i64>) {
        self.track_id = track_id;
        self.playing_for = Duration::ZERO;
        self.recorded = false;
    }

    /// The track being counted, if any.
    pub fn track_id(&self) -> Option<i64> {
        self.track_id
    }

    /// Feeds one tick: the deck's snapshot and the wall time since the previous tick. Returns the
    /// track id exactly once per load, when the threshold is first reached.
    pub fn observe(&mut self, deck: &DeckSnapshot, elapsed: Duration) -> Option<i64> {
        let track_id = self.track_id?;
        if !deck.loaded {
            self.reset(None);
            return None;
        }
        if self.recorded || !deck.playing {
            return None;
        }
        self.playing_for += elapsed.min(MAX_STEP);
        if self.playing_for >= PLAY_THRESHOLD {
            self.recorded = true;
            return Some(track_id);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deck(loaded: bool, playing: bool) -> DeckSnapshot {
        DeckSnapshot {
            loaded,
            playing,
            ..DeckSnapshot::default()
        }
    }

    /// Feeds `ticks` observations of `step` each, returning the ids reported.
    fn run(tracker: &mut PlayTracker, snap: &DeckSnapshot, ticks: u32, step: Duration) -> Vec<i64> {
        (0..ticks)
            .filter_map(|_| tracker.observe(snap, step))
            .collect()
    }

    #[test]
    fn records_once_after_thirty_seconds_of_playing() {
        let mut t = PlayTracker::default();
        t.reset(Some(7));
        let playing = deck(true, true);
        let hits = run(&mut t, &playing, 80, Duration::from_millis(500));
        assert_eq!(hits, vec![7], "one play, on the 60th half-second");
    }

    #[test]
    fn paused_time_does_not_count() {
        let mut t = PlayTracker::default();
        t.reset(Some(1));
        assert!(run(&mut t, &deck(true, false), 100, Duration::from_secs(1)).is_empty());
        // 29 s playing, a pause, then 1 s more reaches the 30 s threshold.
        assert!(run(&mut t, &deck(true, true), 29, Duration::from_secs(1)).is_empty());
        assert!(run(&mut t, &deck(true, false), 10, Duration::from_secs(1)).is_empty());
        assert_eq!(
            run(&mut t, &deck(true, true), 1, Duration::from_secs(1)),
            vec![1]
        );
    }

    #[test]
    fn a_stalled_tick_is_capped() {
        let mut t = PlayTracker::default();
        t.reset(Some(2));
        // One observation of a 10 minute stall credits only MAX_STEP.
        assert!(t
            .observe(&deck(true, true), Duration::from_secs(600))
            .is_none());
        assert_eq!(t.playing_for, MAX_STEP);
    }

    #[test]
    fn reset_on_load_starts_the_count_again() {
        let mut t = PlayTracker::default();
        t.reset(Some(1));
        assert_eq!(
            run(&mut t, &deck(true, true), 30, Duration::from_secs(1)),
            vec![1]
        );
        t.reset(Some(2));
        assert!(run(&mut t, &deck(true, true), 29, Duration::from_secs(1)).is_empty());
        assert_eq!(
            run(&mut t, &deck(true, true), 1, Duration::from_secs(1)),
            vec![2]
        );
    }

    #[test]
    fn a_second_load_of_the_same_track_counts_again() {
        let mut t = PlayTracker::default();
        t.reset(Some(5));
        assert_eq!(
            run(&mut t, &deck(true, true), 30, Duration::from_secs(1)),
            vec![5]
        );
        t.reset(Some(5));
        assert_eq!(
            run(&mut t, &deck(true, true), 30, Duration::from_secs(1)),
            vec![5]
        );
    }

    #[test]
    fn unloading_stops_counting_and_nothing_is_recorded() {
        let mut t = PlayTracker::default();
        t.reset(Some(3));
        assert!(run(&mut t, &deck(true, true), 20, Duration::from_secs(1)).is_empty());
        assert!(t
            .observe(&deck(false, false), Duration::from_secs(1))
            .is_none());
        assert_eq!(t.track_id(), None);
        assert!(run(&mut t, &deck(true, true), 60, Duration::from_secs(1)).is_empty());
    }

    #[test]
    fn no_track_means_no_play() {
        let mut t = PlayTracker::default();
        assert!(run(&mut t, &deck(true, true), 60, Duration::from_secs(1)).is_empty());
    }
}

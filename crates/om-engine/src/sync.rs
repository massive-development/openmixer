//! Beat sync: matches a follower deck's tempo to a leader and nudges its phase onto the leader's
//! beat (or bar). `step` and `apply` run on a control timer (~20-50 Hz); setters may come from any
//! non-audio thread (MIDI). Rates reach the audio thread through `Deck::set_rate`.
//! Params (`ParamId::Sync`): `leader` (deck index, <0 or out of range for none), `mode` (0 beat,
//! 1 bar), `<deck>.sync` (>= 0.5 on).

use crate::deck::Deck;
use crate::grid::BeatGrid;
use om_core::param_id::SyncParam;
use om_core::sync::LockExt;
use om_core::{ParamId, ParamTarget, ParameterTarget};
use std::sync::{Arc, Mutex, MutexGuard};

/// Fraction of rate used to close phase error.
pub const MAX_NUDGE: f64 = 0.05;
/// Nudge per beat of error.
pub const PHASE_GAIN: f64 = 0.25;
/// Phase error below this many beats is ignored.
pub const DEADBAND: f64 = 0.002;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SyncDeckState {
    /// Source frames.
    pub position: i64,
    pub rate: f64,
    pub playing: bool,
}

impl Default for SyncDeckState {
    fn default() -> Self {
        Self {
            position: 0,
            rate: 1.0,
            playing: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SyncMode {
    #[default]
    Beat,
    Bar,
}

#[derive(Default)]
struct Slot {
    grid: Option<Arc<dyn BeatGrid>>,
    enabled: bool,
}

#[derive(Default)]
struct State {
    slots: Vec<Slot>,
    leader: Option<usize>,
    mode: SyncMode,
}

pub struct Sync {
    state: Mutex<State>,
}

impl Sync {
    pub fn new(num_decks: usize) -> Self {
        Self {
            state: Mutex::new(State {
                slots: (0..num_decks).map(|_| Slot::default()).collect(),
                leader: None,
                mode: SyncMode::Beat,
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock_unpoisoned()
    }

    /// Grids are shared with the analysis side. `None` removes the grid, so the deck cannot sync.
    pub fn set_grid(&self, deck: usize, grid: Option<Arc<dyn BeatGrid>>) {
        let mut st = self.lock();
        if let Some(slot) = st.slots.get_mut(deck) {
            slot.grid = grid;
        }
    }

    /// Out-of-range indices clear the leader.
    pub fn set_leader(&self, deck: Option<usize>) {
        let mut st = self.lock();
        st.leader = deck.filter(|&d| d < st.slots.len());
    }

    pub fn set_enabled(&self, deck: usize, on: bool) {
        let mut st = self.lock();
        if let Some(slot) = st.slots.get_mut(deck) {
            slot.enabled = on;
        }
    }

    pub fn set_mode(&self, mode: SyncMode) {
        self.lock().mode = mode;
    }

    pub fn grid(&self, deck: usize) -> Option<Arc<dyn BeatGrid>> {
        self.lock().slots.get(deck).and_then(|s| s.grid.clone())
    }

    pub fn is_enabled(&self, deck: usize) -> bool {
        self.lock().slots.get(deck).is_some_and(|s| s.enabled)
    }

    pub fn leader(&self) -> Option<usize> {
        self.lock().leader
    }

    pub fn mode(&self) -> SyncMode {
        self.lock().mode
    }

    /// New rate per deck, only for synced followers that need a change.
    pub fn step(&self, decks: &[SyncDeckState]) -> Vec<Option<f64>> {
        let st = self.lock();
        (0..decks.len())
            .map(|d| {
                let (leader, lg, fg) = sync_pair(&st, decks, d)?;
                Some(follower_rate(&st, decks, leader, d, &lg, &fg))
            })
            .collect()
    }

    /// Convenience for real decks (same order as the grids). `None` entries are skipped.
    pub fn apply(&self, decks: &[Option<&Deck>]) {
        let states: Vec<SyncDeckState> = decks
            .iter()
            .map(|d| match d {
                Some(d) => SyncDeckState {
                    position: d.position_frames(),
                    rate: d.rate(),
                    playing: d.is_playing(),
                },
                None => SyncDeckState::default(),
            })
            .collect();
        let rates = self.step(&states);
        for (rate, deck) in rates.iter().zip(decks) {
            if let (Some(rate), Some(deck)) = (rate, deck) {
                deck.set_rate(*rate);
            }
        }
    }

    /// Follower phase error against the leader, in beats (bar mode: up to half a bar). Zero when
    /// the follower cannot sync.
    pub fn phase_error(&self, decks: &[SyncDeckState], follower: usize) -> f64 {
        let st = self.lock();
        match sync_pair(&st, decks, follower) {
            Some((leader, lg, fg)) => error_beats(&st, decks, leader, follower, &lg, &fg),
            None => 0.0,
        }
    }
}

/// Leader index and both grids, when a follower can sync to the leader right now.
type SyncPair = (usize, Arc<dyn BeatGrid>, Arc<dyn BeatGrid>);

fn sync_pair(st: &State, decks: &[SyncDeckState], follower: usize) -> Option<SyncPair> {
    let leader = st.leader?;
    if follower == leader || follower >= st.slots.len() {
        return None;
    }
    let count = decks.len().min(st.slots.len());
    if follower >= count || leader >= count || !st.slots[follower].enabled {
        return None;
    }
    let lg = st.slots[leader].grid.clone()?;
    let fg = st.slots[follower].grid.clone()?;
    let playing = decks[leader].playing;
    (lg.bpm() > 0.0 && fg.bpm() > 0.0 && playing).then_some((leader, lg, fg))
}

/// Phase of a deck within the beat (or bar) unit, in [0, 1).
fn phase_of(grid: &dyn BeatGrid, mode: SyncMode, deck: &SyncDeckState) -> f64 {
    let unit = unit_for(grid, mode);
    let beat = grid.beat_at_frame(deck.position) / unit;
    beat - beat.floor()
}

fn unit_for(grid: &dyn BeatGrid, mode: SyncMode) -> f64 {
    match mode {
        SyncMode::Bar => f64::from(grid.beats_per_bar().max(1)),
        SyncMode::Beat => 1.0,
    }
}

/// Wraps to [-0.5, 0.5).
fn wrap_half(x: f64) -> f64 {
    x - (x + 0.5).floor()
}

fn error_beats(
    st: &State,
    decks: &[SyncDeckState],
    leader: usize,
    follower: usize,
    lg: &Arc<dyn BeatGrid>,
    fg: &Arc<dyn BeatGrid>,
) -> f64 {
    let unit = unit_for(fg.as_ref(), st.mode);
    let lead_phase = phase_of(lg.as_ref(), st.mode, &decks[leader]);
    let follow_phase = phase_of(fg.as_ref(), st.mode, &decks[follower]);
    wrap_half(lead_phase - follow_phase) * unit
}

fn follower_rate(
    st: &State,
    decks: &[SyncDeckState],
    leader: usize,
    follower: usize,
    lg: &Arc<dyn BeatGrid>,
    fg: &Arc<dyn BeatGrid>,
) -> f64 {
    let ld = decks[leader];
    let fd = decks[follower];
    let base = ld.rate * lg.bpm() / fg.bpm();
    if !fd.playing {
        // Phase is meaningless until the follower plays.
        return base;
    }
    let error = error_beats(st, decks, leader, follower, lg, fg);
    let nudge = if error.abs() < DEADBAND {
        0.0
    } else {
        (PHASE_GAIN * error).clamp(-MAX_NUDGE, MAX_NUDGE)
    };
    base * (1.0 + nudge)
}

impl Sync {
    /// Applies one sync parameter: `leader` (deck index; negative or out of range for none), `mode`
    /// (>= 0.5 bar, else beat) and `<deck>.sync` (>= 0.5 on). Returns `false` for anything else.
    pub fn set_sync_param(&self, p: &SyncParam, value: f64) -> bool {
        match p {
            SyncParam::Leader => {
                let deck = usize::try_from(value as i64).ok();
                self.set_leader(deck);
            }
            SyncParam::Mode => {
                self.set_mode(if value >= 0.5 {
                    SyncMode::Bar
                } else {
                    SyncMode::Beat
                });
            }
            SyncParam::DeckEnabled(deck) => self.set_enabled(usize::from(*deck), value >= 0.5),
        }
        true
    }
}

impl ParamTarget for Sync {
    fn set_param(&self, id: &ParamId, value: f64) -> bool {
        match id {
            ParamId::Sync(p) => self.set_sync_param(p, value),
            _ => false,
        }
    }
}

impl ParameterTarget for Sync {
    /// The id after `sync.`: "leader", "mode", "<deck>.sync".
    fn set_parameter(&self, id: &str, value: f64) -> bool {
        match id.parse::<SyncParam>() {
            Ok(p) => self.set_sync_param(&p, value),
            Err(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::FixedTempoGrid;

    const RATE: f64 = 44_100.0;

    /// Stands in for an analysis grid: 3 beats per bar, tempo from a table.
    struct MockGrid {
        bpm: f64,
        origin: i64,
    }

    impl BeatGrid for MockGrid {
        fn bpm(&self) -> f64 {
            self.bpm
        }
        fn beats_per_bar(&self) -> i32 {
            3
        }
        fn beat_at_frame(&self, f: i64) -> f64 {
            (f - self.origin) as f64 * self.bpm / (60.0 * RATE)
        }
        fn frame_at_beat(&self, b: f64) -> i64 {
            self.origin + (b * 60.0 * RATE / self.bpm).round() as i64
        }
    }

    /// Advances two virtual decks the way the audio thread would and lets Sync steer the follower.
    struct Sim {
        decks: Vec<SyncDeckState>,
    }

    impl Sim {
        fn new() -> Self {
            Self {
                decks: vec![
                    SyncDeckState {
                        position: 0,
                        rate: 1.0,
                        playing: true,
                    },
                    SyncDeckState {
                        position: 0,
                        rate: 1.0,
                        playing: true,
                    },
                ],
            }
        }

        fn run(&mut self, sync: &Sync, seconds: f64) {
            let dt = 0.05;
            let mut t = 0.0;
            while t < seconds {
                let rates = sync.step(&self.decks);
                for (deck, rate) in self.decks.iter_mut().zip(rates) {
                    if let Some(r) = rate {
                        deck.rate = r;
                    }
                    deck.position += (deck.rate * RATE * dt) as i64;
                }
                t += dt;
            }
        }
    }

    fn grid(bpm: f64, origin: i64) -> Arc<dyn BeatGrid> {
        Arc::new(MockGrid { bpm, origin })
    }

    #[test]
    fn follower_converges_to_the_leaders_tempo_and_beat_phase() {
        let sync = Sync::new(2);
        sync.set_grid(0, Some(Arc::new(FixedTempoGrid::new(120.0, RATE, 0))));
        sync.set_grid(1, Some(grid(126.0, 9000)));
        sync.set_leader(Some(0));
        sync.set_enabled(1, true);

        let mut sim = Sim::new();
        assert!(sync.phase_error(&sim.decks, 1).abs() > 0.05);
        sim.run(&sync, 30.0);
        assert!((sim.decks[1].rate - 120.0 / 126.0).abs() < 0.01);
        assert!(sync.phase_error(&sim.decks, 1).abs() < 0.02);
    }

    #[test]
    fn bar_mode_closes_whole_beat_offsets() {
        let sync = Sync::new(2);
        sync.set_grid(0, Some(grid(120.0, 0)));
        sync.set_grid(1, Some(grid(120.0, 22_050))); // one beat off
        sync.set_leader(Some(0));
        sync.set_enabled(1, true);
        sync.set_mode(SyncMode::Bar);

        let mut sim = Sim::new();
        assert!((sync.phase_error(&sim.decks, 1).abs() - 1.0).abs() < 1e-6);
        sim.run(&sync, 40.0);
        assert!(sync.phase_error(&sim.decks, 1).abs() < 0.03);
    }

    #[test]
    fn nothing_changes_when_sync_cannot_apply() {
        let sync = Sync::new(3);
        let mut sim = Sim::new();
        sim.decks.push(SyncDeckState::default());
        sync.set_grid(0, Some(Arc::new(FixedTempoGrid::new(120.0, RATE, 0))));
        sync.set_grid(1, Some(Arc::new(FixedTempoGrid::new(100.0, RATE, 0))));
        sync.set_leader(Some(0));

        assert!(sync.step(&sim.decks)[1].is_none(), "not enabled");
        sync.set_enabled(1, true);
        sync.set_enabled(0, true);
        assert!(
            sync.step(&sim.decks)[0].is_none(),
            "leader never follows itself"
        );
        assert!(sync.step(&sim.decks)[1].is_some());
        sim.decks[0].playing = false;
        assert!(sync.step(&sim.decks)[1].is_none(), "leader stopped");
        sim.decks[0].playing = true;
        sync.set_grid(1, None);
        assert!(sync.step(&sim.decks)[1].is_none(), "no grid");
        assert_eq!(sync.phase_error(&sim.decks, 1), 0.0);
        sync.set_enabled(7, true); // out of range ignored
        assert!(!sync.is_enabled(7));
    }

    #[test]
    fn a_stopped_follower_only_gets_the_tempo_match() {
        let sync = Sync::new(2);
        sync.set_grid(0, Some(Arc::new(FixedTempoGrid::new(120.0, RATE, 0))));
        sync.set_grid(1, Some(Arc::new(FixedTempoGrid::new(100.0, RATE, 0))));
        sync.set_leader(Some(0));
        sync.set_enabled(1, true);
        let mut sim = Sim::new();
        sim.decks[1].playing = false;
        sim.decks[1].position = 5000;
        let rate = sync.step(&sim.decks)[1].expect("tempo match");
        assert!((rate - 1.2).abs() < 1e-9);
    }

    #[test]
    fn parameter_target_ids() {
        let sync = Sync::new(2);
        assert!(sync.set_parameter("leader", 0.0));
        assert_eq!(sync.leader(), Some(0));
        assert!(sync.set_parameter("1.sync", 1.0));
        assert!(sync.is_enabled(1));
        assert!(sync.set_parameter("1.sync", 0.0));
        assert!(!sync.is_enabled(1));
        assert!(sync.set_parameter("mode", 1.0));
        assert_eq!(sync.mode(), SyncMode::Bar);
        assert!(!sync.set_parameter("nope", 1.0));
        assert!(!sync.set_parameter("x.sync", 1.0));
    }

    #[test]
    fn deck_integration_sets_follower_rate_through_apply() {
        let sync = Sync::new(2);
        assert!(sync.set_parameter("leader", 0.0));
        sync.set_grid(0, Some(Arc::new(FixedTempoGrid::new(120.0, RATE, 0))));
        sync.set_grid(1, Some(Arc::new(FixedTempoGrid::new(96.0, RATE, 0))));
        sync.set_enabled(1, true);
        let (leader, _leader_audio) = Deck::new(1 << 12);
        let (follower, _follower_audio) = Deck::new(1 << 12);
        leader.play();
        leader.set_rate(1.0);
        sync.apply(&[Some(&leader), Some(&follower), None]);
        assert!((follower.rate() - 1.25).abs() < 1e-9);
        assert_eq!(leader.rate(), 1.0);
    }
}

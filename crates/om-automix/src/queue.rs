//! The queue source: which track plays next. [`AutomixQueue`] is the trait the host implements
//! (over the library in `openmixer`); [`select_next`] holds the rules, as a pure function over
//! plain [`QueueTrack`]s, so they test without a database.
//!
//! Rules, in order:
//! 1. Tracks played recently (the host passes their ids) are skipped: [`SkipReason::Recent`].
//! 2. A track without analysis (no BPM or no beat grid) is skipped: [`SkipReason::NotAnalysed`].
//! 3. A playlist source plays in order (`Order::InOrder`) and stops there.
//! 4. A search or crate source is shuffled with the injected RNG (`Order::Shuffle`). With `smart`
//!    on and a current track known, the pick prefers (a) a compatible Camelot key (same slot, one
//!    step either way, or the relative key) with a BPM within the window, then (b) a BPM within the
//!    window, then (c) anything left. The window is `bpm_window` (0.08 = 8%) around the current
//!    track's BPM, also counting a half or double tempo as a match.
//!
//! The same RNG seed and inputs give the same pick.

use crate::types::Camelot;
use std::collections::HashSet;

/// What the queue needs to know about a track.
#[derive(Debug, Clone, PartialEq)]
pub struct QueueTrack {
    pub id: i64,
    pub title: String,
    pub artist: String,
    pub path: String,
    pub bpm: Option<f64>,
    /// Camelot code; empty when unknown.
    pub camelot: String,
    /// A beat grid is available (the analysis finished).
    pub has_grid: bool,
}

impl QueueTrack {
    pub fn analysed(&self) -> bool {
        self.has_grid && self.bpm.is_some_and(|b| b.is_finite() && b > 0.0)
    }
}

/// Why a track was left out of a pick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// Played within the history window.
    Recent,
    /// No BPM or no beat grid yet.
    NotAnalysed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
    /// Keep the source order (playlists).
    InOrder,
    /// Shuffle with the RNG (search results, crates).
    Shuffle,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QueueConfig {
    pub order: Order,
    /// Prefer key-compatible, tempo-close tracks (shuffle only).
    pub smart: bool,
    /// Fraction of the current BPM, 0.08 = 8%.
    pub bpm_window: f64,
    /// Number of most recent plays that count as recent.
    pub recent_count: usize,
    /// Plays newer than this many hours count as recent.
    pub recent_hours: f64,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            order: Order::Shuffle,
            smart: true,
            bpm_window: 0.08,
            recent_count: 20,
            recent_hours: 4.0,
        }
    }
}

/// A source of random numbers, injected so picks are reproducible.
pub trait QueueRng {
    fn next_u64(&mut self) -> u64;
}

/// SplitMix64: a small, fast, seedable generator. Good enough for shuffling a queue.
#[derive(Debug, Clone)]
pub struct SplitMix64(pub u64);

impl QueueRng for SplitMix64 {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// The result of one selection.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Selection {
    pub pick: Option<QueueTrack>,
    /// Every candidate that was skipped for a hard reason, with the reason.
    pub skipped: Vec<(i64, SkipReason)>,
}

/// The host's queue: the next track to play, or `None` when nothing qualifies.
pub trait AutomixQueue: Send {
    /// `current` is the track playing now, if known. The pick is consumed: the queue moves on.
    fn next_track(&mut self, current: Option<&QueueTrack>) -> Option<QueueTrack>;

    /// Why candidates were skipped by the last call.
    fn last_skips(&self) -> Vec<(i64, SkipReason)> {
        Vec::new()
    }
}

/// Picks the next track from `candidates` (source order). See the module docs for the rules.
pub fn select_next(
    candidates: &[QueueTrack],
    current: Option<&QueueTrack>,
    recent: &HashSet<i64>,
    cfg: &QueueConfig,
    rng: &mut dyn QueueRng,
) -> Selection {
    let mut skipped = Vec::new();
    let mut eligible: Vec<&QueueTrack> = Vec::new();
    for track in candidates {
        if recent.contains(&track.id) || current.is_some_and(|c| c.id == track.id) {
            skipped.push((track.id, SkipReason::Recent));
        } else if !track.analysed() {
            skipped.push((track.id, SkipReason::NotAnalysed));
        } else {
            eligible.push(track);
        }
    }
    if cfg.order == Order::Shuffle {
        // Fisher-Yates.
        for i in (1..eligible.len()).rev() {
            let j = (rng.next_u64() % (i as u64 + 1)) as usize;
            eligible.swap(i, j);
        }
    }
    let smart = cfg.smart && cfg.order == Order::Shuffle;
    let pick = match current.filter(|_| smart) {
        Some(cur) => {
            let close = |t: &QueueTrack| bpm_close(cur.bpm, t.bpm, cfg.bpm_window);
            let key_ok = |t: &QueueTrack| keys_agree(&cur.camelot, &t.camelot);
            let mut all = eligible.iter().copied();
            all.clone()
                .find(|t| close(t) && key_ok(t))
                .or_else(|| all.clone().find(|t| close(t)))
                .or_else(|| all.next())
        }
        None => eligible.first().copied(),
    };
    Selection {
        pick: pick.cloned(),
        skipped,
    }
}

/// `b` is within `window` of `a`, or of half or double `a`.
fn bpm_close(a: Option<f64>, b: Option<f64>, window: f64) -> bool {
    let (Some(a), Some(b)) = (a, b) else {
        return false;
    };
    if a <= 0.0 || b <= 0.0 {
        return false;
    }
    [a, a * 2.0, a / 2.0]
        .into_iter()
        .any(|x| ((b - x) / x).abs() <= window)
}

fn keys_agree(a: &str, b: &str) -> bool {
    match (Camelot::parse(a), Camelot::parse(b)) {
        (Some(a), Some(b)) => a.compatible(b),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: i64, bpm: Option<f64>, camelot: &str, grid: bool) -> QueueTrack {
        QueueTrack {
            id,
            title: format!("t{id}"),
            artist: String::new(),
            path: String::new(),
            bpm,
            camelot: camelot.to_owned(),
            has_grid: grid,
        }
    }

    fn pool() -> Vec<QueueTrack> {
        vec![
            track(1, Some(124.0), "8A", true),
            track(2, Some(126.0), "9A", true),
            track(3, Some(170.0), "2B", true),
            track(4, None, "8A", false),
            track(5, Some(125.0), "3B", true),
            track(6, Some(125.0), "8B", false),
        ]
    }

    fn cfg(order: Order, smart: bool) -> QueueConfig {
        QueueConfig {
            order,
            smart,
            ..QueueConfig::default()
        }
    }

    #[test]
    fn in_order_takes_the_first_eligible_and_reports_skips() {
        let recent: HashSet<i64> = [1].into_iter().collect();
        let sel = select_next(
            &pool(),
            None,
            &recent,
            &cfg(Order::InOrder, true),
            &mut SplitMix64(1),
        );
        assert_eq!(sel.pick.unwrap().id, 2);
        assert!(sel.skipped.contains(&(1, SkipReason::Recent)));
        assert!(sel.skipped.contains(&(4, SkipReason::NotAnalysed)));
        assert!(
            sel.skipped.contains(&(6, SkipReason::NotAnalysed)),
            "a BPM without a grid is not analysed"
        );
    }

    #[test]
    fn the_current_track_is_never_picked_again() {
        let cur = track(2, Some(126.0), "9A", true);
        let sel = select_next(
            &pool()[1..2],
            Some(&cur),
            &HashSet::new(),
            &cfg(Order::InOrder, false),
            &mut SplitMix64(1),
        );
        assert!(sel.pick.is_none());
    }

    #[test]
    fn smart_prefers_compatible_key_and_close_bpm() {
        let cur = track(9, Some(124.0), "8A", true);
        // Only 2 (9A, 126) is both: every seed must find it.
        let candidates = vec![pool()[2].clone(), pool()[4].clone(), pool()[1].clone()];
        for seed in 0..40 {
            let sel = select_next(
                &candidates,
                Some(&cur),
                &HashSet::new(),
                &cfg(Order::Shuffle, true),
                &mut SplitMix64(seed),
            );
            assert_eq!(sel.pick.unwrap().id, 2, "seed {seed}");
        }
    }

    #[test]
    fn smart_falls_back_to_tempo_then_to_anything() {
        let cur = track(9, Some(124.0), "8A", true);
        let tempo_only = vec![pool()[2].clone(), pool()[4].clone()];
        let sel = select_next(
            &tempo_only,
            Some(&cur),
            &HashSet::new(),
            &cfg(Order::Shuffle, true),
            &mut SplitMix64(3),
        );
        assert_eq!(
            sel.pick.unwrap().id,
            5,
            "125 BPM is in the window, 170 is not"
        );
        let far = vec![pool()[2].clone()];
        let sel = select_next(
            &far,
            Some(&cur),
            &HashSet::new(),
            &cfg(Order::Shuffle, true),
            &mut SplitMix64(3),
        );
        assert_eq!(sel.pick.unwrap().id, 3);
    }

    #[test]
    fn half_and_double_tempo_count_as_close() {
        assert!(bpm_close(Some(85.0), Some(170.0), 0.08));
        assert!(bpm_close(Some(170.0), Some(86.0), 0.08));
        assert!(!bpm_close(Some(124.0), Some(140.0), 0.08));
        assert!(!bpm_close(None, Some(124.0), 0.08));
    }

    #[test]
    fn the_same_seed_gives_the_same_pick_and_seeds_differ() {
        let many: Vec<QueueTrack> = (1..=30)
            .map(|i| track(i, Some(120.0), "8A", true))
            .collect();
        let pick = |seed| {
            select_next(
                &many,
                None,
                &HashSet::new(),
                &cfg(Order::Shuffle, false),
                &mut SplitMix64(seed),
            )
            .pick
            .unwrap()
            .id
        };
        assert_eq!(pick(7), pick(7));
        let distinct: HashSet<i64> = (0..20).map(pick).collect();
        assert!(distinct.len() > 3, "{distinct:?}");
    }

    #[test]
    fn an_empty_pool_picks_nothing() {
        let sel = select_next(
            &[],
            None,
            &HashSet::new(),
            &QueueConfig::default(),
            &mut SplitMix64(0),
        );
        assert!(sel.pick.is_none() && sel.skipped.is_empty());
    }
}

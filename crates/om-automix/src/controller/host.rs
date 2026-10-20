//! What the controller needs from the app ([`AutomixHost`]), the settings it runs with
//! ([`AutomixConfig`]) and the typed `automix.*` control target ([`AutomixControl`]).

use crate::stem_fade::CancelMode;
use crate::types::DeckInfo;
use om_core::param_id::AutomixParam;
use om_core::{AutomixStyle, ParamId, ParamTarget};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// The track the queue chose to play next.
#[derive(Debug, Clone, PartialEq)]
pub struct NextTrack {
    pub id: i64,
    pub title: String,
    /// Tempo, when the track is analysed (shown on the automix bar).
    pub bpm: Option<f64>,
    /// Camelot key; empty when unknown.
    pub key: String,
}

/// How a started load is going.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadPoll {
    Pending,
    /// The deck holds the track and its analysis is available through `deck_info`.
    Ready,
    Failed(String),
}

/// The app side of automix, called on the control thread from [`super::AutomixController::tick`].
/// Every method must return quickly: the controller never waits. Loading is asynchronous: start it,
/// then answer `poll_load` until it is ready.
pub trait AutomixHost {
    /// Chooses and consumes the next track. `playing_deck` is the deck that plays now (the queue
    /// uses its track for key and tempo preference). `tempo_gate` is `Some(ratio)` for an explicit
    /// beat-matched style: the pick must be within `ratio` of the playing track's tempo (no half or
    /// double time). `None` when nothing qualifies.
    fn next_track(&mut self, playing_deck: u8, tempo_gate: Option<f64>) -> Option<NextTrack>;

    /// Starts loading `track` on `deck`. An immediate failure is an `Err`.
    fn start_load(&mut self, deck: u8, track: &NextTrack) -> Result<(), String>;

    fn poll_load(&mut self, deck: u8) -> LoadPoll;

    /// Grid, key, sections and stems of the track now on `deck`, from the analysis cache. `None`
    /// when the deck is empty or nothing is known about the track.
    fn deck_info(&mut self, deck: u8) -> Option<DeckInfo>;

    /// A display title for the track on `deck` (used when the controller did not load it).
    fn deck_title(&mut self, _deck: u8) -> String {
        String::new()
    }

    /// Audio underruns so far (deck underruns plus device xruns). A rise during a transition
    /// cancels it and turns automix off.
    fn underruns(&self) -> u64 {
        0
    }
}

/// Settings of the controller. `Default` is what the app starts with.
#[derive(Debug, Clone)]
pub struct AutomixConfig {
    /// Start loading the next track when the playing deck has this many beats left.
    pub arm_beats: f64,
    /// Largest tempo gap a beat-matched style accepts (fraction).
    pub max_bpm_ratio: f64,
    pub min_length_beats: f64,
    /// A fixed transition length (snapped to 8, 16 or 32); `None` lets the planner pick.
    pub length_beats: Option<f64>,
    /// Least beats between "now" and the start of a transition.
    pub lead_beats: f64,
    /// FX unit that holds the echo on a channel's rack.
    pub echo_unit: u16,
    /// A control that differs from the value automix last set by more than this was moved by the
    /// user.
    pub eps: f64,
    /// Beats of unexplained playhead movement tolerated before it counts as a seek.
    pub seek_slack_beats: f64,
    /// On a cancel, put EQ, filter and FX moves back to their plan-time values (never the
    /// crossfader, faders, play or seek).
    pub restore_on_cancel: bool,
    /// What a cancel does to stem moves.
    pub stem_cancel: CancelMode,
    /// Seconds a load may stay pending.
    pub load_timeout_s: f64,
    /// Consecutive load failures after which automix turns itself off.
    pub max_load_failures: u32,
    /// Ticks spent in `Cleanup` before going idle.
    pub cleanup_ticks: u32,
    /// Seconds (control clock) the cued position of the incoming deck may take to show up before a
    /// mismatch counts as the user's seek. The decoder acknowledges seeks in wall time, so this is
    /// time, not ticks.
    pub cue_grace_s: f64,
    /// Start the next track by itself when the playing one nears its end. Off: only `automix.start`
    /// starts a transition.
    pub autostart: bool,
}

impl Default for AutomixConfig {
    fn default() -> Self {
        Self {
            arm_beats: 96.0,
            max_bpm_ratio: 0.06,
            min_length_beats: 8.0,
            length_beats: None,
            lead_beats: 4.0,
            echo_unit: 0,
            eps: 0.02,
            seek_slack_beats: 2.0,
            restore_on_cancel: true,
            stem_cancel: CancelMode::Restore,
            load_timeout_s: 30.0,
            max_load_failures: 3,
            cleanup_ticks: 2,
            cue_grace_s: 0.2,
            autostart: true,
        }
    }
}

/// The `automix` namespace target: `automix.enable`, `automix.style` (a value of
/// [`AutomixStyle::as_u8`]), `automix.start` and `automix.cancel` (presses, value >= 0.5). Setting
/// these only records the request; the controller reads them on its next tick, so the router call
/// never blocks and never touches the audio thread.
#[derive(Debug, Default)]
pub struct AutomixControl {
    enabled: AtomicBool,
    style: AtomicU8,
    start: AtomicBool,
    cancel: AtomicBool,
}

impl AutomixControl {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, Ordering::Relaxed);
    }

    pub fn style(&self) -> AutomixStyle {
        AutomixStyle::from_u8(self.style.load(Ordering::Relaxed))
    }

    pub fn set_style(&self, style: AutomixStyle) {
        self.style.store(style.as_u8(), Ordering::Relaxed);
    }

    /// Takes a pending `automix.start` press.
    pub(crate) fn take_start(&self) -> bool {
        self.start.swap(false, Ordering::AcqRel)
    }

    /// Takes a pending `automix.cancel` press.
    pub(crate) fn take_cancel(&self) -> bool {
        self.cancel.swap(false, Ordering::AcqRel)
    }
}

impl ParamTarget for AutomixControl {
    fn set_param(&self, id: &ParamId, value: f64) -> bool {
        let ParamId::Automix(p) = id else {
            return false;
        };
        match p {
            AutomixParam::Enable => self.set_enabled(value >= 0.5),
            AutomixParam::Style => self.set_style(AutomixStyle::from_value(value)),
            AutomixParam::Start => {
                if value >= 0.5 {
                    self.start.store(true, Ordering::Release);
                }
            }
            AutomixParam::Cancel => {
                if value >= 0.5 {
                    self.cancel.store(true, Ordering::Release);
                }
            }
        }
        true
    }
}

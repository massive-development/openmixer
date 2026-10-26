//! The automix bar's view-model: what the main window's Automix strip shows, built from the
//! automix snapshot and the Automix preferences. Pure: no drawing and no service calls, so the
//! labels and the enabled rules are tested here and the window only lays them out.

use crate::preferences::AutomixPrefs;
use crate::settings::STYLE_LABELS;
use om_core::{AutomixSnapshot, AutomixState};

/// Widths of the bar's parts, in pixels. The window lays the bar out with these; the tests check
/// that both rows (idle and running) fit the strip the header gives the bar at the minimum window.
/// The caption of the segmented switch.
pub const TOGGLE_LABEL: &str = "AUTOMIX";
pub const LABEL_WIDTH: f32 = 52.0;
/// The segmented Off | On switch.
pub const SWITCH_WIDTH: f32 = 76.0;
/// The transition length chip: a `TRANSITION` caption over `16 beats`.
pub const LENGTH_WIDTH: f32 = 64.0;
/// The running state word (`Loading`, `Armed`, `Mixing`).
pub const STATE_WIDTH: f32 = 60.0;
pub const PROGRESS_WIDTH: f32 = 40.0;
pub const REMAINING_WIDTH: f32 = 72.0;
pub const START_WIDTH: f32 = 44.0;
pub const SKIP_WIDTH: f32 = 36.0;
pub const CANCEL_WIDTH: f32 = 48.0;
/// The next-track text shrinks to this and then clips.
pub const NEXT_MIN_WIDTH: f32 = 100.0;
/// Gap between the row's items, and its left and right padding.
pub const ROW_GAP: f32 = 4.0;
pub const ROW_PADDING: f32 = 8.0;

/// The buttons the bar offers right now. Only what does something is drawn: an Off automix shows
/// just its toggle, a transition shows Skip and Cancel, and so on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AutomixActions {
    pub start: bool,
    pub skip: bool,
    pub cancel: bool,
}

impl AutomixActions {
    /// How many buttons are offered.
    pub fn count(&self) -> usize {
        usize::from(self.start) + usize::from(self.skip) + usize::from(self.cancel)
    }
}

/// What the bar shows.
#[derive(Debug, Clone, PartialEq)]
pub struct AutomixBar {
    /// The Enable toggle (the preference, which the snapshot follows).
    pub enabled: bool,
    /// The style the preference asks for.
    pub style: &'static str,
    pub length_beats: i32,
    /// Off, Idle, Loading, Armed, Mixing or Finishing. The window shows it only while `running`.
    pub state: &'static str,
    /// A pick is loading, armed or mixing: the state word, progress, Skip and Cancel show.
    pub running: bool,
    /// `Next: <title> · <bpm> BPM · <key>`, or `Next: Nothing queued`.
    pub next: String,
    /// 0 to 1 through the running transition.
    pub progress: f64,
    /// `12 beats left` while a deck leads; empty otherwise.
    pub remaining: String,
    /// The last error or the reason a plan was dropped; empty when none.
    pub error: String,
    /// Start now is useful: a track is loading or armed.
    pub can_start: bool,
    /// Cancel is useful: anything is planned or running.
    pub can_cancel: bool,
    /// Skip does something: a pick is armed or loading, or a transition is running. Idle has
    /// nothing to skip (the controller would only report so).
    pub can_skip: bool,
}

impl AutomixBar {
    /// The buttons worth drawing: those whose `can_*` flag is set. Start now is left out while a
    /// pick is already armed or loading, since Skip and Cancel are what the DJ wants then.
    pub fn actions(&self) -> AutomixActions {
        AutomixActions {
            start: self.can_start && !self.can_skip,
            skip: self.can_skip,
            cancel: self.can_cancel,
        }
    }

    /// True while there is something to read besides the toggle: a plan, a transition or an error.
    pub fn has_detail(&self) -> bool {
        self.enabled || !self.error.is_empty()
    }

    /// True while the progress bar and the beats left mean something.
    pub fn shows_progress(&self) -> bool {
        self.enabled && !self.remaining.is_empty()
    }

    pub fn new(snapshot: &AutomixSnapshot, prefs: &AutomixPrefs) -> Self {
        // The running flag, not the preference: the controller can turn itself off (an underrun,
        // repeated load failures) and the bar says so with the reason in `error`.
        let enabled = snapshot.enabled;
        let state = state_label(snapshot.state, enabled);
        let style = STYLE_LABELS
            .get(prefs.style.clamp(0, (STYLE_LABELS.len() - 1) as i32) as usize)
            .copied()
            .unwrap_or(STYLE_LABELS[0]);
        let planned = !matches!(snapshot.state, AutomixState::Idle);
        Self {
            enabled,
            style,
            length_beats: prefs.length_beats,
            state,
            running: enabled
                && matches!(
                    snapshot.state,
                    AutomixState::Loading | AutomixState::Armed | AutomixState::Transitioning
                ),
            next: next_text(
                &snapshot.next_track_title,
                snapshot.next_track_bpm,
                &snapshot.next_track_key,
            ),
            progress: snapshot.progress.clamp(0.0, 1.0),
            remaining: snapshot
                .remaining_beats
                .map(remaining_text)
                .unwrap_or_default(),
            error: snapshot.last_error.clone(),
            can_start: enabled && snapshot.state != AutomixState::Transitioning,
            can_cancel: enabled && planned,
            can_skip: enabled
                && matches!(
                    snapshot.state,
                    AutomixState::Loading | AutomixState::Armed | AutomixState::Transitioning
                ),
        }
    }
}

/// The label for a controller state. Anything reads as Off while automix is disabled.
pub fn state_label(state: AutomixState, enabled: bool) -> &'static str {
    if !enabled {
        return "Off";
    }
    match state {
        AutomixState::Idle => "Idle",
        AutomixState::Loading => "Loading",
        AutomixState::Armed => "Armed",
        AutomixState::Transitioning => "Mixing",
        AutomixState::Cleanup => "Finishing",
    }
}

/// `Next: <title> · <bpm> BPM · <key>` or `Next: Nothing queued`, leaving out the parts that are unknown.
pub fn next_text(title: &str, bpm: Option<f64>, key: &str) -> String {
    if title.is_empty() {
        return "Next: Nothing queued".to_owned();
    }
    let mut text = format!("Next: {title}");
    if let Some(bpm) = bpm.filter(|b| b.is_finite() && *b > 0.0) {
        text.push_str(&format!(" · {bpm:.1} BPM"));
    }
    if !key.is_empty() {
        text.push_str(&format!(" · {key}"));
    }
    text
}

/// `Transition 16 beats`: the length the next transition runs.
pub fn transition_text(length_beats: i32) -> String {
    format!("Transition {length_beats} beats")
}

/// `12 beats left`, `1 beat left`, rounded to whole beats.
pub fn remaining_text(beats: f64) -> String {
    let whole = beats.max(0.0).round() as i64;
    if whole == 1 {
        "1 beat left".to_owned()
    } else {
        format!("{whole} beats left")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(state: AutomixState) -> AutomixSnapshot {
        AutomixSnapshot {
            enabled: true,
            state,
            next_track_title: "Night Drive".into(),
            next_track_bpm: Some(124.04),
            next_track_key: "8A".into(),
            progress: 1.7,
            remaining_beats: Some(11.6),
            ..Default::default()
        }
    }

    #[test]
    fn labels_follow_the_state_and_the_switch() {
        assert_eq!(state_label(AutomixState::Armed, true), "Armed");
        assert_eq!(state_label(AutomixState::Transitioning, true), "Mixing");
        assert_eq!(state_label(AutomixState::Armed, false), "Off");
    }

    #[test]
    fn next_line_leaves_out_what_is_unknown() {
        assert_eq!(
            next_text("Night Drive", Some(124.04), "8A"),
            "Next: Night Drive · 124.0 BPM · 8A"
        );
        assert_eq!(next_text("Cue", None, ""), "Next: Cue");
        assert_eq!(next_text("", Some(120.0), "8A"), "Next: Nothing queued");
        assert_eq!(next_text("Cue", Some(f64::NAN), ""), "Next: Cue");
    }

    #[test]
    fn remaining_beats_round_and_never_go_negative() {
        assert_eq!(remaining_text(11.6), "12 beats left");
        assert_eq!(remaining_text(1.0), "1 beat left");
        assert_eq!(remaining_text(-3.0), "0 beats left");
    }

    #[test]
    fn bar_enables_start_and_cancel_by_state() {
        let prefs = AutomixPrefs {
            enabled: true,
            ..Default::default()
        };
        let armed = AutomixBar::new(&snap(AutomixState::Armed), &prefs);
        assert!(armed.can_start && armed.can_cancel);
        assert_eq!(armed.progress, 1.0, "progress is clamped");
        assert_eq!(armed.remaining, "12 beats left");
        assert_eq!(armed.style, "Auto");
        let idle = AutomixBar::new(&snap(AutomixState::Idle), &prefs);
        assert!(
            idle.can_start,
            "start now works from Idle: it plans the next track"
        );
        assert!(!idle.can_cancel);
        let mut off_snap = snap(AutomixState::Armed);
        off_snap.enabled = false;
        let off = AutomixBar::new(&off_snap, &prefs);
        assert_eq!(off.state, "Off");
        assert!(!off.can_start && !off.can_cancel);
    }

    #[test]
    fn start_now_is_offered_in_every_state_but_a_running_transition() {
        let prefs = AutomixPrefs {
            enabled: true,
            ..Default::default()
        };
        for state in [
            AutomixState::Idle,
            AutomixState::Loading,
            AutomixState::Armed,
            AutomixState::Cleanup,
        ] {
            assert!(
                AutomixBar::new(&snap(state), &prefs).can_start,
                "{state:?} accepts start"
            );
        }
        let running = AutomixBar::new(&snap(AutomixState::Transitioning), &prefs);
        assert!(!running.can_start, "a transition is already running");
        assert!(running.can_cancel);
        let mut off_snap = snap(AutomixState::Idle);
        off_snap.enabled = false;
        let off = AutomixBar::new(&off_snap, &prefs);
        assert!(!off.can_start, "disabled: nothing starts");
    }

    /// The controller turns itself off after an underrun while the preference still says On: the
    /// bar shows what runs (Off) and why, not the preference.
    #[test]
    fn bar_shows_the_running_flag_not_the_preference() {
        let prefs = AutomixPrefs {
            enabled: true,
            ..Default::default()
        };
        let mut snapshot = snap(AutomixState::Idle);
        snapshot.enabled = false;
        snapshot.last_error = "cancelled: audio underrun, automix is off".into();
        let bar = AutomixBar::new(&snapshot, &prefs);
        assert!(!bar.enabled);
        assert_eq!(bar.state, "Off");
        assert!(bar.error.contains("underrun"), "{}", bar.error);
    }

    #[test]
    fn skip_is_enabled_while_armed_loading_or_transitioning() {
        let prefs = AutomixPrefs {
            enabled: true,
            ..Default::default()
        };
        for (state, expected) in [
            (AutomixState::Idle, false),
            (AutomixState::Loading, true),
            (AutomixState::Armed, true),
            (AutomixState::Transitioning, true),
            (AutomixState::Cleanup, false),
        ] {
            assert_eq!(
                AutomixBar::new(&snap(state), &prefs).can_skip,
                expected,
                "{state:?}"
            );
        }
    }

    #[test]
    fn skip_is_disabled_while_automix_is_off() {
        let prefs = AutomixPrefs::default();
        for state in [
            AutomixState::Loading,
            AutomixState::Armed,
            AutomixState::Transitioning,
        ] {
            let mut off = snap(state);
            off.enabled = false;
            assert!(!AutomixBar::new(&off, &prefs).can_skip, "{state:?}");
        }
    }

    fn bar(state: AutomixState, enabled: bool) -> AutomixBar {
        let mut snapshot = snap(state);
        snapshot.enabled = enabled;
        AutomixBar::new(
            &snapshot,
            &AutomixPrefs {
                enabled,
                ..Default::default()
            },
        )
    }

    #[test]
    fn an_off_automix_offers_nothing_but_its_toggle() {
        let off = bar(AutomixState::Armed, false);
        assert_eq!(off.actions(), AutomixActions::default());
        assert!(!off.has_detail());
        assert!(!off.shows_progress());
    }

    #[test]
    fn each_state_offers_only_the_buttons_that_do_something() {
        let idle = bar(AutomixState::Idle, true).actions();
        assert_eq!(
            idle,
            AutomixActions {
                start: true,
                skip: false,
                cancel: false
            }
        );
        let armed = bar(AutomixState::Armed, true).actions();
        assert_eq!(
            armed,
            AutomixActions {
                start: false,
                skip: true,
                cancel: true
            },
            "an armed pick is skipped or cancelled; starting is the transition's own job"
        );
        let running = bar(AutomixState::Transitioning, true).actions();
        assert!(!running.start && running.skip && running.cancel);
        assert_eq!(running.count(), 2);
        assert!(bar(AutomixState::Transitioning, true).shows_progress());
    }

    /// The state word, progress and the running buttons only show while a pick runs.
    #[test]
    fn running_follows_the_pick() {
        for (state, running) in [
            (AutomixState::Idle, false),
            (AutomixState::Loading, true),
            (AutomixState::Armed, true),
            (AutomixState::Transitioning, true),
            (AutomixState::Cleanup, false),
        ] {
            assert_eq!(bar(state, true).running, running, "{state:?}");
        }
        assert!(!bar(AutomixState::Armed, false).running);
        assert_eq!(transition_text(16), "Transition 16 beats");
        assert_eq!(TOGGLE_LABEL, "AUTOMIX");
    }

    /// Both rows fit the strip the header gives the bar at the minimum window width: idle (switch,
    /// transition length, next track at its minimum, Start) and running (state, progress with
    /// beats left, Skip and Cancel; the next track gets whatever is left).
    #[test]
    fn both_rows_fit_the_header_at_the_minimum_window_width() {
        let idle = 2.0 * ROW_PADDING
            + LABEL_WIDTH
            + SWITCH_WIDTH
            + LENGTH_WIDTH
            + NEXT_MIN_WIDTH
            + START_WIDTH
            + 4.0 * ROW_GAP;
        let running = 2.0 * ROW_PADDING
            + LABEL_WIDTH
            + SWITCH_WIDTH
            + STATE_WIDTH
            + PROGRESS_WIDTH
            + REMAINING_WIDTH
            + SKIP_WIDTH
            + CANCEL_WIDTH
            + 6.0 * ROW_GAP;
        let (min_width, min_height) = crate::main_layout::MIN_WINDOW;
        let needs = crate::header_layout::HeaderNeeds {
            automix: true,
            record: true,
            settings: true,
            notice: false,
        };
        let header_h = crate::main_layout::header_height(min_height);
        let available = crate::header_layout::header_rects(min_width, header_h, needs)
            .automix
            .expect("bar")
            .w as f32;
        assert!(idle <= available, "idle row needs {idle} px of {available}");
        assert!(
            running <= available,
            "running row needs {running} px of {available}"
        );
    }

    #[test]
    fn style_label_follows_the_preference() {
        let prefs = AutomixPrefs {
            style: 4,
            ..Default::default()
        };
        assert_eq!(
            AutomixBar::new(&snap(AutomixState::Idle), &prefs).style,
            "Cut"
        );
    }
}

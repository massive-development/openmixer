//! The automix bar's view-model: what the main window's Automix strip shows, built from the
//! automix snapshot and the Automix preferences. Pure: no drawing and no service calls, so the
//! labels and the enabled rules are tested here and the window only lays them out.

use crate::preferences::AutomixPrefs;
use crate::settings::STYLE_LABELS;
use om_core::{AutomixSnapshot, AutomixState};

/// Height of the bar, in pixels. The bar shares the header strip (the same height as the settings
/// header), so the deck rows keep the room they had: the bar takes the strip's left part and the
/// Settings button keeps the right.
pub const BAR_HEIGHT: i32 = 32;

/// Widths of the bar's fixed-size parts, in pixels. The window lays the bar out with these; the
/// tests check that the row still fits at the minimum window width.
pub const STATE_WIDTH: f32 = 80.0;
pub const PROGRESS_WIDTH: f32 = 48.0;
pub const REMAINING_WIDTH: f32 = 72.0;
/// Gap between the row's items, and its left and right padding.
pub const ROW_GAP: f32 = 4.0;
pub const ROW_PADDING: f32 = 10.0;

/// What the bar shows.
#[derive(Debug, Clone, PartialEq)]
pub struct AutomixBar {
    /// The Enable toggle (the preference, which the snapshot follows).
    pub enabled: bool,
    /// The style the preference asks for.
    pub style: &'static str,
    pub length_beats: i32,
    /// Off, Idle, Loading, Armed, Transitioning or Cleanup.
    pub state: &'static str,
    /// `Next: <title> · <bpm> BPM · <key>`, or `Next: none`.
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
        AutomixState::Transitioning => "Transitioning",
        AutomixState::Cleanup => "Finishing",
    }
}

/// `Next: <title> · <bpm> BPM · <key>`, leaving out the parts that are unknown.
pub fn next_text(title: &str, bpm: Option<f64>, key: &str) -> String {
    if title.is_empty() {
        return "Next: none".to_owned();
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
        assert_eq!(
            state_label(AutomixState::Transitioning, true),
            "Transitioning"
        );
        assert_eq!(state_label(AutomixState::Armed, false), "Off");
    }

    #[test]
    fn next_line_leaves_out_what_is_unknown() {
        assert_eq!(
            next_text("Night Drive", Some(124.04), "8A"),
            "Next: Night Drive · 124.0 BPM · 8A"
        );
        assert_eq!(next_text("Cue", None, ""), "Next: Cue");
        assert_eq!(next_text("", Some(120.0), "8A"), "Next: none");
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

    /// A rough width of the bar's row with the widest labels: 6 px per glyph at the small type
    /// size, 28 px of chip padding, border and margin. It must fit the strip at the minimum
    /// window width, next to the Record button and the Settings reserve (96 px in the window).
    #[test]
    fn the_row_fits_the_minimum_window_width() {
        const GLYPH: f32 = 6.0;
        const CHIP: f32 = 28.0;
        let widest_style = STYLE_LABELS.iter().map(|l| l.len()).max().unwrap_or(0);
        let chips = [2, widest_style, "16 beats".len(), "Start now".len(), 6, 4];
        let chip_width: f32 = chips.iter().map(|n| *n as f32 * GLYPH + CHIP).sum();
        let label = "Automix".len() as f32 * GLYPH;
        // label, On, style, length, state, next, progress group, Start now, Skip, Cancel
        let items = 10.0;
        let width = 2.0 * ROW_PADDING
            + label
            + chip_width
            + STATE_WIDTH
            + PROGRESS_WIDTH
            + ROW_GAP
            + REMAINING_WIDTH
            + (items - 1.0) * ROW_GAP;
        let (min_width, _) = crate::main_layout::MIN_WINDOW;
        let available = crate::record_bar::automix_width(min_width, 96) as f32;
        assert!(
            width <= available,
            "bar needs about {width} px, {available} available"
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

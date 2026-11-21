//! The top bar's Automix strip and Record button: what they show and what a press does. The
//! view-models are om-ui's `AutomixBar` and `RecordBar` (the labels and the rules for which button
//! is offered, tested there); this module turns them into what the front end draws, and carries
//! the presses back to the app through a [`HeaderHost`]. The host is built by the binary, which
//! owns the automix service, the recorder and the preferences; its methods are called from Tauri's
//! command threads and must not block on the audio thread or the library.

use om_core::{AutomixSnapshot, RecordSnapshot};
use om_engine::DjEngine;
use om_ui::automix_bar::AutomixBar;
use om_ui::library::hub::SharedHub;
use om_ui::preferences::AutomixPrefs;
use om_ui::record_bar::RecordBar;
use om_ui::settings::{length_label, LENGTH_CHOICES};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tauri::State;

/// The three presses the automix bar offers, which the app sends as `automix.start`,
/// `automix.skip` and `automix.cancel`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AutomixAction {
    Start,
    Skip,
    Cancel,
}

impl AutomixAction {
    /// The router id the press goes to.
    pub fn id(self) -> &'static str {
        match self {
            Self::Start => "automix.start",
            Self::Skip => "automix.skip",
            Self::Cancel => "automix.cancel",
        }
    }
}

/// What the app gives the header: the two snapshots and the four things a press can do.
pub trait HeaderHost: Send + Sync {
    /// The automix controller's snapshot and the stored Automix settings.
    fn automix(&self) -> (AutomixSnapshot, AutomixPrefs);
    /// The recorder's snapshot and the last start or stop failure (empty when none).
    fn record(&self) -> (RecordSnapshot, String);
    /// The Off | On switch: turns the running controller on or off and stores the choice.
    fn set_automix_enabled(&self, on: bool);
    /// The length chip: stores the transition length (a `LENGTH_CHOICES` value) and applies it.
    fn set_automix_length(&self, beats: i32);
    /// Start, Skip or Cancel.
    fn press_automix(&self, action: AutomixAction);
    /// The Record button and the `r` key: `record.toggle`.
    fn toggle_record(&self);
}

/// The Automix strip as the front end draws it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomixBarDto {
    pub enabled: bool,
    /// A pick is loading, armed or mixing: the state word, progress, Skip and Cancel show.
    pub running: bool,
    /// Loading, Armed, Mixing and so on; shown while `running`.
    pub state: String,
    /// `Next: <title> · <bpm> BPM · <key>`, or `Next: Nothing queued`.
    pub next: String,
    /// 0 to 1 through the running transition.
    pub progress: f64,
    /// `12 beats left`; empty when no deck leads.
    pub remaining: String,
    /// The last error or the reason a plan was dropped; empty when none.
    pub error: String,
    pub length_beats: i32,
    /// `16 beats`, or `Auto`.
    pub length_label: String,
    /// The length a press of the chip steps to.
    pub next_length_beats: i32,
    pub can_start: bool,
    pub can_skip: bool,
    pub can_cancel: bool,
}

/// The Record button as the front end draws it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordBarDto {
    pub recording: bool,
    pub failed: bool,
    /// `Record`, the elapsed time while recording, or `Stopped` after a failure.
    pub label: String,
    /// `12.3 MB` written so far.
    pub size: String,
    /// `12 frames dropped`; empty when none were.
    pub warning: String,
    pub dropped_frames: u64,
    pub peak_left: f32,
    pub peak_right: f32,
    pub error: String,
}

/// Both parts of the header, read together.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeaderState {
    pub automix: AutomixBarDto,
    pub record: RecordBarDto,
}

/// The length a press of the chip steps to: the next larger choice, wrapping to the first.
pub fn next_length(current: i32) -> i32 {
    LENGTH_CHOICES
        .iter()
        .copied()
        .find(|&beats| beats > current)
        .unwrap_or(LENGTH_CHOICES[0])
}

/// The Automix strip for `snapshot` and `prefs`.
pub fn automix_dto(snapshot: &AutomixSnapshot, prefs: &AutomixPrefs) -> AutomixBarDto {
    let bar = AutomixBar::new(snapshot, prefs);
    let actions = bar.actions();
    AutomixBarDto {
        enabled: bar.enabled,
        running: bar.running,
        state: bar.state.to_owned(),
        next: bar.next,
        progress: bar.progress,
        remaining: bar.remaining,
        error: bar.error,
        length_beats: bar.length_beats,
        length_label: length_label(bar.length_beats),
        next_length_beats: next_length(bar.length_beats),
        can_start: actions.start,
        can_skip: actions.skip,
        can_cancel: actions.cancel,
    }
}

/// The Record button for `snapshot`.
pub fn record_dto(snapshot: &RecordSnapshot, error: &str) -> RecordBarDto {
    let bar = RecordBar::new(snapshot, error);
    RecordBarDto {
        recording: bar.recording,
        failed: bar.failed,
        label: bar.label,
        size: bar.size,
        warning: bar.warning,
        dropped_frames: snapshot.dropped_frames,
        peak_left: bar.peak_left,
        peak_right: bar.peak_right,
        error: bar.error,
    }
}

/// The header as `host` has it now.
pub fn state(host: &dyn HeaderHost) -> HeaderState {
    let (automix, prefs) = host.automix();
    let (record, error) = host.record();
    HeaderState {
        automix: automix_dto(&automix, &prefs),
        record: record_dto(&record, &error),
    }
}

/// What the control thread runs each tick (30 Hz) with the engine and the seconds since it
/// started: the automix runner's step. Built on that thread, so it may hold the library hub.
pub type Ticker = Box<dyn FnMut(&DjEngine, f64)>;

/// Builds the [`Ticker`] on the control thread, once the library hub exists. Not called for a run
/// with no library.
pub type TickSetup = Box<dyn FnOnce(&SharedHub) -> Ticker + Send>;

/// What the binary installs before the window runs: the host the commands call, and the step the
/// control thread runs beside the library (automix reads the library and the track loader, which
/// stay on that thread).
pub struct HeaderParts {
    pub host: Arc<dyn HeaderHost>,
    pub tick: Option<TickSetup>,
}

static INSTALLED: Mutex<Option<HeaderParts>> = Mutex::new(None);

/// Keeps `parts` for the next [`run`](crate::run); without it the header commands refuse and the
/// front end hides the strip. Not a parameter of `run`, so the pages that each add a part do not
/// all change its signature.
pub fn install_header(parts: HeaderParts) {
    *crate::lock(&INSTALLED) = Some(parts);
}

/// Takes what [`install_header`] kept.
pub(crate) fn take_installed() -> Option<HeaderParts> {
    crate::lock(&INSTALLED).take()
}

/// The header as it stands now, 10 Hz from the front end.
#[tauri::command]
pub fn header_state(slot: State<'_, HeaderSlot>) -> Result<HeaderState, String> {
    slot.host().map(state)
}

/// The Off | On switch.
#[tauri::command]
pub fn automix_enable(on: bool, slot: State<'_, HeaderSlot>) -> Result<(), String> {
    slot.host()?.set_automix_enabled(on);
    Ok(())
}

/// The length chip: `beats` must be one of the usual choices.
#[tauri::command]
pub fn automix_length(beats: i32, slot: State<'_, HeaderSlot>) -> Result<(), String> {
    if !LENGTH_CHOICES.contains(&beats) {
        return Err(format!("{beats} beats is not a transition length"));
    }
    slot.host()?.set_automix_length(beats);
    Ok(())
}

/// Start, Skip or Cancel.
#[tauri::command]
pub fn automix_press(action: AutomixAction, slot: State<'_, HeaderSlot>) -> Result<(), String> {
    slot.host()?.press_automix(action);
    Ok(())
}

/// The Record button and the `r` key.
#[tauri::command]
pub fn record_toggle(slot: State<'_, HeaderSlot>) -> Result<(), String> {
    slot.host()?.toggle_record();
    Ok(())
}

/// The host the commands share; `None` for a run with no automix or recorder.
pub(crate) struct HeaderSlot(pub(crate) Option<Arc<dyn HeaderHost>>);

impl HeaderSlot {
    pub(crate) fn host(&self) -> Result<&dyn HeaderHost, String> {
        self.0
            .as_deref()
            .ok_or_else(|| "automix and recording are not available here".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use om_core::AutomixState;

    fn running() -> AutomixSnapshot {
        AutomixSnapshot {
            enabled: true,
            state: AutomixState::Armed,
            next_track_title: "Night Drive".into(),
            next_track_bpm: Some(124.0),
            next_track_key: "8A".into(),
            progress: 0.25,
            remaining_beats: Some(31.6),
            ..Default::default()
        }
    }

    #[test]
    fn the_chip_steps_through_the_lengths_and_wraps() {
        assert_eq!(next_length(0), 8);
        assert_eq!(next_length(8), 16);
        assert_eq!(next_length(16), 32);
        assert_eq!(next_length(32), 0);
    }

    #[test]
    fn an_off_automix_offers_no_button() {
        let dto = automix_dto(&AutomixSnapshot::default(), &AutomixPrefs::default());
        assert!(!dto.enabled && !dto.running);
        assert!(!dto.can_start && !dto.can_skip && !dto.can_cancel);
    }

    #[test]
    fn an_armed_pick_offers_skip_and_cancel_but_not_start() {
        let dto = automix_dto(&running(), &AutomixPrefs::default());
        assert!(dto.running && dto.can_skip && dto.can_cancel && !dto.can_start);
        assert_eq!(dto.state, "Armed");
        assert_eq!(dto.next, "Next: Night Drive · 124.0 BPM · 8A");
        assert_eq!(dto.remaining, "32 beats left");
    }

    #[test]
    fn the_length_label_says_auto_for_zero() {
        let prefs = AutomixPrefs {
            length_beats: 0,
            ..AutomixPrefs::default()
        };
        assert_eq!(automix_dto(&running(), &prefs).length_label, "Auto");
        let prefs = AutomixPrefs {
            length_beats: 16,
            ..AutomixPrefs::default()
        };
        assert_eq!(automix_dto(&running(), &prefs).length_label, "16 beats");
    }

    #[test]
    fn the_record_button_reads_idle_recording_and_failed() {
        let idle = record_dto(&RecordSnapshot::default(), "");
        assert_eq!((idle.recording, idle.label.as_str()), (false, "Record"));
        let live = record_dto(
            &RecordSnapshot {
                recording: true,
                elapsed_s: 75.0,
                bytes: 12_300_000,
                dropped_frames: 3,
                peak_left: 1.5,
                ..Default::default()
            },
            "",
        );
        assert_eq!(live.label, "01:15");
        assert_eq!(live.size, "12.3 MB");
        assert_eq!(live.warning, "3 frames dropped");
        assert_eq!(live.peak_left, 1.0, "a peak over full scale is clamped");
        let failed = record_dto(
            &RecordSnapshot {
                failed: true,
                ..Default::default()
            },
            "disk full",
        );
        assert_eq!((failed.failed, failed.label.as_str()), (true, "Stopped"));
        assert_eq!(failed.error, "disk full");
    }

    #[test]
    fn the_state_serialises_in_camel_case() {
        let json = serde_json::to_string(&HeaderState {
            automix: automix_dto(&running(), &AutomixPrefs::default()),
            record: record_dto(&RecordSnapshot::default(), ""),
        })
        .unwrap();
        for key in [
            "\"lengthBeats\"",
            "\"canSkip\"",
            "\"peakLeft\"",
            "\"droppedFrames\"",
        ] {
            assert!(json.contains(key), "{key}: {json}");
        }
    }

    #[test]
    fn an_action_reads_its_word_and_names_its_router_id() {
        let a: AutomixAction = serde_json::from_str("\"skip\"").unwrap();
        assert_eq!(a.id(), "automix.skip");
        assert!(serde_json::from_str::<AutomixAction>("\"explode\"").is_err());
    }
}

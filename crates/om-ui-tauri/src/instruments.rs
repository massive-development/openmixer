//! The sampler pads and the deck stems as the front end sees them: wire forms for each, the
//! commands' shared state, and the control thread's step that works the stem actions and reports
//! what changed. The models are `om_ui::sampler_binding` and `om_ui::stem_binding`; nothing here
//! runs on the audio thread.

use crate::lock;
use om_engine::sampler::{PadMode, MAX_PAD_GAIN, MAX_PITCH_SEMITONES};
use om_ui::library::hub::{LibraryHub, LibraryPanel};
use om_ui::sampler_binding::{mode_hint, mode_label, PadView, SamplerBinding};
use om_ui::stem_binding::{StemBinding, FILTER_DETENT, MAX_VOLUME, STEM_NAMES};
use om_ui::stems::{StemJob, StemLabel};
use serde::Serialize;
use std::path::Path;
use std::sync::Mutex;

/// Velocity of a click or key press on a pad.
const PRESS_VELOCITY: f32 = 0.9;
/// Event carrying the sampler's [`SamplerDto`] whenever it changes.
pub const SAMPLER_EVENT: &str = "sampler-state";
/// Event carrying one deck's [`StemsDto`] whenever it changes.
pub const STEMS_EVENT: &str = "stems-state";

/// One pad as the front end draws it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PadDto {
    pub pad: usize,
    /// The key that presses it (`1` to `8`).
    pub key: String,
    pub name: String,
    pub loaded: bool,
    pub missing: bool,
    pub loading: bool,
    pub playing: bool,
    pub gain: f32,
    pub pitch: f32,
    /// `one-shot`, `gate` or `loop`.
    pub mode: &'static str,
    pub selected: bool,
}

/// The sampler strip: the pads, the selected pad's caption and the ranges its controls take.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SamplerDto {
    /// False when the app has no sample bank (a headless or failed start).
    pub available: bool,
    pub pads: Vec<PadDto>,
    pub selected: usize,
    /// One line about the selected pad, or a sentence when it is empty.
    pub caption: String,
    /// What the selected pad's mode does.
    pub mode_hint: &'static str,
    /// The last problem a sampler action had, until the next action.
    pub status: Option<String>,
    pub max_gain: f32,
    pub max_pitch: f32,
    /// The velocity a click or key press hits a pad with (a full-scale hit leaves headroom).
    pub press_velocity: f32,
}

/// One stem of a deck.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StemDto {
    pub name: &'static str,
    pub muted: bool,
    pub soloed: bool,
    pub volume: f32,
    /// -1 (low-pass) to +1 (high-pass), 0 off.
    pub filter: f32,
    /// How loud the stem sounds, 0..1, for the activity fill.
    pub level: f32,
}

/// A deck's stems: whether it plays from them and what the track's stems are doing.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StemsDto {
    pub deck: usize,
    /// The deck plays from stems now.
    pub live: bool,
    /// The loaded track has a complete cached set to load.
    pub cached: bool,
    /// The deck has a track in the library, so its stems can be prepared.
    pub can_prepare: bool,
    /// The track's separation: `None`, `Preparing 42%`, `Ready` or `Failed`.
    pub label: String,
    /// Percent while a separation runs.
    pub preparing: Option<u32>,
    /// Why the last stems action did not happen, until the next one.
    pub note: Option<String>,
    pub stems: Vec<StemDto>,
    pub max_volume: f64,
    /// A filter value within this of the centre snaps to off.
    pub filter_detent: f64,
}

/// An action for the control thread, which owns the library services.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StemAction {
    /// 'Load stems' / 'Stems off'.
    Toggle(usize),
    /// Separate the deck's track.
    Prepare(usize),
}

/// What the commands share: the sampler model, one stem model per deck, and the queued actions.
pub(crate) struct Instruments {
    pub(crate) sampler: Option<SamplerBinding>,
    pub(crate) stems: Vec<StemBinding>,
    sampler_status: Mutex<Option<String>>,
    actions: Mutex<Vec<StemAction>>,
    notes: Mutex<Vec<Option<String>>>,
    /// Each deck's stems as the control thread last reported them, for `stems_state`.
    latest: Mutex<Vec<StemsDto>>,
}

impl Instruments {
    pub(crate) fn new(sampler: Option<SamplerBinding>, stems: Vec<StemBinding>) -> Self {
        let notes = vec![None; stems.len()];
        Self {
            sampler,
            stems,
            sampler_status: Mutex::new(None),
            actions: Mutex::new(Vec::new()),
            notes: Mutex::new(notes),
            latest: Mutex::new(Vec::new()),
        }
    }

    /// Runs one sampler action and keeps its failure for the strip's caption.
    pub(crate) fn sampler_action<T, E: std::fmt::Display>(
        &self,
        run: impl FnOnce(&SamplerBinding) -> Result<T, E>,
    ) -> Result<(), String> {
        let Some(sampler) = &self.sampler else {
            return Err("the sampler is not running".to_owned());
        };
        let result = run(sampler).map(|_| ()).map_err(|e| e.to_string());
        *lock(&self.sampler_status) = result.clone().err();
        result
    }

    pub(crate) fn load_pad(&self, pad: usize, path: &Path) -> Result<(), String> {
        let loaded = self.sampler_action(|s| s.load(pad, path));
        if loaded.is_ok() {
            *lock(&self.sampler_status) = None;
        }
        loaded
    }

    pub(crate) fn sampler_dto(&self) -> SamplerDto {
        let Some(sampler) = &self.sampler else {
            return SamplerDto::unavailable();
        };
        let pads: Vec<PadDto> = sampler.pads().into_iter().map(pad_dto).collect();
        let selected = sampler.selected();
        let mode = sampler.mode(selected).unwrap_or(PadMode::OneShot);
        let status = lock(&self.sampler_status).clone();
        SamplerDto {
            available: true,
            caption: sampler.caption(),
            mode_hint: mode_hint(mode),
            pads,
            selected,
            status,
            max_gain: MAX_PAD_GAIN,
            max_pitch: MAX_PITCH_SEMITONES,
            press_velocity: PRESS_VELOCITY,
        }
    }

    /// Queues a stems action for the control thread.
    pub(crate) fn queue(&self, action: StemAction) -> Result<(), String> {
        let deck = match action {
            StemAction::Toggle(d) | StemAction::Prepare(d) => d,
        };
        if deck >= self.stems.len() {
            return Err(format!("no deck {deck}"));
        }
        lock(&self.actions).push(action);
        Ok(())
    }

    fn set_note(&self, deck: usize, note: Option<String>) {
        if let Some(slot) = lock(&self.notes).get_mut(deck) {
            *slot = note;
        }
    }

    /// Deck `deck`'s stems from the engine and the facts the control thread holds.
    pub(crate) fn stems_dto(&self, deck: usize, facts: &StemFacts) -> Option<StemsDto> {
        let binding = self.stems.get(deck)?;
        let soloed = binding.soloed();
        let snapshot = binding.stems();
        let stems = (0..STEM_NAMES.len())
            .map(|i| StemDto {
                name: STEM_NAMES[i],
                muted: snapshot[i].muted,
                soloed: soloed == Some(i),
                volume: snapshot[i].gain,
                filter: snapshot[i].filter,
                level: binding.level(i),
            })
            .collect();
        let note = lock(&self.notes).get(deck).cloned().flatten();
        Some(StemsDto {
            deck,
            live: binding.has_stems(),
            cached: facts.cached,
            can_prepare: facts.can_prepare,
            label: facts.label.clone(),
            preparing: facts.preparing,
            note: note.or_else(|| facts.loader_note.clone()),
            stems,
            max_volume: MAX_VOLUME,
            filter_detent: FILTER_DETENT,
        })
    }
}

impl SamplerDto {
    fn unavailable() -> Self {
        Self {
            available: false,
            pads: Vec::new(),
            selected: 0,
            caption: String::new(),
            mode_hint: "",
            status: Some("No sample bank is open.".to_owned()),
            max_gain: MAX_PAD_GAIN,
            max_pitch: MAX_PITCH_SEMITONES,
            press_velocity: PRESS_VELOCITY,
        }
    }
}

fn pad_dto(pad: PadView) -> PadDto {
    PadDto {
        pad: pad.pad,
        key: pad.key.to_string(),
        name: pad.name,
        loaded: pad.loaded,
        missing: pad.missing,
        loading: pad.loading,
        playing: pad.playing,
        gain: pad.gain,
        pitch: pad.pitch,
        mode: mode_label(pad.mode),
        selected: pad.selected,
    }
}

/// What the control thread knows about a deck's track and its stems.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct StemFacts {
    pub(crate) cached: bool,
    pub(crate) can_prepare: bool,
    pub(crate) label: String,
    pub(crate) preparing: Option<u32>,
    pub(crate) loader_note: Option<String>,
}

/// What the instruments report after a tick: the sampler and each deck's stems, when changed.
#[derive(Default)]
pub(crate) struct Reported {
    sampler: Option<SamplerDto>,
    stems: Vec<Option<StemsDto>>,
}

/// One event for the front end.
pub(crate) enum InstrumentEvent {
    Sampler(SamplerDto),
    Stems(StemsDto),
}

impl Reported {
    /// The changes since the last call, given what the instruments are now.
    pub(crate) fn changes(
        &mut self,
        sampler: SamplerDto,
        stems: Vec<StemsDto>,
    ) -> Vec<InstrumentEvent> {
        let mut out = Vec::new();
        if self.sampler.as_ref() != Some(&sampler) {
            self.sampler = Some(sampler.clone());
            out.push(InstrumentEvent::Sampler(sampler));
        }
        self.stems.resize(stems.len(), None);
        for (slot, now) in self.stems.iter_mut().zip(stems) {
            if slot.as_ref() != Some(&now) {
                *slot = Some(now.clone());
                out.push(InstrumentEvent::Stems(now));
            }
        }
        out
    }
}

impl Instruments {
    /// The control thread's step: runs the queued stem actions, then reports what changed.
    pub(crate) fn tick(
        &self,
        mut panel: Option<&mut LibraryPanel>,
        reported: &mut Reported,
    ) -> Vec<InstrumentEvent> {
        let actions = std::mem::take(&mut *lock(&self.actions));
        for action in actions {
            let result = match (action, panel.as_deref_mut()) {
                (_, None) => Err("no library is open".to_owned()),
                (StemAction::Toggle(deck), Some(panel)) => toggle(panel, deck),
                (StemAction::Prepare(deck), Some(panel)) => prepare(panel, deck),
            };
            let (StemAction::Toggle(deck) | StemAction::Prepare(deck)) = action;
            self.set_note(deck, result.err());
        }
        for (deck, binding) in self.stems.iter().enumerate() {
            let track = panel.as_deref().and_then(|p| loaded_track(p, deck));
            binding.observe_track(track);
        }
        if let Some(sampler) = &self.sampler {
            sampler.prune_pending();
        }
        let stems: Vec<StemsDto> = (0..self.stems.len())
            .filter_map(|deck| self.stems_dto(deck, &facts(deck, panel.as_deref())))
            .collect();
        *lock(&self.latest) = stems.clone();
        reported.changes(self.sampler_dto(), stems)
    }
}

impl Instruments {
    /// Deck `deck`'s stems at the last tick.
    pub(crate) fn stems_latest(&self, deck: usize) -> Option<StemsDto> {
        lock(&self.latest).get(deck).cloned()
    }

    /// Runs one write on deck `deck`'s stems; `Err` when the deck is unknown or the write fails.
    pub(crate) fn stem_write(
        &self,
        deck: usize,
        run: impl FnOnce(&StemBinding) -> bool,
    ) -> Result<(), String> {
        let binding = self
            .stems
            .get(deck)
            .ok_or_else(|| format!("no deck {deck}"))?;
        if run(binding) {
            Ok(())
        } else {
            Err("the engine did not take the stem change".to_owned())
        }
    }
}

fn loaded_track(panel: &LibraryPanel, deck: usize) -> Option<i64> {
    panel.hub().borrow().loader.loaded_track(deck).map(|t| t.id)
}

fn toggle(panel: &mut LibraryPanel, deck: usize) -> Result<(), String> {
    panel
        .toggle_stems(deck)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

// Separates the deck's track through the hub's host and queue, as the library's Prepare does.
fn prepare(panel: &LibraryPanel, deck: usize) -> Result<(), String> {
    let mut hub = panel.hub().borrow_mut();
    let LibraryHub {
        services,
        loader,
        stems,
        stem_host,
        ..
    } = &mut *hub;
    let track = loader.loaded_track(deck).ok_or("no track is loaded")?;
    let job = StemJob {
        track_id: track.id,
        path: track.path.clone().into(),
        content_hash: track.content_hash.clone(),
    };
    let host = stem_host.as_deref_mut().ok_or(NO_SEPARATION)?;
    stems
        .prepare(host, services.library(), &[job])
        .map(|_| ())
        .map_err(|e| e.to_string())
}

const NO_SEPARATION: &str = "stem separation is not available";

fn facts(deck: usize, panel: Option<&LibraryPanel>) -> StemFacts {
    let Some(panel) = panel else {
        return StemFacts {
            label: "None".to_owned(),
            ..StemFacts::default()
        };
    };
    let hub = panel.hub().borrow();
    let loader_note = hub.loader.stem_note(deck).map(str::to_owned);
    let Some(track) = hub.loader.loaded_track(deck) else {
        return StemFacts {
            label: "None".to_owned(),
            loader_note,
            ..StemFacts::default()
        };
    };
    let label = match hub.stem_host.as_deref() {
        None => StemLabel::None,
        Some(host) => {
            let (sep, version) = host.separator();
            hub.stems
                .label(hub.services.library(), track.id, (&sep, version))
        }
    };
    let preparing = match label {
        StemLabel::Preparing(percent) => Some(percent),
        _ => None,
    };
    StemFacts {
        cached: hub
            .loader
            .cached_stems(hub.services.library(), track.id)
            .is_some(),
        can_prepare: hub.stem_host.is_some(),
        label: label.text(),
        preparing,
        loader_note,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unchanged_state_reports_nothing() {
        let mut reported = Reported::default();
        let sampler = SamplerDto::unavailable();
        assert_eq!(reported.changes(sampler.clone(), Vec::new()).len(), 1);
        assert!(reported.changes(sampler, Vec::new()).is_empty());
    }

    #[test]
    fn a_missing_bank_says_so_and_refuses_actions() {
        let instruments = Instruments::new(None, Vec::new());
        let dto = instruments.sampler_dto();
        assert!(!dto.available);
        assert!(instruments.sampler_action(|s| s.clear(0)).is_err());
        assert!(instruments.queue(StemAction::Toggle(0)).is_err());
    }

    #[test]
    fn the_pad_wire_form_is_camel_case_with_the_mode_name() {
        let pad = PadDto {
            pad: 0,
            key: "1".into(),
            name: "Kick".into(),
            loaded: true,
            missing: false,
            loading: false,
            playing: false,
            gain: 1.0,
            pitch: 0.0,
            mode: "loop",
            selected: true,
        };
        let json = serde_json::to_string(&pad).unwrap();
        assert!(json.contains("\"mode\":\"loop\""));
        assert!(!json.contains('_'));
    }
}

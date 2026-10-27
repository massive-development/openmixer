//! Drag interaction for faders, knobs and sliders. A press starts a drag on one control; pointer
//! moves set its value through the om-ui model (which sends through the control surface), and the
//! release ends it. Faders and sliders map the pointer onto the value range directly; knobs move by
//! the pointer's travel, so a click does not jump them.

use crate::app::Shell;
use crate::controls::{key_step, wheel_step};
use crate::fader_paint::FaderLaw;
use crate::knob_paint::{dragged_value, snap_with_bypass};
use gpui::Point;
use om_engine::sampler::{MAX_PAD_GAIN, MAX_PITCH_SEMITONES};
use om_ui::geometry::Rect;

/// A control that a drag can change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// Deck pitch fader.
    Pitch(usize),
    /// Channel fader.
    Channel(usize),
    Master,
    Crossfader,
    Trim(usize),
    Filter(usize),
    Eq(usize, usize),
    CueMix,
    CueLevel,
    FxMix,
    FxParam(usize),
    /// The selected sampler pad's gain (linear, unity detent).
    SamplerGain,
    /// The selected sampler pad's pitch in semitones (0 detent).
    SamplerPitch,
    /// A stem's volume slider (deck, stem).
    StemVolume(usize, usize),
    /// A stem's filter slider, horizontal with a centre detent (deck, stem).
    StemFilter(usize, usize),
}

impl Target {
    /// Knobs move by travel along their sweep; faders and sliders follow the pointer.
    pub(crate) fn is_knob(self) -> bool {
        matches!(
            self,
            Target::Trim(_)
                | Target::Filter(_)
                | Target::Eq(..)
                | Target::FxMix
                | Target::FxParam(_)
                | Target::CueMix
                | Target::CueLevel
                | Target::SamplerGain
                | Target::SamplerPitch
        )
    }
}

/// The drag in progress.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Drag {
    pub target: Target,
    /// The control's rectangle in window coordinates.
    pub rect: Rect,
    /// Vertical axis (faders and knobs) or horizontal (the crossfader).
    pub vertical: bool,
    /// Pointer position on the drag axis when the press began.
    pub anchor: f64,
    /// The value when the press began.
    pub anchor_value: f64,
    /// Shift was held at the press: a knob moves a tenth as far and skips its detent pull.
    pub fine: bool,
}

/// Axis for a slider drawn in `rect`: the longer side.
pub fn vertical_for(rect: Rect) -> bool {
    rect.h >= rect.w
}

/// Fraction of `value` within `min..=max`, 0..=1.
pub fn fraction(value: f64, min: f64, max: f64) -> f64 {
    if max > min {
        ((value - min) / (max - min)).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

impl Shell {
    /// Starts a drag on `target` at the pointer and applies the value under the pointer at once.
    pub(crate) fn begin_drag(&mut self, target: Target, rect: Rect, pos: Point<gpui::Pixels>) {
        self.begin_drag_fine(target, rect, pos, false);
    }

    /// `begin_drag` with fine mode (shift held at the press) for knobs.
    pub(crate) fn begin_drag_fine(
        &mut self,
        target: Target,
        rect: Rect,
        pos: Point<gpui::Pixels>,
        fine: bool,
    ) {
        let vertical = if target == Target::Crossfader {
            false
        } else {
            vertical_for(rect) || target.is_knob()
        };
        let anchor = axis_of(pos, vertical);
        self.drag = Some(Drag {
            target,
            rect,
            vertical,
            anchor,
            anchor_value: self.value_of(target),
            fine,
        });
        self.drag_move(pos);
    }

    /// Applies the pointer position to the drag in progress, if any.
    pub(crate) fn drag_move(&mut self, pos: Point<gpui::Pixels>) {
        let Some(drag) = self.drag else { return };
        let (min, max) = self.range_of(drag.target);
        let value = if drag.target.is_knob() {
            // Up is more. Travel turns the pointer: a full sweep is KNOB_TRAVEL_PX.
            let dy = drag.anchor - axis_of(pos, true);
            let detent = self.detent_of(drag.target);
            dragged_value(drag.anchor_value, dy, drag.fine, min, detent, max)
        } else if drag.vertical {
            let frac =
                (axis_of(pos, true) - f64::from(drag.rect.y)) / f64::from(drag.rect.h.max(1));
            let travel = 1.0 - frac.clamp(0.0, 1.0);
            match fader_law(drag.target) {
                // The cap position goes through the taper; the stored value stays linear gain.
                Some(law) => f64::from(law.gain_at(travel as f32)),
                None => min + travel * (max - min),
            }
        } else {
            let frac =
                (axis_of(pos, false) - f64::from(drag.rect.x)) / f64::from(drag.rect.w.max(1));
            min + frac.clamp(0.0, 1.0) * (max - min)
        };
        let value = value.clamp(min, max);
        // A press alone never moves a knob, so the detent only pulls once the pointer has moved.
        let value = match self.knob_of(drag.target) {
            Some(k) if (value - drag.anchor_value).abs() > 1e-9 => {
                snap_with_bypass(k, value, drag.fine)
            }
            _ => value,
        };
        self.apply(drag.target, value);
    }

    /// True while `target` is the control under the pointer drag: it shows its value popup.
    pub(crate) fn is_dragging(&self, target: Target) -> bool {
        self.drag.is_some_and(|d| d.target == target)
    }

    /// True while a knob is dragged or hovered: it shows its value in a popup.
    pub(crate) fn shows_value(&self, target: Target) -> bool {
        self.is_dragging(target) || self.hovered == Some(target)
    }

    /// Puts a mixer control back to its default (a double-click). Returns false for controls that
    /// have no default to return to (the deck pitch fader and the stems), which drag as usual.
    pub(crate) fn reset_target(&mut self, target: Target) -> bool {
        let Some(default) = self.default_of(target) else {
            return false;
        };
        self.apply(target, default);
        true
    }

    /// The model of a knob target, for its detent.
    fn knob_of(&self, target: Target) -> Option<&om_ui::control::Knob> {
        match target {
            Target::Trim(c) => Some(&self.mixer.strips[c].trim),
            Target::Filter(c) => Some(&self.mixer.strips[c].filter),
            Target::Eq(c, b) => Some(&self.mixer.strips[c].eq[b]),
            Target::FxMix => Some(&self.mixer.fx.mix),
            Target::FxParam(i) => self.mixer.fx.params.get(i),
            _ => None,
        }
    }

    /// The value a knob's pointer rests at 12 o'clock on, if it has one. The master and phones
    /// level mark unity; the cue mix has none.
    pub(crate) fn detent_of(&self, target: Target) -> Option<f64> {
        match target {
            Target::Master | Target::CueLevel | Target::SamplerGain => Some(1.0),
            Target::SamplerPitch => Some(0.0),
            _ => self.knob_of(target).and_then(|k| k.detent),
        }
    }

    /// Moves a knob by `notches` wheel notches (positive: more).
    pub(crate) fn knob_wheel(&mut self, target: Target, notches: f64, shift: bool) {
        let (min, max) = self.range_of(target);
        let value = wheel_step(
            self.value_of(target),
            notches,
            shift,
            min,
            self.detent_of(target),
            max,
        );
        self.apply(target, value);
    }

    /// Applies a key to a focused knob. Returns false for a key the knob does not use.
    pub(crate) fn knob_key(&mut self, target: Target, key: &str, shift: bool) -> bool {
        let (min, max) = self.range_of(target);
        let reset = self.default_of(target).unwrap_or(min);
        let detent = self.detent_of(target);
        let value = self.value_of(target);
        match key_step(key, shift, (value, reset), (min, detent, max)) {
            Some(v) => {
                self.apply(target, v);
                true
            }
            None => false,
        }
    }

    fn default_of(&self, target: Target) -> Option<f64> {
        match target {
            Target::Channel(c) => Some(self.mixer.strips[c].fader.default),
            Target::Master => Some(self.mixer.master.fader.range.default),
            Target::Crossfader => Some(self.mixer.crossfader.slider.default),
            Target::CueMix => Some(self.mixer.cue.mix.range.default),
            Target::CueLevel => Some(self.mixer.cue.level.range.default),
            Target::SamplerGain => Some(1.0),
            Target::SamplerPitch => Some(0.0),
            Target::Pitch(_) | Target::StemVolume(..) | Target::StemFilter(..) => None,
            knob => self.knob_of(knob).map(|k| k.range.default),
        }
    }

    /// Ends the drag (pointer released).
    pub(crate) fn end_drag(&mut self) {
        self.drag = None;
    }

    /// The dragged control shows the pointer: fader drags are excluded from pulling the model.
    pub(crate) fn dragging_channel_fader(&self) -> bool {
        matches!(
            self.drag.map(|d| d.target),
            Some(Target::Channel(_) | Target::Master)
        )
    }

    fn value_of(&self, target: Target) -> f64 {
        match target {
            Target::Pitch(d) => self.decks[d].pitch.fader_value(),
            Target::Channel(c) => self.mixer.strips[c].fader.value(),
            Target::Master => self.mixer.master.fader.value(),
            Target::Crossfader => self.mixer.crossfader.slider.value(),
            Target::Trim(c) => self.mixer.strips[c].trim.value(),
            Target::Filter(c) => self.mixer.strips[c].filter.value(),
            Target::Eq(c, b) => self.mixer.strips[c].eq[b].value(),
            Target::CueMix => self.mixer.cue.mix.value(),
            Target::CueLevel => self.mixer.cue.level.value(),
            Target::FxMix => self.mixer.fx.mix.value(),
            Target::FxParam(i) => self.mixer.fx.params[i].value(),
            Target::SamplerGain => f64::from(self.sampler_selected_pad().gain),
            Target::SamplerPitch => f64::from(self.sampler_selected_pad().pitch),
            Target::StemVolume(d, s) => f64::from(self.decks[d].stems.stems()[s].gain),
            Target::StemFilter(d, s) => f64::from(self.decks[d].stems.stems()[s].filter),
        }
    }

    fn range_of(&self, target: Target) -> (f64, f64) {
        match target {
            Target::Pitch(d) => {
                let r = self.decks[d].pitch.range();
                (1.0 - r, 1.0 + r)
            }
            Target::Channel(c) => (
                self.mixer.strips[c].fader.min,
                self.mixer.strips[c].fader.max,
            ),
            Target::Master => knob_range(&self.mixer.master.fader),
            Target::Crossfader => (
                self.mixer.crossfader.slider.min,
                self.mixer.crossfader.slider.max,
            ),
            Target::Trim(c) => knob_range(&self.mixer.strips[c].trim),
            Target::Filter(c) => knob_range(&self.mixer.strips[c].filter),
            Target::Eq(c, b) => knob_range(&self.mixer.strips[c].eq[b]),
            Target::CueMix => knob_range(&self.mixer.cue.mix),
            Target::CueLevel => knob_range(&self.mixer.cue.level),
            Target::FxMix => knob_range(&self.mixer.fx.mix),
            Target::FxParam(i) => knob_range(&self.mixer.fx.params[i]),
            Target::SamplerGain => (0.0, f64::from(MAX_PAD_GAIN)),
            Target::SamplerPitch => (
                -f64::from(MAX_PITCH_SEMITONES),
                f64::from(MAX_PITCH_SEMITONES),
            ),
            Target::StemVolume(..) => (0.0, om_ui::stem_binding::MAX_VOLUME),
            Target::StemFilter(..) => (-1.0, 1.0),
        }
    }

    fn apply(&mut self, target: Target, value: f64) {
        match target {
            Target::Pitch(d) => {
                self.decks[d].pitch.user_set(value);
            }
            Target::Channel(c) => {
                self.mixer.strips[c].fader_changed(value);
            }
            Target::Master => {
                self.mixer.master.fader_changed(value);
            }
            Target::Crossfader => {
                self.mixer.crossfader.slider_changed(value);
            }
            Target::Trim(c) => {
                self.mixer.strips[c].trim_changed(value);
            }
            Target::Filter(c) => {
                self.mixer.strips[c].filter_changed(value);
            }
            Target::Eq(c, b) => {
                self.mixer.strips[c].eq_changed(b, value);
            }
            Target::CueMix => {
                self.mixer.cue.mix_changed(value);
            }
            Target::CueLevel => {
                self.mixer.cue.level_changed(value);
            }
            Target::FxMix => {
                self.mixer.fx.mix_changed(value);
            }
            Target::FxParam(i) => {
                self.mixer.fx.param_changed(i, value);
            }
            Target::SamplerGain => self.sampler_set_gain(value as f32),
            Target::SamplerPitch => self.sampler_set_pitch(value as f32),
            Target::StemVolume(d, s) => {
                self.decks[d].stems.set_volume(s, value);
            }
            Target::StemFilter(d, s) => {
                self.decks[d].stems.set_filter(s, value);
            }
        }
    }
}

/// The taper a dragged fader follows from cap position to stored gain; None for the linear ones.
fn fader_law(target: Target) -> Option<FaderLaw> {
    match target {
        Target::Channel(_) => Some(FaderLaw::Channel),
        Target::Master => Some(FaderLaw::Master),
        _ => None,
    }
}

fn knob_range(knob: &om_ui::control::Knob) -> (f64, f64) {
    (knob.range.min, knob.range.max)
}

fn axis_of(pos: Point<gpui::Pixels>, vertical: bool) -> f64 {
    let v = if vertical { pos.y } else { pos.x };
    f64::from(f32::from(v))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{load_theme, RunOptions, Shell};
    use gpui::{point, px};
    use om_core::ParameterRouter;
    use om_engine::DjEngine;
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;

    /// A shell over a real two-deck engine and router, with no window.
    fn shell() -> (Shell, Arc<DjEngine>) {
        shell_with_scene(crate::app::SceneOptions::default())
    }

    fn shell_with_scene(scene: crate::app::SceneOptions) -> (Shell, Arc<DjEngine>) {
        let (engine, _audio) = DjEngine::new(2);
        let engine = Arc::new(engine);
        let router = Arc::new(ParameterRouter::new());
        engine.register(&router);
        let options = RunOptions {
            on_frame: None,
            automix: None,
            record: false,
            smoke_frames: None,
            banner: None,
            debug: None,
            settings: None,
            library: None,
            library_rows_out: None,
            demo_grid_bpm: None,
            samples: None,
            scene,
        };
        let shell = Shell::new(
            Arc::clone(&engine),
            router,
            load_theme(),
            options,
            Arc::new(AtomicU64::new(0)),
        );
        (shell, engine)
    }

    #[test]
    fn a_demo_scene_stages_the_drawer_and_the_stems_tab_and_nothing_by_default() {
        use om_ui::widgets::deck::PadMode;
        let (plain, _) = shell();
        assert!(!plain.sampler_open);
        assert_eq!(plain.decks[0].pad_mode, PadMode::Cues);
        let (staged, _) = shell_with_scene(crate::app::SceneOptions {
            sampler_open: true,
            stems_tab: true,
            platter: false,
            press_pad: None,
        });
        assert!(staged.sampler_open);
        assert_eq!(staged.decks[0].pad_mode, PadMode::Stems);
        assert_eq!(staged.decks[1].pad_mode, PadMode::Cues, "only deck A");
    }

    fn at(x: f32, y: f32) -> gpui::Point<gpui::Pixels> {
        point(px(x), px(y))
    }

    #[test]
    fn fraction_maps_a_value_onto_its_range() {
        assert_eq!(fraction(0.5, 0.0, 1.0), 0.5);
        assert_eq!(fraction(-1.0, 0.0, 1.0), 0.0);
        assert_eq!(fraction(2.0, 0.0, 1.0), 1.0);
        assert_eq!(fraction(1.0, 1.0, 1.0), 0.0, "an empty range is safe");
    }

    #[test]
    fn a_channel_fader_follows_the_pointer_through_the_engine() {
        let (mut shell, engine) = shell();
        let track = Rect::new(100, 100, 20, 200);
        // Top of the track is the fader maximum.
        shell.begin_drag(Target::Channel(0), track, at(110.0, 100.0));
        let max = shell.mixer.strips[0].fader.max;
        assert!((f64::from(engine.mixer().fader(0)) - max).abs() < 1e-3);
        // Bottom is the minimum.
        shell.drag_move(at(110.0, 300.0));
        let min = shell.mixer.strips[0].fader.min;
        assert!((f64::from(engine.mixer().fader(0)) - min).abs() < 1e-3);
        shell.end_drag();
        assert!(shell.drag.is_none());
    }

    #[test]
    fn a_channel_fader_with_the_taper_gives_unity_at_the_top() {
        let (mut shell, engine) = shell();
        let track = Rect::new(100, 100, 20, 200);
        shell.begin_drag(Target::Channel(0), track, at(110.0, 100.0));
        assert!((engine.mixer().fader(0) - 1.0).abs() < 1e-6, "top is 0 dB");
        // Halfway down the travel is -30 dB on the dB-linear part of the law, not -6 dB.
        shell.drag_move(at(110.0, 200.0));
        let g = engine.mixer().fader(0);
        let db = 20.0 * g.log10();
        assert!((db + 30.0).abs() < 0.5, "{db} dB");
        shell.drag_move(at(110.0, 300.0));
        assert_eq!(engine.mixer().fader(0), 0.0);
    }

    #[test]
    fn a_stored_half_gain_still_reads_minus_six_db() {
        let (mut shell, engine) = shell();
        engine.mixer().set_fader(0, 0.5);
        shell.mixer.refresh(0.0, engine.mixer(), false);
        let v = shell.mixer.strips[0].fader.value();
        assert!((v - 0.5).abs() < 1e-6, "the stored value is untouched");
        assert_eq!(om_ui::control::Readout::Gain.text(v), "-6 dB");
        // Only the cap moves: -6 dB sits high on the taper.
        let pos = crate::fader_paint::FaderLaw::Channel.pos_of(v as f32);
        assert!(pos > 0.85 && pos < 0.95, "{pos}");
    }

    #[test]
    fn a_double_click_resets_a_tapered_fader_to_its_stored_default() {
        let (mut shell, engine) = shell();
        engine.mixer().set_fader(0, 0.3);
        shell.mixer.refresh(0.0, engine.mixer(), false);
        let default = shell.mixer.strips[0].fader.default;
        assert!(shell.reset_target(Target::Channel(0)));
        assert!((f64::from(engine.mixer().fader(0)) - default).abs() < 1e-6);
    }

    #[test]
    fn the_crossfader_runs_left_to_right() {
        let (mut shell, engine) = shell();
        let slider = Rect::new(0, 0, 200, 12);
        shell.begin_drag(Target::Crossfader, slider, at(0.0, 6.0));
        assert!(engine.mixer().crossfader().abs() < 1e-3);
        shell.drag_move(at(200.0, 6.0));
        assert!((engine.mixer().crossfader() - 1.0).abs() < 1e-3);
    }

    #[test]
    fn the_pitch_fader_sets_the_deck_rate_at_its_ends() {
        let (mut shell, engine) = shell();
        let track = Rect::new(0, 0, 20, 200);
        shell.begin_drag(Target::Pitch(0), track, at(5.0, 0.0));
        let r = shell.decks[0].pitch.range();
        assert!(
            (engine.deck(0).rate() - (1.0 + r)).abs() < 1e-3,
            "top is fastest"
        );
        shell.drag_move(at(5.0, 200.0));
        assert!(
            (engine.deck(0).rate() - (1.0 - r)).abs() < 1e-3,
            "bottom is slowest"
        );
        // Deck 1 is untouched.
        assert!((engine.deck(1).rate() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_knob_moves_by_travel_not_by_position() {
        let (mut shell, engine) = shell();
        let knob = Rect::new(0, 0, 40, 40);
        // Pressing does not jump the knob.
        let before = engine.mixer().trim(0);
        shell.begin_drag(Target::Trim(0), knob, at(20.0, 100.0));
        assert!((engine.mixer().trim(0) - before).abs() < 1e-4);
        // Moving up half the sweep moves it half its range.
        shell.drag_move(at(20.0, 0.0));
        let (min, max) = (
            shell.mixer.strips[0].trim.range.min,
            shell.mixer.strips[0].trim.range.max,
        );
        assert!(engine.mixer().trim(0) > before);
        assert!(f64::from(engine.mixer().trim(0)) <= max + 1e-3 && min <= max);
    }

    #[test]
    fn an_eq_knob_sticks_at_zero_db_and_leaves_it_when_pulled_further() {
        let (mut shell, engine) = shell();
        let knob = Rect::new(0, 0, 40, 56);
        shell.begin_drag(Target::Eq(0, 0), knob, at(20.0, 100.0));
        assert_eq!(engine.mixer().eq(0, 0), 0.0, "a press does not move it");
        // 40 px up is 0.2 of the sweep, 2.4 dB on the boost side: outside the detent.
        shell.drag_move(at(20.0, 60.0));
        assert!((engine.mixer().eq(0, 0) - 2.4).abs() < 0.05);
        // 20 px up is 1.2 dB, inside the detent (3% of 54 dB is 1.62): it reads exactly 0.
        shell.drag_move(at(20.0, 80.0));
        assert_eq!(engine.mixer().eq(0, 0), 0.0);
        shell.end_drag();
    }

    #[test]
    fn eq_knob_drag_moves_by_angle() {
        let (mut shell, engine) = shell();
        let knob = Rect::new(0, 0, 40, 56);
        shell.begin_drag(Target::Eq(0, 0), knob, at(20.0, 300.0));
        // 50 px is a quarter of the 200 px sweep: the pointer turns 67.5 degrees, which is +3 dB
        // (a linear drag would give +13.5 dB).
        shell.drag_move(at(20.0, 250.0));
        assert!((engine.mixer().eq(0, 0) - 3.0).abs() < 0.05);
        // Down by 50 px from the press is -24 dB: the lower half is 48 dB wide.
        shell.drag_move(at(20.0, 350.0));
        assert!((engine.mixer().eq(0, 0) + 24.0).abs() < 0.05);
    }

    #[test]
    fn shift_drag_is_finer() {
        let (mut shell, engine) = shell();
        let knob = Rect::new(0, 0, 40, 56);
        shell.begin_drag_fine(Target::Eq(0, 0), knob, at(20.0, 300.0), true);
        assert!(shell.drag.is_some_and(|d| d.fine));
        // 50 px with shift is 5 px of travel: 0.025 of the sweep, 0.3 dB.
        shell.drag_move(at(20.0, 250.0));
        assert!((engine.mixer().eq(0, 0) - 0.3).abs() < 0.02);
    }

    #[test]
    fn detent_pulls_without_shift_and_not_with_it() {
        let (mut shell, engine) = shell();
        let knob = Rect::new(0, 0, 40, 56);
        // 10 px up is 0.6 dB: inside the pull.
        shell.begin_drag(Target::Eq(0, 0), knob, at(20.0, 300.0));
        shell.drag_move(at(20.0, 290.0));
        assert_eq!(engine.mixer().eq(0, 0), 0.0);
        shell.end_drag();
        // The same travel with shift is 1 px, 0.06 dB, and shift bypasses the pull.
        shell.begin_drag_fine(Target::Eq(0, 0), knob, at(20.0, 300.0), true);
        shell.drag_move(at(20.0, 290.0));
        let v = engine.mixer().eq(0, 0);
        assert!(v > 0.01 && v < 0.2, "{v}");
    }

    /// Master is a fader (faders-meters.md section 3 master anatomy): it follows the pointer through
    /// the master taper, unity at 3/4 of the travel and +6 dB (stored 2.0) at the top.
    #[test]
    fn the_master_is_a_tapered_fader() {
        assert!(!Target::Master.is_knob());
        let (mut shell, engine) = shell();
        let track = Rect::new(100, 100, 60, 200);
        shell.begin_drag(Target::Master, track, at(110.0, 100.0));
        assert!(
            (engine.mixer().master_gain() - 2.0).abs() < 1e-3,
            "top is +6 dB"
        );
        shell.drag_move(at(110.0, 150.0));
        assert!(
            (engine.mixer().master_gain() - 1.0).abs() < 1e-3,
            "3/4 up is unity"
        );
        shell.drag_move(at(110.0, 300.0));
        assert_eq!(engine.mixer().master_gain(), 0.0);
    }

    #[test]
    fn cue_controls_are_knobs() {
        for target in [Target::CueMix, Target::CueLevel] {
            assert!(target.is_knob(), "{target:?}");
        }
        assert!(!Target::Channel(0).is_knob());
        let (mut shell, engine) = shell();
        engine.mixer().set_master_gain(0.4);
        shell.mixer.refresh(0.0, engine.mixer(), false);
        assert!(shell.reset_target(Target::Master));
        assert!((engine.mixer().master_gain() - 1.0).abs() < 1e-3);
        assert_eq!(shell.detent_of(Target::Master), Some(1.0));
        assert_eq!(shell.detent_of(Target::CueLevel), Some(1.0));
        assert_eq!(shell.detent_of(Target::CueMix), None);
        assert_eq!(shell.detent_of(Target::Eq(0, 0)), Some(0.0));
    }

    #[test]
    fn sampler_gain_and_pitch_are_knobs_over_the_selected_pad() {
        for target in [Target::SamplerGain, Target::SamplerPitch] {
            assert!(target.is_knob(), "{target:?}");
        }
        let (mut shell, engine) = shell();
        assert_eq!(shell.detent_of(Target::SamplerGain), Some(1.0));
        assert_eq!(shell.detent_of(Target::SamplerPitch), Some(0.0));
        let pad = shell.sampler.selected();
        // A drag moves the selected pad only.
        let knob = Rect::new(0, 0, 40, 40);
        shell.begin_drag(Target::SamplerGain, knob, at(20.0, 300.0));
        shell.drag_move(at(20.0, 200.0));
        shell.end_drag();
        let g = engine.snapshot().sampler.pads[pad].gain;
        assert!(g > 1.0 && g <= MAX_PAD_GAIN, "{g}");
        assert_eq!(engine.snapshot().sampler.pads[(pad + 1) % 8].gain, 1.0);
        assert!(shell.reset_target(Target::SamplerGain));
        assert_eq!(engine.snapshot().sampler.pads[pad].gain, 1.0);
        assert!(shell.reset_target(Target::SamplerPitch));
        assert_eq!(engine.snapshot().sampler.pads[pad].pitch, 0.0);
    }

    #[test]
    fn the_sampler_setters_clamp_to_the_engine_limits() {
        let (mut shell, engine) = shell();
        let pad = shell.sampler.selected();
        shell.sampler_set_gain(9.0);
        assert_eq!(engine.snapshot().sampler.pads[pad].gain, MAX_PAD_GAIN);
        shell.sampler_set_gain(-1.0);
        assert_eq!(engine.snapshot().sampler.pads[pad].gain, 0.0);
        shell.sampler_set_pitch(99.0);
        assert_eq!(
            engine.snapshot().sampler.pads[pad].pitch,
            MAX_PITCH_SEMITONES
        );
        shell.sampler_set_pitch(-3.0);
        assert_eq!(engine.snapshot().sampler.pads[pad].pitch, -3.0);
        assert_eq!(shell.sampler_selected_pad().pitch, -3.0);
    }

    #[test]
    fn wheel_and_keys_move_a_knob_by_the_sweep() {
        let (mut shell, engine) = shell();
        let target = Target::Eq(0, 0);
        shell.knob_wheel(target, 10.0, false);
        // 10 notches is 0.1 of the sweep: 1.2 dB on the boost side.
        assert!((engine.mixer().eq(0, 0) - 1.2).abs() < 0.05);
        assert!(shell.knob_key(target, "home", false));
        assert_eq!(engine.mixer().eq(0, 0), 0.0);
        assert!(shell.knob_key(target, "end", false));
        assert!((engine.mixer().eq(0, 0) - 6.0).abs() < 0.05);
        assert!(!shell.knob_key(target, "tab", false), "unused keys pass on");
    }

    #[test]
    fn a_double_click_puts_mixer_controls_back_to_their_defaults() {
        let (mut shell, engine) = shell();
        engine.mixer().set_master_gain(0.4);
        shell.mixer.refresh(0.0, engine.mixer(), false);
        assert!(shell.reset_target(Target::Master));
        assert!((engine.mixer().master_gain() - 1.0).abs() < 1e-3);
        shell.begin_drag(Target::Filter(1), Rect::new(0, 0, 40, 40), at(20.0, 100.0));
        shell.drag_move(at(20.0, 20.0));
        assert!(engine.mixer().filter(1) > 0.1);
        assert!(shell.reset_target(Target::Filter(1)));
        assert_eq!(engine.mixer().filter(1), 0.0);
        assert!(
            !shell.reset_target(Target::Pitch(0)),
            "the pitch fader keeps its own behaviour"
        );
    }

    #[test]
    fn a_knob_shows_its_value_while_dragged_or_hovered() {
        let (mut shell, _engine) = shell();
        let target = Target::Trim(0);
        assert!(!shell.shows_value(target));
        shell.hovered = Some(target);
        assert!(shell.shows_value(target));
        assert!(!shell.shows_value(Target::Trim(1)));
        shell.hovered = None;
        shell.begin_drag(target, Rect::new(0, 0, 40, 40), at(20.0, 20.0));
        assert!(shell.shows_value(target));
        shell.end_drag();
        assert!(!shell.shows_value(target));
    }

    #[test]
    fn pulling_the_model_follows_engine_changes_made_elsewhere() {
        let (mut shell, engine) = shell();
        engine.mixer().set_master_gain(0.5);
        shell.mixer.refresh(0.0, engine.mixer(), false);
        assert!((shell.mixer.master.fader.value() - 0.5).abs() < 1e-3);
    }
}

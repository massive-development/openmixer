//! Drag interaction for faders, knobs and sliders. A press starts a drag on one control; pointer
//! moves set its value through the om-ui model (which sends through the control surface), and the
//! release ends it. Faders and sliders map the pointer onto the value range directly; knobs move by
//! the pointer's travel, so a click does not jump them.

use crate::app::Shell;
use gpui::Point;
use om_ui::geometry::Rect;

/// Pixels of vertical travel that span a knob's full range.
const KNOB_TRAVEL_PX: f64 = 200.0;

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
}

impl Target {
    /// Knobs move by travel; faders and sliders follow the pointer.
    fn is_knob(self) -> bool {
        matches!(
            self,
            Target::Trim(_)
                | Target::Filter(_)
                | Target::Eq(..)
                | Target::FxMix
                | Target::FxParam(_)
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
        });
        self.drag_move(pos);
    }

    /// Applies the pointer position to the drag in progress, if any.
    pub(crate) fn drag_move(&mut self, pos: Point<gpui::Pixels>) {
        let Some(drag) = self.drag else { return };
        let (min, max) = self.range_of(drag.target);
        let value = if drag.target.is_knob() {
            // Up is more. A full sweep of the knob is KNOB_TRAVEL_PX.
            let travel = (drag.anchor - axis_of(pos, true)) / KNOB_TRAVEL_PX;
            drag.anchor_value + travel * (max - min)
        } else if drag.vertical {
            let frac =
                (axis_of(pos, true) - f64::from(drag.rect.y)) / f64::from(drag.rect.h.max(1));
            max - frac.clamp(0.0, 1.0) * (max - min)
        } else {
            let frac =
                (axis_of(pos, false) - f64::from(drag.rect.x)) / f64::from(drag.rect.w.max(1));
            min + frac.clamp(0.0, 1.0) * (max - min)
        };
        self.apply(drag.target, value.clamp(min, max));
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
            Target::Master => (self.mixer.master.fader.min, self.mixer.master.fader.max),
            Target::Crossfader => (
                self.mixer.crossfader.slider.min,
                self.mixer.crossfader.slider.max,
            ),
            Target::Trim(c) => knob_range(&self.mixer.strips[c].trim),
            Target::Filter(c) => knob_range(&self.mixer.strips[c].filter),
            Target::Eq(c, b) => knob_range(&self.mixer.strips[c].eq[b]),
            Target::CueMix => (self.mixer.cue.mix.min, self.mixer.cue.mix.max),
            Target::CueLevel => (self.mixer.cue.level.min, self.mixer.cue.level.max),
            Target::FxMix => knob_range(&self.mixer.fx.mix),
            Target::FxParam(i) => knob_range(&self.mixer.fx.params[i]),
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
        }
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
        let (engine, _audio) = DjEngine::new(2);
        let engine = Arc::new(engine);
        let router = Arc::new(ParameterRouter::new());
        engine.register(&router);
        let options = RunOptions {
            smoke_frames: None,
            banner: None,
            debug: None,
            settings: None,
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
    fn pulling_the_model_follows_engine_changes_made_elsewhere() {
        let (mut shell, engine) = shell();
        engine.mixer().set_master_gain(0.5);
        shell.mixer.refresh(0.0, engine.mixer(), false);
        assert!((shell.mixer.master.fader.value() - 0.5).abs() < 1e-3);
    }
}

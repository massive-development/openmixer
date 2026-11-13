//! The sampler drawer: collapsed it is a narrow PADS handle beside the library; open it shows eight
//! pads (a 4x2 grid) and a control column for the selected pad (mode and stop, load and clear,
//! gain and pitch knobs). Pads light while playing. A pad is pressed by a click
//! or its digit key (1 to 8) while the strip has focus; a gate pad is held until the button or key
//! comes up. Models come from `om-ui`'s `SamplerBinding`; the layout from `widgets::sampler_strip`.

use crate::app::Shell;
use crate::controls::{
    button, caption, focus_handle, knob_face_with, metrics, value_caption, KnobFace, KnobLook,
};
use crate::drag::Target;
use crate::font::TypeExt as _;
use crate::knob_paint::KnobRole;
use crate::paint::{focus_layer, paint, place, skin};
use crate::parts::{pad_face, PadState};
use crate::text_entry::{apply_key, EntryKey};
use gpui::{
    prelude::*, px, ClickEvent, Context, Div, FocusHandle, KeyDownEvent, KeyUpEvent, MouseButton,
    MouseDownEvent, MouseUpEvent, SharedString,
};
use om_engine::sampler::{PadMode, MAX_PAD_GAIN, MAX_PITCH_SEMITONES};
use om_ui::geometry::Rect;
use om_ui::sampler_binding::{mode_label, pad_for_key, PadView};
use om_ui::style::LookAndFeel;
use om_ui::theme::{Colour, TypeRole};
use om_ui::widgets::sampler_strip::{sampler_strip_layout, SamplerStripLayout};

/// Velocity of a click or key press on a pad (a full-scale hit leaves headroom for the master).
const PRESS_VELOCITY: f32 = 0.9;
/// How many characters of a sample name fit on a pad.
const PAD_NAME_CHARS: usize = 8;

impl Shell {
    /// Selects `pad` and presses it. The pad is held until [`Shell::sampler_release`]; a pad still
    /// held is released first.
    pub(crate) fn sampler_press(&mut self, pad: usize) {
        self.sampler.press_held(pad, PRESS_VELOCITY);
    }

    /// Lets go of the held pad, if any; a gate's voices end, others ignore it.
    pub(crate) fn sampler_release(&mut self) {
        self.sampler.release_held();
    }

    /// A key on the strip: typed text while a path field is open, else the digit keys press pads.
    pub(crate) fn sampler_key(&mut self, key: &str, typed: Option<&str>) {
        if self.sampler_editing {
            let pad = self.sampler.selected();
            let mut text = self.sampler.path(pad);
            match apply_key(&mut text, key, typed) {
                EntryKey::Submit => {
                    self.sampler.set_path(pad, &text);
                    self.sampler_editing = false;
                    self.sampler_load_typed();
                }
                EntryKey::Cancel => self.sampler_editing = false,
                EntryKey::Edited => self.sampler.set_path(pad, &text),
                EntryKey::Ignored => {}
            }
            return;
        }
        if let Some(pad) = pad_for_key(key) {
            if self.sampler.held() != Some(pad) {
                self.sampler_press(pad);
            }
        }
    }

    /// Lets go of a pad whose digit key came up.
    pub(crate) fn sampler_key_up(&mut self, key: &str) {
        if let Some(pad) = pad_for_key(key) {
            self.sampler.release_held_if(pad);
        }
    }

    /// Opens the selected pad's path field for typing.
    pub(crate) fn sampler_begin_path(&mut self) {
        self.sampler_editing = true;
        self.sampler_status = None;
    }

    /// Loads the path typed for the selected pad through the bank. A failure is kept for the caption.
    pub(crate) fn sampler_load_typed(&mut self) {
        let pad = self.sampler.selected();
        self.sampler_status = Some(match self.sampler.load_typed_path(pad) {
            Ok(_) => ("Loading…".to_owned(), false),
            Err(error) => (error.to_string(), true),
        });
    }

    /// Empties the selected pad.
    pub(crate) fn sampler_clear_selected(&mut self) {
        let pad = self.sampler.selected();
        self.sampler_status = match self.sampler.clear(pad) {
            Ok(()) => None,
            Err(error) => Some((error.to_string(), true)),
        };
    }

    /// The selected pad as the window draws it (gain and pitch live per pad).
    pub(crate) fn sampler_selected_pad(&self) -> PadView {
        let pad = self.sampler.selected();
        self.sampler.pads().swap_remove(pad)
    }

    /// Sets the selected pad's gain (linear; the engine clamps it to its ceiling).
    pub(crate) fn sampler_set_gain(&mut self, gain: f32) {
        let pad = self.sampler.selected();
        if let Err(error) = self.sampler.set_gain(pad, gain) {
            self.sampler_status = Some((error.to_string(), true));
        }
    }

    /// Sets the selected pad's pitch in semitones (the engine clamps it to its limit).
    pub(crate) fn sampler_set_pitch(&mut self, semitones: f32) {
        let pad = self.sampler.selected();
        if let Err(error) = self.sampler.set_pitch(pad, semitones) {
            self.sampler_status = Some((error.to_string(), true));
        }
    }

    /// Moves the selected pad to its next mode.
    pub(crate) fn sampler_cycle_mode(&mut self) {
        let pad = self.sampler.selected();
        if let Err(error) = self.sampler.cycle_mode(pad) {
            self.sampler_status = Some((error.to_string(), true));
        }
    }
}

/// The collapsed drawer: a handle that opens the sampler when clicked. The digit keys still play
/// pads while it has focus.
fn collapsed_handle(shell: &Shell, card: Rect, focus: FocusHandle, cx: &mut Context<Shell>) -> Div {
    let t = &shell.theme;
    let look = LookAndFeel::from_tokens(t);
    let hover = look.button.hover.fill;
    // The letters clip inside the handle so a short band never spills them over its neighbours.
    let letters = gpui::div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(4.))
        .overflow_hidden()
        .children("SAMPLER".chars().map(|c| gpui::div().child(c.to_string())))
        .child("\u{203A}");
    skin(place(card), &look.button.idle)
        .type_role(t, TypeRole::Label)
        .hover(move |style| style.bg(paint(hover)))
        .track_focus(&focus)
        .child(letters)
        .child(crate::paint::focus_layer(
            look.focus,
            look.button.idle.radius,
            Some(focus.clone()),
        ))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                this.sampler_open = true;
                if let Some(focus) = this.sampler_focus.clone() {
                    window.focus(&focus, cx);
                }
                cx.notify();
            }),
        )
}

/// The sampler drawn at `card`: the handle while collapsed, else the pads and their controls.
/// `focus` takes the keys while the strip is clicked.
pub fn sampler_card(shell: &Shell, card: Rect, focus: FocusHandle, cx: &mut Context<Shell>) -> Div {
    if !shell.sampler_open {
        return collapsed_handle(shell, card, focus, cx);
    }
    let t = &shell.theme;
    let look = LookAndFeel::from_tokens(t);
    let accent = look.accent;
    let l: SamplerStripLayout = sampler_strip_layout(Rect::sized(card.w, card.h), t);
    let pads = shell.sampler.pads();
    let selected = shell.sampler.selected();

    let mut root = place(card)
        .bg(paint(t.surface))
        .border_1()
        .border_color(paint(t.border))
        .rounded(px(t.control_radius))
        .text_color(paint(t.text_primary))
        .overflow_hidden()
        .track_focus(&focus)
        .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _, cx| {
            let keys = &ev.keystroke;
            if !(keys.modifiers.control || keys.modifiers.platform) {
                this.sampler_key(&keys.key, keys.key_char.as_deref());
                cx.notify();
            }
        }))
        .on_key_up(cx.listener(|this, ev: &KeyUpEvent, _, cx| {
            this.sampler_key_up(&ev.keystroke.key);
            cx.notify();
        }))
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|this, _: &MouseUpEvent, _, cx| {
                this.sampler_release();
                cx.notify();
            }),
        )
        // A release outside the strip (the press began on a pad) must still let go.
        .on_mouse_up_out(
            MouseButton::Left,
            cx.listener(|this, _: &MouseUpEvent, _, cx| {
                this.sampler_release();
                cx.notify();
            }),
        );

    // Caption: the path field while it is open, else the selected pad, or the last problem.
    let text = if shell.sampler_editing {
        format!("Path: {}|", shell.sampler.path(selected))
    } else if let Some((message, _)) = &shell.sampler_status {
        message.clone()
    } else if pads[selected].loaded {
        // Mode, pitch and gain: the pad shows its own name.
        shell.sampler.caption()
    } else {
        om_ui::sampler_binding::EMPTY_PAD_HINT.to_owned()
    };
    let error = shell.sampler_status.as_ref().is_some_and(|(_, e)| *e);
    // The loaded pad's caption is its mode, pitch and gain; anything else is a sentence.
    let values = pads[selected].loaded && !shell.sampler_editing && shell.sampler_status.is_none();
    let line = if values {
        value_caption(t, l.caption, &text)
    } else {
        caption(t, l.caption, &text).type_role(t, TypeRole::Body)
    };
    root = root.child(line.text_color(paint(if error {
        look.button.idle.text
    } else {
        t.text_primary
    })));
    root = root.child(button(t, l.close, "Close", false, accent, cx, |this| {
        this.sampler_open = false;
    }));

    for (index, pad) in pads.iter().enumerate() {
        root = root.child(pad_tile(
            shell,
            l.grid[index],
            pad,
            accent,
            focus.clone(),
            cx,
        ));
    }

    root = root.child(button(
        t,
        l.mode,
        mode_label(pads[selected].mode),
        pads[selected].mode != PadMode::OneShot,
        accent,
        cx,
        |this| this.sampler_cycle_mode(),
    ));
    root = root.child(button(
        t,
        l.stop_all,
        "Stop all",
        false,
        accent,
        cx,
        |this| this.sampler.stop_all(),
    ));
    root = root.child(button(
        t,
        l.load,
        "Load",
        shell.sampler_editing,
        accent,
        cx,
        |this| this.sampler_begin_path(),
    ));
    root = root.child(button(t, l.clear, "Clear", false, accent, cx, |this| {
        this.sampler_clear_selected()
    }));
    // Gain and pitch knobs over the selected pad, each across its row (the layout's stepper cells
    // merged); disabled while the pad is empty.
    let pad = &pads[selected];
    let gain_text = om_ui::control::Readout::Gain.text(f64::from(pad.gain));
    let pitch_text = format!(
        "{} st",
        om_ui::format::signed_fixed(f64::from(pad.pitch), 0)
    );
    let row = |a: Rect, b: Rect| Rect::new(a.x, a.y, b.right() - a.x, a.h);
    let knobs = [
        (
            row(l.gain_down, l.gain_up),
            ("GAIN", &gain_text, Some(1.0)),
            (f64::from(pad.gain), 0.0, f64::from(MAX_PAD_GAIN)),
            Target::SamplerGain,
        ),
        (
            row(l.pitch_down, l.pitch_up),
            ("PITCH", &pitch_text, Some(0.0)),
            (
                f64::from(pad.pitch),
                -f64::from(MAX_PITCH_SEMITONES),
                f64::from(MAX_PITCH_SEMITONES),
            ),
            Target::SamplerPitch,
        ),
    ];
    for (rect, (label, readout, detent), range, target) in knobs {
        let abs = Rect::new(card.x + rect.x, card.y + rect.y, rect.w, rect.h);
        root = root.child(knob_face_with(
            t,
            rect,
            KnobFace {
                label,
                readout,
                detent,
                active: shell.shows_value(target),
            },
            range,
            (target, abs),
            KnobLook {
                role: KnobRole::Sampler,
                accent: Some(accent),
                dragging: shell.is_dragging(target),
                disabled: !pad.loaded,
                ..KnobLook::default()
            },
            cx,
        ));
    }
    root
}

/// One pad: the shared pad face (playing, set, empty; dashed while its sample decodes), a tab stop
/// with the focus ring. A press selects and plays it and the release ends a gate; Enter or Space
/// taps it. The selected pad's edge is the text colour.
fn pad_tile(
    shell: &Shell,
    rect: Rect,
    pad: &PadView,
    accent: Colour,
    focus: FocusHandle,
    cx: &mut Context<Shell>,
) -> Div {
    let t = &shell.theme;
    let m = metrics(t);
    let index = pad.pad;
    let state = match (pad.loading, pad.playing, pad.loaded) {
        (true, _, _) => PadState::Disabled,
        (_, true, _) => PadState::Playing,
        (_, _, true) => PadState::Set,
        _ => PadState::Empty,
    };
    let name: String = pad.name.chars().take(PAD_NAME_CHARS).collect();
    let key = if pad.missing {
        format!("{} !", pad.key)
    } else {
        pad.key.to_string()
    };
    let id = format!("sampler-pad-{index}");
    let handle = focus_handle(&id, cx);
    let ring = LookAndFeel::from_tokens(t).focus;
    let mut face = pad_face(t, &m, (&key, &name), state, accent)
        .w(px(rect.w as f32))
        .h(px(rect.h as f32))
        .relative();
    if pad.selected {
        face = face.border_color(paint(t.text_primary));
    }
    let el = face
        .id(SharedString::from(id))
        .role(gpui::accesskit::Role::Button)
        .aria_label(format!("Pad {key} {name}"))
        .track_focus(&handle)
        .child(focus_layer(ring, m.panel_radius, Some(handle)))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                // The strip takes the keys (1 to 8), not the pad.
                window.focus(&focus, cx);
                this.sampler_press(index);
                cx.notify();
            }),
        )
        .on_click(cx.listener(move |this, ev: &ClickEvent, _, cx| {
            if matches!(ev, ClickEvent::Keyboard(_)) {
                this.sampler_press(index);
                this.sampler_release();
                cx.notify();
            }
        }));
    place(rect).child(el)
}

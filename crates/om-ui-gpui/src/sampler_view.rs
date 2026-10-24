//! The sampler drawer: collapsed it is a narrow PADS handle beside the library; open it shows eight
//! pads (a 4x2 grid) and a control column for the selected pad (mode and stop, load and clear,
//! gain and pitch steppers). Pads light while playing. A pad is pressed by a click
//! or its digit key (1 to 8) while the strip has focus; a gate pad is held until the button or key
//! comes up. Models come from `om-ui`'s `SamplerBinding`; the layout from `widgets::sampler_strip`.

use crate::app::Shell;
use crate::controls::{button, caption, mark};
use crate::paint::{paint, place, skin};
use crate::text_entry::{apply_key, EntryKey};
use gpui::{
    prelude::*, px, Context, Div, FocusHandle, KeyDownEvent, KeyUpEvent, MouseButton,
    MouseDownEvent, MouseUpEvent,
};
use om_engine::sampler::PadMode;
use om_ui::geometry::Rect;
use om_ui::sampler_binding::{mode_label, pad_for_key, PadView};
use om_ui::style::LookAndFeel;
use om_ui::theme::Colour;
use om_ui::widgets::sampler_strip::{sampler_strip_layout, SamplerStripLayout};

/// Velocity of a click or key press on a pad (a full-scale hit leaves headroom for the master).
const PRESS_VELOCITY: f32 = 0.9;
/// Gain step of the steppers (linear).
const GAIN_STEP: f32 = 0.1;
/// Pitch step of the steppers (semitones).
const PITCH_STEP: f32 = 1.0;
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

    /// Steps the selected pad's gain by `delta` (linear).
    pub(crate) fn sampler_step_gain(&mut self, delta: f32) {
        let pad = self.sampler.selected();
        let gain = self.sampler.pads()[pad].gain + delta;
        if let Err(error) = self.sampler.set_gain(pad, gain) {
            self.sampler_status = Some((error.to_string(), true));
        }
    }

    /// Steps the selected pad's pitch by `delta` semitones.
    pub(crate) fn sampler_step_pitch(&mut self, delta: f32) {
        let pad = self.sampler.selected();
        let pitch = self.sampler.pads()[pad].pitch + delta;
        if let Err(error) = self.sampler.set_pitch(pad, pitch) {
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
    skin(place(card), &look.button.idle)
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(4.))
        .text_size(px(t.type_caption))
        .hover(move |style| style.bg(paint(hover)))
        .track_focus(&focus)
        .children("SAMPLER".chars().map(|c| gpui::div().child(c.to_string())))
        .child("\u{203A}")
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                this.sampler_open = true;
                if let Some(focus) = this.sampler_focus.clone() {
                    window.focus(&focus);
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
        format!("{} {}", pads[selected].key, pads[selected].name)
    } else {
        om_ui::sampler_binding::EMPTY_PAD_HINT.to_owned()
    };
    let error = shell.sampler_status.as_ref().is_some_and(|(_, e)| *e);
    root = root.child(caption(t, l.caption, &text).text_color(paint(if error {
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
    // Gain and pitch steppers: `-`, the name and value between, `+`.
    let gain = format!("GAIN {}%", (pads[selected].gain * 100.0).round() as i32);
    let pitch = format!(
        "PITCH {}",
        om_ui::format::signed_fixed(f64::from(pads[selected].pitch), 0)
    );
    root = root.child(button(t, l.gain_down, "-", false, accent, cx, |this| {
        this.sampler_step_gain(-GAIN_STEP)
    }));
    root = root.child(mark(t, l.gain_value, &gain));
    root = root.child(button(t, l.gain_up, "+", false, accent, cx, |this| {
        this.sampler_step_gain(GAIN_STEP)
    }));
    root = root.child(button(t, l.pitch_down, "-", false, accent, cx, |this| {
        this.sampler_step_pitch(-PITCH_STEP)
    }));
    root = root.child(mark(t, l.pitch_value, &pitch));
    root = root.child(button(t, l.pitch_up, "+", false, accent, cx, |this| {
        this.sampler_step_pitch(PITCH_STEP)
    }));
    root
}

/// One pad: a click selects and presses it, the release ends a gate. Lit while playing, filled when
/// loaded, outlined when empty, and marked with the accent when selected.
fn pad_tile(
    shell: &Shell,
    rect: Rect,
    pad: &PadView,
    accent: Colour,
    focus: FocusHandle,
    cx: &mut Context<Shell>,
) -> Div {
    let t = &shell.theme;
    let look = LookAndFeel::from_tokens(t);
    let index = pad.pad;
    let style = if pad.playing {
        look.lit_button(accent)
    } else if pad.loaded {
        look.button.idle
    } else {
        look.panel
    };
    let name: String = pad.name.chars().take(PAD_NAME_CHARS).collect();
    let label = if pad.loaded {
        format!("{} {}", pad.key, name)
    } else {
        pad.key.to_string()
    };
    let border_colour = if pad.selected { accent } else { style.border };
    let mut el = skin(
        place(rect)
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(t.type_caption)),
        &style,
    )
    .border_color(paint(border_colour))
    .child(if pad.missing {
        format!("{label} !")
    } else {
        label
    })
    .on_mouse_down(
        MouseButton::Left,
        cx.listener(move |this, _: &MouseDownEvent, window, cx| {
            window.focus(&focus);
            this.sampler_press(index);
            cx.notify();
        }),
    );
    if pad.loading {
        el = el.opacity(0.6);
    }
    el
}

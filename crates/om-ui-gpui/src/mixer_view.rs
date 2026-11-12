//! The mixer column (comp 4a/4b mixer): each channel's gain, EQ and filter knobs at the outer
//! edges with their values beside them, the channel faders and meters inside them, the master
//! fader and meter in the middle; under them the crossfader between the A and B tags, each
//! channel's headphone cue and crossfader assignment around the curve switch, and the headphone
//! knobs. Also the FX drawer a deck's FX tab opens. Models come from `om-ui`'s `MixerPanel`; every
//! control sends through the control surface.
//!
//! An EQ band's caption (its name over its value) is its kill switch, as the band label was before.

use crate::app::Shell;
use crate::controls::{
    face_button, knob_face_with, metrics, segmented, slider_focus, tip_overlay, BarState, KnobFace,
    KnobLook,
};
use crate::drag::Target;
use crate::fader_paint::{self, FaderFace, FaderLaw};
use crate::font::{numeral, Numeral, TypeExt as _};
use crate::knob_paint::KnobRole;
use crate::meter_paint::{clip_word, lamp_meter};
use crate::paint::{focus_layer, paint, place};
use crate::parts::deck_tag;
use gpui::{div, prelude::*, px, ClickEvent, Context, Div, MouseButton, SharedString};
use om_ui::control::{Knob, Readout};
use om_ui::geometry::Rect;
use om_ui::labels::channel_labels;
use om_ui::meter::MeterBar;
use om_ui::style::{Face, LookAndFeel};
use om_ui::theme::{Colour, TypeRole};
use om_ui::widgets::channel::{channel_letter, ASSIGN_LABELS};
use om_ui::widgets::crossfader::{CURVE_LABELS, CURVE_TIPS};
use om_ui::widgets::cue::{LEVEL_LABEL, MIX_LABEL};
use om_ui::widgets::fx_panel::FxPanel;
use om_ui::widgets::mixer::{column_layout, ChannelColumn, KnobCell, KnobRow, KNOB_ORDER};

/// The word a killed band shows in place of its value (the Signal Rule keeps red off it).
pub const KILL_WORD: &str = "kill";

/// The mixer column at `card` (window coordinates).
pub fn mixer_card(shell: &Shell, card: Rect, cx: &mut Context<Shell>) -> Div {
    let t = &shell.theme;
    let m = metrics(t);
    let l = column_layout(Rect::sized(card.w, card.h), &m);
    let mut root = place(card)
        .border_l_1()
        .border_r_1()
        .border_color(paint(t.border))
        .text_color(paint(t.text_primary));
    for (ch, column) in l.channels.iter().enumerate() {
        if ch < shell.mixer.strips.len() {
            root = root.children(channel(shell, ch, column, card, cx));
        }
    }
    root = root.children(master(shell, l.master_fader, l.master_meter, card, cx));
    if shell.mixer.any_clip_latched() {
        let headroom = t.meter_headroom.round() as i32;
        root = root.child(clip_word(t, l.clip_word(headroom)));
    }
    for (ch, tag) in l.tags.iter().enumerate() {
        let hue = t.accent_for(ch as i32);
        root = root.child(
            place(*tag)
                .flex()
                .items_center()
                .justify_center()
                .child(deck_tag(t, &channel_letter(ch).to_string(), hue)),
        );
    }
    root = root.child(crossfader(shell, l.crossfader, card, cx));
    root = root.child(curve(shell, l.curve, cx));
    let c = &shell.mixer.cue;
    let neutral = t.text_primary;
    root = root.children(knob_cell(
        shell,
        &l.phones_mix,
        (&c.mix, MIX_LABEL, Target::CueMix),
        (card, neutral, false),
        cx,
    ));
    root.children(knob_cell(
        shell,
        &l.phones_level,
        (&c.level, LEVEL_LABEL, Target::CueLevel),
        (card, neutral, false),
        cx,
    ))
}

/// The bars of a stereo meter as the view model reports them.
fn bar_state(bar: &MeterBar) -> BarState {
    let b = bar.ballistics();
    BarState {
        level: b.level(),
        hold: b.peak_hold(),
        clipping: bar.is_latched(),
    }
}

/// A value as a knob's caption prints it: the readout without its unit (the label names it).
fn caption_value(text: &str) -> String {
    text.trim_end_matches(" dB")
        .trim_end_matches(" %")
        .to_owned()
}

/// Channel `ch`'s knobs, fader, meter, cue and assignment.
fn channel(
    shell: &Shell,
    ch: usize,
    column: &ChannelColumn,
    card: Rect,
    cx: &mut Context<Shell>,
) -> Vec<Div> {
    let t = &shell.theme;
    let strip = &shell.mixer.strips[ch];
    let hue = t.accent_for(ch as i32);
    let labels = channel_labels();
    let mut out = Vec::new();
    for (row, cell) in KNOB_ORDER.iter().zip(&column.knobs) {
        let (model, label, target) = match *row {
            KnobRow::Gain => (&strip.trim, labels.gain, Target::Trim(ch)),
            KnobRow::Filter => (&strip.filter, labels.filter, Target::Filter(ch)),
            KnobRow::Band(b) => (&strip.eq[b], strip.eq[b].label.as_str(), Target::Eq(ch, b)),
        };
        out.extend(knob_cell(
            shell,
            cell,
            (model, label, target),
            (card, hue, ch == 1),
            cx,
        ));
    }
    let fader = &strip.fader;
    let target = Target::Channel(ch);
    let popup = shell
        .is_dragging(target)
        .then(|| Readout::Gain.text(fader.value()));
    let abs = Rect::new(
        card.x + column.fader.x,
        card.y + column.fader.y,
        column.fader.w,
        column.fader.h,
    );
    out.push(
        fader_paint::fader(
            t,
            column.fader,
            &FaderFace::channel(FaderLaw::Channel).for_density(&metrics(t)),
            FaderLaw::Channel.pos_of(fader.value() as f32),
            hue,
            target,
            abs,
            popup.as_deref(),
            cx,
        )
        .child(slider_focus(t, target, cx)),
    );
    let bars = [bar_state(&strip.meter.left), bar_state(&strip.meter.right)];
    out.push(lamp_meter(t, column.meter, bars, cx, move |this| {
        let meter = &mut this.mixer.strips[ch].meter;
        meter.left.click();
        meter.right.click();
    }));
    let cue = if strip.pfl { Face::On } else { Face::Rest };
    out.push(
        place(column.cue).child(
            face_button(t, labels.cue, (cue, hue), cx, move |this, _, _| {
                this.mixer.strips[ch].pfl_pressed();
            })
            .size_full()
            .child(tip_overlay(t, format!("pfl-{ch}"), "Headphone cue")),
        ),
    );
    let assigned = strip.assign;
    out.push(
        place(column.assign).child(
            face_button(
                t,
                ASSIGN_LABELS[assigned % 3],
                (Face::Rest, hue),
                cx,
                move |this, _, _| {
                    let next = (this.mixer.strips[ch].assign + 1) % ASSIGN_LABELS.len();
                    this.mixer.strips[ch].assign_pressed(next);
                },
            )
            .size_full()
            .child(tip_overlay(
                t,
                format!("assign-{ch}"),
                "Crossfader side: Thru, A or B (press for the next)",
            )),
        ),
    );
    out
}

/// A knob and its caption (the label over the value in mono) at `cell`. An EQ band's caption is
/// its kill switch: a press toggles the kill and a killed band shows the word "kill".
fn knob_cell(
    shell: &Shell,
    cell: &KnobCell,
    (model, label, target): (&Knob, &str, Target),
    (card, hue, text_right): (Rect, Colour, bool),
    cx: &mut Context<Shell>,
) -> Vec<Div> {
    let t = &shell.theme;
    let readout = model.text();
    let abs = Rect::new(
        card.x + cell.knob.x,
        card.y + cell.knob.y,
        cell.knob.w,
        cell.knob.h,
    );
    let role = match target {
        Target::Eq(..) => KnobRole::Band,
        Target::CueMix | Target::CueLevel => KnobRole::Cue,
        _ => KnobRole::Wide,
    };
    let knob = knob_face_with(
        t,
        cell.knob,
        KnobFace {
            label: "",
            readout: &readout,
            detent: model.detent,
            active: shell.shows_value(target),
        },
        (model.value(), model.range.min, model.range.max),
        (target, abs),
        KnobLook {
            role,
            accent: Some(hue),
            dragging: shell.is_dragging(target),
            disabled: false,
            density: crate::controls::density(),
            value_beside: false,
        },
        cx,
    );
    let killed = match target {
        Target::Eq(ch, band) => Some((ch, band, shell.mixer.strips[ch].is_killed(band))),
        _ => None,
    };
    let value = match killed {
        Some((.., true)) => KILL_WORD.to_owned(),
        _ => caption_value(&readout),
    };
    let value_colour = if matches!(killed, Some((.., true))) {
        t.text_primary
    } else {
        t.text_secondary
    };
    let text = place(cell.text)
        .flex()
        .flex_col()
        .justify_center()
        .when(!text_right, |el| el.items_end())
        .whitespace_nowrap()
        .child(
            div()
                .type_role(t, TypeRole::Label)
                .text_color(paint(t.text_muted))
                .child(label.to_owned()),
        )
        .child(numeral(t, Numeral::Value, value).text_color(paint(value_colour)));
    let text = match killed {
        Some((ch, band, on)) => kill_switch(shell, text, (ch, band, on), cx),
        None => text,
    };
    vec![knob, text]
}

/// Makes a band's caption its kill switch: a tab stop, a tonal step under the pointer, the deck
/// tint while killed, and a press (or Enter) toggling the kill.
fn kill_switch(
    shell: &Shell,
    text: Div,
    (ch, band, killed): (usize, usize, bool),
    cx: &mut Context<Shell>,
) -> Div {
    let t = &shell.theme;
    let look = LookAndFeel::from_tokens(t);
    let hue = t.accent_for(ch as i32);
    let id = format!("kill-{ch}-{band}");
    let handle = crate::controls::focus_handle(&id, cx);
    let hover = look.face(Face::Hover, hue).fill;
    let lit = look.face(Face::On, hue);
    let el = text
        .rounded(px(t.control_radius))
        .px(px(metrics(t).space[0]))
        .when(killed, |el| {
            el.bg(paint(lit.fill))
                .border_1()
                .border_color(paint(lit.edge))
        });
    div().child(
        el.id(SharedString::from(id))
            .role(gpui::accesskit::Role::Button)
            .aria_label(format!("Kill {}", shell.mixer.strips[ch].eq[band].label))
            .track_focus(&handle)
            .child(focus_layer(look.focus, t.control_radius, Some(handle)))
            .hover(move |s| s.bg(paint(hover)))
            .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.mixer.strips[ch].kill_pressed(band);
                cx.notify();
            }))
            .child(tip_overlay(
                t,
                format!("kill-tip-{ch}-{band}"),
                "Kill this band",
            )),
    )
}

/// The master fader (+6 dB at the top, unity at 3/4) beside the master meter.
fn master(
    shell: &Shell,
    fader_rect: Rect,
    meter_rect: Rect,
    card: Rect,
    cx: &mut Context<Shell>,
) -> Vec<Div> {
    let t = &shell.theme;
    let m = &shell.mixer.master;
    let bars = [bar_state(&m.meter.left), bar_state(&m.meter.right)];
    let abs = Rect::new(
        card.x + fader_rect.x,
        card.y + fader_rect.y,
        fader_rect.w,
        fader_rect.h,
    );
    let popup = shell
        .is_dragging(Target::Master)
        .then(|| Readout::Gain.text(m.fader.value()));
    vec![
        fader_paint::fader(
            t,
            fader_rect,
            &FaderFace::master(FaderLaw::Master).for_density(&metrics(t)),
            FaderLaw::Master.pos_of(m.fader.value() as f32),
            t.text_primary,
            Target::Master,
            abs,
            popup.as_deref(),
            cx,
        )
        .child(slider_focus(t, Target::Master, cx))
        .child(tip_overlay(t, "master-tip", "Master level")),
        lamp_meter(t, meter_rect, bars, cx, |this| {
            let meter = &mut this.mixer.master.meter;
            meter.left.click();
            meter.right.click();
        }),
    ]
}

/// The crossfader between the A and B tags.
fn crossfader(shell: &Shell, rect: Rect, card: Rect, cx: &mut Context<Shell>) -> Div {
    let t = &shell.theme;
    let x = &shell.mixer.crossfader;
    let abs = Rect::new(card.x + rect.x, card.y + rect.y, rect.w, rect.h);
    let pos = crate::drag::fraction(x.slider.value(), x.slider.min, x.slider.max) as f32;
    fader_paint::crossfader(
        t,
        rect,
        pos,
        t.text_primary,
        Target::Crossfader,
        abs,
        None,
        cx,
    )
    .child(slider_focus(t, Target::Crossfader, cx))
}

/// The crossfader curve: one button showing the engine's curve; a press takes the next one.
fn curve(shell: &Shell, rect: Rect, cx: &mut Context<Shell>) -> Div {
    let t = &shell.theme;
    let selected = shell.mixer.crossfader.selected_curve();
    let label = CURVE_LABELS[selected % CURVE_LABELS.len()];
    place(rect).child(
        face_button(t, label, (Face::Rest, t.text_primary), cx, |this, _, _| {
            this.mixer.crossfader.curve_cycled();
        })
        .size_full()
        .child(tip_overlay(
            t,
            "curve-tip",
            CURVE_TIPS[selected % CURVE_TIPS.len()],
        )),
    )
}

/// The FX drawer of deck `index`'s FX tab at `area` (card coordinates): the rack scopes (master
/// and each channel), the unit and its switch, then the mix and the unit's parameters as knobs
/// with their values beside them, and the beat the unit syncs to.
pub(crate) fn fx_drawer(
    shell: &Shell,
    index: usize,
    card: Rect,
    area: Rect,
    cx: &mut Context<Shell>,
) -> Div {
    let t = &shell.theme;
    let m = metrics(t);
    let fx = &shell.mixer.fx;
    let hue = t.accent_for(index as i32);
    let h = px(m.control_height);
    let scopes: Vec<String> = (0..fx.num_scopes()).map(FxPanel::scope_label).collect();
    let scope_refs: Vec<&str> = scopes.iter().map(String::as_str).collect();
    let names = fx.unit_names();
    let unit = fx.unit_index();
    let count = names.len().max(1);
    let unit_label = names.get(unit).cloned().unwrap_or_default();
    let top = div()
        .flex()
        .items_center()
        .gap(px(m.space[2]))
        .child(segmented(
            t,
            &scope_refs,
            (Some(fx.scope()), hue),
            cx,
            |this: &mut Shell, s| {
                this.mixer.fx.set_scope(s);
            },
        ))
        .child(
            face_button(t, &unit_label, (Face::Rest, hue), cx, move |this, _, _| {
                let next = (this.mixer.fx.unit_index() + 1) % count;
                this.mixer.fx.set_unit(next);
            })
            .h(h)
            .flex_none(),
        )
        .child(
            face_button(
                t,
                "On",
                (if fx.enabled() { Face::On } else { Face::Rest }, hue),
                cx,
                |this, _, _| {
                    this.mixer.fx.enable_pressed();
                },
            )
            .h(h)
            .flex_none(),
        )
        .child(
            numeral(t, Numeral::Value, fx.beat_label().to_owned())
                .text_color(paint(t.text_secondary)),
        );
    let mut el =
        place(area).child(place(Rect::sized(area.w, m.control_height.round() as i32)).child(top));
    let k = m.knob_size.round() as i32;
    let labels =
        std::iter::once(fx.mix.label.as_str()).chain(fx.params.iter().map(|p| p.label.as_str()));
    let text_w = om_ui::widgets::mixer::fx_knob_text_width(&m, t.type_label, labels);
    let cell_w = k + m.space[1].round() as i32 + text_w;
    let row_y = m.control_height.round() as i32 + m.space[2].round() as i32;
    let knobs = std::iter::once((&fx.mix, Target::FxMix)).chain(
        fx.params
            .iter()
            .enumerate()
            .map(|(i, k)| (k, Target::FxParam(i))),
    );
    let per_row = ((area.w + m.space[3] as i32) / (cell_w + m.space[3] as i32)).max(1);
    for (i, (model, target)) in knobs.enumerate() {
        let (col, row) = (i as i32 % per_row, i as i32 / per_row);
        let x = col * (cell_w + m.space[3] as i32);
        let y = row_y + row * (k + m.space[2] as i32);
        if y + k > area.h {
            break;
        }
        let row_rect = Rect::new(x, y, cell_w, k);
        let cell = KnobCell {
            knob: Rect::new(row_rect.x, row_rect.y, k, k),
            text: Rect::new(
                row_rect.x + k + m.space[1] as i32,
                row_rect.y,
                cell_w - k - m.space[1] as i32,
                k,
            ),
        };
        let abs_card = Rect::new(card.x + area.x, card.y + area.y, area.w, area.h);
        el = el.children(knob_cell(
            shell,
            &cell,
            (model, &model.label, target),
            (abs_card, hue, true),
            cx,
        ));
    }
    el
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_caption_drops_the_unit_its_label_names() {
        assert_eq!(caption_value("+4.0 dB"), "+4.0");
        assert_eq!(caption_value("-inf"), "-inf");
        assert_eq!(caption_value("65 %"), "65");
    }
}

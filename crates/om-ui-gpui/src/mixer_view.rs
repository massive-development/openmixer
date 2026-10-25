//! The mixer: channel strips, crossfader, master, headphone cue and the FX page. Models come from
//! `om-ui`'s `MixerPanel`; every control sends through the string-id control surface.
//!
//! Labels appear only where position does not already say what a control is: the band names (which
//! double as kill switches), GAIN, FILTER, A / THRU / B, MASTER and PHONES. Values appear in a
//! popup while a control is dragged (knobs also on hover), never at rest.

use crate::app::Shell;
use crate::controls::{
    button, caption, knob_face_with, mark, slider_with_popup, stereo_meter, tip_overlay, BarState,
    KnobFace, KnobLook,
};
use crate::drag::Target;
use crate::knob_paint::KnobRole;
use crate::paint::{paint, place};
use gpui::{prelude::*, px, Context, Div, FontWeight};
use om_ui::control::{Knob, Readout};
use om_ui::geometry::Rect;
use om_ui::meter::MeterBar;
use om_ui::theme::Colour;
use om_ui::widgets::channel::{channel_letter, ChannelStrip, ASSIGN_LABELS};
use om_ui::widgets::crossfader::{CrossfaderSection, CURVE_LABELS};
use om_ui::widgets::cue::CueSection;
use om_ui::widgets::fx_panel::FxPanel;
use om_ui::widgets::master::MasterSection;
use om_ui::widgets::mixer::{MixerPanel, Page};

/// Narrower than this a knob drops its label (a stem filter beside a strip), in px.
const MIN_LABELLED_KNOB: i32 = 36;

/// The mixer module drawn at `card` (window coordinates): the FX toggle, master section,
/// headphone cue and crossfader. The channel strips are separate cards ([`channel_card`]).
pub fn mixer_card(shell: &Shell, card: Rect, cx: &mut Context<Shell>) -> Div {
    let t = &shell.theme;
    let m = &shell.mixer;
    let l = MixerPanel::module_layout(Rect::sized(card.w, card.h), t);
    let accent = t.accent_for(0);

    let mut root = place(card)
        .bg(paint(t.surface))
        .border_1()
        .border_color(paint(t.border))
        .rounded(px(t.control_radius))
        .text_color(paint(t.text_primary));

    // One toggle: the FX page replaces the stage while it is lit.
    root = root.child(button(t, l.fx, "FX", m.fx_visible(), accent, cx, |this| {
        let page = if this.mixer.fx_visible() {
            Page::Mixer
        } else {
            Page::Fx
        };
        this.mixer.set_page(page);
    }));
    root = root.children(master(shell, l.master, card, cx));
    root = root.children(cue(shell, l.cue, card, cx));
    root = root.children(crossfader(shell, l.crossfader, card, cx));
    root
}

/// Channel `ch`'s strip as a card drawn at `card` (window coordinates), edged in the deck's colour.
pub fn channel_card(shell: &Shell, ch: usize, card: Rect, cx: &mut Context<Shell>) -> Div {
    let t = &shell.theme;
    place(card)
        .bg(paint(t.surface))
        .border_1()
        .border_color(paint(t.accent_for(ch as i32)))
        .rounded(px(t.control_radius))
        .text_color(paint(t.text_primary))
        .children(channel(shell, ch, Rect::sized(card.w, card.h), card, cx))
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

/// One channel strip, top to bottom: the deck letter, gain, the three bands (each label is its
/// kill switch), filter, PFL and FX, the fader beside its meter, and the A / THRU / B switch.
fn channel(
    shell: &Shell,
    ch: usize,
    bounds: Rect,
    card: Rect,
    cx: &mut Context<Shell>,
) -> Vec<Div> {
    let t = &shell.theme;
    let strip: &ChannelStrip = &shell.mixer.strips[ch];
    let lay = ChannelStrip::layout(bounds, t);
    let abs = |r: Rect| Rect::new(card.x + r.x, card.y + r.y, r.w, r.h);
    let accent = t.accent_for(ch as i32);
    let mut out = Vec::new();

    out.push(
        place(lay.title)
            .flex()
            .items_center()
            .text_size(px(t.type_label))
            .font_weight(FontWeight::BOLD)
            .text_color(paint(accent))
            .child(channel_letter(ch).to_string()),
    );
    out.push(knob_view(
        shell,
        lay.trim,
        &strip.trim,
        Target::Trim(ch),
        card,
        (true, KnobRole::Wide, Some(accent)),
        cx,
    ));
    for band in 0..3 {
        // One framed group per band, so the name (its kill switch) and the knob read as a pair.
        let (k, e) = (lay.kill[band], lay.eq[band]);
        let (x0, x1) = (k.x.min(e.x), k.right().max(e.right()));
        let (y0, y1) = (k.y.min(e.y), k.bottom().max(e.bottom()));
        out.push(
            place(Rect::new(x0 - 1, y0 - 1, x1 - x0 + 2, y1 - y0 + 2))
                .bg(paint(t.well))
                .border_1()
                .border_color(paint(t.border))
                .rounded(px(t.control_radius)),
        );
        // The band's name is its kill switch: lit while the band is killed.
        let killed = strip.is_killed(band);
        out.push(
            button(
                t,
                k,
                &strip.eq[band].label,
                killed,
                accent,
                cx,
                move |this| {
                    this.mixer.strips[ch].kill_pressed(band);
                },
            )
            .child(tip_overlay(
                t,
                format!("kill-{ch}-{band}"),
                format!("Kill {} band", strip.eq[band].label),
            )),
        );
        out.push(knob_view(
            shell,
            lay.eq[band],
            &strip.eq[band],
            Target::Eq(ch, band),
            card,
            (false, KnobRole::Band, Some(accent)),
            cx,
        ));
    }
    out.push(knob_view(
        shell,
        lay.filter,
        &strip.filter,
        Target::Filter(ch),
        card,
        (true, KnobRole::Wide, Some(accent)),
        cx,
    ));
    out.push(button(
        t,
        lay.pfl,
        "PFL",
        strip.pfl,
        accent,
        cx,
        move |this| {
            this.mixer.strips[ch].pfl_pressed();
        },
    ));
    out.push(button(t, lay.fx, "FX", false, accent, cx, move |this| {
        this.mixer.request_fx(ch);
    }));
    let fader = &strip.fader;
    let target = Target::Channel(ch);
    let popup = shell
        .is_dragging(target)
        .then(|| Readout::Gain.text(fader.value()));
    out.push(slider_with_popup(
        t,
        lay.fader,
        (fader.value(), fader.min, fader.max),
        target,
        abs(lay.fader),
        popup.as_deref(),
        cx,
    ));
    let bars = [bar_state(&strip.meter.left), bar_state(&strip.meter.right)];
    out.push(stereo_meter(
        t,
        lay.meter,
        lay.meter_labels,
        bars,
        cx,
        move |this| {
            let meter = &mut this.mixer.strips[ch].meter;
            meter.left.click();
            meter.right.click();
        },
    ));
    let assigned = strip.assign;
    for (k, label) in ASSIGN_LABELS.iter().enumerate() {
        out.push(button(
            t,
            lay.assign[k],
            label,
            assigned == k,
            accent,
            cx,
            move |this| {
                this.mixer.strips[ch].assign_pressed(k);
            },
        ));
    }
    out
}

/// A knob at `rect` for `model`. `labelled` prints its name beside or below it; `role` sizes the
/// cap; `accent` colours its value arc. The value is in the popup (while dragged or hovered).
fn knob_view(
    shell: &Shell,
    rect: Rect,
    model: &Knob,
    target: Target,
    card: Rect,
    (labelled, role, accent): (bool, KnobRole, Option<Colour>),
    cx: &mut Context<Shell>,
) -> Div {
    let abs = Rect::new(card.x + rect.x, card.y + rect.y, rect.w, rect.h);
    let readout = model.text();
    let label = if labelled && rect.w >= MIN_LABELLED_KNOB {
        model.label.as_str()
    } else {
        ""
    };
    knob_face_with(
        &shell.theme,
        rect,
        KnobFace {
            label,
            readout: &readout,
            detent: model.detent,
            active: shell.shows_value(target),
        },
        (model.value(), model.range.min, model.range.max),
        (target, abs),
        KnobLook {
            role,
            accent,
            dragging: shell.is_dragging(target),
            disabled: false,
        },
        cx,
    )
}

/// Master level knob and stereo meter under the MASTER caption.
fn master(shell: &Shell, bounds: Rect, card: Rect, cx: &mut Context<Shell>) -> Vec<Div> {
    let t = &shell.theme;
    let lay = MasterSection::layout(bounds, t);
    let m = &shell.mixer.master;
    let bars = [bar_state(&m.meter.left), bar_state(&m.meter.right)];
    vec![
        caption(t, lay.title, "MASTER"),
        // The caption above already names it, so the knob prints no label of its own.
        knob_view(
            shell,
            lay.fader,
            &m.fader,
            Target::Master,
            card,
            (false, KnobRole::Cue, Some(t.accent_for(0))),
            cx,
        ),
        stereo_meter(t, lay.meter, lay.meter_labels, bars, cx, |this| {
            let meter = &mut this.mixer.master.meter;
            meter.left.click();
            meter.right.click();
        }),
    ]
}

/// The crossfader, with A and B printed at its ends, and the curve switch beside it.
fn crossfader(shell: &Shell, bounds: Rect, card: Rect, cx: &mut Context<Shell>) -> Vec<Div> {
    let t = &shell.theme;
    let lay = CrossfaderSection::layout(bounds, t);
    let x = &shell.mixer.crossfader;
    let abs = Rect::new(
        card.x + lay.slider.x,
        card.y + lay.slider.y,
        lay.slider.w,
        lay.slider.h,
    );
    let end = lay.slider.h.min(lay.slider.w / 4);
    let selected = x.selected_curve().min(CURVE_LABELS.len() - 1);
    vec![
        slider_with_popup(
            t,
            lay.slider,
            (x.slider.value(), x.slider.min, x.slider.max),
            Target::Crossfader,
            abs,
            None,
            cx,
        ),
        mark(
            t,
            Rect::new(lay.slider.x, lay.slider.y, end, lay.slider.h),
            "A",
        ),
        mark(
            t,
            Rect::new(lay.slider.right() - end, lay.slider.y, end, lay.slider.h),
            "B",
        ),
        button(
            t,
            lay.curve,
            CURVE_LABELS[selected],
            false,
            t.accent_for(0),
            cx,
            |this| {
                this.mixer.crossfader.curve_cycled();
            },
        ),
    ]
}

/// Headphone cue blend and phones level, side by side under one PHONES caption.
fn cue(shell: &Shell, bounds: Rect, card: Rect, cx: &mut Context<Shell>) -> Vec<Div> {
    let t = &shell.theme;
    let lay = CueSection::layout(bounds, t);
    let c = &shell.mixer.cue;
    let accent = Some(t.accent_for(0));
    vec![
        caption(t, lay.caption, "PHONES"),
        knob_view(
            shell,
            lay.mix,
            &c.mix,
            Target::CueMix,
            card,
            (true, KnobRole::Cue, accent),
            cx,
        ),
        knob_view(
            shell,
            lay.level,
            &c.level,
            Target::CueLevel,
            card,
            (true, KnobRole::Cue, accent),
            cx,
        ),
    ]
}

/// The FX page: scope buttons, unit selector, enable, the mix knob and the unit's parameters.
pub(crate) fn fx_page(
    shell: &Shell,
    bounds: Rect,
    card: Rect,
    cx: &mut Context<Shell>,
) -> Vec<Div> {
    let t = &shell.theme;
    let fx = &shell.mixer.fx;
    let lay = fx.layout(bounds, t);
    let accent = t.accent_for(0);
    let mut out = Vec::new();

    for (s, rect) in lay.scopes.iter().enumerate() {
        out.push(button(
            t,
            *rect,
            &FxPanel::scope_label(s),
            fx.scope() == s,
            accent,
            cx,
            move |this| {
                this.mixer.fx.set_scope(s);
            },
        ));
    }
    let names = fx.unit_names();
    let unit = fx.unit_index();
    let unit_label = names.get(unit).cloned().unwrap_or_default();
    let count = names.len().max(1);
    out.push(button(
        t,
        lay.unit_box,
        &unit_label,
        false,
        accent,
        cx,
        move |this| {
            let next = (this.mixer.fx.unit_index() + 1) % count;
            this.mixer.fx.set_unit(next);
        },
    ));
    out.push(button(
        t,
        lay.enable,
        "On",
        fx.enabled(),
        accent,
        cx,
        |this| {
            this.mixer.fx.enable_pressed();
        },
    ));
    if let Some(rect) = lay.knobs.first() {
        out.push(knob_view(
            shell,
            *rect,
            &fx.mix,
            Target::FxMix,
            card,
            (true, KnobRole::Fx, Some(accent)),
            cx,
        ));
    }
    for (i, param) in fx.params.iter().enumerate() {
        if let Some(rect) = lay.knobs.get(i + 1) {
            out.push(knob_view(
                shell,
                *rect,
                param,
                Target::FxParam(i),
                card,
                (true, KnobRole::Fx, Some(accent)),
                cx,
            ));
        }
    }
    out.push(caption(t, lay.beat, fx.beat_label()));
    out
}

//! The mixer card: tabs, channel strips, crossfader, master, headphone cue and the FX page. Models
//! come from `om-ui`'s `MixerPanel`; every control sends through the string-id control surface.

use crate::app::Shell;
use crate::controls::{button, caption, knob, slider, stereo_meter};
use crate::drag::Target;
use crate::paint::{paint, place};
use gpui::{prelude::*, px, Context, Div};
use om_ui::control::{Knob, Readout};
use om_ui::geometry::Rect;
use om_ui::theme::ThemeTokens;
use om_ui::widgets::channel::{ChannelStrip, ASSIGN_LABELS};
use om_ui::widgets::crossfader::{CrossfaderSection, CURVE_LABELS};
use om_ui::widgets::cue::CueSection;
use om_ui::widgets::fx_panel::FxPanel;
use om_ui::widgets::master::MasterSection;
use om_ui::widgets::mixer::{MixerPanel, Page};

/// The mixer card drawn at `card` (window coordinates).
pub fn mixer_card(shell: &Shell, card: Rect, cx: &mut Context<Shell>) -> Div {
    let t = &shell.theme;
    let m = &shell.mixer;
    let l = MixerPanel::layout(Rect::sized(card.w, card.h), m.strips.len(), t);
    let accent = t.accent_for(0);

    let mut root = place(card)
        .bg(paint(t.surface))
        .border_1()
        .border_color(paint(t.border))
        .rounded(px(t.control_radius))
        .text_color(paint(t.text_primary));

    // Page tabs.
    let on_mixer = m.mixer_visible();
    root = root.child(button(
        t,
        l.tabs[0],
        "Mixer",
        on_mixer,
        accent,
        cx,
        |this| {
            this.mixer.set_page(Page::Mixer);
        },
    ));
    root = root.child(button(
        t,
        l.tabs[1],
        "FX",
        m.fx_visible(),
        accent,
        cx,
        |this| {
            this.mixer.set_page(Page::Fx);
        },
    ));

    if on_mixer {
        for (ch, strip_rect) in l.strips.iter().enumerate() {
            root = root.children(channel(shell, ch, *strip_rect, card, cx));
        }
        root = root.children(master(shell, l.master, card, cx));
        root = root.children(crossfader(shell, l.crossfader, card, cx));
        root = root.children(cue(shell, l.cue, card, cx));
    } else {
        root = root.children(fx_page(shell, l.fx, card, cx));
    }
    root
}

/// One channel strip: title, trim and filter knobs, EQ bands with kill buttons, PFL, FX, the
/// crossfader assignment, the fader and the meter.
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

    out.push(caption(t, lay.title, &format!("Ch {}", ch + 1)));
    out.push(knob_view(
        t,
        lay.trim,
        &strip.trim,
        Target::Trim(ch),
        card,
        cx,
    ));
    out.push(knob_view(
        t,
        lay.filter,
        &strip.filter,
        Target::Filter(ch),
        card,
        cx,
    ));
    for band in 0..3 {
        out.push(knob_view(
            t,
            lay.eq[band],
            &strip.eq[band],
            Target::Eq(ch, band),
            card,
            cx,
        ));
        let killed = strip.is_killed(band);
        out.push(button(
            t,
            lay.kill[band],
            "Kill",
            killed,
            accent,
            cx,
            move |this| {
                this.mixer.strips[ch].kill_pressed(band);
            },
        ));
    }
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
    let fader = &strip.fader;
    out.push(caption(
        t,
        lay.fader_caption,
        &Readout::Gain.text(fader.value()),
    ));
    out.push(slider(
        t,
        lay.fader,
        (fader.value(), fader.min, fader.max),
        Target::Channel(ch),
        abs(lay.fader),
        cx,
    ));
    let levels = [
        strip.meter.left.ballistics().level(),
        strip.meter.right.ballistics().level(),
    ];
    let clip = [
        strip.meter.left.ballistics().is_clipping(),
        strip.meter.right.ballistics().is_clipping(),
    ];
    out.push(stereo_meter(t, lay.meter, levels, clip));
    out
}

/// A knob at `rect` for `knob`, with its own label and readout.
fn knob_view(
    t: &ThemeTokens,
    rect: Rect,
    model: &Knob,
    target: Target,
    card: Rect,
    cx: &mut Context<Shell>,
) -> Div {
    let abs = Rect::new(card.x + rect.x, card.y + rect.y, rect.w, rect.h);
    knob(
        t,
        rect,
        (&model.label, &model.text()),
        (model.value(), model.range.min, model.range.max),
        target,
        abs,
        cx,
    )
}

/// Master fader and stereo meter.
fn master(shell: &Shell, bounds: Rect, card: Rect, cx: &mut Context<Shell>) -> Vec<Div> {
    let t = &shell.theme;
    let lay = MasterSection::layout(bounds, t);
    let abs = Rect::new(
        card.x + lay.fader.x,
        card.y + lay.fader.y,
        lay.fader.w,
        lay.fader.h,
    );
    let m = &shell.mixer.master;
    let levels = [
        m.meter.left.ballistics().level(),
        m.meter.right.ballistics().level(),
    ];
    let clip = [
        m.meter.left.ballistics().is_clipping(),
        m.meter.right.ballistics().is_clipping(),
    ];
    vec![
        caption(t, lay.title, "Master"),
        caption(t, lay.fader_caption, &Readout::Gain.text(m.fader.value())),
        slider(
            t,
            lay.fader,
            (m.fader.value(), m.fader.min, m.fader.max),
            Target::Master,
            abs,
            cx,
        ),
        stereo_meter(t, lay.meter, levels, clip),
    ]
}

/// Crossfader slider and the Linear / Power curve buttons.
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
    let mut out = vec![
        caption(t, lay.caption, "Crossfader"),
        caption(t, lay.curve_caption, "Curve"),
        slider(
            t,
            lay.slider,
            (x.slider.value(), x.slider.min, x.slider.max),
            Target::Crossfader,
            abs,
            cx,
        ),
    ];
    let selected = x.selected_curve();
    for (i, label) in CURVE_LABELS.iter().enumerate() {
        out.push(button(
            t,
            lay.curves[i],
            label,
            selected == i,
            t.accent_for(0),
            cx,
            move |this| {
                this.mixer.crossfader.curve_pressed(i);
            },
        ));
    }
    out
}

/// Headphone cue blend and phones level.
fn cue(shell: &Shell, bounds: Rect, card: Rect, cx: &mut Context<Shell>) -> Vec<Div> {
    let t = &shell.theme;
    let lay = CueSection::layout(bounds, t);
    let c = &shell.mixer.cue;
    let abs = |r: Rect| Rect::new(card.x + r.x, card.y + r.y, r.w, r.h);
    vec![
        caption(t, lay.mix_caption, "Cue mix"),
        slider(
            t,
            lay.mix,
            (c.mix.value(), c.mix.min, c.mix.max),
            Target::CueMix,
            abs(lay.mix),
            cx,
        ),
        caption(t, lay.level_caption, "Phones"),
        slider(
            t,
            lay.level,
            (c.level.value(), c.level.min, c.level.max),
            Target::CueLevel,
            abs(lay.level),
            cx,
        ),
    ]
}

/// The FX page: scope buttons, unit selector, enable, the mix knob and the unit's parameters.
fn fx_page(shell: &Shell, bounds: Rect, card: Rect, cx: &mut Context<Shell>) -> Vec<Div> {
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
        out.push(knob_view(t, *rect, &fx.mix, Target::FxMix, card, cx));
    }
    for (i, param) in fx.params.iter().enumerate() {
        if let Some(rect) = lay.knobs.get(i + 1) {
            out.push(knob_view(t, *rect, param, Target::FxParam(i), card, cx));
        }
    }
    out.push(caption(t, lay.beat, fx.beat_label()));
    out
}

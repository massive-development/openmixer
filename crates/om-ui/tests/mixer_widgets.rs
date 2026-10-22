//! Mixer and FX panel widgets: routing, mirroring,
//! meters, page switching and layout, headless. Painting and font-metric checks are not covered.

mod common;

use common::Rig;
use om_core::ParameterTarget;
use om_dsp::{CrossfaderAssign, CrossfaderCurve};
use om_ui::binding::{LevelSource, MixerBinding};
use om_ui::geometry::Rect;
use om_ui::meter::MeterParams;
use om_ui::theme::ThemeTokens;
use om_ui::widgets::channel::ChannelStrip;
use om_ui::widgets::fx_panel::{capitalised, FxPanel};
use om_ui::widgets::mixer::{MixerPanel, Page};

const FRAME: f32 = 1.0 / 30.0;

fn meter_params() -> MeterParams {
    MeterParams {
        floor_db: -48.0,
        clip_level: 0.99,
        release_seconds: 0.5,
        hold_seconds: 1.0,
        clip_seconds: 2.0,
    }
}

fn mixer(rig: &Rig) -> MixerBinding {
    MixerBinding::new(rig.engine.clone(), rig.recorder.clone())
}

/// Synthetic levels: a fixed peak per channel and for the master bus.
#[derive(Default)]
struct Levels {
    channel0: (f32, f32),
    master: (f32, f32),
}

impl LevelSource for Levels {
    fn channel_peaks(&self, channel: usize) -> (f32, f32) {
        if channel == 0 {
            self.channel0
        } else {
            (0.0, 0.0)
        }
    }

    fn master_peaks(&self) -> (f32, f32) {
        self.master
    }
}

fn strip(rig: &Rig, channel: usize) -> ChannelStrip {
    ChannelStrip::new(mixer(rig), channel, meter_params())
}

#[test]
fn channel_knobs_and_fader_route_mixer_ids() {
    let rig = Rig::new(2);
    let mut s = strip(&rig, 1);
    s.trim_changed(2.0);
    s.filter_changed(-0.5);
    s.eq_changed(0, -6.0);
    s.eq_changed(1, 3.0);
    s.eq_changed(2, -12.0);
    s.fader_changed(0.3);

    assert!(rig.recorder.saw("mixer.ch1.trim", 2.0));
    assert!(rig.recorder.saw("mixer.ch1.filter", -0.5));
    assert!(rig.recorder.saw("mixer.ch1.eq.low", -6.0));
    assert!(rig.recorder.saw("mixer.ch1.eq.mid", 3.0));
    assert!(rig.recorder.saw("mixer.ch1.eq.high", -12.0));
    assert!(rig.recorder.saw("mixer.ch1.fader", 0.3));

    let m = rig.engine.mixer();
    assert!((m.trim(1) - 2.0).abs() < 1e-6);
    assert!((m.filter(1) + 0.5).abs() < 1e-6);
    assert!((m.eq(1, 0) + 6.0).abs() < 1e-6);
    assert!((m.eq(1, 1) - 3.0).abs() < 1e-6);
    assert!((m.eq(1, 2) + 12.0).abs() < 1e-6);
    assert!((m.fader(1) - 0.3).abs() < 1e-6);
    assert!((m.fader(0) - 1.0).abs() < 1e-6, "other channel untouched");
}

#[test]
fn pfl_and_assign_buttons_route_and_mirror_the_engine() {
    let rig = Rig::new(2);
    let mut s = strip(&rig, 0);
    assert!(s.pfl_pressed());
    assert!(rig.recorder.saw("mixer.ch0.pfl", 1.0));
    assert!(rig.engine.mixer().pfl(0));
    assert!(s.pfl);
    s.pfl_pressed();
    assert!(rig.recorder.saw("mixer.ch0.pfl", 0.0));
    assert!(!s.pfl);

    assert!(s.assign_pressed(1));
    assert!(rig.recorder.saw("mixer.ch0.xfader", 1.0));
    assert_eq!(
        rig.engine.mixer().crossfader_assign(0),
        CrossfaderAssign::Left
    );
    assert_eq!(s.assign, 1);
    s.assign_pressed(2);
    assert!(rig.recorder.saw("mixer.ch0.xfader", 2.0));
    assert_eq!(s.assign, 2);
    s.assign_pressed(0);
    assert_eq!(
        rig.engine.mixer().crossfader_assign(0),
        CrossfaderAssign::Thru
    );
    assert!(!s.assign_pressed(7), "unknown assignment does nothing");
}

#[test]
fn eq_kill_drops_the_band_and_release_restores_it() {
    let rig = Rig::new(2);
    let mut s = strip(&rig, 0);
    s.eq_changed(1, 4.0);
    s.kill_pressed(1);
    assert!(rig.recorder.saw("mixer.ch0.eq.mid", -48.0));
    assert!((rig.engine.mixer().eq(0, 1) + 48.0).abs() < 1e-6);
    assert!(s.is_killed(1));
    assert!(!s.is_killed(0), "other bands unaffected");
    assert!((s.eq[1].value() + 48.0).abs() < 1e-4);

    s.kill_pressed(1);
    assert!((rig.engine.mixer().eq(0, 1) - 4.0).abs() < 1e-6);
    assert!(!s.is_killed(1));
}

#[test]
fn every_band_kill_routes_its_own_id() {
    let rig = Rig::new(2);
    let mut s = strip(&rig, 0);
    for (band, id) in ["mixer.ch0.eq.low", "mixer.ch0.eq.mid", "mixer.ch0.eq.high"]
        .into_iter()
        .enumerate()
    {
        s.kill_pressed(band);
        assert!(rig.recorder.saw(id, -48.0), "{id}");
    }
}

#[test]
fn fx_button_requests_the_channels_rack() {
    let rig = Rig::new(2);
    let mut panel = MixerPanel::new(mixer(&rig), meter_params());
    assert_eq!(panel.strips[1].fx_pressed(), 1);
    panel.request_fx(1);
    assert_eq!(panel.fx.scope(), 2, "scope 2 is channel 1");
}

#[test]
fn crossfader_routes_position_and_curve() {
    let rig = Rig::new(2);
    let mut section = om_ui::widgets::crossfader::CrossfaderSection::new(mixer(&rig));
    assert_eq!(
        section.selected_curve(),
        1,
        "the engine starts on constant power"
    );

    section.slider_changed(0.8);
    assert!(rig.recorder.saw("mixer.crossfader", 0.8));
    assert!((rig.engine.mixer().crossfader() - 0.8).abs() < 1e-6);

    assert_eq!(section.curve_pressed(0), Some(CrossfaderCurve::Linear));
    assert!(rig.recorder.saw("mixer.crossfader.curve", 0.0));
    assert_eq!(
        rig.engine.mixer().crossfader_curve(),
        CrossfaderCurve::Linear
    );
    assert_eq!(section.selected_curve(), 0);

    section.curve_pressed(1);
    assert!(rig.recorder.saw("mixer.crossfader.curve", 1.0));
    assert_eq!(
        rig.engine.mixer().crossfader_curve(),
        CrossfaderCurve::ConstantPower
    );
}

#[test]
fn master_and_cue_route_their_ids() {
    let rig = Rig::new(2);
    let mut master = om_ui::widgets::master::MasterSection::new(mixer(&rig), meter_params());
    master.fader_changed(0.7);
    assert!(rig.recorder.saw("mixer.master", 0.7));
    assert!((rig.engine.mixer().master_gain() - 0.7).abs() < 1e-6);

    let mut cue = om_ui::widgets::cue::CueSection::new(mixer(&rig));
    cue.mix_changed(0.6);
    cue.level_changed(0.4);
    assert!(rig.recorder.saw("mixer.cue.mix", 0.6));
    assert!(rig.recorder.saw("mixer.cue.level", 0.4));
    assert!((rig.engine.mixer().cue_mix() - 0.6).abs() < 1e-6);
    assert!((rig.engine.mixer().cue_level() - 0.4).abs() < 1e-6);
}

#[test]
fn refresh_follows_changes_made_elsewhere_without_echoing() {
    let rig = Rig::new(2);
    let mut panel = MixerPanel::new(mixer(&rig), meter_params());
    let m = &rig.router;
    m.set_parameter("mixer.ch0.fader", 0.25);
    m.set_parameter("mixer.ch1.trim", 3.0);
    m.set_parameter("mixer.ch0.pfl", 1.0);
    m.set_parameter("mixer.ch1.xfader", 2.0);
    m.set_parameter("mixer.master", 0.5);
    m.set_parameter("mixer.crossfader", 0.1);
    m.set_parameter("mixer.crossfader.curve", 0.0);
    m.set_parameter("mixer.cue.mix", 0.9);
    rig.recorder.clear();

    panel.refresh(FRAME, &Levels::default(), false);
    assert!((panel.strips[0].fader.value() - 0.25).abs() < 1e-6);
    assert!((panel.strips[1].trim.value() - 3.0).abs() < 1e-6);
    assert!(panel.strips[0].pfl);
    assert_eq!(panel.strips[1].assign, 2);
    assert!((panel.master.fader.value() - 0.5).abs() < 1e-6);
    assert!((panel.crossfader.slider.value() - 0.1).abs() < 1e-6);
    assert_eq!(panel.crossfader.selected_curve(), 0);
    assert!((panel.cue.mix.value() - 0.9).abs() < 1e-6);
    assert!(
        rig.recorder.is_empty(),
        "mirroring must not echo back into the router"
    );
}

#[test]
fn master_and_cue_sliders_reach_the_engine_ceiling() {
    let rig = Rig::new(2);
    let mut panel = MixerPanel::new(mixer(&rig), meter_params());
    rig.router.set_parameter("mixer.master", 2.0);
    rig.router.set_parameter("mixer.cue.level", 2.0);
    panel.refresh(FRAME, &Levels::default(), false);
    assert!(
        (panel.master.fader.value() - 2.0).abs() < 1e-6,
        "MIDI-set values above 1.5 must display"
    );
    assert!((panel.cue.level.value() - 2.0).abs() < 1e-6);
    assert!((panel.master.fader.max - 2.0).abs() < 1e-6);
    assert!((panel.cue.level.max - 2.0).abs() < 1e-6);
    panel.master.fader_changed(1.9);
    assert!((rig.engine.mixer().master_gain() - 1.9).abs() < 1e-6);
}

#[test]
fn channel_meters_read_levels_per_side() {
    let rig = Rig::new(2);
    let mut panel = MixerPanel::new(mixer(&rig), meter_params());
    let levels = Levels {
        channel0: (0.5, 0.0),
        master: (0.4, 0.4),
    };
    panel.refresh(FRAME, &levels, false);
    assert!(panel.strips[0].meter.left.ballistics().level() > 0.5);
    assert_eq!(
        panel.strips[0].meter.right.ballistics().level(),
        0.0,
        "right input is silent"
    );
    assert_eq!(panel.strips[1].meter.left.ballistics().level(), 0.0);
    assert!(panel.master.meter.left.ballistics().level() > 0.0);
}

#[test]
fn clipping_lights_the_lamp_and_silence_releases_the_level() {
    let rig = Rig::new(2);
    let mut panel = MixerPanel::new(mixer(&rig), meter_params());
    panel.refresh(
        FRAME,
        &Levels {
            channel0: (1.0, 1.0),
            master: (0.0, 0.0),
        },
        false,
    );
    assert!(panel.strips[0].meter.left.ballistics().is_clipping());
    for _ in 0..2000 {
        panel.refresh(FRAME, &Levels::default(), false);
    }
    assert_eq!(panel.strips[0].meter.left.ballistics().level(), 0.0);
}

#[test]
fn fx_shortcut_switches_page_and_scope() {
    let rig = Rig::new(2);
    let mut panel = MixerPanel::new(mixer(&rig), meter_params());
    assert_eq!(panel.page(), Page::Mixer);
    assert!(panel.mixer_visible());
    assert!(!panel.fx_visible());

    panel.request_fx(1);
    assert_eq!(panel.page(), Page::Fx);
    assert_eq!(panel.fx.scope(), 2);
    assert!(panel.fx_visible());
    assert!(!panel.mixer_visible());

    panel.set_page(Page::Mixer);
    assert!(panel.mixer_visible());
}

fn assert_no_overlap(parts: &[(&str, Rect)], context: &str) {
    for (i, (a, ra)) in parts.iter().enumerate() {
        assert!(!ra.is_empty(), "{context}: {a} is empty");
        for (b, rb) in &parts[i + 1..] {
            assert!(!ra.intersects(rb), "{context}: {a} overlaps {b}");
        }
    }
}

#[test]
fn module_layout_has_no_overlaps_across_sizes() {
    let theme = ThemeTokens::builtin_dark();
    for (w, h) in [(220, 280), (270, 300), (330, 420)] {
        let context = format!("module {w}x{h}");
        let bounds = Rect::sized(w, h);
        let layout = MixerPanel::module_layout(bounds, &theme);
        let parts: Vec<(&str, Rect)> = vec![
            ("tab mixer", layout.tabs[0]),
            ("tab fx", layout.tabs[1]),
            ("master", layout.master),
            ("crossfader", layout.crossfader),
            ("cue", layout.cue),
        ];
        assert_no_overlap(&parts, &context);
        for (name, r) in &parts {
            assert!(bounds.contains(r), "{context}: {name} clipped");
        }
    }
}

/// The cell the standard layout gives a named slot at the default window size.
fn slot_cell(decks: usize, name: &str) -> Rect {
    let theme = ThemeTokens::builtin_dark();
    let gap = theme.gap;
    let main = Rect::sized(1280, 800).reduced(gap, gap);
    om_ui::main_layout::visible_cells(
        &om_ui::main_layout::LayoutSpec::standard(decks),
        decks,
        main,
        gap,
    )
    .into_iter()
    .find(|c| c.kind == om_ui::main_layout::CellKind::Slot(name.into()))
    .unwrap_or_else(|| panic!("{name} slot"))
    .bounds
}

#[test]
fn strip_children_fit_and_do_not_overlap_at_the_default_slot_size() {
    let theme = ThemeTokens::builtin_dark();
    for c in 0..2 {
        let strip_rect = slot_cell(2, &om_ui::main_layout::channel_slot(c));
        let strip_rect = Rect::sized(strip_rect.w, strip_rect.h);
        let s = ChannelStrip::layout(strip_rect, &theme);
        let mut kids: Vec<(&str, Rect)> = vec![
            ("title", s.title),
            ("trim", s.trim),
            ("filter", s.filter),
            ("pfl", s.pfl),
            ("fx", s.fx),
            ("fader caption", s.fader_caption),
            ("fader", s.fader),
            ("meter", s.meter),
        ];
        for (i, r) in s.eq.iter().enumerate() {
            kids.push(("eq", *r));
            kids.push(("kill", s.kill[i]));
        }
        for r in &s.assign {
            kids.push(("assign", *r));
        }
        assert_no_overlap(&kids, &format!("strip {c}"));
        for (name, r) in &kids {
            assert!(strip_rect.contains(r), "strip {c}: {name} clipped");
        }
        assert!(s.fader.h >= 80, "the fader needs real travel, not a sliver");
    }
}

#[test]
fn mixer_module_is_wide_enough_at_the_default_window_size() {
    let slot = slot_cell(2, "mixer");
    assert!(slot.w >= 200, "mixer module is {} px wide", slot.w);
    assert!(
        slot_cell(2, &om_ui::main_layout::channel_slot(0)).w >= 140,
        "a channel strip needs room for three EQ knobs and the fader"
    );
}

// ---- FX panel ----

fn fx(rig: &Rig) -> FxPanel {
    FxPanel::new(mixer(rig))
}

#[test]
fn scope_buttons_cover_master_and_every_channel() {
    let rig = Rig::new(4);
    let mut panel = fx(&rig);
    assert_eq!(panel.num_scopes(), 5);
    assert_eq!(FxPanel::scope_label(0), "Master");
    assert_eq!(FxPanel::scope_label(3), "Ch 3");
    assert_eq!(panel.scope(), 0);
    panel.set_scope(3);
    assert_eq!(panel.scope(), 3);
}

#[test]
fn unit_list_comes_from_the_rack_and_covers_every_effect() {
    let rig = Rig::new(2);
    let panel = fx(&rig);
    let rack = rig.engine.fx().master_rack();
    let names = panel.unit_names();
    assert_eq!(names.len(), rack.num_units());
    for (i, name) in names.iter().enumerate() {
        let effect = rack.unit(i).expect("unit").effect_name();
        assert_eq!(name.to_lowercase(), effect);
        assert_eq!(*name, capitalised(effect));
    }
}

#[test]
fn param_knobs_are_built_from_the_effect_descriptors() {
    let rig = Rig::new(2);
    let mut panel = fx(&rig);
    let rack = rig.engine.fx().master_rack();
    for u in 0..rack.num_units() {
        panel.set_unit(u);
        let unit = rack.unit(u).expect("unit");
        let params = unit.params();
        assert_eq!(panel.params.len(), params.len(), "{}", unit.effect_name());
        for p in 0..params.len() {
            let spec = params.spec(p).expect("spec");
            let knob = &panel.params[p];
            assert!((knob.range.min - f64::from(spec.min)).abs() < 1e-6);
            assert!((knob.range.max - f64::from(spec.max)).abs() < 1e-6);
            assert!(
                (knob.value() - f64::from(params.target(p).expect("target"))).abs() < 1e-4,
                "starts at the engine value"
            );
            assert!(!knob.label.is_empty());
        }
    }
}

#[test]
fn param_knobs_route_their_fx_ids() {
    let rig = Rig::new(2);
    let mut panel = fx(&rig);
    let rack = rig.engine.fx().master_rack();
    let echo = rack
        .find_unit("echo")
        .expect("echo in the default master rack");
    panel.set_unit(echo);
    let params = rack.unit(echo).expect("unit").params();
    let feedback = params.find("feedback").expect("feedback");
    panel.param_changed(feedback, 0.8);
    assert!(rig
        .recorder
        .saw(&format!("fx.master.fx{echo}.feedback"), 0.8));
    assert!((params.target(feedback).expect("target") - 0.8).abs() < 1e-6);
}

#[test]
fn every_parameter_of_every_effect_routes_on_every_rack() {
    let rig = Rig::new(2);
    let mut panel = fx(&rig);
    for scope in 0..panel.num_scopes() {
        panel.set_scope(scope);
        let name = if scope == 0 {
            "master".to_owned()
        } else {
            format!("ch{}", scope - 1)
        };
        let rack = if scope == 0 {
            rig.engine.fx().master_rack()
        } else {
            rig.engine
                .fx()
                .channel_rack(scope - 1)
                .expect("channel rack")
        };
        for u in 0..rack.num_units() {
            panel.set_unit(u);
            let unit = rack.unit(u).expect("unit");
            let params = unit.params();
            for p in 0..params.len() {
                let spec = params.spec(p).expect("spec");
                let current = f64::from(params.target(p).expect("target"));
                let mut target =
                    f64::from(spec.min) + 0.5 * (f64::from(spec.max) - f64::from(spec.min));
                if (target - current).abs() < 0.05 {
                    target = f64::from(spec.max);
                }
                panel.param_changed(p, target);
                let id = format!("fx.{name}.fx{u}.{}", spec.name);
                assert!(rig.recorder.calls().iter().any(|(i, _)| *i == id), "{id}");
                assert!(
                    (f64::from(params.target(p).expect("target")) - target).abs() < 0.01,
                    "{id}"
                );
            }
        }
    }
}

#[test]
fn enable_and_mix_route_and_mirror_the_unit() {
    let rig = Rig::new(2);
    let mut panel = fx(&rig);
    panel.show_channel(1);
    panel.set_unit(1);
    assert_eq!(panel.enable_pressed(), Some(1.0));
    assert!(rig.recorder.saw("fx.ch1.fx1.enable", 1.0));
    assert!(rig
        .engine
        .fx()
        .channel_rack(1)
        .expect("rack")
        .unit(1)
        .expect("unit")
        .is_enabled());
    assert!(panel.enabled());
    assert_eq!(panel.enable_pressed(), Some(0.0));
    assert!(rig.recorder.saw("fx.ch1.fx1.enable", 0.0));

    panel.mix_changed(0.35);
    assert!(rig.recorder.saw("fx.ch1.fx1.mix", 0.35));
    assert!(
        (rig.engine
            .fx()
            .channel_rack(1)
            .expect("rack")
            .unit(1)
            .expect("unit")
            .mix()
            - 0.35)
            .abs()
            < 1e-6
    );
}

#[test]
fn refresh_mirrors_external_changes_without_echoing() {
    let rig = Rig::new(2);
    let mut panel = fx(&rig);
    panel.set_unit(0);
    rig.router.set_parameter("fx.master.fx0.mix", 0.2);
    rig.router.set_parameter("fx.master.fx0.enable", 1.0);
    rig.recorder.clear();
    panel.refresh();
    assert!((panel.mix.value() - 0.2).abs() < 1e-6);
    assert!(panel.enabled());
    assert!(rig.recorder.is_empty());
}

#[test]
fn beat_readout_follows_the_rack_tempo() {
    let rig = Rig::new(2);
    let mut panel = fx(&rig);
    assert!(
        panel.beat_label().contains("120.0 BPM"),
        "{}",
        panel.beat_label()
    );
    assert!(
        panel.beat_label().contains("500 ms"),
        "{}",
        panel.beat_label()
    );
    rig.router.set_parameter("fx.master.bpm", 150.0);
    panel.refresh();
    assert!(
        panel.beat_label().contains("150.0 BPM"),
        "{}",
        panel.beat_label()
    );
    assert!(
        panel.beat_label().contains("400 ms"),
        "{}",
        panel.beat_label()
    );
}

#[test]
fn scopes_remember_their_own_unit() {
    let rig = Rig::new(2);
    let mut panel = fx(&rig);
    panel.set_unit(3);
    panel.set_scope(1);
    assert_eq!(panel.unit_index(), 0);
    panel.set_unit(5);
    panel.set_scope(0);
    assert_eq!(panel.unit_index(), 3);
}

#[test]
fn controls_stay_inside_the_panel_and_do_not_overlap() {
    let rig = Rig::new(2);
    let theme = ThemeTokens::builtin_dark();
    let mut panel = fx(&rig);
    for (w, h) in [(420, 400), (560, 500)] {
        let bounds = Rect::sized(w, h);
        for u in 0..panel.unit_count() {
            panel.set_unit(u);
            let layout = panel.layout(bounds, &theme);
            let mut parts: Vec<(&str, Rect)> = vec![
                ("unit box", layout.unit_box),
                ("enable", layout.enable),
                ("beat", layout.beat),
            ];
            parts.extend(layout.scopes.iter().map(|r| ("scope", *r)));
            parts.extend(layout.knobs.iter().map(|r| ("knob", *r)));
            assert_no_overlap(&parts, &format!("unit {u} at {w}x{h}"));
            for (name, r) in &parts {
                assert!(
                    bounds.contains(r),
                    "unit {u} at {w}x{h}: {name} escapes the panel"
                );
            }
        }
    }
}

#[test]
fn setting_the_rack_knobs_matches_the_descriptor_defaults_when_unrefreshed() {
    let rig = Rig::new(2);
    let mut panel = fx(&rig);
    panel.set_unit(0);
    let count = panel.params.len();
    assert_eq!(
        count,
        rig.engine
            .fx()
            .master_rack()
            .unit(0)
            .expect("unit")
            .params()
            .len()
    );
}

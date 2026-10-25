//! Theme JSON parsing.

use om_ui::theme::{parse_theme, parse_theme_file, Colour, ThemeTokens};
use std::path::Path;

fn dark() -> ThemeTokens {
    ThemeTokens::builtin_dark()
}

fn assert_close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 1e-5, "{actual} != {expected}");
}

#[test]
fn minimal_theme_inherits_everything_from_dark() {
    let p = parse_theme(r##"{ "id": "t.min" }"##).expect("minimal theme parses");
    assert_eq!(p.info.id, "t.min");
    assert_eq!(p.info.name, "t.min", "name defaults to the id");
    let d = dark();
    assert_eq!(p.tokens.backdrop_top, d.backdrop_top);
    assert_eq!(p.tokens.gap, d.gap);
    assert_eq!(p.tokens.accents.len(), d.accents.len());
}

#[test]
fn parses_every_section() {
    let p = parse_theme(
        r##"{
        "id": "t.full", "name": "Full", "author": "me",
        "colours": { "textPrimary": "#112233", "well": "#80112233" },
        "accents": ["#FF0000", "#00FF00"],
        "spacing": { "gap": 6, "pad": 7, "innerGap": 3 },
        "radii": { "card": 20, "control": 4.5 },
        "type": { "title": 22, "mono": 13.5 },
        "translucency": { "surface": 0.5, "backdrop": 0.25 } }"##,
    )
    .expect("full theme parses");
    assert_eq!(p.info.author, "me");
    assert_eq!(
        p.tokens.text_primary,
        Colour::from_argb(0xFF11_2233),
        "#RRGGBB is opaque"
    );
    assert_eq!(p.tokens.well, Colour::from_argb(0x8011_2233));
    assert_eq!(p.tokens.accents.len(), 2);
    assert_eq!(p.tokens.accents[1], Colour::from_argb(0xFF00_FF00));
    assert_eq!(p.tokens.gap, 6);
    assert_eq!(p.tokens.inner_gap, 3);
    assert_close(p.tokens.card_radius, 20.0);
    assert_close(p.tokens.control_radius, 4.5);
    assert_close(p.tokens.type_title, 22.0);
    assert_close(p.tokens.surface_opacity, 0.5);
    assert_close(p.tokens.backdrop_opacity, 0.25);
    assert_close(p.tokens.type_body, dark().type_body);
}

#[test]
fn parses_library_metrics_and_rejects_out_of_range_values() {
    let p = parse_theme(
        r##"{ "id": "t.lib", "library": { "rowHeight": 30, "sidebarWidth": 210, "compatAlpha": 0.5, "ratingDot": 6 } }"##,
    )
    .expect("library section parses");
    assert_close(p.tokens.library_row_height, 30.0);
    assert_close(p.tokens.library_sidebar_width, 210.0);
    assert_close(p.tokens.library_compat_alpha, 0.5);
    assert_close(
        p.tokens.library_toolbar_height,
        dark().library_toolbar_height,
    );
    assert!(parse_theme(r##"{ "id": "t.bad", "library": { "rowHeight": -1 } }"##).is_err());
    assert!(parse_theme(r##"{ "id": "t.bad", "library": { "compatAlpha": 2 } }"##).is_err());
}

#[test]
fn parses_mixer_metrics_and_meter_ballistics() {
    let p = parse_theme(
        r##"{ "id": "t.mix", "mixer": { "knobHeight": 50, "meterHoldSeconds": 2.5, "meterFloorDb": -60 } }"##,
    )
    .expect("mixer section parses");
    assert_close(p.tokens.mixer_knob_height, 50.0);
    assert_close(p.tokens.meter_hold_seconds, 2.5);
    assert_close(p.tokens.meter_floor_db, -60.0);

    let sized = parse_theme(
        r##"{ "id": "t.sz", "mixer": { "titleHeight": 30, "faderWidth": 50, "curveWidth": 100, "fxBoxWidth": 120,
            "fxEnableWidth": 40, "pageButtonWidth": 70, "padX": 4, "padY": 5, "accentDot": 6, "meterHoldTickHeight": 3 } }"##,
    )
    .expect("sized mixer section parses");
    assert_close(sized.tokens.mixer_title_height, 30.0);
    assert_close(sized.tokens.mixer_fx_box_width, 120.0);
    assert_close(sized.tokens.mixer_accent_dot, 6.0);
    assert_close(sized.tokens.meter_hold_tick_height, 3.0);
    assert!(parse_theme(r##"{ "id": "t.bad", "mixer": { "faderWidth": -1 } }"##).is_err());
    assert!(parse_theme(r##"{ "id": "t.bad", "mixer": { "meterHoldTickHeight": 99 } }"##).is_err());
    assert_close(p.tokens.mixer_cue_height, dark().mixer_cue_height);
    assert!(parse_theme(r##"{ "id": "t.bad", "mixer": { "meterClipLevel": 3 } }"##).is_err());
    assert!(parse_theme(r##"{ "id": "t.bad", "mixer": { "meterHoldSeconds": -1 } }"##).is_err());
}

#[test]
fn parses_deck_metrics() {
    let p = parse_theme(
        r##"{ "id": "t.deck",
            "deck": { "rowHeight": 22, "pitchWidth": 70, "gridBarAlpha": 0.9, "playheadWidth": 3 } }"##,
    )
    .expect("deck section parses");
    assert_close(p.tokens.deck_row_height, 22.0);
    assert_close(p.tokens.deck_pitch_width, 70.0);
    assert_close(p.tokens.grid_bar_alpha, 0.9);
    assert_close(p.tokens.playhead_width, 3.0);
    assert_close(p.tokens.deck_header_height, dark().deck_header_height);

    assert!(parse_theme(r##"{ "id": "t.bad", "deck": { "gridBarAlpha": 2 } }"##).is_err());
    assert!(parse_theme(r##"{ "id": "t.bad", "deck": { "rowHeight": -1 } }"##).is_err());
}

#[test]
fn rejects_bad_input() {
    let bad = [
        "{ not json",
        "[1, 2]",
        r##"{ "name": "no id" }"##,
        r##"{ "id": "Has Spaces" }"##,
        r##"{ "id": "t", "name": "  " }"##,
        r##"{ "id": "t", "colours": { "well": "red" } }"##,
        r##"{ "id": "t", "colours": { "well": "#12345" } }"##,
        r##"{ "id": "t", "colours": { "well": "#GG0000" } }"##,
        r##"{ "id": "t", "colours": 5 }"##,
        r##"{ "id": "t", "accents": [] }"##,
        r##"{ "id": "t", "accents": ["#FF0000", "nope"] }"##,
        r##"{ "id": "t", "accents": ["#111111","#111111","#111111","#111111","#111111","#111111","#111111","#111111","#111111"] }"##,
        r##"{ "id": "t", "spacing": { "gap": -1 } }"##,
        r##"{ "id": "t", "spacing": { "gap": 12.5 } }"##,
        r##"{ "id": "t", "spacing": { "gap": 500 } }"##,
        r##"{ "id": "t", "radii": { "card": "big" } }"##,
        r##"{ "id": "t", "type": { "title": 2 } }"##,
        r##"{ "id": "t", "translucency": { "surface": 1.5 } }"##,
    ];
    for json in bad {
        match parse_theme(json) {
            Ok(_) => panic!("accepted {json}"),
            Err(e) => assert!(!e.to_string().is_empty(), "{json}"),
        }
    }
}

#[test]
fn unknown_keys_warn_but_do_not_fail() {
    let p = parse_theme(
        r##"{ "id": "t", "colurs": {}, "colours": { "wel": "#000000" }, "type": { "huge": 30 } }"##,
    )
    .expect("unknown keys are not fatal");
    assert_eq!(p.warnings.len(), 3, "{:?}", p.warnings);
}

#[test]
fn missing_file_fails() {
    assert!(parse_theme_file(Path::new("/definitely/not/here.json")).is_err());
}

#[test]
fn parses_the_waveform_curve_and_limits() {
    let p = parse_theme(
        r##"{ "id": "t.wave", "waveform": { "saturationMax": 0.5, "lightnessMax": 0.7,
            "hueStops": [ { "pos": 0, "hue": 10, "saturation": 1, "lightness": 1 },
                          { "pos": 1, "hue": 400, "saturation": 0.2, "lightness": 0.5 } ] } }"##,
    )
    .expect("waveform section parses");
    assert_close(p.tokens.wave_saturation_max, 0.5);
    assert_close(p.tokens.wave_lightness_max, 0.7);
    assert_close(p.tokens.wave_lightness_min, dark().wave_lightness_min);
    assert_eq!(p.tokens.wave_stops.len(), 2);
    assert_close(p.tokens.wave_stops[1].hue, 400.0);
    assert!(p.warnings.is_empty(), "{:?}", p.warnings);
}

#[test]
fn a_theme_without_waveform_keys_inherits_the_built_in_curve() {
    let p = parse_theme(r##"{ "id": "t.plain" }"##).expect("minimal theme");
    assert_eq!(p.tokens.wave_stops, dark().wave_stops);
    assert_close(p.tokens.wave_floor_db, dark().wave_floor_db);
}

#[test]
fn old_wave_colour_keys_are_ignored_with_a_warning() {
    let p = parse_theme(r##"{ "id": "t.old", "colours": { "waveLow": "#010203" } }"##)
        .expect("old theme still loads");
    assert!(p.warnings.iter().any(|w| w.contains("waveLow")));
}

#[test]
fn rejects_a_bad_waveform_curve() {
    let bad = |body: &str| parse_theme(&format!(r##"{{ "id": "t.bad", "waveform": {body} }}"##));
    assert!(bad(r##"{ "hueStops": [] }"##).is_err());
    assert!(bad(
        r##"{ "hueStops": [ { "pos": 0, "hue": 0, "saturation": 1, "lightness": 1 } ] }"##
    )
    .is_err());
    let backwards = r##"{ "hueStops": [ { "pos": 0.6, "hue": 0, "saturation": 1, "lightness": 1 },
        { "pos": 0.4, "hue": 90, "saturation": 1, "lightness": 1 } ] }"##;
    assert!(bad(backwards).is_err());
    let missing = r##"{ "hueStops": [ { "pos": 0, "hue": 0, "lightness": 1 },
        { "pos": 1, "hue": 90, "saturation": 1, "lightness": 1 } ] }"##;
    assert!(bad(missing).is_err());
    assert!(bad(r##"{ "saturationMax": 2 }"##).is_err());
    assert!(bad(r##"{ "lightnessMin": 0.9, "lightnessMax": 0.5 }"##).is_err());
    assert!(bad(r##"{ "spreadNarrow": 3, "spreadWide": 1 }"##).is_err());
}

#[test]
fn both_built_in_themes_define_every_waveform_key() {
    for json in [
        include_str!("../../../resources/themes/openmixer-dark.json"),
        include_str!("../../../resources/themes/openmixer-daylight.json"),
    ] {
        let value: serde_json::Value = serde_json::from_str(json).expect("valid json");
        let section = &value["waveform"];
        for key in [
            "saturationMax",
            "lightnessMin",
            "lightnessMax",
            "floorDb",
            "spreadNarrow",
            "spreadWide",
            "spreadSaturation",
            "hueStops",
        ] {
            assert!(!section[key].is_null(), "missing waveform.{key}");
        }
        assert!(parse_theme(json).expect("parses").warnings.is_empty());
    }
}

#[test]
fn meter_colours_keep_three_to_one_on_their_well_in_every_built_in_theme() {
    // WCAG 1.4.11 (non-text contrast, 3:1) for the lit meter zones, daylight amber included
    // (faders-meters research, risk 7). The well is a translucent tint over the card over the
    // backdrop.
    use om_ui::style::{contrast_ratio, over};
    for (name, json) in [
        (
            "dark",
            include_str!("../../../resources/themes/openmixer-dark.json"),
        ),
        (
            "daylight",
            include_str!("../../../resources/themes/openmixer-daylight.json"),
        ),
    ] {
        let t = parse_theme(json).expect("parses").tokens;
        let well = over(t.well, over(t.surface, t.backdrop_bottom));
        for (zone, colour) in [
            ("low", t.meter_low),
            ("mid", t.meter_mid),
            ("high", t.meter_high),
        ] {
            let ratio = contrast_ratio(colour, well);
            assert!(ratio >= 3.0, "{name} {zone}: {ratio:.2}:1 on the well");
        }
    }
}

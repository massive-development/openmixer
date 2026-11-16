//! Theme tokens as CSS custom properties, so the web front end styles from the same tokens as the
//! native window. Runs on a control thread whenever the theme or density changes.

use om_ui::theme::{Colour, Density, DensityMetrics, ThemeTokens};

/// One `:root { ... }` block: every colour and numeric token as `--om-<field>`, and the metric set
/// for `density` as `--om-density-<field>`, sorted by name.
pub fn theme_css(tokens: &ThemeTokens, density: Density) -> String {
    let mut vars = colour_vars(tokens);
    vars.extend(number_vars(tokens));
    vars.extend(density_vars(tokens.metrics(density)));
    vars.sort_unstable();
    let body: String = vars
        .iter()
        .map(|(name, value)| format!("  --om-{name}: {value};\n"))
        .collect();
    format!(":root {{\n{body}}}\n")
}

type Var = (String, String);

fn colour_vars(t: &ThemeTokens) -> Vec<Var> {
    let accents = t.accents.iter().enumerate();
    let accents = accents.map(|(i, c)| (format!("accent-{i}"), css_colour(*c)));
    let named = colour_fields(t).into_iter();
    let named = named.map(|(field, c)| (kebab(field), css_colour(c)));
    named.chain(accents).collect()
}

fn number_vars(t: &ThemeTokens) -> Vec<Var> {
    let fields = number_fields(t).into_iter().filter(|(_, v)| v.is_finite());
    fields
        .map(|(field, v)| (kebab(field), format!("{v}{}", unit(field))))
        .collect()
}

fn density_vars(m: &DensityMetrics) -> Vec<Var> {
    let space = m.space.iter().enumerate();
    let space = space.map(|(i, v)| (format!("space-{}", i + 1), *v));
    let named = density_fields(m).into_iter();
    let named = named.map(|(field, v)| (kebab(field), v));
    let all = named.chain(space).filter(|(_, v)| v.is_finite());
    all.map(|(name, v)| (format!("density-{name}"), format!("{v}px")))
        .collect()
}

fn kebab(field: &str) -> String {
    field.replace('_', "-")
}

/// CSS unit by naming convention: alphas, decibels, levels, seconds, opacities and the waveform
/// curve limits are plain numbers; `_ms` is a duration; everything else is a length.
fn unit(field: &str) -> &'static str {
    const UNITLESS: [&str; 5] = ["_alpha", "_db", "_seconds", "_level", "_opacity"];
    if field.ends_with("_ms") {
        "ms"
    } else if field.starts_with("wave_") || UNITLESS.iter().any(|s| field.ends_with(s)) {
        ""
    } else {
        "px"
    }
}

/// `#rrggbb` when opaque, else `rgb(r g b / a)` so translucency survives.
fn css_colour(c: Colour) -> String {
    let argb = c.argb();
    let (r, g, b) = ((argb >> 16) & 0xFF, (argb >> 8) & 0xFF, argb & 0xFF);
    if argb >> 24 == 0xFF {
        return format!("#{:06x}", argb & 0x00FF_FFFF);
    }
    let alpha = format!("{:.3}", c.alpha());
    let alpha = alpha.trim_end_matches('0').trim_end_matches('.');
    format!("rgb({r} {g} {b} / {alpha})")
}

// A new colour token must be added here; `every_numeric_and_colour_field_is_emitted` checks it.
fn colour_fields(t: &ThemeTokens) -> [(&'static str, Colour); 28] {
    [
        ("ground", t.ground),
        ("surface", t.surface),
        ("surface_raised", t.surface_raised),
        ("surface_hover", t.surface_hover),
        ("well", t.well),
        ("border", t.border),
        ("border_strong", t.border_strong),
        ("focus_ring", t.focus_ring),
        ("highlight", t.highlight),
        ("shadow", t.shadow),
        ("text_primary", t.text_primary),
        ("text_secondary", t.text_secondary),
        ("text_muted", t.text_muted),
        ("on_accent", t.on_accent),
        ("meter_low", t.meter_low),
        ("meter_mid", t.meter_mid),
        ("meter_high", t.meter_high),
        ("playhead", t.playhead),
        ("cue", t.cue),
        ("loop_mark", t.loop_mark),
        ("sync_on", t.sync_on),
        ("warning", t.warning),
        ("danger", t.danger),
        ("master", t.master),
        ("readout", t.readout),
        ("readout_dim", t.readout_dim),
        ("popup", t.popup),
        ("drop_target", t.drop_target),
    ]
}

// A new numeric token must be added here; `every_numeric_and_colour_field_is_emitted` checks it.
fn number_fields(t: &ThemeTokens) -> [(&'static str, f32); 100] {
    [
        ("gap", t.gap as f32),
        ("pad", t.pad as f32),
        ("inner_gap", t.inner_gap as f32),
        ("min_target", t.min_target),
        ("min_pointer_target", t.min_pointer_target),
        ("card_radius", t.card_radius),
        ("control_radius", t.control_radius),
        ("type_display", t.type_display),
        ("type_headline", t.type_headline),
        ("type_body", t.type_body),
        ("type_label", t.type_label),
        ("type_numeral", t.type_numeral),
        ("type_numeral_large", t.type_numeral_large),
        ("type_scale", t.type_scale),
        ("deck_header_height", t.deck_header_height),
        ("deck_overview_height", t.deck_overview_height),
        ("deck_transport_height", t.deck_transport_height),
        ("deck_row_height", t.deck_row_height),
        ("deck_pitch_width", t.deck_pitch_width),
        ("deck_min_zoom_height", t.deck_min_zoom_height),
        ("deck_readout_width", t.deck_readout_width),
        ("deck_range_box_height", t.deck_range_box_height),
        ("playhead_width", t.playhead_width),
        ("cue_marker_width", t.cue_marker_width),
        ("cue_guide_alpha", t.cue_guide_alpha),
        ("grid_bar_alpha", t.grid_bar_alpha),
        ("grid_beat_alpha", t.grid_beat_alpha),
        ("loop_active_alpha", t.loop_active_alpha),
        ("loop_idle_alpha", t.loop_idle_alpha),
        ("deck_tint_alpha", t.deck_tint_alpha),
        ("deck_border_alpha", t.deck_border_alpha),
        ("deck_tint_hover_alpha", t.deck_tint_hover_alpha),
        ("deck_border_hover_alpha", t.deck_border_hover_alpha),
        ("mixer_knob_height", t.mixer_knob_height),
        ("mixer_button_height", t.mixer_button_height),
        ("mixer_meter_width", t.mixer_meter_width),
        ("mixer_master_width", t.mixer_master_width),
        ("mixer_tab_height", t.mixer_tab_height),
        ("mixer_crossfader_height", t.mixer_crossfader_height),
        ("mixer_cue_height", t.mixer_cue_height),
        ("meter_floor_db", t.meter_floor_db),
        ("meter_mid_db", t.meter_mid_db),
        ("meter_high_db", t.meter_high_db),
        ("meter_clip_level", t.meter_clip_level),
        ("meter_hold_seconds", t.meter_hold_seconds),
        ("meter_clip_seconds", t.meter_clip_seconds),
        ("mixer_title_height", t.mixer_title_height),
        ("mixer_fader_width", t.mixer_fader_width),
        ("mixer_curve_width", t.mixer_curve_width),
        ("mixer_fx_box_width", t.mixer_fx_box_width),
        ("mixer_fx_enable_width", t.mixer_fx_enable_width),
        ("mixer_page_button_width", t.mixer_page_button_width),
        ("mixer_pad_x", t.mixer_pad_x),
        ("mixer_pad_y", t.mixer_pad_y),
        ("mixer_accent_dot", t.mixer_accent_dot),
        ("meter_segment", t.meter_segment),
        ("meter_segment_gap", t.meter_segment_gap),
        ("meter_hold_tick_height", t.meter_hold_tick_height),
        ("library_row_height", t.library_row_height),
        ("library_header_height", t.library_header_height),
        ("library_sidebar_width", t.library_sidebar_width),
        ("library_toolbar_height", t.library_toolbar_height),
        ("library_button_width", t.library_button_width),
        ("library_sidebar_row_height", t.library_sidebar_row_height),
        ("library_rating_dot", t.library_rating_dot),
        ("library_search_min_width", t.library_search_min_width),
        ("library_compat_alpha", t.library_compat_alpha),
        ("library_stripe_alpha", t.library_stripe_alpha),
        ("library_drop_alpha", t.library_drop_alpha),
        ("settings_panel_width", t.settings_panel_width),
        ("settings_panel_height", t.settings_panel_height),
        ("settings_tab_height", t.settings_tab_height),
        ("settings_row_height", t.settings_row_height),
        ("settings_label_width", t.settings_label_width),
        ("settings_button_width", t.settings_button_width),
        ("settings_dim_alpha", t.settings_dim_alpha),
        ("border_width", t.border_width),
        ("focus_border_width", t.focus_border_width),
        ("thumb_size", t.thumb_size),
        ("inset_small", t.inset_small),
        ("inset_large", t.inset_large),
        ("bar_radius", t.bar_radius),
        ("lamp_height", t.lamp_height),
        ("meter_headroom", t.meter_headroom),
        ("disabled_alpha", t.disabled_alpha),
        ("edge_line_alpha", t.edge_line_alpha),
        ("edge_highlight_alpha", t.edge_highlight_alpha),
        ("wave_saturation_max", t.wave_saturation_max),
        ("wave_lightness_min", t.wave_lightness_min),
        ("wave_lightness_max", t.wave_lightness_max),
        ("wave_floor_db", t.wave_floor_db),
        ("wave_spread_narrow", t.wave_spread_narrow),
        ("wave_spread_wide", t.wave_spread_wide),
        ("wave_spread_saturation", t.wave_spread_saturation),
        ("shadow_blur", t.shadow_blur),
        ("shadow_offset_y", t.shadow_offset_y),
        ("motion_fast_ms", t.motion_fast_ms),
        ("motion_base_ms", t.motion_base_ms),
        ("surface_opacity", t.surface_opacity),
        ("backdrop_opacity", t.backdrop_opacity),
    ]
}

fn density_fields(m: &DensityMetrics) -> [(&'static str, f32); 7] {
    [
        ("control_height", m.control_height),
        ("knob_size", m.knob_size),
        ("pad_size", m.pad_size),
        ("row_height", m.row_height),
        ("mixer_width", m.mixer_width),
        ("panel_radius", m.panel_radius),
        ("control_radius", m.control_radius),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use om_ui::theme::ThemeRegistry;

    fn dark() -> ThemeTokens {
        let mut registry = ThemeRegistry::new();
        assert!(registry.load_builtins() > 0);
        assert!(registry.select("openmixer.dark"));
        registry.current().clone()
    }

    fn css() -> String {
        theme_css(&dark(), Density::Roomy)
    }

    #[test]
    fn dark_theme_colours() {
        let css = css();
        assert!(css.starts_with(":root {"));
        assert!(css.contains("--om-ground: #121211;"));
        assert!(css.contains("--om-playhead: #ef5a3f;"));
        assert!(css.contains("--om-accent-0: #e3a24f;"));
        assert!(css.contains("--om-accent-1: #86a9cb;"));
    }

    #[test]
    fn translucent_colour_keeps_alpha() {
        assert!(css().contains("--om-shadow: rgb(0 0 0 / 0.5"));
    }

    #[test]
    fn lengths_and_numbers() {
        let css = css();
        assert!(css.contains("--om-gap: 13px;"));
        assert!(css.contains("--om-type-numeral-large: 21px;"));
        assert!(css.contains("--om-mixer-meter-width: 12px;"));
        assert!(css.contains("--om-grid-bar-alpha: 0.14;"));
        assert!(css.contains("--om-meter-floor-db: -48;"));
        assert!(css.contains("--om-motion-fast-ms: 90ms;"));
    }

    #[test]
    fn density_picks_the_metric_set() {
        let tokens = dark();
        let roomy = theme_css(&tokens, Density::Roomy);
        let compact = theme_css(&tokens, Density::Compact);
        assert!(roomy.contains("--om-density-space-4: 13px;"));
        assert!(roomy.contains("--om-density-knob-size: 34px;"));
        assert!(compact.contains("--om-density-space-4: 8px;"));
        assert!(compact.contains("--om-density-knob-size: 26px;"));
    }

    #[test]
    fn output_is_sorted_and_stable() {
        let css = css();
        let names: Vec<&str> = css
            .lines()
            .filter_map(|l| l.trim().split(':').next())
            .filter(|n| n.starts_with("--om-"))
            .collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted);
        assert_eq!(css, theme_css(&dark(), Density::Roomy));
    }

    /// Tokens that are neither a colour nor a number, so they have no CSS variable yet.
    const SKIPPED: [&str; 6] = [
        "wave_stops",
        "font_display",
        "font_body",
        "font_mono",
        "tabular_numerals",
        "reduced_motion",
    ];

    #[test]
    fn every_numeric_and_colour_field_is_emitted() {
        let tokens = dark();
        let css = theme_css(&tokens, Density::Roomy);
        for field in debug_field_names(&format!("{tokens:?}")) {
            let var = match field.as_str() {
                f if SKIPPED.contains(&f) => continue,
                "accents" => "accent-0".to_owned(),
                "roomy" | "compact" => continue,
                f => f.replace('_', "-"),
            };
            assert!(
                css.contains(&format!("--om-{var}:")),
                "{field} is not emitted"
            );
        }
        for field in debug_field_names(&format!("{:?}", tokens.roomy)) {
            let var = if field == "space" {
                "space-1".to_owned()
            } else {
                field.replace('_', "-")
            };
            assert!(
                css.contains(&format!("--om-density-{var}:")),
                "{field} is not emitted"
            );
        }
    }

    /// Top-level field names of a `Debug`-printed struct; Rust has no reflection to list them.
    fn debug_field_names(debug: &str) -> Vec<String> {
        let body = &debug[debug.find('{').unwrap() + 1..debug.rfind('}').unwrap()];
        let mut names = Vec::new();
        let (mut depth, mut start) = (0, 0);
        for (i, c) in body.char_indices() {
            match c {
                '{' | '[' | '(' => depth += 1,
                '}' | ']' | ')' => depth -= 1,
                ',' if depth == 0 => {
                    names.push(field_name(&body[start..i]));
                    start = i + 1;
                }
                _ => {}
            }
        }
        names.push(field_name(&body[start..]));
        names
    }

    fn field_name(segment: &str) -> String {
        segment.split(':').next().unwrap().trim().to_owned()
    }
}

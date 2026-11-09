//! Theme file parsing: JSON to [`ThemeTokens`] with validation.
//! Pure; no global state. Missing keys inherit the built-in dark theme; a malformed value rejects
//! the whole theme; unknown keys are reported as warnings and ignored.

use super::density::{self, DensityMetrics};
use super::tokens::{Colour, ThemeTokens, WaveStop};
use serde_json::{Map, Value};
use std::path::Path;

const MAX_ACCENTS: usize = 8;
const MAX_SPACING: i64 = 64;
const MIN_SCALE_TYPE: f32 = 10.0;
const MAX_TYPE: f32 = 64.0;
const MAX_RADIUS: f32 = 48.0;
const MAX_PX: f32 = 400.0;
const MAX_WINDOW_PX: f32 = 4000.0;
const HEX_HINT: &str = "expected \"#RRGGBB\" or \"#AARRGGBB\"";

/// Identity of a theme: what the registry and the picker show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeInfo {
    pub id: String,
    pub name: String,
    pub author: String,
}

/// A successfully parsed theme. `warnings` lists ignored unknown keys.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedTheme {
    pub info: ThemeInfo,
    pub tokens: ThemeTokens,
    pub warnings: Vec<String>,
}

/// Why a theme was rejected. Messages read `where: why`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ThemeError {
    #[error("invalid JSON: {0}")]
    Json(String),
    #[error("theme must be a JSON object")]
    NotObject,
    #[error("{field}: {why}")]
    Field { field: String, why: String },
    #[error("not a file: {0}")]
    NotAFile(String),
    #[error("cannot read {path}: {why}")]
    Read { path: String, why: String },
}

fn field(field: impl Into<String>, why: impl Into<String>) -> ThemeError {
    ThemeError::Field {
        field: field.into(),
        why: why.into(),
    }
}

/// Parses `#RRGGBB` or `#AARRGGBB` (case-insensitive). Anything else is `None`.
pub fn parse_colour_hex(text: &str) -> Option<Colour> {
    let digits = text.strip_prefix('#')?;
    if (digits.len() != 6 && digits.len() != 8) || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let value = u32::from_str_radix(digits, 16).ok()?;
    Some(Colour::from_argb(if digits.len() == 6 {
        0xFF00_0000 | value
    } else {
        value
    }))
}

/// Float keys of one section, with the accepted range. Generates the key list, the setter and the range.
macro_rules! float_keys {
    ($( $key:literal => $field:ident : $lo:expr, $hi:expr ; )*) => {
        pub(super) const KEYS: &[&str] = &[$($key),*];

        /// Stores `value` if `key` is in this section; `false` for unknown keys.
        pub(super) fn set(t: &mut ThemeTokens, key: &str, value: f32) -> bool {
            match key {
                $( $key => { t.$field = value; true } )*
                _ => false,
            }
        }

        pub(super) fn range(key: &str) -> (f32, f32) {
            match key {
                $( $key => ($lo, $hi), )*
                _ => (0.0, 0.0),
            }
        }
    };
}

mod radii_keys {
    use super::*;
    float_keys! {
        "card" => card_radius: 0.0, MAX_RADIUS;
        "control" => control_radius: 0.0, MAX_RADIUS;
    }
}

// The floors are DESIGN.md's rules: display family 21, prose 12, words 11, the dB scale 10.
mod type_keys {
    use super::*;
    use crate::theme::{MIN_DISPLAY_SIZE, MIN_LABEL_SIZE, MIN_PROSE_SIZE};
    float_keys! {
        "display" => type_display: MIN_DISPLAY_SIZE, MAX_TYPE;
        "headline" => type_headline: MIN_DISPLAY_SIZE, MAX_TYPE;
        "body" => type_body: MIN_PROSE_SIZE, MAX_TYPE;
        "label" => type_label: MIN_LABEL_SIZE, MAX_TYPE;
        "numeral" => type_numeral: MIN_LABEL_SIZE, MAX_TYPE;
        "numeralLarge" => type_numeral_large: MIN_LABEL_SIZE, MAX_TYPE;
        "scale" => type_scale: MIN_SCALE_TYPE, MAX_TYPE;
    }
}

mod translucency_keys {
    use super::*;
    float_keys! {
        "surface" => surface_opacity: 0.0, 1.0;
        "backdrop" => backdrop_opacity: 0.0, 1.0;
    }
}

mod deck_keys {
    use super::*;
    float_keys! {
        "headerHeight" => deck_header_height: 0.0, MAX_PX;
        "overviewHeight" => deck_overview_height: 0.0, MAX_PX;
        "transportHeight" => deck_transport_height: 0.0, MAX_PX;
        "rowHeight" => deck_row_height: 0.0, MAX_PX;
        "pitchWidth" => deck_pitch_width: 0.0, MAX_PX;
        "minZoomHeight" => deck_min_zoom_height: 0.0, MAX_PX;
        "readoutWidth" => deck_readout_width: 0.0, MAX_PX;
        "rangeBoxHeight" => deck_range_box_height: 0.0, MAX_PX;
        "playheadWidth" => playhead_width: 0.0, MAX_RADIUS;
        "cueMarkerWidth" => cue_marker_width: 0.0, MAX_RADIUS;
        "cueGuideAlpha" => cue_guide_alpha: 0.0, 1.0;
        "gridBarAlpha" => grid_bar_alpha: 0.0, 1.0;
        "gridBeatAlpha" => grid_beat_alpha: 0.0, 1.0;
        "loopActiveAlpha" => loop_active_alpha: 0.0, 1.0;
        "loopIdleAlpha" => loop_idle_alpha: 0.0, 1.0;
        "onTintAlpha" => deck_tint_alpha: 0.0, 1.0;
        "onBorderAlpha" => deck_border_alpha: 0.0, 1.0;
        "onHoverTintAlpha" => deck_tint_hover_alpha: 0.0, 1.0;
        "onHoverBorderAlpha" => deck_border_hover_alpha: 0.0, 1.0;
    }
}

mod mixer_keys {
    use super::*;
    float_keys! {
        "knobHeight" => mixer_knob_height: 0.0, MAX_PX;
        "buttonHeight" => mixer_button_height: 0.0, MAX_PX;
        "meterWidth" => mixer_meter_width: 0.0, MAX_PX;
        "masterWidth" => mixer_master_width: 0.0, MAX_PX;
        "tabHeight" => mixer_tab_height: 0.0, MAX_PX;
        "crossfaderHeight" => mixer_crossfader_height: 0.0, MAX_PX;
        "cueHeight" => mixer_cue_height: 0.0, MAX_PX;
        "meterFloorDb" => meter_floor_db: -96.0, -6.0;
        "meterMidDb" => meter_mid_db: -96.0, 0.0;
        "meterHighDb" => meter_high_db: -96.0, 0.0;
        "meterClipLevel" => meter_clip_level: 0.1, 1.0;
        "meterHoldSeconds" => meter_hold_seconds: 0.0, 10.0;
        "meterClipSeconds" => meter_clip_seconds: 0.0, 30.0;
        "titleHeight" => mixer_title_height: 0.0, MAX_PX;
        "faderWidth" => mixer_fader_width: 0.0, MAX_PX;
        "curveWidth" => mixer_curve_width: 0.0, MAX_PX;
        "fxBoxWidth" => mixer_fx_box_width: 0.0, MAX_PX;
        "fxEnableWidth" => mixer_fx_enable_width: 0.0, MAX_PX;
        "pageButtonWidth" => mixer_page_button_width: 0.0, MAX_PX;
        "padX" => mixer_pad_x: 0.0, MAX_PX;
        "padY" => mixer_pad_y: 0.0, MAX_PX;
        "accentDot" => mixer_accent_dot: 0.0, MAX_PX;
        "meterHoldTickHeight" => meter_hold_tick_height: 0.0, 20.0;
        "meterSegment" => meter_segment: 1.0, 20.0;
        "meterSegmentGap" => meter_segment_gap: 0.0, 10.0;
    }
}

mod library_keys {
    use super::*;
    float_keys! {
        "rowHeight" => library_row_height: 0.0, MAX_PX;
        "headerHeight" => library_header_height: 0.0, MAX_PX;
        "sidebarWidth" => library_sidebar_width: 0.0, MAX_PX;
        "toolbarHeight" => library_toolbar_height: 0.0, MAX_PX;
        "buttonWidth" => library_button_width: 0.0, MAX_PX;
        "sidebarRowHeight" => library_sidebar_row_height: 0.0, MAX_PX;
        "ratingDot" => library_rating_dot: 0.0, 20.0;
        "searchMinWidth" => library_search_min_width: 0.0, MAX_PX;
        "compatAlpha" => library_compat_alpha: 0.0, 1.0;
        "stripeAlpha" => library_stripe_alpha: 0.0, 1.0;
        "dropAlpha" => library_drop_alpha: 0.0, 1.0;
    }
}

mod style_keys {
    use super::*;
    float_keys! {
        "borderWidth" => border_width: 0.0, 8.0;
        "focusBorderWidth" => focus_border_width: 0.0, 8.0;
        "thumbSize" => thumb_size: 2.0, MAX_PX;
        "insetSmall" => inset_small: 0.0, 16.0;
        "insetLarge" => inset_large: 0.0, 32.0;
        "barRadius" => bar_radius: 0.0, 16.0;
        "lampHeight" => lamp_height: 0.0, 32.0;
        "meterHeadroom" => meter_headroom: 0.0, 64.0;
        "disabledAlpha" => disabled_alpha: 0.0, 1.0;
        "minTarget" => min_target: 0.0, MAX_PX;
        "minPointerTarget" => min_pointer_target: 0.0, MAX_PX;
        "shadowBlur" => shadow_blur: 0.0, 64.0;
        "shadowOffsetY" => shadow_offset_y: 0.0, 32.0;
        "motionFastMs" => motion_fast_ms: 0.0, 1000.0;
        "motionBaseMs" => motion_base_ms: 0.0, 2000.0;
    }
}

mod edge_keys {
    use super::*;
    float_keys! {
        "line" => edge_line_alpha: 0.0, 1.0;
        "highlight" => edge_highlight_alpha: 0.0, 1.0;
    }
}

mod settings_keys {
    use super::*;
    float_keys! {
        "panelWidth" => settings_panel_width: 0.0, MAX_WINDOW_PX;
        "panelHeight" => settings_panel_height: 0.0, MAX_WINDOW_PX;
        "tabHeight" => settings_tab_height: 0.0, MAX_PX;
        "rowHeight" => settings_row_height: 0.0, MAX_PX;
        "labelWidth" => settings_label_width: 0.0, MAX_PX;
        "buttonWidth" => settings_button_width: 0.0, MAX_PX;
        "dimAlpha" => settings_dim_alpha: 0.0, 1.0;
    }
}

mod wave_keys {
    use super::*;
    float_keys! {
        "saturationMax" => wave_saturation_max: 0.0, 1.0;
        "lightnessMin" => wave_lightness_min: 0.0, 1.0;
        "lightnessMax" => wave_lightness_max: 0.0, 1.0;
        "floorDb" => wave_floor_db: -90.0, -6.0;
        "spreadNarrow" => wave_spread_narrow: 0.0, 4.0;
        "spreadWide" => wave_spread_wide: 0.0, 4.0;
        "spreadSaturation" => wave_spread_saturation: 0.0, 1.0;
    }
}

/// Colour keys of the `colours` section.
macro_rules! colour_keys {
    ($( $key:literal => $field:ident ; )*) => {
        const COLOUR_KEYS: &[&str] = &[$($key),*];

        fn set_colour(t: &mut ThemeTokens, key: &str, value: Colour) -> bool {
            match key {
                $( $key => { t.$field = value; true } )*
                _ => false,
            }
        }
    };
}

colour_keys! {
    "ground" => ground;
    "surface" => surface;
    "surfaceRaised" => surface_raised;
    "surfaceHover" => surface_hover;
    "well" => well;
    "border" => border;
    "borderStrong" => border_strong;
    "focusRing" => focus_ring;
    "highlight" => highlight;
    "shadow" => shadow;
    "textPrimary" => text_primary;
    "textSecondary" => text_secondary;
    "textMuted" => text_muted;
    "onAccent" => on_accent;
    "meterLow" => meter_low;
    "meterMid" => meter_mid;
    "meterHigh" => meter_high;
    "playhead" => playhead;
    "cue" => cue;
    "loopMark" => loop_mark;
    "syncOn" => sync_on;
    "warning" => warning;
    "danger" => danger;
    "master" => master;
    "popup" => popup;
    "dropTarget" => drop_target;
    "brand" => brand;
    "readout" => readout;
    "readoutDim" => readout_dim;
}

/// Colour keys older theme files carry under another name: (old, new). The old value is used only
/// when the file does not set the new key.
const RENAMED_COLOURS: [(&str, &str); 2] = [("backdropTop", "ground"), ("recordRed", "playhead")];

/// Glass and glow colours the faceplate themes no longer have.
const RETIRED_COLOURS: [&str; 5] = [
    "backdropBottom",
    "glowCool",
    "glowWarm",
    "capTop",
    "capBottom",
];

const SPACING_KEYS: [&str; 3] = ["gap", "pad", "innerGap"];
const DENSITY_KEYS: [&str; 2] = ["roomy", "compact"];
const ROOT_KEYS: [&str; 20] = [
    "id",
    "name",
    "author",
    "colours",
    "accents",
    "spacing",
    "radii",
    "type",
    "translucency",
    "deck",
    "mixer",
    "library",
    "settings",
    "style",
    "waveform",
    "font",
    "accessibility",
    "edge",
    "density",
    "notes",
];

/// Parses a theme document. On error no tokens are returned, so a caller can never use a half-filled
/// set; the caller keeps its current theme.
pub fn parse_theme(json: &str) -> Result<ParsedTheme, ThemeError> {
    let root: Value = serde_json::from_str(json).map_err(|e| ThemeError::Json(e.to_string()))?;
    let Value::Object(root) = root else {
        return Err(ThemeError::NotObject);
    };
    let mut out = ParsedTheme {
        info: read_identity(&root)?,
        tokens: ThemeTokens::builtin_dark(),
        warnings: Vec::new(),
    };
    let w = &mut out.warnings;
    let t = &mut out.tokens;
    if let Some(s) = section(&root, "colours")? {
        read_colours(s, t, w)?;
    }
    if let Some(s) = section(&root, "spacing")? {
        read_spacing(s, t, w)?;
    }
    read_floats(
        section(&root, "radii")?,
        "radii",
        w,
        t,
        radii_keys::KEYS,
        radii_keys::set,
        radii_keys::range,
    )?;
    read_floats(
        section(&root, "type")?,
        "type",
        w,
        t,
        type_keys::KEYS,
        type_keys::set,
        type_keys::range,
    )?;
    read_floats(
        section(&root, "translucency")?,
        "translucency",
        w,
        t,
        translucency_keys::KEYS,
        translucency_keys::set,
        translucency_keys::range,
    )?;
    read_floats(
        section(&root, "deck")?,
        "deck",
        w,
        t,
        deck_keys::KEYS,
        deck_keys::set,
        deck_keys::range,
    )?;
    read_floats(
        section(&root, "mixer")?,
        "mixer",
        w,
        t,
        mixer_keys::KEYS,
        mixer_keys::set,
        mixer_keys::range,
    )?;
    read_floats(
        section(&root, "library")?,
        "library",
        w,
        t,
        library_keys::KEYS,
        library_keys::set,
        library_keys::range,
    )?;
    read_floats(
        section(&root, "settings")?,
        "settings",
        w,
        t,
        settings_keys::KEYS,
        settings_keys::set,
        settings_keys::range,
    )?;
    read_floats(
        section(&root, "style")?,
        "style",
        w,
        t,
        style_keys::KEYS,
        style_keys::set,
        style_keys::range,
    )?;
    read_floats(
        section(&root, "edge")?,
        "edge",
        w,
        t,
        edge_keys::KEYS,
        edge_keys::set,
        edge_keys::range,
    )?;
    read_density(section(&root, "density")?, w, t)?;
    read_waveform(section(&root, "waveform")?, w, t)?;
    read_font(section(&root, "font")?, w, t)?;
    read_accessibility(section(&root, "accessibility")?, w, t)?;
    if let Some(accents) = root.get("accents") {
        t.accents = read_accents(accents)?;
    }
    if root.get("notes").is_some_and(|v| !v.is_string()) {
        return Err(field("notes", "must be a string"));
    }
    warn_unknown(&root, &ROOT_KEYS, "", w);
    Ok(out)
}

/// Reads and parses a theme file. A missing or unreadable file is an error for that file only.
pub fn parse_theme_file(path: &Path) -> Result<ParsedTheme, ThemeError> {
    if !path.is_file() {
        return Err(ThemeError::NotAFile(path.display().to_string()));
    }
    let text = std::fs::read_to_string(path).map_err(|e| ThemeError::Read {
        path: path.display().to_string(),
        why: e.to_string(),
    })?;
    parse_theme(&text)
}

const MAX_FONT_FAMILY_LEN: usize = 64;

const FONT_KEYS: [&str; 5] = ["display", "body", "mono", "family", "tabularNumerals"];

/// `font`: `display`, `body` and `mono` (family names), and `tabularNumerals` (bool). The older
/// single `family` is read as `body` when `body` is absent.
fn read_font(
    section: Option<&Map<String, Value>>,
    warnings: &mut Vec<String>,
    t: &mut ThemeTokens,
) -> Result<(), ThemeError> {
    let Some(section) = section else {
        return Ok(());
    };
    for key in ["display", "body", "mono"] {
        if let Some(value) = section.get(key) {
            *family_slot(t, key) = family_name(key, value)?;
        }
    }
    if let Some(value) = section.get("family") {
        let name = family_name("family", value)?;
        if !section.contains_key("body") {
            t.font_body = name;
        }
        warnings.push("renamed key 'font.family' read as 'font.body'".to_owned());
    }
    if let Some(value) = section.get("tabularNumerals") {
        t.tabular_numerals = value
            .as_bool()
            .ok_or_else(|| field("font.tabularNumerals", "expected true or false"))?;
    }
    warn_unknown(section, &FONT_KEYS, "font.", warnings);
    Ok(())
}

fn family_slot<'a>(t: &'a mut ThemeTokens, key: &str) -> &'a mut String {
    match key {
        "display" => &mut t.font_display,
        "mono" => &mut t.font_mono,
        _ => &mut t.font_body,
    }
}

fn family_name(key: &str, value: &Value) -> Result<String, ThemeError> {
    match value.as_str().map(str::trim) {
        Some(name) if !name.is_empty() && name.len() <= MAX_FONT_FAMILY_LEN => Ok(name.to_owned()),
        _ => Err(field(
            format!("font.{key}"),
            "expected a non-empty font family name",
        )),
    }
}

/// `accessibility`: `reducedMotion` (bool).
fn read_accessibility(
    section: Option<&Map<String, Value>>,
    warnings: &mut Vec<String>,
    t: &mut ThemeTokens,
) -> Result<(), ThemeError> {
    let Some(section) = section else {
        return Ok(());
    };
    if let Some(value) = section.get("reducedMotion") {
        t.reduced_motion = value
            .as_bool()
            .ok_or_else(|| field("accessibility.reducedMotion", "expected true or false"))?;
    }
    warn_unknown(section, &["reducedMotion"], "accessibility.", warnings);
    Ok(())
}

const WAVE_STOP_KEYS: [&str; 4] = ["pos", "hue", "saturation", "lightness"];
const WAVE_STOPS_KEY: &str = "hueStops";
const MAX_WAVE_STOPS: usize = 16;

fn read_waveform(
    section: Option<&Map<String, Value>>,
    warnings: &mut Vec<String>,
    t: &mut ThemeTokens,
) -> Result<(), ThemeError> {
    let Some(section) = section else {
        return Ok(());
    };
    let floats: Map<String, Value> = section
        .iter()
        .filter(|(k, _)| k.as_str() != WAVE_STOPS_KEY)
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    read_floats(
        Some(&floats),
        "waveform",
        warnings,
        t,
        wave_keys::KEYS,
        wave_keys::set,
        wave_keys::range,
    )?;
    if let Some(stops) = section.get(WAVE_STOPS_KEY) {
        t.wave_stops = read_wave_stops(stops)?;
    }
    check_wave_limits(t)
}

fn check_wave_limits(t: &ThemeTokens) -> Result<(), ThemeError> {
    if t.wave_lightness_min >= t.wave_lightness_max {
        return Err(field("waveform.lightnessMin", "must be below lightnessMax"));
    }
    if t.wave_spread_narrow >= t.wave_spread_wide {
        return Err(field("waveform.spreadNarrow", "must be below spreadWide"));
    }
    Ok(())
}

fn read_wave_stops(value: &Value) -> Result<Vec<WaveStop>, ThemeError> {
    let bad = |why: String| field(format!("waveform.{WAVE_STOPS_KEY}"), why);
    let items = match value {
        Value::Array(items) if (2..=MAX_WAVE_STOPS).contains(&items.len()) => items,
        _ => return Err(bad(format!("expected 2-{MAX_WAVE_STOPS} stops"))),
    };
    let mut stops: Vec<WaveStop> = Vec::with_capacity(items.len());
    for item in items {
        let get = |key: &str, lo: f64, hi: f64| {
            item.get(key)
                .and_then(Value::as_f64)
                .filter(|v| *v >= lo && *v <= hi)
                .map(|v| v as f32)
                .ok_or_else(|| bad(format!("each stop needs {key} in {lo}-{hi}")))
        };
        let stop = WaveStop {
            pos: get(WAVE_STOP_KEYS[0], 0.0, 1.0)?,
            hue: get(WAVE_STOP_KEYS[1], 0.0, 720.0)?,
            saturation: get(WAVE_STOP_KEYS[2], 0.0, 1.5)?,
            lightness: get(WAVE_STOP_KEYS[3], 0.0, 2.0)?,
        };
        if stops.last().is_some_and(|prev| prev.pos >= stop.pos) {
            return Err(bad("pos must rise from stop to stop".to_owned()));
        }
        stops.push(stop);
    }
    Ok(stops)
}

fn read_identity(root: &Map<String, Value>) -> Result<ThemeInfo, ThemeError> {
    let id = root.get("id").and_then(Value::as_str).unwrap_or("");
    if !is_theme_id(id) {
        return Err(field(
            "id",
            "required, lowercase letters, digits, '.', '_' or '-'",
        ));
    }
    let name = match root.get("name") {
        None => id.to_owned(),
        Some(Value::String(s)) => s.trim().to_owned(),
        Some(_) => return Err(field("name", "must be a string")),
    };
    if name.is_empty() {
        return Err(field("name", "must not be empty"));
    }
    let author = root
        .get("author")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    Ok(ThemeInfo {
        id: id.to_owned(),
        name,
        author,
    })
}

fn is_theme_id(id: &str) -> bool {
    !id.is_empty()
        && id.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-')
        })
}

/// An optional object section. Present but not an object is an error.
fn section<'a>(
    root: &'a Map<String, Value>,
    name: &str,
) -> Result<Option<&'a Map<String, Value>>, ThemeError> {
    match root.get(name) {
        None => Ok(None),
        Some(Value::Object(obj)) => Ok(Some(obj)),
        Some(_) => Err(field(name, "must be an object")),
    }
}

/// An optional object inside `parent` (named `parent_name` in errors).
fn section_in<'a>(
    parent: &'a Map<String, Value>,
    parent_name: &str,
    name: &str,
) -> Result<Option<&'a Map<String, Value>>, ThemeError> {
    match parent.get(name) {
        None => Ok(None),
        Some(Value::Object(obj)) => Ok(Some(obj)),
        Some(_) => Err(field(format!("{parent_name}.{name}"), "must be an object")),
    }
}

/// Reports keys the schema does not know (typos) without rejecting the theme.
fn warn_unknown(
    obj: &Map<String, Value>,
    known: &[&str],
    prefix: &str,
    warnings: &mut Vec<String>,
) {
    for key in obj.keys() {
        if !known.contains(&key.as_str()) {
            warnings.push(format!("unknown key '{prefix}{key}' ignored"));
        }
    }
}

fn read_colours(
    section: &Map<String, Value>,
    t: &mut ThemeTokens,
    warnings: &mut Vec<String>,
) -> Result<(), ThemeError> {
    let colour = |key: &str, value: &Value| {
        value
            .as_str()
            .and_then(parse_colour_hex)
            .ok_or_else(|| field(format!("colours.{key}"), HEX_HINT))
    };
    for (key, value) in section {
        if COLOUR_KEYS.contains(&key.as_str()) {
            set_colour(t, key, colour(key, value)?);
        }
    }
    for (old, new) in RENAMED_COLOURS {
        let Some(value) = section.get(old) else {
            continue;
        };
        if !section.contains_key(new) {
            set_colour(t, new, colour(old, value)?);
        }
        warnings.push(format!(
            "renamed key 'colours.{old}' read as 'colours.{new}'"
        ));
    }
    for old in RETIRED_COLOURS
        .into_iter()
        .filter(|k| section.contains_key(*k))
    {
        warnings.push(format!("retired key 'colours.{old}' ignored"));
    }
    let known: Vec<&str> = COLOUR_KEYS
        .iter()
        .copied()
        .chain(RENAMED_COLOURS.map(|(old, _)| old))
        .chain(RETIRED_COLOURS)
        .collect();
    warn_unknown(section, &known, "colours.", warnings);
    Ok(())
}

/// `density`: a `roomy` and a `compact` metric set; a key left out keeps the built-in value.
fn read_density(
    section: Option<&Map<String, Value>>,
    warnings: &mut Vec<String>,
    t: &mut ThemeTokens,
) -> Result<(), ThemeError> {
    let Some(section) = section else {
        return Ok(());
    };
    if let Some(set) = section_in(section, "density", "roomy")? {
        read_density_set(set, "roomy", &mut t.roomy, warnings)?;
    }
    if let Some(set) = section_in(section, "density", "compact")? {
        read_density_set(set, "compact", &mut t.compact, warnings)?;
    }
    warn_unknown(section, &DENSITY_KEYS, "density.", warnings);
    Ok(())
}

fn read_density_set(
    set: &Map<String, Value>,
    name: &str,
    metrics: &mut DensityMetrics,
    warnings: &mut Vec<String>,
) -> Result<(), ThemeError> {
    for key in density::KEYS {
        let Some(value) = set.get(*key) else {
            continue;
        };
        let (lo, hi) = density::range(key);
        let number = value
            .as_f64()
            .map(|v| v as f32)
            .filter(|v| (lo..=hi).contains(v));
        let Some(v) = number else {
            return Err(field(
                format!("density.{name}.{key}"),
                format!("expected a number {lo}-{hi}"),
            ));
        };
        metrics.set(key, v);
    }
    if !metrics.steps_rise() {
        return Err(field(
            format!("density.{name}"),
            "spacing steps s1..s6 must rise",
        ));
    }
    warn_unknown(set, density::KEYS, &format!("density.{name}."), warnings);
    Ok(())
}

fn read_accents(value: &Value) -> Result<Vec<Colour>, ThemeError> {
    let items = match value {
        Value::Array(items) if !items.is_empty() && items.len() <= MAX_ACCENTS => items,
        _ => {
            return Err(field(
                "accents",
                format!("expected 1-{MAX_ACCENTS} colours"),
            ))
        }
    };
    items
        .iter()
        .map(|item| {
            item.as_str()
                .and_then(parse_colour_hex)
                .ok_or_else(|| field("accents", HEX_HINT))
        })
        .collect()
}

fn read_spacing(
    section: &Map<String, Value>,
    t: &mut ThemeTokens,
    warnings: &mut Vec<String>,
) -> Result<(), ThemeError> {
    for key in SPACING_KEYS {
        let Some(value) = section.get(key) else {
            continue;
        };
        let whole = value.as_i64().filter(|v| (0..=MAX_SPACING).contains(v));
        let Some(v) = whole else {
            return Err(field(
                format!("spacing.{key}"),
                format!("expected an integer 0-{MAX_SPACING}"),
            ));
        };
        // Range-checked above, so the narrowing is lossless.
        let v = v as i32;
        match key {
            "gap" => t.gap = v,
            "pad" => t.pad = v,
            _ => t.inner_gap = v,
        }
    }
    warn_unknown(section, &SPACING_KEYS, "spacing.", warnings);
    Ok(())
}

/// Reads one float section: every present key must be a number inside its range.
fn read_floats(
    section: Option<&Map<String, Value>>,
    name: &str,
    warnings: &mut Vec<String>,
    t: &mut ThemeTokens,
    keys: &[&str],
    set: fn(&mut ThemeTokens, &str, f32) -> bool,
    range: fn(&str) -> (f32, f32),
) -> Result<(), ThemeError> {
    let Some(section) = section else {
        return Ok(());
    };
    for key in keys {
        let Some(value) = section.get(*key) else {
            continue;
        };
        let (lo, hi) = range(key);
        let number = value
            .as_f64()
            .map(|v| v as f32)
            .filter(|v| *v >= lo && *v <= hi);
        match number {
            Some(v) => {
                set(t, key, v);
            }
            None => {
                return Err(field(
                    format!("{name}.{key}"),
                    format!("expected a number {lo}-{hi}"),
                ))
            }
        }
    }
    let in_section = |list: &'static [(&'static str, &'static str)]| {
        list.iter().filter(|(s, _)| *s == name).map(|(_, k)| *k)
    };
    for key in in_section(&WARNED_RETIRED_KEYS).filter(|k| section.contains_key(*k)) {
        warnings.push(format!("retired key '{name}.{key}' ignored"));
    }
    let known: Vec<&str> = keys
        .iter()
        .copied()
        .chain(in_section(&RETIRED_KEYS))
        .chain(in_section(&WARNED_RETIRED_KEYS))
        .collect();
    warn_unknown(section, &known, &format!("{name}."), warnings);
    Ok(())
}

/// Keys that older theme files carry and no token reads any more: (section, key). They are
/// accepted without a warning so those files keep loading quietly.
const RETIRED_KEYS: [(&str, &str); 1] = [("mixer", "meterReleaseSeconds")];

/// Retired glow keys and the E1 type scale: accepted so older theme files still load, with a
/// warning that they do nothing (the faceplate tiers take their defaults).
const WARNED_RETIRED_KEYS: [(&str, &str); 9] = [
    ("style", "glowAlpha"),
    ("style", "glowRadius"),
    ("type", "small"),
    ("type", "caption"),
    ("type", "title"),
    ("type", "heading"),
    ("type", "mono"),
    ("type", "readout"),
    ("type", "readoutSmall"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_hex_is_strict() {
        assert_eq!(
            parse_colour_hex("#aAbBcC"),
            Some(Colour::from_argb(0xFFAA_BBCC))
        );
        assert_eq!(
            parse_colour_hex("#80112233"),
            Some(Colour::from_argb(0x8011_2233))
        );
        assert_eq!(parse_colour_hex("112233"), None);
        assert_eq!(parse_colour_hex("#1122"), None);
        assert_eq!(parse_colour_hex(""), None);
        assert_eq!(parse_colour_hex("#GG0000"), None);
    }

    #[test]
    fn a_theme_with_the_retired_release_key_loads_without_a_warning() {
        let old =
            r#"{"id":"t.old","name":"T","mixer":{"meterReleaseSeconds":0.3,"meterMidDb":-12}}"#;
        let parsed = parse_theme(old).expect("parses");
        assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
        assert_eq!(parsed.tokens.meter_mid_db, -12.0);
        let typo = r#"{"id":"t.typo","name":"T","mixer":{"meterReleaseSecs":0.3}}"#;
        assert_eq!(parse_theme(typo).expect("parses").warnings.len(), 1);
    }

    #[test]
    fn brand_tokens_parse_and_are_range_checked() {
        let ok = r##"{"id":"t.one","name":"T","colours":{"brand":"#FF112233"},
            "type":{"numeralLarge":30},"edge":{"line":0.5,"highlight":0.1}}"##;
        let t = parse_theme(ok).expect("parses").tokens;
        assert_eq!(t.brand, Colour::from_argb(0xFF11_2233));
        assert_eq!(t.type_numeral_large, 30.0);
        assert_eq!((t.edge_line_alpha, t.edge_highlight_alpha), (0.5, 0.1));
        // A file that leaves them out inherits the defaults.
        let bare = parse_theme(r#"{"id":"t.two","name":"T"}"#)
            .expect("parses")
            .tokens;
        assert_eq!(bare.brand, ThemeTokens::builtin_dark().brand);
        let bad = r#"{"id":"t.three","name":"T","edge":{"highlight":2}}"#;
        assert!(parse_theme(bad).is_err());
    }

    #[test]
    fn e1_glass_and_glow_keys_still_load_with_a_warning_each() {
        let old = r##"{"id":"t.e1","colours":{"backdropTop":"#FF010203","backdropBottom":"#FF000000",
            "glowCool":"#0F00C2FF","glowWarm":"#0AFF6B35","capTop":"#FF2C3446","capBottom":"#FF161B26",
            "recordRed":"#FFFF3B4E"},"style":{"glowAlpha":2,"glowRadius":10}}"##;
        let parsed = parse_theme(old).expect("an E1 theme still loads");
        assert_eq!(parsed.warnings.len(), 9, "{:?}", parsed.warnings);
        assert!(parsed.warnings.iter().all(|w| !w.starts_with("unknown")));
        assert_eq!(parsed.tokens.ground, Colour::from_argb(0xFF01_0203));
        assert_eq!(parsed.tokens.record_red(), Colour::from_argb(0xFFFF_3B4E));
    }

    #[test]
    fn a_new_key_wins_over_its_old_name() {
        let both =
            r##"{"id":"t.both","colours":{"ground":"#FF0A0A0A","backdropTop":"#FF010203"}}"##;
        let parsed = parse_theme(both).expect("parses");
        assert_eq!(parsed.tokens.ground, Colour::from_argb(0xFF0A_0A0A));
        assert_eq!(parsed.warnings.len(), 1);
        let bad = r##"{"id":"t.bad","colours":{"backdropTop":"blue"}}"##;
        assert!(parse_theme(bad).is_err(), "an old key is still checked");
    }

    #[test]
    fn density_sets_parse_and_are_range_checked() {
        let ok = r#"{"id":"t.d","density":{"compact":{"rowHeight":24,"s6":20}}}"#;
        let t = parse_theme(ok).expect("parses").tokens;
        assert_eq!(t.compact.row_height, 24.0);
        assert_eq!(t.compact.space[5], 20.0);
        assert_eq!(
            t.roomy,
            DensityMetrics::roomy(),
            "an absent set keeps the default"
        );
        for bad in [
            r#"{"id":"t.d","density":{"roomy":{"rowHeight":-1}}}"#,
            r#"{"id":"t.d","density":{"roomy":{"s2":1}}}"#,
            r#"{"id":"t.d","density":{"roomy":3}}"#,
            r#"{"id":"t.d","notes":7}"#,
        ] {
            assert!(parse_theme(bad).is_err(), "{bad}");
        }
        let typo = r#"{"id":"t.d","density":{"rooomy":{},"compact":{"rowHieght":2}}}"#;
        assert_eq!(parse_theme(typo).expect("parses").warnings.len(), 2);
    }

    #[test]
    fn font_and_accessibility_sections_parse_and_validate() {
        let ok = r#"{"id":"t.f","font":{"display":"Plex Serif","body":"Plex Sans",
            "mono":"Plex Mono","tabularNumerals":false},"accessibility":{"reducedMotion":true}}"#;
        let parsed = parse_theme(ok).expect("parses");
        let t = parsed.tokens;
        assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
        assert_eq!(
            (
                t.font_display.as_str(),
                t.font_body.as_str(),
                t.font_mono.as_str()
            ),
            ("Plex Serif", "Plex Sans", "Plex Mono")
        );
        assert!(!t.tabular_numerals && t.reduced_motion);
        let bare = parse_theme(r#"{"id":"t.g"}"#).expect("parses").tokens;
        assert_eq!(
            (
                bare.font_display.as_str(),
                bare.font_body.as_str(),
                bare.font_mono.as_str()
            ),
            ("Sansation", "Source Sans 3", "Ubuntu Mono")
        );
        assert!(bare.tabular_numerals && !bare.reduced_motion);
        for bad in [
            r#"{"id":"t.h","font":{"family":""}}"#,
            r#"{"id":"t.h","font":{"mono":7}}"#,
            r#"{"id":"t.h","font":{"display":" "}}"#,
            r#"{"id":"t.i","accessibility":{"reducedMotion":1}}"#,
        ] {
            assert!(parse_theme(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_older_font_family_key_is_read_as_the_body_family() {
        let old = parse_theme(r#"{"id":"t.f","font":{"family":"Inter"}}"#).expect("parses");
        assert_eq!(old.tokens.font_body, "Inter");
        assert_eq!(
            old.tokens.font_mono, "Ubuntu Mono",
            "the other slots keep the default"
        );
        assert_eq!(old.warnings.len(), 1, "{:?}", old.warnings);
        let both = r#"{"id":"t.f","font":{"family":"Inter","body":"Plex Sans"}}"#;
        assert_eq!(
            parse_theme(both).expect("parses").tokens.font_body,
            "Plex Sans"
        );
    }

    #[test]
    fn the_e1_type_scale_still_loads_with_a_warning_per_retired_key() {
        let old = r#"{"id":"t.e1","type":{"small":12,"caption":12,"body":14,"label":16,"title":20,
            "heading":20,"mono":16,"readout":40,"readoutSmall":28}}"#;
        let parsed = parse_theme(old).expect("an E1 type section still loads");
        assert_eq!(parsed.warnings.len(), 7, "{:?}", parsed.warnings);
        assert!(parsed
            .warnings
            .iter()
            .all(|w| w.starts_with("retired key 'type.")));
        let t = parsed.tokens;
        assert_eq!(
            (t.type_body, t.type_label),
            (14.0, 16.0),
            "kept keys still apply"
        );
        assert_eq!(
            t.type_numeral_large, 21.0,
            "retired keys leave the tier at its default"
        );
    }

    #[test]
    fn type_sizes_below_the_design_floors_are_rejected() {
        for bad in [
            r#"{"id":"t.t","type":{"headline":20}}"#,
            r#"{"id":"t.t","type":{"display":18}}"#,
            r#"{"id":"t.t","type":{"body":11}}"#,
            r#"{"id":"t.t","type":{"label":10}}"#,
            r#"{"id":"t.t","type":{"numeral":10}}"#,
            r#"{"id":"t.t","type":{"scale":9}}"#,
        ] {
            assert!(parse_theme(bad).is_err(), "{bad}");
        }
        let floor = r#"{"id":"t.t","type":{"headline":21,"body":12,"label":11,"scale":10}}"#;
        assert!(parse_theme(floor).is_ok());
    }
}

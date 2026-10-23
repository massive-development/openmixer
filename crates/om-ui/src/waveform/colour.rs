//! Waveform colour as a pure function of the sound: spectral centroid picks the hue along the
//! theme's curve, loudness sets the lightness, spread sets the saturation. The overview, the
//! zoomed strip and the stage view all call [`wave_colour`], so a section looks the same in each.

use super::Column;
use crate::theme::{Colour, ThemeTokens, WaveStop};

/// Hue, saturation and lightness of a column, before conversion to RGB. Hue is in degrees, 0..360.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hsl {
    pub hue: f32,
    pub saturation: f32,
    pub lightness: f32,
}

/// How far into the lightness range a column without spectral data sits (a neutral grey).
const NEUTRAL_LEVEL: f32 = 0.45;

/// The curve's stop values at centroid position `pos`: hue, saturation scale, lightness scale.
fn curve_at(stops: &[WaveStop], pos: f32) -> (f32, f32, f32) {
    let (Some(first), Some(last)) = (stops.first(), stops.last()) else {
        return (0.0, 1.0, 1.0);
    };
    if pos <= first.pos {
        return (first.hue, first.saturation, first.lightness);
    }
    let Some(upper) = stops.iter().position(|s| s.pos >= pos) else {
        return (last.hue, last.saturation, last.lightness);
    };
    let (a, b) = (&stops[upper - 1], &stops[upper]);
    let t = (pos - a.pos) / (b.pos - a.pos).max(f32::EPSILON);
    let lerp = |x: f32, y: f32| x + (y - x) * t;
    (
        lerp(a.hue, b.hue),
        lerp(a.saturation, b.saturation),
        lerp(a.lightness, b.lightness),
    )
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0).max(f32::EPSILON)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Loudness on a perceptual (dB) scale: 0 at the theme's floor, 1 at full scale.
pub fn loudness_level(loudness: f32, floor_db: f32) -> f32 {
    let db = 20.0 * loudness.max(1.0e-6).log10();
    ((db - floor_db) / -floor_db).clamp(0.0, 1.0)
}

/// Whether a column is loud enough for its colour to mean anything (above the theme's floor).
pub fn is_audible(c: &Column, t: &ThemeTokens) -> bool {
    loudness_level(c.loudness, t.wave_floor_db) > 0.0
}

/// The column's colour in HSL. Columns below the floor keep their hue but sit at the dimmest
/// lightness.
pub fn wave_hsl(c: &Column, t: &ThemeTokens) -> Hsl {
    let (hue, stop_saturation, stop_lightness) = curve_at(&t.wave_stops, c.centroid);
    let spread_octaves = c.spread * om_analysis::SPREAD_MAX_OCTAVES;
    let narrowness = 1.0 - smoothstep(t.wave_spread_narrow, t.wave_spread_wide, spread_octaves);
    let spread_scale = t.wave_spread_saturation + (1.0 - t.wave_spread_saturation) * narrowness;
    let level = loudness_level(c.loudness, t.wave_floor_db);
    let range = t.wave_lightness_max - t.wave_lightness_min;
    Hsl {
        hue: hue.rem_euclid(360.0),
        saturation: (stop_saturation * spread_scale).clamp(0.0, t.wave_saturation_max),
        lightness: (t.wave_lightness_min + range * level * stop_lightness)
            .clamp(t.wave_lightness_min, t.wave_lightness_max),
    }
}

/// Grey silhouette colour for a waveform whose spectrum is not analysed yet.
fn neutral_hsl(t: &ThemeTokens) -> Hsl {
    let range = t.wave_lightness_max - t.wave_lightness_min;
    Hsl {
        hue: 0.0,
        saturation: 0.0,
        lightness: t.wave_lightness_min + range * NEUTRAL_LEVEL,
    }
}

fn channel(p: f32, q: f32, hue_sixths: f32) -> f32 {
    let h = hue_sixths.rem_euclid(6.0);
    match h {
        h if h < 1.0 => p + (q - p) * h,
        h if h < 3.0 => q,
        h if h < 4.0 => p + (q - p) * (4.0 - h),
        _ => p,
    }
}

/// Opaque RGB for an HSL triple.
pub fn hsl_to_colour(hsl: Hsl) -> Colour {
    let q = if hsl.lightness < 0.5 {
        hsl.lightness * (1.0 + hsl.saturation)
    } else {
        hsl.lightness + hsl.saturation - hsl.lightness * hsl.saturation
    };
    let p = 2.0 * hsl.lightness - q;
    let h = hsl.hue / 60.0;
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    let (r, g, b) = (
        byte(channel(p, q, h + 2.0)),
        byte(channel(p, q, h)),
        byte(channel(p, q, h - 2.0)),
    );
    Colour::from_argb(0xFF00_0000 | (r << 16) | (g << 8) | b)
}

/// Colour of a strip or overview column. `spectral` is false for a silhouette that has no
/// spectrum yet, which is drawn neutral.
pub fn wave_colour(c: &Column, spectral: bool, t: &ThemeTokens) -> Colour {
    let hsl = if spectral {
        wave_hsl(c, t)
    } else {
        neutral_hsl(t)
    };
    hsl_to_colour(hsl)
}

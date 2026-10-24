//! Waveform colour: three fixed band hues (low red, mid green, high blue; the role convention
//! Serato documents, see docs/design/research/waveform.md section 2) blended per column from the
//! band peaks the way Mixxx's RGB waveform does: the colour is the sum of the three band colours
//! weighted by the bands' linear peaks, divided by its largest channel (rgbsignal.frag,
//! https://raw.githubusercontent.com/mixxxdj/mixxx/main/res/shaders/rgbsignal.frag). Band peaks are
//! used as they are, not squashed, so a column's hue follows the ratios of its bands and a loud
//! full-range column with a strong bass reads orange, not white.
//!
//! The strip is dark in every theme ([`WAVE_WELL`], by the user's decision), so the additive blend
//! never runs on a light ground. Palettes are checked against it (4.5:1).
//!
//! Painting can only afford a few distinct colours per strip (one path each), so the weights are
//! quantised to a [`Key`] and the painter draws one shape per key ([`cap_keys`] limits how many).
//! Pure: no painting. The values are local to the waveform and do not come from the theme.

use super::Column;
use crate::style::{contrast_ratio, relative_luminance};
use crate::theme::{Colour, ThemeTokens};

/// Which palette draws the waveform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum WaveStyle {
    /// Fixed red, green and blue band hues, blended per column.
    #[default]
    Colour,
    /// Three nested silhouettes, one colour per band.
    ThreeBand,
    /// Grey only: the band sets the shade.
    Mono,
    /// Orange, yellow and sky blue (Okabe and Ito), kept apart under red-green colour blindness.
    ColourBlind,
}

impl WaveStyle {
    pub const ALL: [WaveStyle; 4] = [
        WaveStyle::Colour,
        WaveStyle::ThreeBand,
        WaveStyle::Mono,
        WaveStyle::ColourBlind,
    ];

    /// The name used in preferences and `OPENMIXER_WAVE_STYLE`.
    pub const fn name(self) -> &'static str {
        match self {
            WaveStyle::Colour => "colour",
            WaveStyle::ThreeBand => "three-band",
            WaveStyle::Mono => "mono",
            WaveStyle::ColourBlind => "colour-blind",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.name() == name)
    }
}

// Band hues: Mixxx's LateNight "Classic" scheme, SignalLowColor red, SignalMidColor green (CSS
// green, #008000) and SignalHighColor blue
// (https://raw.githubusercontent.com/mixxxdj/mixxx/main/res/skins/LateNight/skin.xml). They replace
// the plan table's #FF3B3B / #35E06A / #3D8BFF: those carry 23% to 55% in their off channels, which
// washed 68% of the columns of a loud full-range track out to near white (measured). Brought to
// 4.5:1 on the dark well by `Palette::new`, so the pure blue and green are lightened a little.
pub const DARK_LOW: Colour = Colour::from_argb(0xFFFF_0000);
pub const DARK_MID: Colour = Colour::from_argb(0xFF00_8000);
pub const DARK_HIGH: Colour = Colour::from_argb(0xFF00_00FF);
pub const DARK_SILHOUETTE: Colour = Colour::from_argb(0xFF8A_94A6);
pub const SAFE_LOW: Colour = Colour::from_argb(0xFFE6_9F00);
pub const SAFE_MID: Colour = Colour::from_argb(0xFFF0_E442);
pub const SAFE_HIGH: Colour = Colour::from_argb(0xFF56_B4E9);
/// The nested three-band look: blue, amber and near-white.
pub const LOW_COLOUR: Colour = Colour::from_argb(0xFF25_63FF);
pub const MID_COLOUR: Colour = Colour::from_argb(0xFFFF_A726);
pub const HIGH_COLOUR: Colour = Colour::from_argb(0xFFF5_F7FA);
/// A waveform whose spectrum is not analysed yet is one neutral grey silhouette.
pub const SILHOUETTE_COLOUR: Colour = DARK_SILHOUETTE;
/// Mono: the band shows as a grey level (55%, 75%, 95%; plan table).
const MONO_LEVELS: [f32; 3] = [0.55, 0.75, 0.95];

/// A column's weight for each band is `(band peak / peak)^WEIGHT_EXPONENT`. 1 is Mixxx's linear
/// band peaks (the plan and waveform.md section 3 say 0.7, which lifts the quiet bands and washes
/// the hue out on loud material).
pub const WEIGHT_EXPONENT: f32 = 1.0;
/// A silent column keeps this share of the silhouette colour's brightness.
pub const BRIGHTNESS_FLOOR: f32 = 0.35;
/// Contrast of a band colour against the strip well (WCAG, for the body of the waveform).
pub const BODY_CONTRAST: f32 = 4.5;
/// Contrast of grid lines and the playhead.
pub const LINE_CONTRAST: f32 = 3.0;
/// The background of the overview and zoomed strips in every theme: the dark theme's well
/// (resources/themes/openmixer-dark.json, `#07090D`). Kept dark in the light theme too (user
/// decision), so the additive blend and white grid and playhead always have a dark ground.
pub const WAVE_WELL: Colour = Colour::from_argb(0xFF07_090D);

fn rgb(c: Colour) -> [f32; 3] {
    let a = c.argb();
    [16, 8, 0].map(|shift| ((a >> shift) & 0xFF) as f32 / 255.0)
}

fn from_rgb(c: [f32; 3]) -> Colour {
    let [r, g, b] = c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u32);
    Colour::from_argb(0xFF00_0000 | (r << 16) | (g << 8) | b)
}

fn lerp(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}

fn grey(level: f32) -> Colour {
    from_rgb([level; 3])
}

/// `c` moved toward white (on a dark background) or black (on a light one) until it reaches
/// `ratio`:1 against `background`. A colour that already does is unchanged.
pub fn ensure_contrast(c: Colour, background: Colour, ratio: f32) -> Colour {
    let target = if relative_luminance(background) < 0.5 {
        [1.0; 3]
    } else {
        [0.0; 3]
    };
    let from = rgb(c);
    for step in 0..=50 {
        let candidate = from_rgb(lerp(from, target, step as f32 / 50.0));
        if contrast_ratio(candidate, background) >= ratio {
            return candidate;
        }
    }
    from_rgb(target)
}

/// The colour the waveform strips are painted on: [`WAVE_WELL`] whatever the theme.
pub fn strip_well(_theme: &ThemeTokens) -> Colour {
    WAVE_WELL
}

/// The three weights of a column, each 0..=1, in low, mid, high order. All zero for a silent one.
pub fn weights(c: &Column) -> [f32; 3] {
    if c.peak <= f32::EPSILON {
        return [0.0; 3];
    }
    [c.low, c.mid, c.high].map(|b| (b / c.peak).clamp(0.0, 1.0).powf(WEIGHT_EXPONENT))
}

/// Band colours for one style and background, with the blend that turns weights into a colour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub style: WaveStyle,
    pub well: Colour,
    pub low: Colour,
    pub mid: Colour,
    pub high: Colour,
    pub silhouette: Colour,
}

impl Palette {
    /// The palette for `style` on a strip whose background is `well` (opaque). Every colour is
    /// brought to [`BODY_CONTRAST`] against it.
    pub fn new(style: WaveStyle, well: Colour) -> Self {
        let (low, mid, high) = match style {
            WaveStyle::Colour => (DARK_LOW, DARK_MID, DARK_HIGH),
            WaveStyle::ThreeBand => (LOW_COLOUR, MID_COLOUR, HIGH_COLOUR),
            WaveStyle::ColourBlind => (SAFE_LOW, SAFE_MID, SAFE_HIGH),
            WaveStyle::Mono => (
                grey(MONO_LEVELS[0]),
                grey(MONO_LEVELS[1]),
                grey(MONO_LEVELS[2]),
            ),
        };
        let silhouette = DARK_SILHOUETTE;
        let fit = |c| ensure_contrast(c, well, BODY_CONTRAST);
        Self {
            style,
            well,
            low: fit(low),
            mid: fit(mid),
            high: fit(high),
            silhouette: fit(silhouette),
        }
    }

    /// The three band colours, low first.
    pub fn bands(&self) -> [Colour; 3] {
        [self.low, self.mid, self.high]
    }

    /// The colour of a column with these [`weights`]. Silent columns get the silhouette at the
    /// brightness floor.
    pub fn colour(&self, w: [f32; 3]) -> Colour {
        let total: f32 = w.iter().sum();
        if total <= f32::EPSILON {
            return self.at_floor(self.silhouette);
        }
        let bands = self.bands().map(rgb);
        if self.style == WaveStyle::Mono {
            // Greys do not add up to white: a weighted mean keeps the band's shade.
            let sharp = w.map(|v| v * v);
            let norm: f32 = sharp.iter().sum();
            let level: f32 = sharp.iter().zip(&bands).map(|(m, b)| m / norm * b[0]).sum();
            return from_rgb([level; 3]);
        }
        // Mixxx rgbsignal.frag: sum of the band colours weighted by the band peaks, divided by the
        // largest channel. No lift toward white.
        let mut sum = [0.0f32; 3];
        for (weight, band) in w.iter().zip(&bands) {
            for (s, c) in sum.iter_mut().zip(band) {
                *s += weight * c;
            }
        }
        let top = sum.iter().fold(f32::EPSILON, |m, v| m.max(*v));
        from_rgb(sum.map(|v| v / top))
    }

    fn at_floor(&self, c: Colour) -> Colour {
        from_rgb(rgb(c).map(|v| v * BRIGHTNESS_FLOOR))
    }

    /// The colour a bucket is drawn with.
    pub fn bucket_colour(&self, key: Key) -> Colour {
        self.colour(key.weights())
    }
}

/// Levels per band in a [`Key`]: 0 to this value.
pub const KEY_LEVELS: u8 = 3;
/// The painter draws at most this many colour buckets per strip.
pub const MAX_BUCKETS: usize = 24;

/// A column's colour reduced to four levels per band (so at most 64 values, 37 of them reachable).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Key(u8);

impl Key {
    pub fn new(low: u8, mid: u8, high: u8) -> Self {
        let level = |v: u8| v.min(KEY_LEVELS);
        Self((level(low) << 4) | (level(mid) << 2) | level(high))
    }

    pub const fn levels(self) -> [u8; 3] {
        [self.0 >> 4 & 3, self.0 >> 2 & 3, self.0 & 3]
    }

    /// The weights this key stands for: levels as shares of [`KEY_LEVELS`].
    pub fn weights(self) -> [f32; 3] {
        self.levels().map(|l| f32::from(l) / f32::from(KEY_LEVELS))
    }

    /// Sum of the level differences: how far apart two buckets look.
    pub fn distance(self, other: Key) -> u32 {
        self.levels()
            .iter()
            .zip(other.levels())
            .map(|(a, b)| u32::from(a.abs_diff(b)))
            .sum()
    }

    /// Index for tables of all 64 values.
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// The key of a column's [`weights`]: scaled so the largest is the top level (the colour only
/// depends on the balance), then rounded. All-zero weights give the zero key.
pub fn bucket_key(w: [f32; 3]) -> Key {
    let top = w.iter().fold(0.0f32, |m, v| m.max(*v));
    if top <= f32::EPSILON {
        return Key::default();
    }
    let level = |v: f32| (v / top * f32::from(KEY_LEVELS)).round() as u8;
    Key::new(level(w[0]), level(w[1]), level(w[2]))
}

/// Limits `keys` to `max` distinct values by merging the rarest into the nearest of the others
/// (ties go to the more common), repeatedly. Keys that survive are never changed.
pub fn cap_keys(keys: &mut [Key], max: usize) {
    let mut count = [0usize; 64];
    for k in keys.iter() {
        count[k.index()] += 1;
    }
    let mut remap: [u8; 64] = std::array::from_fn(|i| i as u8);
    let present = |count: &[usize; 64]| count.iter().filter(|&&c| c > 0).count();
    while present(&count) > max.max(1) {
        let Some(rare) = (0..64u8)
            .filter(|&i| count[i as usize] > 0)
            .min_by_key(|&i| (count[i as usize], i))
        else {
            break;
        };
        let rare_key = Key(rare);
        let Some(near) = (0..64u8)
            .filter(|&i| count[i as usize] > 0 && i != rare)
            .min_by_key(|&i| (rare_key.distance(Key(i)), usize::MAX - count[i as usize], i))
        else {
            break;
        };
        count[near as usize] += count[rare as usize];
        count[rare as usize] = 0;
        for target in remap.iter_mut().filter(|t| **t == rare) {
            *target = near;
        }
    }
    for k in keys.iter_mut() {
        *k = Key(remap[k.index()]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_pack_and_unpack() {
        let k = Key::new(3, 1, 2);
        assert_eq!(k.levels(), [3, 1, 2]);
        assert_eq!(Key::new(9, 9, 9).levels(), [3, 3, 3], "clamped");
        assert_eq!(k.distance(Key::new(3, 3, 0)), 4);
        assert_eq!(k.weights(), [1.0, 1.0 / 3.0, 2.0 / 3.0]);
    }

    #[test]
    fn the_largest_weight_is_always_the_top_level() {
        assert_eq!(bucket_key([0.5, 0.25, 0.0]).levels(), [3, 2, 0]);
        assert_eq!(bucket_key([0.0; 3]), Key::default());
        assert_eq!(bucket_key([1.0, 1.0, 1.0]).levels(), [3, 3, 3]);
    }

    #[test]
    fn contrast_is_added_toward_the_far_end() {
        let dark = Colour::from_argb(0xFF07_090D);
        let light = Colour::from_argb(0xFFE8_ECF2);
        let c = Colour::from_argb(0xFF80_8080);
        assert!(relative_luminance(ensure_contrast(c, dark, 7.0)) > relative_luminance(c));
        assert!(relative_luminance(ensure_contrast(c, light, 5.0)) < relative_luminance(c));
        let fine = Colour::from_argb(0xFFFF_FFFF);
        assert_eq!(ensure_contrast(fine, dark, 4.5), fine);
    }
}

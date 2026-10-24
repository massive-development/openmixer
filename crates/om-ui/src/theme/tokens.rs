//! Theme tokens: colours, spacing, shape, type scale, widget metrics and translucency. The built-in dark theme is defined here too.

/// An ARGB colour, stored as `0xAARRGGBB`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Colour(u32);

impl Colour {
    pub const fn from_argb(argb: u32) -> Self {
        Self(argb)
    }

    pub const fn argb(self) -> u32 {
        self.0
    }

    /// Alpha in 0..=1.
    pub fn alpha(self) -> f32 {
        (self.0 >> 24) as f32 / 255.0
    }

    pub fn is_transparent(self) -> bool {
        self.0 >> 24 == 0
    }

    /// Same colour with alpha set to `alpha` (0..=1, quantised to 8 bits).
    pub fn with_alpha(self, alpha: f32) -> Self {
        let a = (alpha.clamp(0.0, 1.0) * 255.0).round() as u32;
        Self((self.0 & 0x00FF_FFFF) | (a << 24))
    }

    /// Same colour with its alpha multiplied by `factor`.
    pub fn with_multiplied_alpha(self, factor: f32) -> Self {
        self.with_alpha(self.alpha() * factor)
    }

    /// Weighted blend of `parts` in RGB (alpha blends the same way). Weights need not sum to 1; a
    /// zero or negative total returns the first colour.
    pub fn mix(parts: &[(Colour, f32)]) -> Colour {
        let total: f32 = parts.iter().map(|(_, w)| w.max(0.0)).sum();
        let Some((first, _)) = parts.first() else {
            return Colour(0);
        };
        if total <= f32::EPSILON {
            return *first;
        }
        let mut channels = [0.0_f32; 4];
        for (colour, weight) in parts {
            let w = weight.max(0.0) / total;
            for (i, shift) in [24u32, 16, 8, 0].into_iter().enumerate() {
                channels[i] += ((colour.0 >> shift) & 0xFF) as f32 * w;
            }
        }
        let [a, r, g, b] = channels.map(|c| c.round().clamp(0.0, 255.0) as u32);
        Colour((a << 24) | (r << 16) | (g << 8) | b)
    }

    /// Largest channel as 0..=1.
    pub fn brightness(self) -> f32 {
        let r = (self.0 >> 16) & 0xFF;
        let g = (self.0 >> 8) & 0xFF;
        let b = self.0 & 0xFF;
        r.max(g).max(b) as f32 / 255.0
    }
}

/// One point of the waveform hue curve: where on the centroid axis, which hue (degrees, unwrapped
/// so the curve can run past 360), and how much of the saturation and lightness the column keeps.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaveStop {
    pub pos: f32,
    pub hue: f32,
    pub saturation: f32,
    pub lightness: f32,
}

const fn stop(pos: f32, hue: f32, saturation: f32, lightness: f32) -> WaveStop {
    WaveStop {
        pos,
        hue,
        saturation,
        lightness,
    }
}

/// Every design token one theme sets. Field names are snake case.
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeTokens {
    // Backdrop: gradient plus two soft glows the glass surfaces pick up.
    pub backdrop_top: Colour,
    pub backdrop_bottom: Colour,
    pub glow_cool: Colour,
    pub glow_warm: Colour,
    // Glass surfaces, from the card itself to the sunken wells that hold tracks and meters.
    pub surface: Colour,
    pub surface_raised: Colour,
    pub surface_hover: Colour,
    pub well: Colour,
    /// Decorative hairline between and around surfaces. Deliberately quiet (about 1.4:1); anything
    /// that must be *found* (a control's outline, a marker) uses `border_strong` (>= 3:1).
    pub border: Colour,
    /// Outline of an interactive control; WCAG 1.4.11 non-text contrast (>= 3:1) on every surface.
    pub border_strong: Colour,
    /// Keyboard focus ring; >= 3:1 on every surface.
    pub focus_ring: Colour,
    pub highlight: Colour,
    pub shadow: Colour,
    pub text_primary: Colour,
    pub text_secondary: Colour,
    pub text_muted: Colour,
    /// Text drawn on an accent fill.
    pub on_accent: Colour,
    pub meter_low: Colour,
    pub meter_mid: Colour,
    pub meter_high: Colour,
    /// The Record button's dot and the recording state (armed or running).
    pub record_red: Colour,
    /// Semantic colours (chrome, markers and state; never a deck identity and never waveform).
    /// Cue point and "main" marker.
    pub cue: Colour,
    /// Loop region and loop-active state.
    pub loop_mark: Colour,
    /// Sync engaged.
    pub sync_on: Colour,
    pub warning: Colour,
    /// Clip, error and record.
    pub danger: Colour,
    pub master: Colour,
    /// Waveform band colours (bass / mids / highs).
    /// The OpenMixer signature colour (E1). Reserved for brand marks and the "now" cues (the
    /// playhead, the master); it is never one of the deck accents.
    pub brand: Colour,
    /// BPM, key and time numerals, and the dimmer colour of their unit labels.
    pub readout: Colour,
    pub readout_dim: Colour,
    /// Menus, tooltips and knob/fader caps.
    pub popup: Colour,
    pub cap_top: Colour,
    pub cap_bottom: Colour,
    /// Tint of a deck or pad a library track is dragged over.
    pub drop_target: Colour,
    /// One per deck or channel; indices wrap. Never empty in a usable theme.
    pub accents: Vec<Colour>,

    // Spacing scale (px) and shape.
    pub gap: i32,
    pub pad: i32,
    pub inner_gap: i32,
    /// Smallest size of a button or pad, and of any mouse-only target (px).
    pub min_target: f32,
    pub min_pointer_target: f32,
    pub card_radius: f32,
    pub control_radius: f32,

    // Type scale (px height).
    pub type_small: f32,
    pub type_caption: f32,
    pub type_body: f32,
    pub type_label: f32,
    pub type_title: f32,
    pub type_heading: f32,
    pub type_mono: f32,
    /// Large numerals for BPM and time, and the smaller companion size (key, remaining time).
    pub type_readout: f32,
    pub type_readout_small: f32,

    // Deck widget metrics (px) and overlay strengths (0..1 alpha).
    pub deck_header_height: f32,
    pub deck_overview_height: f32,
    pub deck_transport_height: f32,
    pub deck_row_height: f32,
    pub deck_pitch_width: f32,
    pub deck_min_zoom_height: f32,
    pub deck_readout_width: f32,
    pub deck_range_box_height: f32,
    pub playhead_width: f32,
    pub cue_marker_width: f32,
    pub grid_bar_alpha: f32,
    pub grid_beat_alpha: f32,
    pub loop_active_alpha: f32,
    pub loop_idle_alpha: f32,

    // Mixer metrics (px) and meter ballistics (dB / seconds); meter_clip_level is a linear peak.
    pub mixer_knob_height: f32,
    pub mixer_button_height: f32,
    pub mixer_meter_width: f32,
    pub mixer_master_width: f32,
    pub mixer_tab_height: f32,
    pub mixer_crossfader_height: f32,
    pub mixer_cue_height: f32,
    pub meter_floor_db: f32,
    pub meter_mid_db: f32,
    pub meter_high_db: f32,
    pub meter_clip_level: f32,
    pub meter_release_seconds: f32,
    pub meter_hold_seconds: f32,
    pub meter_clip_seconds: f32,
    pub mixer_title_height: f32,
    pub mixer_fader_width: f32,
    pub mixer_curve_width: f32,
    pub mixer_fx_box_width: f32,
    pub mixer_fx_enable_width: f32,
    pub mixer_page_button_width: f32,
    pub mixer_pad_x: f32,
    pub mixer_pad_y: f32,
    pub mixer_accent_dot: f32,
    pub meter_segment: f32,
    pub meter_segment_gap: f32,
    pub meter_hold_tick_height: f32,

    // Library browser metrics (px) and row tint strengths (0..1 alpha).
    pub library_row_height: f32,
    pub library_header_height: f32,
    pub library_sidebar_width: f32,
    pub library_toolbar_height: f32,
    pub library_button_width: f32,
    pub library_sidebar_row_height: f32,
    pub library_rating_dot: f32,
    pub library_search_min_width: f32,
    pub library_compat_alpha: f32,
    pub library_stripe_alpha: f32,
    pub library_drop_alpha: f32,

    // Settings panel metrics (px) and the dimming of the window behind it (0..1 alpha).
    pub settings_panel_width: f32,
    pub settings_panel_height: f32,
    pub settings_tab_height: f32,
    pub settings_row_height: f32,
    pub settings_label_width: f32,
    pub settings_button_width: f32,
    pub settings_dim_alpha: f32,

    // Control chrome (px) and disabled-state strength (0..1 alpha), shared by every control kind.
    pub border_width: f32,
    pub focus_border_width: f32,
    pub thumb_size: f32,
    pub inset_small: f32,
    pub inset_large: f32,
    pub bar_radius: f32,
    pub lamp_height: f32,
    pub meter_headroom: f32,
    pub disabled_alpha: f32,

    // Waveform colour (`waveform` section): the hue curve and the limits that keep it legible.
    /// Hue curve over the centroid axis (0..1, log frequency); positions rise strictly.
    pub wave_stops: Vec<WaveStop>,
    /// Ceiling on saturation after the spread and curve scales (0..1).
    pub wave_saturation_max: f32,
    /// Lightness at the loudness floor, and the ceiling any column can reach (0..1).
    pub wave_lightness_min: f32,
    pub wave_lightness_max: f32,
    /// Loudness (dBFS RMS) that maps to the lightest and the dimmest column.
    pub wave_floor_db: f32,
    /// Spectral spread (octaves) where saturation starts to fall, and where it has fallen all the
    /// way to `wave_spread_saturation` of its value.
    pub wave_spread_narrow: f32,
    pub wave_spread_wide: f32,
    pub wave_spread_saturation: f32,
    // Depth, glow and motion (E1). Glow is the accent of a lit control spilled around it.
    pub glow_alpha: f32,
    pub glow_radius: f32,
    pub shadow_blur: f32,
    pub shadow_offset_y: f32,
    pub motion_fast_ms: f32,
    pub motion_base_ms: f32,

    /// Interface font family (registered by the front end) and whether numerals are tabular.
    pub font_family: String,
    pub tabular_numerals: bool,
    /// Reduced-motion preference: transitions collapse to zero length and glow pulses stop.
    pub reduced_motion: bool,

    /// Scales the alpha of surface, raised and hover (glass strength).
    pub surface_opacity: f32,
    /// Default backdrop opacity before a window asks for less.
    pub backdrop_opacity: f32,
}

impl ThemeTokens {
    /// The built-in dark theme. Also the fallback for anything a theme file leaves out.
    pub fn builtin_dark() -> Self {
        Self {
            // Opaque palette: contrast no longer depends on what sits behind a surface.
            backdrop_top: Colour::from_argb(0xFF0B_0D12),
            backdrop_bottom: Colour::from_argb(0xFF0B_0D12),
            glow_cool: Colour::from_argb(0x0F00_C2FF),
            glow_warm: Colour::from_argb(0x0AFF_6B35),
            surface: Colour::from_argb(0xFF12_151C),
            surface_raised: Colour::from_argb(0xFF1A_1F2B),
            surface_hover: Colour::from_argb(0xFF1C_222E),
            well: Colour::from_argb(0xFF07_090D),
            border: Colour::from_argb(0xFF2A_3142),
            border_strong: Colour::from_argb(0xFF5F_6A85),
            focus_ring: Colour::from_argb(0xFFF2_F4F8),
            highlight: Colour::from_argb(0x26FF_FFFF),
            shadow: Colour::from_argb(0x6600_0000),
            text_primary: Colour::from_argb(0xFFF2_F4F8),
            text_secondary: Colour::from_argb(0xFFA9_B1C3),
            text_muted: Colour::from_argb(0xFF80_89A0),
            on_accent: Colour::from_argb(0xFF0B_0D12),
            meter_low: Colour::from_argb(0xFF2E_E59D),
            meter_mid: Colour::from_argb(0xFFFF_C83D),
            meter_high: Colour::from_argb(0xFFFF_3B4E),
            record_red: Colour::from_argb(0xFFFF_3B4E),
            cue: Colour::from_argb(0xFFFF_6B35),
            loop_mark: Colour::from_argb(0xFF2E_E59D),
            sync_on: Colour::from_argb(0xFF19_D3A2),
            warning: Colour::from_argb(0xFFFF_C83D),
            danger: Colour::from_argb(0xFFFF_3B4E),
            master: Colour::from_argb(0xFFE2_E8F0),
            popup: Colour::from_argb(0xFF1A_1F2B),
            cap_top: Colour::from_argb(0xFF2C_3446),
            cap_bottom: Colour::from_argb(0xFF16_1B26),
            drop_target: Colour::from_argb(0x402A_6F9E),
            brand: Colour::from_argb(0xFFFF_6B35),
            readout: Colour::from_argb(0xFFF2_F4F8),
            readout_dim: Colour::from_argb(0xFFA9_B1C3),
            accents: builtin_accents(),

            // Spacing scale 4/8/12/16/24/32: card pad and card gap 16, in-group 8.
            gap: 16,
            pad: 16,
            inner_gap: 8,
            min_target: 44.0,
            min_pointer_target: 32.0,
            card_radius: 14.0,
            control_radius: 8.0,

            // Type scale 12/14/16/20/28/40; nothing below 12.
            type_small: 12.0,
            type_caption: 12.0,
            type_body: 14.0,
            type_label: 16.0,
            type_title: 20.0,
            type_heading: 20.0,
            type_mono: 16.0,
            type_readout: 40.0,
            type_readout_small: 28.0,

            deck_header_height: 52.0,
            deck_overview_height: 40.0,
            deck_transport_height: 32.0,
            deck_row_height: 40.0,
            deck_pitch_width: 84.0,
            deck_min_zoom_height: 60.0,
            deck_readout_width: 78.0,
            deck_range_box_height: 24.0,
            playhead_width: 2.0,
            cue_marker_width: 2.0,
            grid_bar_alpha: 0.55,
            grid_beat_alpha: 0.4,
            loop_active_alpha: 0.25,
            loop_idle_alpha: 0.08,

            mixer_knob_height: 70.0,
            mixer_button_height: 26.0,
            mixer_meter_width: 12.0,
            mixer_master_width: 92.0,
            mixer_tab_height: 32.0,
            mixer_crossfader_height: 46.0,
            mixer_cue_height: 46.0,
            meter_floor_db: -48.0,
            meter_mid_db: -18.0,
            meter_high_db: -6.0,
            meter_clip_level: 0.99,
            meter_release_seconds: 0.3,
            meter_hold_seconds: 1.0,
            meter_clip_seconds: 2.0,
            mixer_title_height: 22.0,
            mixer_fader_width: 40.0,
            mixer_curve_width: 128.0,
            mixer_fx_box_width: 150.0,
            mixer_fx_enable_width: 56.0,
            mixer_page_button_width: 80.0,
            mixer_pad_x: 6.0,
            mixer_pad_y: 10.0,
            mixer_accent_dot: 8.0,
            meter_segment: 4.0,
            meter_segment_gap: 1.0,
            meter_hold_tick_height: 2.0,

            library_row_height: 32.0,
            library_header_height: 32.0,
            library_sidebar_width: 190.0,
            library_toolbar_height: 40.0,
            library_button_width: 78.0,
            library_sidebar_row_height: 32.0,
            library_rating_dot: 5.0,
            library_search_min_width: 160.0,
            library_compat_alpha: 0.22,
            library_stripe_alpha: 0.04,
            library_drop_alpha: 0.18,

            settings_panel_width: 760.0,
            settings_panel_height: 540.0,
            settings_tab_height: 32.0,
            settings_row_height: 32.0,
            settings_label_width: 150.0,
            settings_button_width: 96.0,
            settings_dim_alpha: 0.55,

            border_width: 1.0,
            focus_border_width: 2.0,
            thumb_size: 12.0,
            inset_small: 2.0,
            inset_large: 4.0,
            bar_radius: 2.0,
            lamp_height: 8.0,
            meter_headroom: 14.0,
            disabled_alpha: 0.4,

            wave_stops: builtin_wave_stops(),
            wave_saturation_max: 0.9,
            wave_lightness_min: 0.2,
            wave_lightness_max: 0.88,
            wave_floor_db: -42.0,
            wave_spread_narrow: 1.0,
            wave_spread_wide: 3.5,
            wave_spread_saturation: 0.5,
            glow_alpha: 0.45,
            glow_radius: 10.0,
            shadow_blur: 18.0,
            shadow_offset_y: 6.0,
            motion_fast_ms: 90.0,
            motion_base_ms: 180.0,

            font_family: DEFAULT_FONT_FAMILY.to_owned(),
            tabular_numerals: true,
            reduced_motion: false,

            surface_opacity: 1.0,
            backdrop_opacity: 1.0,
        }
    }

    /// Accent for a deck or channel. Indices wrap in both directions; an empty palette falls back
    /// to the built-in accents so this never fails.
    pub fn accent_for(&self, index: i32) -> Colour {
        let palette: &[Colour] = if self.accents.is_empty() {
            &BUILTIN_ACCENTS
        } else {
            &self.accents
        };
        let n = palette.len() as i32;
        palette[(index.rem_euclid(n)) as usize]
    }

    /// Surface colour with the theme's glass strength applied.
    pub fn surface_scaled(&self) -> Colour {
        self.surface.with_multiplied_alpha(self.surface_opacity)
    }

    /// Raised-surface colour with the theme's glass strength applied.
    pub fn surface_raised_scaled(&self) -> Colour {
        self.surface_raised
            .with_multiplied_alpha(self.surface_opacity)
    }

    /// Hover-surface colour with the theme's glass strength applied.
    pub fn surface_hover_scaled(&self) -> Colour {
        self.surface_hover
            .with_multiplied_alpha(self.surface_opacity)
    }
}

/// Family name of the bundled interface font (Inter, SIL OFL 1.1; see `resources/fonts`).
pub const DEFAULT_FONT_FAMILY: &str = "Inter";

/// The spacing scale (px): 4, 8, 12, 16, 24, 32. Layout values come from here.
pub const SPACE_SCALE: [f32; 6] = [4.0, 8.0, 12.0, 16.0, 24.0, 32.0];

/// Deck identity colours A..D: chrome only (never the waveform), always paired with the deck letter.
const BUILTIN_ACCENTS: [Colour; 4] = [
    Colour::from_argb(0xFF00_C2FF),
    Colour::from_argb(0xFFFF_4D8D),
    Colour::from_argb(0xFFB3_88FF),
    Colour::from_argb(0xFFC6_F135),
];

/// Bass red through orange, green, cyan and blue to violet, then a wash toward white for the air.
pub fn builtin_wave_stops() -> Vec<WaveStop> {
    vec![
        stop(0.00, 0.0, 1.0, 0.75),
        stop(0.14, 8.0, 1.0, 0.8),
        stop(0.30, 38.0, 1.0, 0.8),
        stop(0.44, 95.0, 0.95, 0.75),
        stop(0.55, 150.0, 0.9, 0.75),
        stop(0.65, 190.0, 0.95, 0.8),
        stop(0.75, 225.0, 1.0, 0.85),
        stop(0.82, 262.0, 0.95, 0.9),
        stop(0.87, 290.0, 0.6, 0.95),
        stop(0.90, 305.0, 0.2, 0.98),
        stop(1.00, 320.0, 0.08, 1.0),
    ]
}

fn builtin_accents() -> Vec<Colour> {
    BUILTIN_ACCENTS.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_alpha_round_trips_through_eight_bits() {
        let c = Colour::from_argb(0x8011_2233);
        assert_eq!(c.with_alpha(1.0).argb(), 0xFF11_2233);
        assert!((c.alpha() - 0x80 as f32 / 255.0).abs() < 1e-6);
        assert!(
            (Colour::from_argb(0xFF00_0000)
                .with_multiplied_alpha(0.5)
                .alpha()
                - 0.5)
                .abs()
                < 0.01
        );
    }

    #[test]
    fn accent_index_wraps_both_ways_and_empty_palette_falls_back() {
        let mut t = ThemeTokens::builtin_dark();
        assert_eq!(t.accent_for(4), t.accent_for(0));
        assert_eq!(t.accent_for(-1), t.accent_for(3));
        t.accents.clear();
        assert_eq!(t.accent_for(3), BUILTIN_ACCENTS[3]);
    }
}

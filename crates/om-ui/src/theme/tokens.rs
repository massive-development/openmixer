//! Theme tokens: colours, spacing, shape, type scale, widget metrics and translucency. Port of
//! `src/ui/ThemeTokens.h` and the built-in dark theme in `src/ui/Theme.cpp`.

/// An ARGB colour, stored as JUCE does (`0xAARRGGBB`).
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

    /// Same colour with alpha set to `alpha` (0..=1, quantised to 8 bits like JUCE).
    pub fn with_alpha(self, alpha: f32) -> Self {
        let a = (alpha.clamp(0.0, 1.0) * 255.0).round() as u32;
        Self((self.0 & 0x00FF_FFFF) | (a << 24))
    }

    /// Same colour with its alpha multiplied by `factor`.
    pub fn with_multiplied_alpha(self, factor: f32) -> Self {
        self.with_alpha(self.alpha() * factor)
    }

    /// Largest channel as 0..=1 (JUCE's `getBrightness`).
    pub fn brightness(self) -> f32 {
        let r = (self.0 >> 16) & 0xFF;
        let g = (self.0 >> 8) & 0xFF;
        let b = self.0 & 0xFF;
        r.max(g).max(b) as f32 / 255.0
    }
}

/// Every design token one theme sets. Field names follow the C++ `ThemeTokens` in snake case.
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
    pub border: Colour,
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
    pub master: Colour,
    /// Waveform band colours (bass / mids / highs).
    pub wave_low: Colour,
    pub wave_mid: Colour,
    pub wave_high: Colour,
    /// Menus, tooltips and knob/fader caps.
    pub popup: Colour,
    pub cap_top: Colour,
    pub cap_bottom: Colour,
    /// One per deck or channel; indices wrap. Never empty in a usable theme.
    pub accents: Vec<Colour>,

    // Spacing scale (px) and shape.
    pub gap: i32,
    pub pad: i32,
    pub inner_gap: i32,
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

    /// Scales the alpha of surface, raised and hover (glass strength).
    pub surface_opacity: f32,
    /// Default backdrop opacity before a window asks for less.
    pub backdrop_opacity: f32,
}

impl ThemeTokens {
    /// The built-in dark theme. Also the fallback for anything a theme file leaves out.
    pub fn builtin_dark() -> Self {
        Self {
            backdrop_top: Colour::from_argb(0xFF0E_1220),
            backdrop_bottom: Colour::from_argb(0xFF07_090F),
            glow_cool: Colour::from_argb(0x3038_BDF8),
            glow_warm: Colour::from_argb(0x28F4_72B6),
            surface: Colour::from_argb(0x14FF_FFFF),
            surface_raised: Colour::from_argb(0x1FFF_FFFF),
            surface_hover: Colour::from_argb(0x2EFF_FFFF),
            well: Colour::from_argb(0x5900_0000),
            border: Colour::from_argb(0x1FFF_FFFF),
            highlight: Colour::from_argb(0x26FF_FFFF),
            shadow: Colour::from_argb(0x6600_0000),
            text_primary: Colour::from_argb(0xF2F4_F6FB),
            text_secondary: Colour::from_argb(0xA6C9_D1E3),
            text_muted: Colour::from_argb(0x6BC9_D1E3),
            on_accent: Colour::from_argb(0xFF07_090F),
            meter_low: Colour::from_argb(0xFF34_D399),
            meter_mid: Colour::from_argb(0xFFFB_BF24),
            meter_high: Colour::from_argb(0xFFF4_3F5E),
            master: Colour::from_argb(0xFFE2_E8F0),
            wave_low: Colour::from_argb(0xFFF9_7316),
            wave_mid: Colour::from_argb(0xFF34_D399),
            wave_high: Colour::from_argb(0xFF38_BDF8),
            popup: Colour::from_argb(0xF014_1926),
            cap_top: Colour::from_argb(0xFF2C_3446),
            cap_bottom: Colour::from_argb(0xFF16_1B26),
            accents: builtin_accents(),

            gap: 12,
            pad: 12,
            inner_gap: 8,
            card_radius: 14.0,
            control_radius: 8.0,

            type_small: 11.0,
            type_caption: 12.0,
            type_body: 13.0,
            type_label: 14.0,
            type_title: 16.0,
            type_heading: 17.0,
            type_mono: 15.0,

            deck_header_height: 52.0,
            deck_overview_height: 40.0,
            deck_transport_height: 32.0,
            deck_row_height: 30.0,
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
            mixer_tab_height: 28.0,
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

            library_row_height: 26.0,
            library_header_height: 26.0,
            library_sidebar_width: 190.0,
            library_toolbar_height: 34.0,
            library_button_width: 78.0,
            library_sidebar_row_height: 24.0,
            library_rating_dot: 5.0,
            library_search_min_width: 160.0,
            library_compat_alpha: 0.22,
            library_stripe_alpha: 0.04,
            library_drop_alpha: 0.18,

            settings_panel_width: 760.0,
            settings_panel_height: 540.0,
            settings_tab_height: 32.0,
            settings_row_height: 30.0,
            settings_label_width: 150.0,
            settings_button_width: 96.0,
            settings_dim_alpha: 0.55,

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

const BUILTIN_ACCENTS: [Colour; 4] = [
    Colour::from_argb(0xFF38_BDF8),
    Colour::from_argb(0xFFF4_72B6),
    Colour::from_argb(0xFFA3_E635),
    Colour::from_argb(0xFFFB_BF24),
];

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

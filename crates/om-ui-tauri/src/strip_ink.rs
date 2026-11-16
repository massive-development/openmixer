//! The colours and marker sizes of one deck's waveform lane, resolved from the theme by the same
//! `StripInk` the GPUI strip uses, so the shader and the overlays carry no colour of their own.
//! Built once per deck by the composition root, before the window opens.

use om_ui::theme::{Colour, ThemeTokens};
use om_ui::waveform::{
    cue_colour, StripInk, WaveStyle, BAR_NUMBER_MIN_SPACING_PX, BAR_NUMBER_SIZE, CUE_FLAG_HEIGHT,
    CUE_FLAG_WIDTH, CUE_PALETTE, LOW_BAND_HIGH_ALPHA, MAIN_CUE_COLOUR, OVERVIEW_PLAYED_SHADE_ALPHA,
    PLAYHEAD_HALO,
};
use serde::Serialize;

/// One deck's Low-Band lane ink: colours as `#rrggbb`, alphas as separate numbers, sizes in px.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StripInkDto {
    pub well: String,
    pub body: String,
    pub highs: String,
    pub highs_alpha: f32,
    /// Covers the played part; `None` when the style dims it by `playedBrightness` instead.
    pub played_shade: Option<String>,
    pub played_shade_alpha: Option<f32>,
    pub grid_bar: String,
    pub grid_bar_alpha: f32,
    pub grid_beat: String,
    pub grid_beat_alpha: f32,
    pub playhead: String,
    /// The well-coloured edge either side of the playhead; `None` in Low-Band.
    pub playhead_halo: Option<String>,
    pub playhead_halo_width: f32,
    pub playhead_width: f32,
    pub cue_guide: String,
    pub cue_guide_alpha: f32,
    pub main_cue: String,
    pub cues: Vec<String>,
    pub bar_number_size: f32,
    pub bar_number_min_spacing_px: f32,
    pub cue_flag_width: f32,
    pub cue_flag_height: f32,
}

/// Deck `deck`'s lane ink in the default Low-Band style under theme `tokens`.
pub fn strip_ink_dto(tokens: &ThemeTokens, deck: usize) -> StripInkDto {
    let ink = StripInk::new(tokens, WaveStyle::LowBand, deck);
    let palette = ink.palette();
    StripInkDto {
        well: hex(ink.well),
        body: hex(palette.low),
        highs: hex(palette.high),
        highs_alpha: LOW_BAND_HIGH_ALPHA,
        played_shade: ink.played_shade.map(hex),
        played_shade_alpha: ink.played_shade.map(|_| OVERVIEW_PLAYED_SHADE_ALPHA),
        grid_bar: hex(ink.grid(true)),
        grid_bar_alpha: ink.grid(true).alpha(),
        grid_beat: hex(ink.grid(false)),
        grid_beat_alpha: ink.grid(false).alpha(),
        playhead: hex(ink.playhead),
        playhead_halo: ink.halo.map(hex),
        playhead_halo_width: PLAYHEAD_HALO,
        playhead_width: ink.playhead_width,
        cue_guide: hex(ink.cue_guide()),
        cue_guide_alpha: ink.cue_guide().alpha(),
        main_cue: hex(MAIN_CUE_COLOUR),
        cues: (0..CUE_PALETTE.len()).map(|i| hex(cue_colour(i))).collect(),
        ..marker_metrics()
    }
}

// The marker sizes are the GPUI strip's constants, the same for every deck and theme.
fn marker_metrics() -> StripInkDto {
    StripInkDto {
        bar_number_size: BAR_NUMBER_SIZE,
        bar_number_min_spacing_px: BAR_NUMBER_MIN_SPACING_PX,
        cue_flag_width: CUE_FLAG_WIDTH,
        cue_flag_height: CUE_FLAG_HEIGHT,
        ..StripInkDto::default()
    }
}

/// `#rrggbb`; the alpha travels separately.
fn hex(c: Colour) -> String {
    format!("#{:06x}", c.argb() & 0x00FF_FFFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dark_deck(deck: usize) -> (StripInk, StripInkDto) {
        let t = ThemeTokens::builtin_dark();
        (
            StripInk::new(&t, WaveStyle::LowBand, deck),
            strip_ink_dto(&t, deck),
        )
    }

    #[test]
    fn deck_a_body_is_its_accent_on_the_dark_theme() {
        let (ink, dto) = dark_deck(0);
        assert_eq!(dto.body, "#e3a24f", "the dark theme's deck A accent");
        assert_eq!(dto.body, hex(ink.palette().low));
        assert_eq!(dto.highs, hex(ink.palette().high));
        assert_eq!(dto.highs_alpha, LOW_BAND_HIGH_ALPHA);
        assert_eq!(dto.well, hex(ink.well));
    }

    #[test]
    fn lines_carry_the_strip_ink_colours_and_alphas() {
        let (ink, dto) = dark_deck(1);
        assert_eq!(dto.playhead, hex(ink.playhead));
        assert_eq!(dto.playhead_width, ink.playhead_width);
        assert_eq!(dto.playhead_halo, None, "Low-Band has no halo");
        assert_eq!(dto.grid_bar, hex(ink.grid(true)));
        assert_eq!(dto.grid_bar_alpha, ink.grid(true).alpha());
        assert_eq!(dto.grid_beat_alpha, ink.grid(false).alpha());
        assert!(
            dto.grid_bar_alpha > dto.grid_beat_alpha,
            "bars read stronger"
        );
        assert_eq!(dto.cue_guide, hex(ink.cue_guide()));
        assert_eq!(dto.cue_guide_alpha, ink.cue_guide().alpha());
    }

    #[test]
    fn the_played_part_is_shaded_in_the_ground_colour() {
        let t = ThemeTokens::builtin_dark();
        let dto = strip_ink_dto(&t, 0);
        assert_eq!(dto.played_shade, Some(hex(t.ground)));
        assert_eq!(dto.played_shade_alpha, Some(OVERVIEW_PLAYED_SHADE_ALPHA));
    }

    #[test]
    fn cues_and_markers_come_from_the_strip_constants() {
        let (_, dto) = dark_deck(0);
        let expected: Vec<String> = CUE_PALETTE.iter().map(|c| hex(*c)).collect();
        assert_eq!(dto.cues, expected);
        assert_eq!(dto.main_cue, hex(MAIN_CUE_COLOUR));
        assert_eq!(dto.bar_number_size, BAR_NUMBER_SIZE);
        assert_eq!(dto.bar_number_min_spacing_px, BAR_NUMBER_MIN_SPACING_PX);
        assert_eq!(
            (dto.cue_flag_width, dto.cue_flag_height),
            (CUE_FLAG_WIDTH, CUE_FLAG_HEIGHT)
        );
    }

    #[test]
    fn serialises_in_camel_case_with_nulls_for_missing_colours() {
        let (_, dto) = dark_deck(0);
        let json = serde_json::to_value(&dto).unwrap();
        assert_eq!(json["playheadHalo"], serde_json::Value::Null);
        assert!(json["gridBarAlpha"].is_number());
        assert_eq!(json["cues"].as_array().map(Vec::len), Some(8));
    }
}

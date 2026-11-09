//! Drives the `gpui-component` theme from om-ui's theme tokens, so the library's buttons, inputs,
//! tables, tabs, dialogs and toasts are drawn in the colours of the OpenMixer theme in use. The
//! tokens stay the single source: nothing here invents a colour, it only says which token (or
//! which of om-ui's derived control styles) plays which role in the component library. Called at
//! start-up and whenever the window's theme changes.

use crate::paint::paint;
use gpui::{px, App, Hsla};
use gpui_kit::component::{ActiveTheme as _, Theme, ThemeColor, ThemeMode};
use om_ui::style::LookAndFeel;
use om_ui::theme::{Colour, FontSlot, ThemeTokens};

/// Share of the focus colour mixed into a selected sidebar row's fill. Unsourced; small.
const SELECT_TINT: f32 = 0.18;

/// The component library sizes its text in rems of `Theme::font_size`: buttons, inputs and tabs
/// at 0.875 rem, extra-small buttons at 0.75 rem.
const COMPONENT_TEXT_REM: f32 = 0.875;
const COMPONENT_XSMALL_REM: f32 = 0.75;

/// The component rem (px) that puts its usual text on the body tier and its extra-small text no
/// lower than the label tier.
pub fn component_rem(t: &ThemeTokens) -> f32 {
    (t.type_body / COMPONENT_TEXT_REM).max(t.type_label / COMPONENT_XSMALL_REM)
}

/// A token as the component library's colour type.
fn component_colour(colour: Colour) -> Hsla {
    Hsla::from(paint(colour))
}

/// `base` with `tint` mixed in at `share` (0..1).
fn tinted(base: Colour, tint: Colour, share: f32) -> Colour {
    Colour::mix(&[(base, 1.0 - share), (tint, share)])
}

/// The fill of a selected list row (the library sidebar): the well with the focus colour mixed in.
/// Selection also draws a bar in the focus colour, so it does not rest on tone alone
/// (labels-layout.md item 5).
pub fn selected_row_fill(t: &ThemeTokens) -> Colour {
    tinted(t.well, t.focus_ring, SELECT_TINT)
}

/// The component library's mode for a theme: a dark surface selects its dark scheme.
pub fn mode_of(t: &ThemeTokens) -> ThemeMode {
    if t.surface.brightness() < 0.5 {
        ThemeMode::Dark
    } else {
        ThemeMode::Light
    }
}

/// The component colours `t` implies, laid over `base` (the library's own scheme, which keeps the
/// roles a theme has no token for, such as the chart palette). Buttons take the faceplate faces:
/// rest, one tonal step on hover, the well when pressed, secondary text at rest.
pub fn colours(t: &ThemeTokens, base: ThemeColor) -> ThemeColor {
    let look = LookAndFeel::from_tokens(t);
    let accent = t.accent_for(0);
    let accent_hover = tinted(accent, t.text_primary, 0.12);
    let accent_active = tinted(accent, t.well, 0.2);
    let (idle, hover, pressed) = (
        look.button.idle.fill,
        look.button.hover.fill,
        look.button.active.fill,
    );
    ThemeColor {
        background: component_colour(t.surface),
        foreground: component_colour(t.text_primary),
        border: component_colour(t.border),
        input: component_colour(t.border_strong),
        ring: component_colour(t.focus_ring),
        caret: component_colour(t.text_primary),
        selection: component_colour(t.focus_ring.with_alpha(0.35)),
        muted: component_colour(t.well),
        muted_foreground: component_colour(t.text_secondary),
        accent: component_colour(hover),
        accent_foreground: component_colour(t.text_primary),
        primary: component_colour(accent),
        primary_hover: component_colour(accent_hover),
        primary_active: component_colour(accent_active),
        primary_foreground: component_colour(t.on_accent),
        secondary: component_colour(idle),
        secondary_hover: component_colour(hover),
        secondary_active: component_colour(pressed),
        secondary_foreground: component_colour(t.text_primary),
        button: component_colour(idle),
        button_hover: component_colour(hover),
        button_active: component_colour(pressed),
        button_foreground: component_colour(look.button.idle.text),
        button_primary: component_colour(accent),
        button_primary_hover: component_colour(accent_hover),
        button_primary_active: component_colour(accent_active),
        button_primary_foreground: component_colour(t.on_accent),
        danger: component_colour(t.danger),
        danger_hover: component_colour(tinted(t.danger, t.text_primary, 0.12)),
        danger_active: component_colour(tinted(t.danger, t.well, 0.2)),
        danger_foreground: component_colour(t.on_accent),
        warning: component_colour(t.warning),
        warning_foreground: component_colour(t.on_accent),
        success: component_colour(t.sync_on),
        success_foreground: component_colour(t.on_accent),
        info: component_colour(t.cue),
        info_foreground: component_colour(t.on_accent),
        popover: component_colour(look.popup.fill),
        popover_foreground: component_colour(look.popup.text),
        overlay: Hsla::from(crate::paint::scrim(t.settings_dim_alpha)),
        list: component_colour(t.well),
        list_even: component_colour(t.well),
        list_head: component_colour(look.table.header.fill),
        list_hover: component_colour(t.surface_hover),
        list_active: component_colour(selected_row_fill(t)),
        list_active_border: component_colour(t.focus_ring),
        table: component_colour(look.table.row.fill),
        table_even: component_colour(look.table.row_alt.fill),
        table_head: component_colour(look.table.header.fill),
        table_head_foreground: component_colour(look.table.header.text),
        table_hover: component_colour(t.surface_hover),
        table_active: component_colour(look.table.row_selected.fill),
        table_active_border: component_colour(look.table.row_selected.border),
        table_row_border: component_colour(look.table.row.border),
        tab: component_colour(t.surface),
        tab_active: component_colour(t.surface_hover),
        tab_active_foreground: component_colour(t.text_primary),
        tab_foreground: component_colour(t.text_secondary),
        tab_bar: component_colour(t.surface),
        tab_bar_segmented: component_colour(t.well),
        scrollbar: component_colour(t.surface.with_alpha(0.0)),
        scrollbar_thumb: component_colour(t.border_strong.with_alpha(0.6)),
        scrollbar_thumb_hover: component_colour(t.border_strong),
        switch: component_colour(t.border_strong),
        switch_thumb: component_colour(t.text_primary),
        drag_border: component_colour(t.focus_ring),
        drop_target: component_colour(t.drop_target),
        title_bar: component_colour(t.popup),
        title_bar_border: component_colour(t.border),
        link: component_colour(t.text_primary),
        ..base
    }
}

/// Makes the component library follow `t`: its light or dark scheme, its colours, the control
/// radius, the families and the type size ([`component_rem`]). Safe to call again on a theme
/// change.
pub fn apply(cx: &mut App, t: &ThemeTokens) {
    let mode = mode_of(t);
    // A mode switch loads the registered scheme, so the colours go in a second pass.
    Theme::change(mode, None, cx);
    let base = cx.theme().colors;
    let (radius, size, mono_size) = (
        px(t.control_radius),
        px(component_rem(t)),
        px(t.type_numeral),
    );
    let family = crate::font::family(t, FontSlot::Body);
    let mono = crate::font::family(t, FontSlot::Mono);
    let colours = colours(t, base);
    Theme::update(cx, |theme| {
        theme.colors = colours;
        theme.radius = radius;
        theme.radius_lg = radius;
        theme.font_size = size;
        theme.font_family = family;
        theme.mono_font_family = mono;
        theme.mono_font_size = mono_size;
        // No drops on component controls, and the ring outside the edge for keyboard focus: the
        // nearest the library has to the faceplate's 2px ring at a 2px offset.
        theme.shadow = false;
        theme.focus_ring = true;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use om_ui::style::{contrast_ratio, over};
    use om_ui::theme::{parse_theme, BUILTIN_THEMES};

    fn themes() -> [ThemeTokens; 3] {
        ["dark", "daylight", "harbour"].map(|id| {
            let (_, json) = BUILTIN_THEMES
                .iter()
                .find(|(file, _)| file.contains(id))
                .expect("built-in theme");
            parse_theme(json).expect("theme parses").tokens
        })
    }

    /// The component library's small text lands on the body tier and its extra-small button
    /// labels stay at or above the label floor (11 px), in every built-in theme.
    #[test]
    fn component_text_meets_the_body_and_label_tiers() {
        for t in themes() {
            let rem = component_rem(&t);
            assert!((rem * COMPONENT_TEXT_REM - t.type_body).abs() < 1e-3);
            assert!(rem * COMPONENT_XSMALL_REM >= t.type_label);
        }
    }

    #[test]
    fn dark_and_light_surfaces_pick_their_scheme() {
        let [dark, daylight, _] = themes();
        assert_eq!(mode_of(&dark), ThemeMode::Dark);
        assert_eq!(mode_of(&daylight), ThemeMode::Light);
    }

    #[test]
    fn roles_come_from_the_theme_tokens() {
        let t = ThemeTokens::builtin_dark();
        let c = colours(&t, ThemeColor::default());
        assert_eq!(c.background, component_colour(t.surface));
        assert_eq!(c.foreground, component_colour(t.text_primary));
        assert_eq!(c.input, component_colour(t.border_strong));
        assert_eq!(c.ring, component_colour(t.focus_ring));
        assert_eq!(c.danger, component_colour(t.danger));
        assert_eq!(c.primary, component_colour(t.accents[0]));
        assert_eq!(c.list_active, component_colour(selected_row_fill(&t)));
    }

    /// The sidebar's selected row, as the component list item draws it (the `list_active` fill
    /// under primary text, with a focus-colour bar), reads in every built-in theme at WCAG.
    #[test]
    fn a_selected_row_reads_in_every_theme_at_wcag() {
        for t in themes() {
            let ground = over(t.well, over(t.surface, t.ground));
            let fill = over(selected_row_fill(&t), ground);
            assert!(contrast_ratio(t.focus_ring, fill) >= 3.0, "indicator 3:1");
            assert!(
                contrast_ratio(t.focus_ring, over(t.well, ground)) >= 3.0,
                "indicator against an unselected row"
            );
            assert!(contrast_ratio(t.text_primary, fill) >= 4.5, "selected text");
            assert!(
                contrast_ratio(t.text_secondary, over(t.well, ground)) >= 4.5,
                "plain text"
            );
            // The selected text is the stronger one, so selection never reads inverted.
            assert!(
                contrast_ratio(t.text_primary, fill)
                    >= contrast_ratio(t.text_secondary, over(t.well, ground))
            );
        }
    }
}

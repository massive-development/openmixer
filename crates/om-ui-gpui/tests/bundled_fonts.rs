//! The bundled fonts in the platform's real text system (headless, no display or GPU): each family
//! the built-in themes name is registered under that name, GPUI picks the nearest of the static
//! weights, and the mono family keeps every digit the same width. Glyph images are not checked.

use gpui::{font, px, Font, FontId, FontWeight, Hsla, TextRun, TextSystem, WindowTextSystem};
use om_ui::theme::{parse_theme, FontSlot, ThemeTokens, BUILTIN_THEMES};
use om_ui_gpui::font::bundled_faces;
use std::sync::Arc;

/// The platform text system (headless) with the bundled faces added.
fn text_system() -> Arc<TextSystem> {
    let platform = gpui_kit::platform::current_platform(true);
    let text = Arc::new(TextSystem::new(platform.text_system()));
    text.add_fonts(bundled_faces())
        .expect("the bundled faces load");
    text
}

fn weighted(family: &str, weight: f32) -> Font {
    Font {
        weight: FontWeight(weight),
        ..font(family.to_owned())
    }
}

fn face(text: &TextSystem, family: &str, weight: f32) -> FontId {
    text.resolve_font(&weighted(family, weight))
}

fn width(text: &Arc<TextSystem>, family: &str, size: f32, s: &str) -> f32 {
    let run = TextRun {
        len: s.len(),
        font: weighted(family, 500.0),
        color: Hsla::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let line = WindowTextSystem::new(text.clone()).layout_line(s, px(size), &[run], None);
    f32::from(line.width)
}

fn builtins() -> Vec<ThemeTokens> {
    BUILTIN_THEMES
        .iter()
        .map(|(_, json)| parse_theme(json).expect("parses").tokens)
        .collect()
}

#[test]
fn every_family_a_built_in_theme_names_is_registered_under_that_name() {
    let text = text_system();
    let names = text.all_font_names();
    for t in builtins() {
        for slot in [FontSlot::Display, FontSlot::Body, FontSlot::Mono] {
            assert!(names.iter().any(|n| n == t.family(slot)), "{slot:?}");
        }
    }
}

#[test]
fn the_body_family_resolves_regular_and_medium_from_two_static_files() {
    let text = text_system();
    let regular = face(&text, "Source Sans 3", 400.0);
    let medium = face(&text, "Source Sans 3", 500.0);
    assert_ne!(regular, medium, "400 and 500 are different faces");
    // No 600 or 700 is bundled: the nearest weight, Medium, is used.
    assert_eq!(face(&text, "Source Sans 3", 700.0), medium);
    let advance = |id| text.advance(id, px(100.), 'm').expect("advance").width;
    assert!(
        advance(medium) > advance(regular),
        "Medium is the heavier face"
    );
}

#[test]
fn the_display_and_mono_families_have_one_face_each() {
    let text = text_system();
    assert_eq!(
        face(&text, "Sansation", 700.0),
        face(&text, "Sansation", 400.0)
    );
    // Ubuntu Mono has no 500; the numerals take its Regular.
    assert_eq!(
        face(&text, "Ubuntu Mono", 500.0),
        face(&text, "Ubuntu Mono", 400.0)
    );
    let fallback = face(&text, "No Such Family", 400.0);
    for family in ["Sansation", "Source Sans 3", "Ubuntu Mono"] {
        assert_ne!(
            face(&text, family, 400.0),
            fallback,
            "{family} is not the fallback"
        );
    }
}

#[test]
fn mono_digits_never_change_width() {
    let text = text_system();
    let t = ThemeTokens::builtin_dark();
    let mono = t.family(FontSlot::Mono);
    let size = t.type_numeral_large;
    assert_eq!(
        width(&text, mono, size, "111.1"),
        width(&text, mono, size, "888.8")
    );
    assert_eq!(
        width(&text, mono, size, "-1:11"),
        width(&text, mono, size, "-8:48")
    );
}

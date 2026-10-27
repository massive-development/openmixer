//! Lint: view code draws colours from the theme (`om_ui::style` and `ThemeTokens`), never from raw
//! colour literals. Grep-based, so it runs headless and needs no display.
//!
//! A new colour literal in `src/` fails this test. If one is truly needed, add its file to
//! `ALLOWLIST` with a reason, or better, add a token to the theme and use it.

use std::fs;
use std::path::{Path, PathBuf};

/// Files allowed to name colour primitives. `paint.rs` converts om-ui's ARGB into gpui's RGBA and
/// is the only place that calls `rgba(`.
const ALLOWLIST: &[&str] = &["paint.rs"];

/// Call forms that construct a colour from raw numbers or names.
const FORBIDDEN_CALLS: &[&str] = &[
    "rgba(",
    "rgb(",
    "hsla(",
    "hsl(",
    "from_argb(",
    "white()",
    "black()",
    "transparent_black()",
    "transparent_white()",
];

fn source_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("src directory readable") {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            source_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// True when `code` has a hex literal of 6 or 8 digits (`0x2a6f9e`, `0xFF38_BDF8`): a colour
/// written as a bare number.
fn has_hex_colour(code: &str) -> bool {
    let mut rest = code;
    while let Some(at) = rest.find("0x") {
        let digits: String = rest[at + 2..]
            .chars()
            .take_while(|c| c.is_ascii_hexdigit() || *c == '_')
            .filter(|c| *c != '_')
            .collect();
        if digits.len() == 6 || digits.len() == 8 {
            return true;
        }
        rest = &rest[at + 2..];
    }
    false
}

/// Violations in one file as `line: text` strings. Comment-only lines are skipped.
fn violations(text: &str) -> Vec<String> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim_start().starts_with("//"))
        .filter(|(_, line)| {
            FORBIDDEN_CALLS.iter().any(|call| line.contains(call)) || has_hex_colour(line)
        })
        .map(|(n, line)| format!("{}: {}", n + 1, line.trim()))
        .collect()
}

#[test]
fn view_code_has_no_raw_colour_literals() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    source_files(&src, &mut files);
    assert!(
        !files.is_empty(),
        "no source files found under {}",
        src.display()
    );

    let mut found = Vec::new();
    for path in files {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if ALLOWLIST.contains(&name.as_str()) {
            continue;
        }
        let text = fs::read_to_string(&path).expect("source readable");
        for v in violations(&text) {
            found.push(format!(
                "{}:{}",
                path.strip_prefix(&src).unwrap().display(),
                v
            ));
        }
    }
    assert!(
        found.is_empty(),
        "raw colour literals in view code (use om_ui::style / ThemeTokens instead):\n{}",
        found.join("\n")
    );
}

/// Files whose controls draw every colour from a `LookAndFeel` style. They may not read a colour
/// token off `ThemeTokens` (`t.well`, `t.text_primary`, ...); the style carries it.
const MIGRATED: &[&str] = &[
    "debug_panel.rs",
    "settings_view/chips.rs",
    "settings_view/overlay.rs",
];

/// Colour fields of `ThemeTokens` that view code must reach through `LookAndFeel` instead.
const COLOUR_TOKENS: &[&str] = &[
    "accent_for",
    "accents",
    "backdrop_top",
    "backdrop_bottom",
    "border",
    "border_strong",
    "brand",
    "cap_top",
    "danger",
    "drop_target",
    "focus_ring",
    "on_accent",
    "popup",
    "readout",
    "readout_dim",
    "shadow",
    "surface",
    "surface_hover",
    "surface_raised",
    "text_muted",
    "text_primary",
    "text_secondary",
    "warning",
    "well",
];

/// Colour token reads (`t.well`) in `text`, as `line: text` strings. A `t.` that continues an
/// identifier (`light.`, `look.`) is not a read of the theme.
fn token_reads(text: &str) -> Vec<String> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim_start().starts_with("//"))
        .filter(|(_, line)| {
            line.match_indices("t.").any(|(at, _)| {
                let before = line[..at].chars().next_back();
                let starts_ident =
                    before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.');
                let name: String = line[at + 2..]
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                !starts_ident && COLOUR_TOKENS.contains(&name.as_str())
            })
        })
        .map(|(n, line)| format!("{}: {}", n + 1, line.trim()))
        .collect()
}

#[test]
fn migrated_controls_read_colours_from_the_look_and_feel() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut found = Vec::new();
    for file in MIGRATED {
        let text = fs::read_to_string(src.join(file)).expect("migrated file readable");
        for v in token_reads(&text) {
            found.push(format!("{file}:{v}"));
        }
    }
    assert!(
        found.is_empty(),
        "theme colour tokens read directly in migrated controls (use LookAndFeel):\n{}",
        found.join("\n")
    );
}

#[test]
fn the_lint_catches_what_it_claims_to_catch() {
    assert_eq!(token_reads("    .bg(paint(t.well))").len(), 1);
    assert_eq!(token_reads("    .bg(paint(look.popup.fill))").len(), 0);
    assert_eq!(token_reads("    .text_size(px(t.type_small))").len(), 0);

    assert_eq!(violations("    .bg(gpui::rgba(0x2a_6f_9e_40))").len(), 1);
    assert_eq!(
        violations("let c = Colour::from_argb(0xFF38_BDF8);").len(),
        1
    );
    assert_eq!(
        violations("let x = 0x1234;").len(),
        0,
        "short hex is not a colour"
    );
    assert_eq!(violations("// rgba(0x2a6f9e40) in a comment").len(), 0);
    assert_eq!(violations("style.bg(paint(t.well))").len(), 0);
}

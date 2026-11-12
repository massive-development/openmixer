//! Lint: the type rules of `openmixer-workbench/docs/design-system/DESIGN.md` in view code. Grep-based, so it runs
//! headless. Families are chosen only by the type tiers in `font.rs`; numbers are drawn through
//! `font::numeral` (the Mono Numbers Rule); the 10 px scale tier is only the fader dB scale; no
//! literal text size is below the 11 px label floor. The sizes of the tiers themselves are checked
//! in `om_ui::theme::typography` (Sansation 21 and up, labels 11, prose 12).

use std::fs;
use std::path::{Path, PathBuf};

/// The one file that may name a family or a mono tier: it defines the tiers.
const FONT_FILE: &str = "font.rs";

/// The one file that may draw the 10 px scale tier: the fader dB scale (DESIGN.md's exception).
const SCALE_FILES: &[&str] = &["fader_paint.rs"];

/// What chooses a family or a mono tier directly, bypassing `font.rs`.
const FAMILY_TOKENS: &[&str] = &[
    ".font_family(",
    ".font(",
    "font_display",
    "font_body",
    "font_mono",
    "FontSlot::",
    "TypeRole::Numeral",
    "TypeRole::Scale",
    "\"Sansation\"",
    "\"Source Sans 3\"",
    "\"Ubuntu Mono\"",
];

/// Expressions that hold number text (BPM, key, time, dB, gain, counters, table values): the audit
/// of where the window draws numbers. A text sink given one of these must wrap it in a numeral.
const NUMBER_SOURCES: &[&str] = &[
    "format_bpm(",
    "latency_text(",
    "peak_text",
    "summary_text()",
    "beat_label()",
    "pitch.readout()",
    "view.readout",
    "face.readout",
    "text.bpm",
    "text.key",
    "text.remaining",
    "text.elapsed",
    "bar.remaining",
    "bar.size",
    "dropped_frames",
    "bar_number",
    "region.label",
    "flag.label",
    "view.label",
    "mark.text",
    "pad.key",
    "_text)",
    "gain_text",
    "pitch_text",
    "{:.",
    "{:+.",
    ":.0}",
    ":.1}",
    ":.2}",
];

/// Calls that draw their text argument in the body family.
const TEXT_SINKS: &[&str] = &[".child(", "caption(", "mark(", "tile(", "note(", "chip("];

/// Calls that draw their argument in mono (each goes through `font::numeral`).
const NUMERAL_WRAPPERS: &[&str] = &[
    "numeral(",
    "value_caption(",
    "value_mark(",
    "strip_number(",
    "popup(",
    "popup_chip(",
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

/// Every production source file as (path under `src/`, code without its test module or comments).
fn production_sources() -> Vec<(String, String)> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    source_files(&src, &mut files);
    assert!(!files.is_empty(), "no sources under {}", src.display());
    files
        .into_iter()
        .filter(|p| !p.to_string_lossy().ends_with("tests.rs"))
        .map(|path| {
            let name = path.strip_prefix(&src).unwrap().display().to_string();
            let text = fs::read_to_string(&path).expect("source readable");
            (name, strip(&text))
        })
        .collect()
}

/// `text` before its `#[cfg(test)]` module, with comment lines blanked (line numbers kept).
fn strip(text: &str) -> String {
    let code = text.split("#[cfg(test)]").next().unwrap_or_default();
    code.lines()
        .map(|line| {
            if line.trim_start().starts_with("//") {
                ""
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Lines of `code` naming any of `tokens`, as `line: text`.
fn lines_with(code: &str, tokens: &[&str]) -> Vec<String> {
    code.lines()
        .enumerate()
        // The root font comes from `font.rs` too (`crate::font::interface_font`).
        .filter(|(_, line)| !line.contains("crate::font::"))
        .filter(|(_, line)| tokens.iter().any(|t| line.contains(t)))
        .map(|(n, line)| format!("{}: {}", n + 1, line.trim()))
        .collect()
}

/// The balanced-parenthesis argument text of the call that opens at `open` (the index of `(`).
fn call_args(code: &str, open: usize) -> &str {
    let mut depth = 0usize;
    for (i, c) in code[open..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return &code[open + 1..open + i];
                }
            }
            _ => {}
        }
    }
    &code[open + 1..]
}

/// True when `text` has `token` not glued to a longer identifier (`latency_text(` is not in
/// `plugin_latency_text(`).
fn has_token(text: &str, token: &str) -> bool {
    let glued = token.starts_with(|c: char| c.is_alphanumeric());
    text.match_indices(token).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        !(glued && before.is_some_and(|c| c.is_alphanumeric() || c == '_'))
    })
}

/// Text sinks in `code` given a number source with no numeral around it, as `line: call`.
fn numbers_in_body_font(code: &str) -> Vec<String> {
    let mut found = Vec::new();
    for sink in TEXT_SINKS {
        for (at, _) in code.match_indices(sink) {
            let before = code[..at].chars().next_back();
            // `value_caption(` is not `caption(`; a method sink starts with its own dot.
            if !sink.starts_with('.') && before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                continue;
            }
            let args = call_args(code, at + sink.len() - 1);
            let numeric = NUMBER_SOURCES.iter().any(|s| has_token(args, s));
            let wrapped = NUMERAL_WRAPPERS.iter().any(|w| args.contains(w));
            if numeric && !wrapped {
                let line = code[..at].lines().count();
                let shown: String = args.split_whitespace().collect::<Vec<_>>().join(" ");
                found.push(format!("{line}: {sink}{shown})"));
            }
        }
    }
    found
}

fn report(found: &[String], what: &str) {
    assert!(found.is_empty(), "{what}:\n{}", found.join("\n"));
}

#[test]
fn families_and_mono_tiers_are_chosen_only_in_font_rs() {
    let mut found = Vec::new();
    for (path, code) in production_sources() {
        if file_name(&path) == FONT_FILE || path == "theme_bridge.rs" {
            continue;
        }
        found.extend(
            lines_with(&code, FAMILY_TOKENS)
                .into_iter()
                .map(|l| format!("{path}:{l}")),
        );
    }
    report(
        &found,
        "a family or mono tier named outside font.rs (use TypeExt or numeral)",
    );
}

#[test]
fn the_scale_tier_is_only_the_fader_db_scale() {
    let mut found = Vec::new();
    for (path, code) in production_sources() {
        let name = file_name(&path);
        if name == FONT_FILE || SCALE_FILES.contains(&name) {
            continue;
        }
        found.extend(
            lines_with(&code, &["Numeral::Scale"])
                .into_iter()
                .map(|l| format!("{path}:{l}")),
        );
    }
    report(&found, "10 px scale text outside the fader dB scale");
    let fader = production_sources()
        .into_iter()
        .find(|(p, _)| p == "fader_paint.rs")
        .expect("fader_paint.rs");
    assert_eq!(
        lines_with(&fader.1, &["Numeral::Scale"]).len(),
        1,
        "one scale use"
    );
}

/// A `text_size(px(N))` with a literal under 11 px; sizes from tokens are checked in om-ui.
fn small_literal_sizes(code: &str) -> Vec<String> {
    let mut found = Vec::new();
    for (at, _) in code.match_indices("text_size(px(") {
        let rest = &code[at + "text_size(px(".len()..];
        let number: String = rest
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        if number.parse::<f32>().is_ok_and(|n| n < 11.0) {
            found.push(format!(
                "{}: text_size(px({number}",
                code[..at].lines().count()
            ));
        }
    }
    found
}

#[test]
fn no_literal_text_size_is_below_the_label_floor() {
    let mut found = Vec::new();
    for (path, code) in production_sources() {
        found.extend(
            small_literal_sizes(&code)
                .into_iter()
                .map(|l| format!("{path}:{l}")),
        );
    }
    report(&found, "text below 11 px (labels are the smallest words)");
}

#[test]
fn numbers_are_drawn_through_the_numeral_helper() {
    let mut found = Vec::new();
    for (path, code) in production_sources() {
        found.extend(
            numbers_in_body_font(&code)
                .into_iter()
                .map(|l| format!("{path}:{l}")),
        );
    }
    report(
        &found,
        "a number drawn in the body font (wrap it in font::numeral)",
    );
}

#[test]
fn the_lints_catch_what_they_claim_to_catch() {
    assert_eq!(numbers_in_body_font(".child(format_bpm(bpm))").len(), 1);
    assert_eq!(
        numbers_in_body_font(".child(\n    div().child(text.remaining),\n)").len(),
        2,
        "the outer and the inner child both draw it"
    );
    assert!(
        numbers_in_body_font(".child(numeral(t, Numeral::Readout, format_bpm(bpm)))").is_empty()
    );
    assert_eq!(
        numbers_in_body_font("caption(t, lay.beat, fx.beat_label())").len(),
        1
    );
    assert!(numbers_in_body_font("value_caption(t, lay.beat, fx.beat_label())").is_empty());
    assert!(numbers_in_body_font(".child(\"BPM\")").is_empty());
    assert_eq!(small_literal_sizes(".text_size(px(10.))").len(), 1);
    assert!(small_literal_sizes(".text_size(px(12.0))").is_empty());
    assert!(small_literal_sizes(".text_size(px(t.type_label))").is_empty());
    assert_eq!(
        lines_with("    .font_family(\"Inter\")", FAMILY_TOKENS).len(),
        1
    );
    let stripped = strip("let a = 1;\n// .child(text.bpm)\n#[cfg(test)]\nmod t {}");
    assert!(!stripped.contains("text.bpm") && !stripped.contains("mod t"));
    assert!(!has_token("s.plugin_latency_text(id)", "latency_text("));
    assert!(has_token("(latency_text(1.0, 0))", "latency_text("));
}

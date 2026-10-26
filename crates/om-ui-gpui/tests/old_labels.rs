//! Lint: the mixer view takes its strip captions from `om_ui::labels::channel_labels`, never from
//! the retired names the first screenshot showed ("Trim", "PFL", "Lo", ...). Grep-based, headless.

use std::fs;

/// Names the old strip printed; none may come back as a string literal in the view.
const OLD_LITERALS: &[&str] = &[
    "\"Trim\"", "\"PFL\"", "\"Lo\"", "\"Hi\"", "\"Mid\"", "\"Kill\"", "\"Mstr\"",
];

#[test]
fn mixer_view_uses_none_of_the_old_strip_literals() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/mixer_view.rs");
    let source = fs::read_to_string(path).expect("mixer_view.rs readable");
    for (n, line) in source.lines().enumerate() {
        let code = line.split("//").next().unwrap_or("");
        for old in OLD_LITERALS {
            assert!(
                !code.contains(old),
                "mixer_view.rs:{} uses the old label {old}; use labels::channel_labels()",
                n + 1
            );
        }
    }
}

#[test]
fn the_strip_captions_are_the_shared_names() {
    let c = om_ui::labels::channel_labels();
    assert_eq!(c.cue, "CUE");
    assert_eq!(c.high, "HIGH");
}

//! Theme registry cases (the GUI-only cases are not covered here).

use om_ui::theme::{
    default_user_directory, session_theme_override, Colour, Source, ThemeInfo, ThemeRegistry,
    ThemeTokens,
};
use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;

fn theme_json(id: &str, backdrop_top: &str, name: &str) -> String {
    format!(
        r##"{{ "id": "{id}", "name": "{name}", "colours": {{ "backdropTop": "{backdrop_top}" }} }}"##
    )
}

/// A scratch directory under cargo's per-target tmp dir, removed on drop.
struct ScopedDir(PathBuf);

impl ScopedDir {
    fn new(name: &str) -> Self {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("om-ui-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create scratch dir");
        Self(dir)
    }
}

impl Drop for ScopedDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn backdrop_of(reg: &ThemeRegistry, id: &str) -> Colour {
    reg.find(id).expect("theme present").tokens.backdrop_top
}

#[test]
fn loads_built_in_dark_and_light() {
    let mut reg = ThemeRegistry::new();
    assert_eq!(reg.load_builtins(), 3);
    let dark = reg.find(ThemeRegistry::DEFAULT_ID).expect("dark present");
    assert_eq!(dark.source, Source::BuiltIn);
    let light = reg.find("openmixer.daylight").expect("daylight present");
    assert!(light.tokens.backdrop_top.brightness() > 0.8);
    assert!(dark.tokens.backdrop_top.brightness() < 0.2);
}

#[test]
fn builtin_dark_json_matches_the_compiled_default() {
    let mut reg = ThemeRegistry::new();
    reg.load_builtins();
    let json = &reg
        .find(ThemeRegistry::DEFAULT_ID)
        .expect("dark present")
        .tokens;
    assert_eq!(*json, ThemeTokens::builtin_dark());
}

#[test]
fn precedence_is_user_over_plugin_over_builtin() {
    let mut reg = ThemeRegistry::new();
    reg.add_json(&theme_json("x", "#FF000001", "X"), Source::BuiltIn, "b")
        .unwrap();
    reg.add_json(&theme_json("x", "#FF000002", "X"), Source::User, "u")
        .unwrap();
    reg.add_json(&theme_json("x", "#FF000003", "X"), Source::Plugin, "p")
        .unwrap();
    assert_eq!(backdrop_of(&reg, "x"), Colour::from_argb(0xFF00_0002));
    assert_eq!(reg.themes().len(), 1, "one winner per id");

    reg.remove_source(Source::User);
    assert_eq!(backdrop_of(&reg, "x"), Colour::from_argb(0xFF00_0003));
    reg.remove_owner("p");
    assert_eq!(backdrop_of(&reg, "x"), Colour::from_argb(0xFF00_0001));
}

#[test]
fn later_add_within_a_source_wins() {
    let mut reg = ThemeRegistry::new();
    reg.add_json(&theme_json("x", "#FF000001", "X"), Source::User, "a.json")
        .unwrap();
    reg.add_json(&theme_json("x", "#FF000002", "X"), Source::User, "b.json")
        .unwrap();
    assert_eq!(backdrop_of(&reg, "x"), Colour::from_argb(0xFF00_0002));
    reg.add_json(&theme_json("x", "#FF000003", "X"), Source::User, "a.json")
        .unwrap();
    assert_eq!(
        backdrop_of(&reg, "x"),
        Colour::from_argb(0xFF00_0003),
        "same owner replaces itself"
    );
}

#[test]
fn invalid_json_is_rejected_and_leaves_registry_alone() {
    let mut reg = ThemeRegistry::new();
    reg.load_builtins();
    assert!(reg.add_json("{ nope", Source::User, "").is_err());
    assert_eq!(reg.themes().len(), 3);
}

#[test]
fn user_directory_loads_valid_files_and_reports_bad_ones() {
    let tmp = ScopedDir::new("userdir");
    fs::write(
        tmp.0.join("a.json"),
        theme_json("user.a", "#FF0A0B0C", "Aye"),
    )
    .unwrap();
    fs::write(tmp.0.join("broken.json"), "{ nope").unwrap();
    fs::write(tmp.0.join("notes.txt"), "ignored").unwrap();

    let mut reg = ThemeRegistry::new();
    let load = reg.load_user_directory(&tmp.0);
    assert_eq!(load.loaded, 1);
    assert_eq!(load.errors.len(), 1, "{:?}", load.errors);
    assert!(load.errors[0].starts_with("broken.json"));
    assert_eq!(reg.find("user.a").expect("user theme").info.name, "Aye");

    fs::remove_file(tmp.0.join("a.json")).unwrap();
    assert_eq!(
        reg.load_user_directory(&tmp.0).loaded,
        0,
        "a rescan drops themes whose file is gone"
    );
    assert!(reg.find("user.a").is_none());
    assert_eq!(
        reg.load_user_directory(&tmp.0.join("missing")).loaded,
        0,
        "absent dir is fine"
    );
}

#[test]
fn user_overrides_a_built_in_by_id() {
    let tmp = ScopedDir::new("override");
    fs::write(
        tmp.0.join("mine.json"),
        theme_json(ThemeRegistry::DEFAULT_ID, "#FF123456", "My dark"),
    )
    .unwrap();
    let mut reg = ThemeRegistry::new();
    reg.load_builtins();
    reg.load_user_directory(&tmp.0);
    let dark = reg.find(ThemeRegistry::DEFAULT_ID).expect("dark present");
    assert_eq!(dark.info.name, "My dark");
    assert_eq!(dark.source, Source::User);
}

#[test]
fn select_unknown_id_changes_nothing() {
    let mut reg = ThemeRegistry::new();
    reg.load_builtins();
    let before = reg.generation();
    assert!(!reg.select("nope"));
    assert!(reg.active_id().is_none());
    assert_eq!(reg.generation(), before);
}

#[test]
fn select_switches_the_current_theme_and_bumps_generation() {
    let mut reg = ThemeRegistry::new();
    reg.load_builtins();
    let before = reg.generation();
    assert!(reg.select("openmixer.daylight"));
    assert!(reg.generation() > before);
    assert_eq!(
        reg.current().backdrop_top,
        reg.find("openmixer.daylight").unwrap().tokens.backdrop_top
    );
    assert_eq!(
        reg.current().accent_for(0),
        reg.find("openmixer.daylight").unwrap().tokens.accent_for(0)
    );
    assert!(reg.select(ThemeRegistry::DEFAULT_ID));
    assert_eq!(
        reg.current().backdrop_top,
        ThemeTokens::builtin_dark().backdrop_top
    );
}

#[test]
fn removing_the_active_theme_falls_back_to_default() {
    let mut reg = ThemeRegistry::new();
    reg.load_builtins();
    reg.add_json(&theme_json("temp", "#FF010101", "T"), Source::User, "t")
        .unwrap();
    assert!(reg.select("temp"));
    assert_eq!(reg.current().backdrop_top, Colour::from_argb(0xFF01_0101));
    reg.remove_owner("t");
    assert_eq!(reg.active_id(), Some(ThemeRegistry::DEFAULT_ID));
    assert_eq!(
        reg.current().backdrop_top,
        ThemeTokens::builtin_dark().backdrop_top
    );
}

#[test]
fn editing_the_active_theme_reapplies_it() {
    let mut reg = ThemeRegistry::new();
    reg.add_json(&theme_json("live", "#FF010101", "L"), Source::User, "f")
        .unwrap();
    assert!(reg.select("live"));
    reg.add_json(&theme_json("live", "#FF020202", "L"), Source::User, "f")
        .unwrap();
    assert_eq!(reg.current().backdrop_top, Colour::from_argb(0xFF02_0202));
}

#[test]
fn code_can_add_themes_directly() {
    let mut reg = ThemeRegistry::new();
    let mut t = ThemeTokens::builtin_dark();
    t.gap = 20;
    reg.add(
        ThemeInfo {
            id: "code.theme".into(),
            name: "Code".into(),
            author: "dev".into(),
        },
        t,
        Source::Plugin,
        "my.plugin",
    );
    let entry = reg.find("code.theme").expect("added");
    assert_eq!(entry.tokens.gap, 20);
    assert_eq!(entry.owner, "my.plugin");
}

#[test]
fn plugin_themes_are_removed_with_their_owner() {
    let mut reg = ThemeRegistry::new();
    reg.add_json(
        &theme_json("plugin.sky", "#FF102030", "Sky"),
        Source::Plugin,
        "test.theme",
    )
    .unwrap();
    assert_eq!(
        reg.find("plugin.sky").expect("plugin theme").owner,
        "test.theme"
    );
    reg.remove_owner("test.theme");
    assert!(reg.find("plugin.sky").is_none());
}

#[test]
fn an_empty_palette_is_replaced_when_selected() {
    let mut reg = ThemeRegistry::new();
    let mut t = ThemeTokens::builtin_dark();
    t.accents.clear();
    reg.add(
        ThemeInfo {
            id: "bare".into(),
            name: "Bare".into(),
            author: String::new(),
        },
        t,
        Source::User,
        "b",
    );
    assert!(reg.select("bare"));
    assert_eq!(
        reg.current().accent_for(3),
        ThemeTokens::builtin_dark().accent_for(3)
    );
}

#[test]
fn session_override_ignores_an_empty_value() {
    assert_eq!(session_theme_override(Some("my.theme")), Some("my.theme"));
    assert_eq!(session_theme_override(Some("")), None);
    assert_eq!(session_theme_override(None), None);
}

#[test]
fn default_user_directory_is_under_config() {
    assert_eq!(
        default_user_directory(Some(OsStr::new("/home/u"))),
        Some(PathBuf::from("/home/u/.config/OpenMixer/themes"))
    );
    assert_eq!(default_user_directory(None), None);
}

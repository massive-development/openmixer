//! Themes compiled into the binary. The JSON files are the source of truth; a test checks the
//! built-in dark file against [`ThemeTokens::builtin_dark`].

/// `(file name, JSON)` for each built-in theme. The file name doubles as the owner.
pub const BUILTIN_THEMES: &[(&str, &str)] = &[
    (
        "openmixer-dark.json",
        include_str!("../../../../resources/themes/openmixer-dark.json"),
    ),
    (
        "openmixer-daylight.json",
        include_str!("../../../../resources/themes/openmixer-daylight.json"),
    ),
];

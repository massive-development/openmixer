//! Design tokens and the theme loader, registry and built-in themes.

mod builtin;
mod density;
mod json;
mod registry;
mod tokens;
mod typography;

pub use builtin::BUILTIN_THEMES;
pub use density::{Density, DensityMetrics};
pub use json::{
    parse_colour_hex, parse_theme, parse_theme_file, ParsedTheme, ThemeError, ThemeInfo,
};
pub use registry::{
    default_user_directory, session_theme_override, Entry, Source, ThemeRegistry, UserLoad,
};
pub use tokens::{
    builtin_wave_stops, Colour, ThemeTokens, WaveStop, BODY_FONT_FAMILY, DISPLAY_FONT_FAMILY,
    MONO_FONT_FAMILY, SPACE_SCALE,
};
pub use typography::{
    FontSlot, TypeRole, TypeStyle, MIN_DISPLAY_SIZE, MIN_LABEL_SIZE, MIN_PROSE_SIZE,
};

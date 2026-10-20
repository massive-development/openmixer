//! Start-up flags that need parsing beyond a plain match: `--settings[=TAB]`.

/// The tabs `--settings=TAB` accepts, as the overlay titles them.
const TABS: [&str; 5] = ["Audio", "MIDI", "Library", "Appearance", "Plugins"];

/// `--settings` opens the overlay on the first tab, `--settings=midi` on the named one (case is
/// ignored). `Ok(None)` when the flag is absent; `Err` names an unknown tab.
pub fn settings_tab(args: &[String]) -> Result<Option<&'static str>, String> {
    for arg in args {
        if arg == "--settings" {
            return Ok(Some(TABS[0]));
        }
        if let Some(name) = arg.strip_prefix("--settings=") {
            return TABS
                .iter()
                .find(|t| t.eq_ignore_ascii_case(name))
                .map(|t| Some(*t))
                .ok_or_else(|| {
                    format!(
                        "unknown settings tab {name:?} (audio, midi, library, appearance, plugins)"
                    )
                });
        }
    }
    Ok(None)
}

/// `--mock-stems` (or a non-empty `OPENMIXER_MOCK_STEMS`) attaches the stem service with the mock
/// separator: no model, no runtime. For demos and tests.
pub fn mock_stems(args: &[String]) -> bool {
    args.iter().any(|a| a == "--mock-stems")
        || std::env::var_os("OPENMIXER_MOCK_STEMS").is_some_and(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn settings_flag_parses_bare_named_and_absent() {
        assert_eq!(settings_tab(&args(&["--mock-device"])), Ok(None));
        assert_eq!(settings_tab(&args(&["--settings"])), Ok(Some("Audio")));
        assert_eq!(
            settings_tab(&args(&["--mock-device", "--settings=MIDI"])),
            Ok(Some("MIDI"))
        );
        assert_eq!(
            settings_tab(&args(&["--settings=appearance"])),
            Ok(Some("Appearance"))
        );
        assert!(settings_tab(&args(&["--settings=video"])).is_err());
    }

    #[test]
    fn mock_stems_flag_is_recognised() {
        assert!(mock_stems(&args(&["--mock-device", "--mock-stems"])));
        assert!(!mock_stems(&args(&["--mock-device"])));
    }
}

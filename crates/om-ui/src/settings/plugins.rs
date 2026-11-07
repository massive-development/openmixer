//! The plugins page: the plugin folder, loaded plugins and built-ins, and the generic parameter
//! panel every plugin gets (the fallback for a plugin whose own window cannot be shown).

use super::{SettingsError, SettingsResult};
use crate::preferences::Preferences;
use std::path::PathBuf;

/// One plugin parameter as the generic panel shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct PluginParam {
    pub id: String,
    pub name: String,
    pub min: f64,
    pub max: f64,
    pub default: f64,
    pub value: f64,
}

/// How many presses take a parameter across its whole range.
const PANEL_STEPS: f64 = 20.0;

impl PluginParam {
    fn step(&self) -> f64 {
        (self.max - self.min) / PANEL_STEPS
    }

    /// The value one press up (`direction > 0`) or down (`< 0`), kept inside the range.
    pub fn stepped(&self, direction: i32) -> f64 {
        let moved = self.value + self.step() * f64::from(direction.signum());
        moved.clamp(self.min, self.max)
    }

    /// Where the value sits in the range, 0 to 1 (0 for an empty range).
    pub fn fraction(&self) -> f64 {
        let span = self.max - self.min;
        if span <= 0.0 {
            return 0.0;
        }
        ((self.value - self.min) / span).clamp(0.0, 1.0)
    }

    /// The value as text: three significant decimals at most, no trailing zeros.
    pub fn display(&self) -> String {
        let text = format!("{:.3}", self.value);
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    }
}

/// What the plugins page needs. The app implements it over `om-plugins` (`openmixer::plugins`).
pub trait PluginHost {
    fn loaded_plugins(&self) -> Vec<String>;
    fn reload(&mut self) -> SettingsResult<()>;
    /// Built-in plugins that are not loaded now, as `(id, name)`.
    fn builtins(&self) -> Vec<(String, String)> {
        Vec::new()
    }
    fn load_builtin(&mut self, id: &str) -> SettingsResult<()> {
        Err(SettingsError::NoBuiltinPlugin { id: id.to_owned() })
    }
    fn unload(&mut self, id: &str) -> SettingsResult<()> {
        Err(SettingsError::PluginNotLoaded { id: id.to_owned() })
    }
    /// The parameters of loaded plugin `id` with their current values. Empty when it has none.
    fn plugin_parameters(&self, _id: &str) -> Vec<PluginParam> {
        Vec::new()
    }
    /// Sets parameter `param` of plugin `id`; the host clamps the value.
    fn set_plugin_parameter(&mut self, id: &str, _param: &str, _value: f64) -> SettingsResult<()> {
        Err(SettingsError::PluginNotLoaded { id: id.to_owned() })
    }
}

/// Plugins page: shows the folder in use and the loaded plugins, and reloads on request.
pub struct PluginsSettingsPage<'a> {
    prefs: &'a Preferences,
    host: &'a mut dyn PluginHost,
}

impl<'a> PluginsSettingsPage<'a> {
    pub fn new(prefs: &'a Preferences, host: &'a mut dyn PluginHost) -> Self {
        Self { prefs, host }
    }

    pub fn folder(&self) -> PathBuf {
        self.prefs.effective_plugin_folder()
    }

    pub fn loaded(&self) -> Vec<String> {
        self.host.loaded_plugins()
    }

    pub fn reload(&mut self) -> SettingsResult<()> {
        self.host.reload()
    }

    /// Built-in plugins that can be loaded, as `(id, name)`.
    pub fn builtins(&self) -> Vec<(String, String)> {
        self.host.builtins()
    }

    pub fn load_builtin(&mut self, id: &str) -> SettingsResult<()> {
        self.host.load_builtin(id)
    }

    pub fn unload(&mut self, id: &str) -> SettingsResult<()> {
        self.host.unload(id)
    }

    pub fn parameters(&self, id: &str) -> Vec<PluginParam> {
        self.host.plugin_parameters(id)
    }

    pub fn set_parameter(&mut self, id: &str, param: &str, value: f64) -> SettingsResult<()> {
        self.host.set_plugin_parameter(id, param, value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preferences::Preferences;

    #[derive(Default)]
    struct Host {
        loaded: Vec<String>,
        reloads: usize,
    }

    impl PluginHost for Host {
        fn loaded_plugins(&self) -> Vec<String> {
            self.loaded.clone()
        }
        fn reload(&mut self) -> SettingsResult<()> {
            self.reloads += 1;
            Ok(())
        }
    }

    #[test]
    fn plugins_show_the_default_folder_and_what_is_loaded_and_reload_on_request() {
        let prefs = Preferences::default();
        let mut host = Host {
            loaded: vec!["gain".into()],
            reloads: 0,
        };
        let mut page = PluginsSettingsPage::new(&prefs, &mut host);
        assert_eq!(page.folder(), crate::preferences::default_plugin_folder());
        assert_eq!(page.loaded(), vec!["gain".to_owned()]);
        page.reload().unwrap();
        assert_eq!(host.reloads, 1);
    }

    fn param(value: f64) -> PluginParam {
        PluginParam {
            id: "1".into(),
            name: "Gain".into(),
            min: 0.0,
            max: 2.0,
            default: 1.0,
            value,
        }
    }

    #[test]
    fn a_press_moves_a_twentieth_of_the_range_and_stops_at_the_ends() {
        assert!((param(1.0).stepped(1) - 1.1).abs() < 1e-9);
        assert!((param(1.0).stepped(-3) - 0.9).abs() < 1e-9);
        assert_eq!(param(2.0).stepped(1), 2.0);
        assert_eq!(param(0.0).stepped(-1), 0.0);
    }

    #[test]
    fn fraction_and_display_are_safe_for_odd_ranges() {
        assert_eq!(param(0.5).fraction(), 0.25);
        let flat = PluginParam {
            min: 1.0,
            max: 1.0,
            ..param(1.0)
        };
        assert_eq!(flat.fraction(), 0.0);
        assert_eq!(flat.stepped(1), 1.0);
        assert_eq!(param(0.5).display(), "0.5");
        assert_eq!(param(1.0).display(), "1");
        assert_eq!(param(0.12345).display(), "0.123");
    }
}

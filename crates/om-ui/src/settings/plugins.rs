//! The plugins page: the plugin folder, loaded plugins and built-ins.

use super::{SettingsError, SettingsResult};
use crate::preferences::Preferences;
use std::path::PathBuf;

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
}

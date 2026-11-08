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

/// The latency a plugin reports, as the Plugins page shows it. Display only: the engine does no
/// delay compensation (ADR 0007).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PluginLatency {
    pub frames: u32,
    /// The rate the stage runs at; 0 when unknown (then only frames are shown).
    pub sample_rate: u32,
}

impl PluginLatency {
    /// Frames and milliseconds, for example `128 fr / 2.9 ms`.
    pub fn text(&self) -> String {
        if self.sample_rate == 0 {
            return format!("{} fr", self.frames);
        }
        let ms = f64::from(self.frames) * 1000.0 / f64::from(self.sample_rate);
        format!("{} fr / {ms:.1} ms", self.frames)
    }

    /// The text with its label, for a row or a panel header.
    pub fn labelled(&self) -> String {
        format!("Latency {}", self.text())
    }
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

/// The kind of windows the app shows, which decides how a plugin's window can appear.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginSession {
    X11,
    Wayland,
    Other,
}

/// How a plugin's own window will be shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginWindowMode {
    /// A child of a window of ours (X11).
    Embedded,
    /// The plugin's own top-level window.
    Floating,
    /// Not showable here: use the generic parameter panel.
    Parameters,
}

/// A loaded plugin's own window, driven by the app's window layer. `parent` is an X11 window id.
/// The caller must call `close` before it destroys the parent window.
pub trait PluginWindow: Send + Sync {
    fn mode(&self, session: PluginSession) -> PluginWindowMode;
    /// Opens the window; returns its size when the plugin reports one.
    fn open(
        &self,
        mode: PluginWindowMode,
        parent: Option<u64>,
        scale: f64,
    ) -> SettingsResult<Option<(u32, u32)>>;
    fn resize(&self, width: u32, height: u32) -> SettingsResult<()>;
    /// A size the embedded plugin asked for since the last call.
    fn take_resize_request(&self) -> Option<(u32, u32)>;
    fn close(&self);
}

/// What the plugins page shows about CLAP plugins (ADR 0007). Without CLAP support in the build
/// `supported` is false and the page and the prompts say nothing about it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClapStatus {
    pub supported: bool,
    /// The standard folders a scan would add, for the first-run prompt.
    pub standard_folders: Vec<String>,
    /// The `.clap` files found in the folders that are scanned now, as paths.
    pub files: Vec<String>,
    /// How many of those wait for the first-load warning to be accepted.
    pub waiting: usize,
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
    /// The latency loaded plugin `id` reports now. `None` when it reports none. Cheap (atomics),
    /// so the page asks on every draw and follows a latency change.
    fn plugin_latency(&self, _id: &str) -> Option<PluginLatency> {
        None
    }
    /// CLAP support, the `.clap` files found and how many wait for the first-load warning.
    fn clap_status(&self) -> ClapStatus {
        ClapStatus::default()
    }
    /// The own window of loaded plugin `id`, when it has one.
    fn plugin_window(&self, _id: &str) -> Option<std::sync::Arc<dyn PluginWindow>> {
        None
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

    pub fn clap_status(&self) -> ClapStatus {
        self.host.clap_status()
    }

    pub fn window(&self, id: &str) -> Option<std::sync::Arc<dyn PluginWindow>> {
        self.host.plugin_window(id)
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

    #[test]
    fn latency_reads_as_frames_and_milliseconds_at_the_current_rate() {
        let at = |frames, sample_rate| PluginLatency {
            frames,
            sample_rate,
        };
        assert_eq!(at(128, 44_100).text(), "128 fr / 2.9 ms");
        assert_eq!(at(16, 48_000).text(), "16 fr / 0.3 ms");
        assert_eq!(at(0, 48_000).text(), "0 fr / 0.0 ms");
        assert_eq!(at(480, 48_000).text(), "480 fr / 10.0 ms");
        assert_eq!(at(128, 0).text(), "128 fr", "no rate, no milliseconds");
        assert_eq!(at(128, 44_100).labelled(), "Latency 128 fr / 2.9 ms");
    }

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

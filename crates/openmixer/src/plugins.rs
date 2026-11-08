//! Plugins in the app: how the preferences load them (the [`PluginSink`] the targets call), the
//! built-in plugins the Plugins page can load, and what that page sees
//! ([`om_ui::settings::PluginHost`]). The registry itself is [`om_plugins::PluginRegistry`], owned
//! by [`crate::composition::AppComposition`].

use crate::app_targets::{PluginLoad, PluginSink, PluginTheme};
use crate::composition::AppComposition;
use gain_plugin::GainPlugin;
use om_plugins::{Plugin, PluginRegistry};
use om_ui::settings::SettingsError;
use om_ui::settings::{PluginHost, PluginLatency, PluginParam, SettingsResult};
#[cfg(feature = "clap")]
use om_ui::settings::{PluginSession, PluginWindow, PluginWindowMode};
use om_ui::theme::{Source, ThemeRegistry};
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

/// A plugin compiled into the app. It is not loaded at start-up: the Plugins page loads and
/// unloads it.
pub struct BuiltinPlugin {
    pub id: String,
    pub name: String,
    make: fn() -> Box<dyn Plugin>,
}

impl BuiltinPlugin {
    /// A new instance, ready for [`PluginRegistry::add_plugin`].
    pub fn create(&self) -> Box<dyn Plugin> {
        (self.make)()
    }
}

/// Every built-in plugin, with the id and name it reports.
pub fn builtin_catalog() -> Vec<BuiltinPlugin> {
    let factories: [fn() -> Box<dyn Plugin>; 1] = [|| Box::new(GainPlugin::new())];
    factories
        .into_iter()
        .map(|make| {
            let info = make().info();
            BuiltinPlugin {
                id: info.id,
                name: info.name,
                make,
            }
        })
        .collect()
}

/// Makes the registry's plugin themes the window's [`Source::Plugin`] themes, replacing the ones
/// there. A theme the registry refuses is reported on stderr and skipped.
pub fn sync_plugin_themes(themes: &mut ThemeRegistry, plugins: &PluginRegistry) {
    themes.remove_source(Source::Plugin);
    for theme in plugins.themes() {
        if let Err(error) = themes.add_json(&theme.json, Source::Plugin, &theme.owner) {
            eprintln!("openmixer: theme from {}: {error}", theme.owner);
        }
    }
}

impl AppComposition {
    /// Forwards the registry's plugin themes to the composition's own theme registry.
    pub fn refresh_plugin_themes(&mut self) {
        sync_plugin_themes(&mut self.themes, &self.plugins);
    }
}

/// The Plugins page's view of the composition. The window and the page share the composition,
/// so the page's loads and unloads reach the plugins the rest of the app uses.
pub struct PluginsPage(pub Rc<RefCell<AppComposition>>);

impl PluginHost for PluginsPage {
    fn loaded_plugins(&self) -> Vec<String> {
        PluginHost::loaded_plugins(&*self.0.borrow())
    }

    fn reload(&mut self) -> SettingsResult<()> {
        PluginHost::reload(&mut *self.0.borrow_mut())
    }

    fn builtins(&self) -> Vec<(String, String)> {
        PluginHost::builtins(&*self.0.borrow())
    }

    fn load_builtin(&mut self, id: &str) -> SettingsResult<()> {
        PluginHost::load_builtin(&mut *self.0.borrow_mut(), id)
    }

    fn unload(&mut self, id: &str) -> SettingsResult<()> {
        PluginHost::unload(&mut *self.0.borrow_mut(), id)
    }

    fn plugin_parameters(&self, id: &str) -> Vec<PluginParam> {
        PluginHost::plugin_parameters(&*self.0.borrow(), id)
    }

    fn plugin_latency(&self, id: &str) -> Option<PluginLatency> {
        PluginHost::plugin_latency(&*self.0.borrow(), id)
    }

    #[cfg(feature = "clap")]
    fn clap_status(&self) -> om_ui::settings::ClapStatus {
        PluginHost::clap_status(&*self.0.borrow())
    }

    #[cfg(feature = "clap")]
    fn plugin_window(&self, id: &str) -> Option<std::sync::Arc<dyn PluginWindow>> {
        PluginHost::plugin_window(&*self.0.borrow(), id)
    }

    fn set_plugin_parameter(&mut self, id: &str, param: &str, value: f64) -> SettingsResult<()> {
        PluginHost::set_plugin_parameter(&mut *self.0.borrow_mut(), id, param, value)
    }
}

impl PluginSink for PluginRegistry {
    fn unload_all(&mut self) {
        PluginRegistry::unload_all(self);
    }

    #[cfg(feature = "clap")]
    fn set_policy(&mut self, plugins: &om_ui::preferences::PluginPrefs) {
        self.set_clap_policy(
            plugins.scan_standard_folders(),
            plugins.clap_warning_accepted,
        );
    }

    /// Loads the shared libraries in `folder` (C-ABI plugins, see `om_plugins::PluginLoader`) and
    /// the built-in plugins. A library that fails to load is reported in the errors.
    fn load_folder(&mut self, folder: &Path) -> PluginLoad {
        let mut errors = Vec::new();
        #[allow(unused_mut)]
        let mut loaded = self.load_directory(folder, &mut errors);
        // `.clap` files in the folder (and the standard folders, if the user allowed them). Listed
        // always; opened only after the first-load warning was accepted.
        #[cfg(feature = "clap")]
        {
            let home = dirs::home_dir();
            loaded += self.load_clap_folders(folder, home.as_deref(), &mut errors);
        }
        PluginLoad {
            loaded,
            errors,
            themes: self
                .themes()
                .iter()
                .map(|t| PluginTheme {
                    json: t.json.clone(),
                    owner: t.owner.clone(),
                })
                .collect(),
        }
    }
}

/// A CLAP plugin's own window as the Plugins page sees it.
#[cfg(feature = "clap")]
struct ClapWindow(om_plugins::clap::PluginGui);

#[cfg(feature = "clap")]
impl ClapWindow {
    fn session(session: PluginSession) -> om_plugins::clap::GuiSession {
        use om_plugins::clap::GuiSession;
        match session {
            PluginSession::X11 => GuiSession::X11,
            PluginSession::Wayland => GuiSession::Wayland,
            PluginSession::Other => GuiSession::Other,
        }
    }

    fn plan(
        mode: PluginWindowMode,
        session_api: om_plugins::clap::GuiApi,
    ) -> om_plugins::clap::GuiPlan {
        use om_plugins::clap::GuiPlan;
        match mode {
            PluginWindowMode::Embedded => GuiPlan::Embedded(session_api),
            PluginWindowMode::Floating => GuiPlan::Floating(session_api),
            PluginWindowMode::Parameters => GuiPlan::Parameters,
        }
    }
}

#[cfg(feature = "clap")]
impl PluginWindow for ClapWindow {
    fn mode(&self, session: PluginSession) -> PluginWindowMode {
        use om_plugins::clap::GuiPlan;
        match self.0.plan(Self::session(session)) {
            GuiPlan::Embedded(_) => PluginWindowMode::Embedded,
            GuiPlan::Floating(_) => PluginWindowMode::Floating,
            GuiPlan::Parameters => PluginWindowMode::Parameters,
        }
    }

    fn open(
        &self,
        mode: PluginWindowMode,
        parent: Option<u64>,
        scale: f64,
    ) -> SettingsResult<Option<(u32, u32)>> {
        use om_plugins::clap::{GuiApi, ParentWindow};
        // The window API follows the session: an X11 parent means X11, no parent means Wayland.
        let api = if parent.is_some() {
            GuiApi::X11
        } else {
            GuiApi::Wayland
        };
        self.0
            .open(Self::plan(mode, api), parent.map(ParentWindow::X11), scale)
            .map_err(|e| SettingsError::Plugin(Box::new(e)))
    }

    fn resize(&self, width: u32, height: u32) -> SettingsResult<()> {
        self.0
            .resize(width, height)
            .map_err(|e| SettingsError::Plugin(Box::new(e)))
    }

    fn take_resize_request(&self) -> Option<(u32, u32)> {
        self.0.take_resize_request()
    }

    fn close(&self) {
        self.0.close();
    }
}

/// The Plugins page talks to the composition: it lists the loaded plugins and reloads the folder.
impl PluginHost for AppComposition {
    fn loaded_plugins(&self) -> Vec<String> {
        self.plugins.plugin_ids()
    }

    fn reload(&mut self) -> SettingsResult<()> {
        self.reload_plugins();
        Ok(())
    }

    fn builtins(&self) -> Vec<(String, String)> {
        let loaded = self.plugins.plugin_ids();
        builtin_catalog()
            .into_iter()
            .filter(|b| !loaded.contains(&b.id))
            .map(|b| (b.id, b.name))
            .collect()
    }

    fn load_builtin(&mut self, id: &str) -> SettingsResult<()> {
        let builtin = builtin_catalog()
            .into_iter()
            .find(|b| b.id == id)
            .ok_or_else(|| SettingsError::NoBuiltinPlugin { id: id.to_owned() })?;
        self.plugins
            .add_plugin(builtin.create())
            .map_err(|e| SettingsError::Plugin(Box::new(e)))?;
        self.refresh_plugin_themes();
        Ok(())
    }

    fn unload(&mut self, id: &str) -> SettingsResult<()> {
        if !self.plugins.unload(id) {
            return Err(SettingsError::PluginNotLoaded { id: id.to_owned() });
        }
        self.persist_plugin_states();
        self.refresh_plugin_themes();
        Ok(())
    }

    fn plugin_latency(&self, id: &str) -> Option<PluginLatency> {
        let report = self.plugins.plugin_latency(id)?;
        Some(PluginLatency {
            frames: report.frames,
            sample_rate: report.sample_rate,
        })
    }

    #[cfg(feature = "clap")]
    fn clap_status(&self) -> om_ui::settings::ClapStatus {
        let home = dirs::home_dir();
        om_ui::settings::ClapStatus {
            supported: true,
            standard_folders: om_plugins::clap::ClapSearch::standard_paths(home.as_deref())
                .iter()
                .map(|p| p.display().to_string())
                .collect(),
            files: self
                .plugins
                .clap_files_found()
                .iter()
                .map(|p| p.display().to_string())
                .collect(),
            waiting: self.plugins.clap_files_waiting().len(),
        }
    }

    #[cfg(feature = "clap")]
    fn plugin_window(&self, id: &str) -> Option<std::sync::Arc<dyn PluginWindow>> {
        let gui = self.plugins.clap_gui(id)?;
        Some(std::sync::Arc::new(ClapWindow(gui)))
    }

    fn plugin_parameters(&self, id: &str) -> Vec<PluginParam> {
        self.plugins
            .plugin_parameters(id)
            .into_iter()
            .map(|p| PluginParam {
                value: self
                    .plugins
                    .plugin_parameter(id, &p.id)
                    .unwrap_or(p.default),
                id: p.id,
                name: p.name,
                min: p.min,
                max: p.max,
                default: p.default,
            })
            .collect()
    }

    fn set_plugin_parameter(&mut self, id: &str, param: &str, value: f64) -> SettingsResult<()> {
        if !self.plugins.plugin_ids().iter().any(|p| p == id) {
            return Err(SettingsError::PluginNotLoaded { id: id.to_owned() });
        }
        self.plugins.set_plugin_parameter(id, param, value);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use om_core::ParameterTarget;
    use om_midi::MockBackend;
    use om_plugins::{InsertAt, Plugin, PluginInfo, ProcessStage, StageKind};
    use om_ui::theme::Source;
    use std::sync::Arc;

    const ID: &str = "test.demo";

    struct Silence;

    impl ProcessStage for Silence {
        fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
            left.fill(0.0);
            right.fill(0.0);
        }
    }

    struct Knob;

    impl ParameterTarget for Knob {
        fn set_parameter(&self, id: &str, _value: f64) -> bool {
            id == "level"
        }
    }

    /// Registers one of each kind, the way a real plugin would in `on_load`.
    struct DemoPlugin;

    impl Plugin for DemoPlugin {
        fn info(&self) -> PluginInfo {
            PluginInfo {
                id: ID.into(),
                name: "Demo".into(),
                version: "1".into(),
            }
        }

        fn on_load(&mut self, host: &mut dyn om_plugins::PluginHost) {
            host.register_stage(
                "demo.silence",
                StageKind::PostProcess,
                Box::new(|| Box::new(Silence)),
            )
            .expect("stage");
            host.register_parameter_target("demo", Arc::new(Knob))
                .expect("target");
            host.register_midi_map(
                "demo.map",
                r#"{ "mappings": [ { "type": "cc", "number": 3, "target": "demo.level" } ] }"#,
            )
            .expect("map");
            host.register_theme(r#"{ "id": "demo.theme", "name": "Demo" }"#)
                .expect("theme");
        }
    }

    fn composition() -> AppComposition {
        let dir = tempfile::tempdir().expect("temp dir");
        // A prefs file that does not exist: the defaults apply.
        AppComposition::new(&dir.path().join("prefs.json"), None)
    }

    #[test]
    fn an_in_process_plugin_reaches_the_router_engine_midi_and_themes() {
        let mut app = composition();
        app.plugins.add_plugin(Box::new(DemoPlugin)).expect("loads");

        assert!(app.router.set_parameter("demo.level", 0.5));
        app.attach_plugin_stage("demo.silence", InsertAt::Master, 0)
            .expect("attaches");
        assert!(app.plugins.themes().iter().any(|t| t.id == "demo.theme"));

        assert!(
            app.apply_plugin_midi_map("demo.map").is_err(),
            "no MIDI service yet"
        );
        app.attach_midi(Box::new(MockBackend::new()));
        app.apply_plugin_midi_map("demo.map").expect("applies");
        assert_eq!(
            app.services
                .midi
                .as_ref()
                .expect("midi")
                .mapper()
                .num_mappings(),
            1
        );

        app.detach_plugin_stage(InsertAt::Master, 0);
        app.plugins.unload(ID);
        assert!(!app.router.set_parameter("demo.level", 0.5));
        assert!(app.plugins.stage_ids(None).is_empty());
        assert!(app.plugins.themes().is_empty());
    }

    #[test]
    fn a_folder_load_reports_shared_libraries_it_cannot_load_and_forwards_no_themes() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join("gain.so"), "x").expect("write");
        std::fs::write(dir.path().join("notes.txt"), "x").expect("write");
        let mut registry = PluginRegistry::new();

        let loaded = PluginSink::load_folder(&mut registry, dir.path());
        assert_eq!(loaded.loaded, 0);
        assert_eq!(
            loaded.errors.len(),
            1,
            "only the library, not the text file"
        );
        assert!(loaded.errors[0].contains("gain.so"), "{:?}", loaded.errors);
        assert!(loaded.themes.is_empty());
    }

    /// `.clap` files are listed by the folder load and opened only after the first-load warning was
    /// accepted (a broken file shows up as an error exactly when it is opened).
    #[cfg(feature = "clap")]
    #[test]
    fn clap_files_in_the_plugin_folder_wait_for_the_warning_then_load() {
        use om_ui::preferences::{ClapScan, PluginPrefs};
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join("broken.clap"), "x").expect("write");
        let mut app = composition();

        let load = PluginSink::load_folder(&mut app.plugins, dir.path());
        assert_eq!(
            (load.loaded, load.errors.len()),
            (0, 0),
            "{:?}",
            load.errors
        );
        let status = PluginHost::clap_status(&app);
        assert!(status.supported);
        assert_eq!(status.files.len(), 1);
        assert!(status.files[0].ends_with("broken.clap"));
        assert_eq!(status.waiting, 1);
        assert!(!status.standard_folders.is_empty());

        PluginSink::set_policy(
            &mut app.plugins,
            &PluginPrefs {
                clap_scan: ClapScan::Declined,
                clap_warning_accepted: true,
            },
        );
        let load = PluginSink::load_folder(&mut app.plugins, dir.path());
        assert_eq!(load.errors.len(), 1, "opened now: {:?}", load.errors);
        assert!(load.errors[0].contains("broken.clap"));
        assert_eq!(PluginHost::clap_status(&app).waiting, 0);
    }

    #[test]
    fn plugin_themes_reach_the_theme_registry_as_plugin_source() {
        let mut app = composition();
        app.plugins.add_plugin(Box::new(DemoPlugin)).expect("loads");
        let load = PluginSink::load_folder(&mut app.plugins, Path::new("/nonexistent/plugins"));
        assert_eq!(
            load.loaded, 0,
            "a folder that does not exist has no libraries"
        );
        app.themes.remove_source(Source::Plugin);
        for theme in app.plugins.themes().to_vec() {
            app.themes
                .add_json(&theme.json, Source::Plugin, &theme.owner)
                .expect("valid theme");
        }
        let entry = app.themes.find("demo.theme").expect("forwarded");
        assert_eq!(entry.source, Source::Plugin);
        assert_eq!(entry.owner, ID);
    }

    /// A plugin with state: one counter, saved as its bytes.
    struct Stateful(std::sync::Mutex<u8>);

    impl Plugin for Stateful {
        fn info(&self) -> PluginInfo {
            PluginInfo {
                id: "test.stateful".into(),
                name: "Stateful".into(),
                version: "1".into(),
            }
        }

        fn on_load(&mut self, _host: &mut dyn om_plugins::PluginHost) {}

        fn save_state(&self) -> Result<Option<Vec<u8>>, om_plugins::PluginError> {
            Ok(Some(vec![*self.0.lock().unwrap()]))
        }

        fn restore_state(&self, state: &[u8]) -> Result<(), om_plugins::PluginError> {
            *self.0.lock().unwrap() = state.first().copied().unwrap_or(0);
            Ok(())
        }
    }

    #[test]
    fn plugin_state_is_saved_on_unload_and_restored_in_the_next_session() {
        let dir = tempfile::tempdir().expect("temp dir");
        let prefs = dir.path().join("prefs.json");
        let mut first = AppComposition::new(&prefs, None);
        first
            .plugins
            .add_plugin(Box::new(Stateful(std::sync::Mutex::new(7))))
            .expect("loads");
        PluginHost::unload(&mut first, "test.stateful").expect("unloads");
        let file = dir.path().join("plugin-state.json");
        assert!(
            file.is_file(),
            "unloading wrote the state beside the preferences"
        );
        drop(first);

        let mut second = AppComposition::new(&prefs, None);
        let plugin = Stateful(std::sync::Mutex::new(0));
        second.plugins.add_plugin(Box::new(plugin)).expect("loads");
        let states = second.plugins.plugin_states();
        assert_eq!(states.get("test.stateful"), Some(&vec![7]));
    }

    #[test]
    fn the_plugins_page_lists_what_is_loaded_and_reload_keeps_working() {
        let mut app = composition();
        app.plugins.add_plugin(Box::new(DemoPlugin)).expect("loads");
        assert_eq!(PluginHost::loaded_plugins(&app), vec![ID.to_owned()]);
        PluginHost::reload(&mut app).expect("reload");
        // The folder was never applied, so the reload loads nothing and removes nothing.
        assert_eq!(PluginHost::loaded_plugins(&app), vec![ID.to_owned()]);
    }

    #[test]
    fn a_built_in_plugin_loads_and_unloads_through_the_page_with_its_theme() {
        let shared = Rc::new(RefCell::new(composition()));
        let mut page = PluginsPage(Rc::clone(&shared));
        let gain = "org.openmixer.example.gain";
        assert!(PluginHost::builtins(&page).iter().any(|(id, _)| id == gain));

        PluginHost::load_builtin(&mut page, gain).expect("loads");
        assert_eq!(PluginHost::loaded_plugins(&page), vec![gain.to_owned()]);
        assert!(!PluginHost::builtins(&page).iter().any(|(id, _)| id == gain));
        assert!(shared
            .borrow()
            .themes
            .find("org.openmixer.example.theme")
            .is_some_and(|e| e.source == Source::Plugin));

        PluginHost::unload(&mut page, gain).expect("unloads");
        assert!(PluginHost::loaded_plugins(&page).is_empty());
        assert!(shared
            .borrow()
            .themes
            .find("org.openmixer.example.theme")
            .is_none());
        assert!(PluginHost::unload(&mut page, gain).is_err());
        assert!(PluginHost::load_builtin(&mut page, "no.such").is_err());
    }
}

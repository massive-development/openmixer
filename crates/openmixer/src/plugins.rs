//! Plugins in the app: how the preferences load them (the [`PluginSink`] the targets call), the
//! built-in plugins the Plugins page can load, and what that page sees
//! ([`om_ui::settings::PluginHost`]). The registry itself is [`om_plugins::PluginRegistry`], owned
//! by [`crate::composition::AppComposition`].

use crate::app_targets::{PluginLoad, PluginSink, PluginTheme};
use crate::composition::AppComposition;
use gain_plugin::GainPlugin;
use om_plugins::{Plugin, PluginRegistry};
use om_ui::settings::SettingsError;
use om_ui::settings::{PluginHost, SettingsResult};
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
}

impl PluginSink for PluginRegistry {
    fn unload_all(&mut self) {
        PluginRegistry::unload_all(self);
    }

    /// Loads the shared libraries in `folder` and the built-in plugins. Shared libraries are
    /// reported as errors until dynamic loading is decided (see `om_plugins::PluginLoader`).
    fn load_folder(&mut self, folder: &Path) -> PluginLoad {
        let mut errors = Vec::new();
        let loaded = self.load_directory(folder, &mut errors);
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
        self.refresh_plugin_themes();
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
        assert!(loaded.errors[0].contains("not supported"));
        assert!(loaded.themes.is_empty());
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

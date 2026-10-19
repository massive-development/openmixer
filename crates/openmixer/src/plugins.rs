//! Plugins in the app: how the preferences load them (the [`PluginSink`] the targets call) and what
//! the Plugins settings page sees ([`om_ui::settings::PluginHost`]). The registry itself is
//! [`om_plugins::PluginRegistry`], owned by [`crate::composition::AppComposition`].

use crate::app_targets::{PluginLoad, PluginSink, PluginTheme};
use crate::composition::AppComposition;
use om_plugins::PluginRegistry;
use om_ui::settings::{PluginHost, SettingsResult};
use std::path::Path;

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
        assert_eq!(app.midi.as_ref().expect("midi").mapper().num_mappings(), 1);

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
        assert_eq!(loaded.errors.len(), 1, "only the library, not the text file");
        assert!(loaded.errors[0].contains("not supported"));
        assert!(loaded.themes.is_empty());
    }

    #[test]
    fn plugin_themes_reach_the_theme_registry_as_plugin_source() {
        let mut app = composition();
        app.plugins.add_plugin(Box::new(DemoPlugin)).expect("loads");
        let load = PluginSink::load_folder(&mut app.plugins, Path::new("/nonexistent/plugins"));
        assert_eq!(load.loaded, 0, "a folder that does not exist has no libraries");
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
}

//! The Plugins tab: loaded plugins and the built-ins.

use super::chips::*;
use super::*;

pub(super) fn plugins_page(
    t: &ThemeTokens,
    settings: &SettingsState,
    cx: &mut Context<Shell>,
) -> Div {
    let mut loaded = div().flex().flex_col();
    if settings.lists.loaded_plugins.is_empty() {
        loaded = loaded.child(note(t, "No plugin is loaded."));
    }
    for id in &settings.lists.loaded_plugins {
        let (press, toggle) = (id.clone(), id.clone());
        let open = settings.lists.open_params.as_deref() == Some(id.as_str());
        loaded = loaded.child(
            div()
                .flex()
                .justify_between()
                .items_center()
                .mb(px(4.))
                .child(div().flex_1().overflow_hidden().child(id.clone()))
                .child(chip(t, "Parameters", open, cx, move |this| {
                    let id = toggle.clone();
                    this.settings_mut(|s| s.plugin_toggle_params(&id));
                }))
                .child(chip(t, "Unload", false, cx, move |this| {
                    let id = press.clone();
                    this.settings_mut(|s| s.plugin_unload(&id));
                    this.refresh_theme();
                })),
        );
        if open {
            loaded = loaded.child(param_panel(t, id, &settings.lists.params, cx));
        }
    }

    let mut builtins = div().flex().flex_col();
    if settings.lists.builtins.is_empty() {
        builtins = builtins.child(note(t, "No built-in plugin to load."));
    }
    for (id, name) in &settings.lists.builtins {
        let press = id.clone();
        builtins = builtins.child(
            div()
                .flex()
                .justify_between()
                .items_center()
                .mb(px(4.))
                .child(
                    div()
                        .flex_1()
                        .overflow_hidden()
                        .child(format!("{name} ({id})")),
                )
                .child(chip(t, "Load", false, cx, move |this| {
                    let id = press.clone();
                    this.settings_mut(|s| s.plugin_load_builtin(&id));
                    this.refresh_theme();
                })),
        );
    }

    div()
        .flex()
        .flex_col()
        .child(choice_row(t, "Loaded plugins", loaded))
        .child(choice_row(t, "Built-in plugins", builtins))
        .child(choice_row(
            t,
            "Folder",
            row_of(vec![chip(t, "Reload the plugin folder", false, cx, |this| {
                this.settings_mut(|s| s.plugin_reload());
                this.refresh_theme();
            })]),
        ))
        .child(note(
            t,
            &format!(
                "Plugin folder: {}. Shared-library (C ABI) plugins in it are loaded; any that fail are listed with the reason. {} plugin theme(s) are in Appearance.",
                settings.prefs.effective_plugin_folder().display(),
                settings.plugin_theme_count()
            ),
        ))
}

/// The generic parameter panel: one row per parameter with the value, down and up steps and a reset.
/// This is the fallback for a plugin whose own window cannot be shown (ADR 0007).
fn param_panel(
    t: &ThemeTokens,
    plugin: &str,
    params: &[PluginParam],
    cx: &mut Context<Shell>,
) -> Div {
    let mut panel = div().flex().flex_col().mb(px(8.));
    if params.is_empty() {
        return panel.child(note(t, "This plugin has no parameters."));
    }
    for p in params {
        let ids = (plugin.to_owned(), p.id.clone());
        let step = |direction: i32, label: &str, cx: &mut Context<Shell>| {
            let (plugin, param) = ids.clone();
            chip(t, label, false, cx, move |this| {
                this.settings_mut(|s| s.plugin_step_param(&plugin, &param, direction));
            })
        };
        let reset = {
            let (plugin, param) = ids.clone();
            chip(t, "Reset", false, cx, move |this| {
                this.settings_mut(|s| s.plugin_reset_param(&plugin, &param));
            })
        };
        panel = panel.child(
            div()
                .flex()
                .justify_between()
                .items_center()
                .child(div().flex_1().overflow_hidden().child(p.name.clone()))
                .child(step(-1, "-", cx))
                .child(div().w(px(64.)).flex().justify_center().child(p.display()))
                .child(step(1, "+", cx))
                .child(reset),
        );
    }
    panel
}

impl SettingsState {
    /// Opens the generic parameter panel of plugin `id`, or closes it when it is the open one.
    pub fn plugin_toggle_params(&mut self, id: &str) {
        let open = self.lists.open_params.as_deref() == Some(id);
        self.lists.open_params = (!open).then(|| id.to_owned());
        self.refresh_lists();
    }

    /// One press of a parameter's down (`direction < 0`) or up (`> 0`) button.
    pub fn plugin_step_param(&mut self, plugin: &str, param: &str, direction: i32) {
        let Some(p) = self.lists.params.iter().find(|p| p.id == param) else {
            return;
        };
        let value = p.stepped(direction);
        self.set_plugin_param(plugin, param, value);
    }

    /// Puts a parameter back to its default.
    pub fn plugin_reset_param(&mut self, plugin: &str, param: &str) {
        let Some(p) = self.lists.params.iter().find(|p| p.id == param) else {
            return;
        };
        let default = p.default;
        self.set_plugin_param(plugin, param, default);
    }

    fn set_plugin_param(&mut self, plugin: &str, param: &str, value: f64) {
        match self.with_plugins_page(|p| p.set_parameter(plugin, param, value)) {
            Ok(()) => self.refresh_lists(),
            Err(error) => self.set_notice(error.to_string(), true),
        }
    }

    /// Runs `f` on the Plugins page over the plugin host. Without a host the page says so.
    pub(super) fn with_plugins_page<T>(
        &mut self,
        f: impl FnOnce(&mut PluginsSettingsPage<'_>) -> SettingsResult<T>,
    ) -> SettingsResult<T> {
        let Some(host) = self.plugins.as_deref_mut() else {
            return Err(SettingsError::PluginsUnavailable);
        };
        let mut page = PluginsSettingsPage::new(&self.prefs, host);
        f(&mut page)
    }

    /// After a plugin change: the registry's plugin themes follow, the lists are read again, and
    /// the page says what happened.
    pub(super) fn plugin_changed(&mut self, result: SettingsResult<()>, done: String) {
        match result {
            Ok(()) => {
                self.sync_plugin_themes();
                self.refresh_lists();
                self.set_notice(done, false);
            }
            Err(error) => self.set_notice(error.to_string(), true),
        }
    }

    pub fn plugin_load_builtin(&mut self, id: &str) {
        let result = self.with_plugins_page(|p| p.load_builtin(id));
        self.plugin_changed(result, format!("Loaded {id}"));
    }

    pub fn plugin_unload(&mut self, id: &str) {
        let result = self.with_plugins_page(|p| p.unload(id));
        self.plugin_changed(result, format!("Unloaded {id}"));
    }

    pub fn plugin_reload(&mut self) {
        let result = self.with_plugins_page(|p| p.reload());
        self.plugin_changed(result, "Reloaded the plugin folder".to_owned());
    }

    /// How many themes the plugins provide (they appear in the Appearance page).
    pub(super) fn plugin_theme_count(&self) -> usize {
        self.themes
            .themes()
            .iter()
            .filter(|e| e.source == Source::Plugin)
            .count()
    }

    // Text field.
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings_view::fixtures::*;
    use om_ui::settings::{PluginHost, SettingsResult};
    use std::cell::Cell;

    use std::rc::Rc;

    /// Plugins that can be loaded and unloaded by id.
    #[derive(Default)]
    struct FakePlugins {
        loaded: std::cell::RefCell<Vec<String>>,
        gain: std::cell::Cell<f64>,
    }

    impl PluginHost for FakePlugins {
        fn loaded_plugins(&self) -> Vec<String> {
            self.loaded.borrow().clone()
        }
        fn reload(&mut self) -> SettingsResult<()> {
            Ok(())
        }
        fn builtins(&self) -> Vec<(String, String)> {
            [("demo.a", "Demo A")]
                .into_iter()
                .filter(|(id, _)| !self.loaded.borrow().iter().any(|l| l == id))
                .map(|(id, name)| (id.to_owned(), name.to_owned()))
                .collect()
        }
        fn load_builtin(&mut self, id: &str) -> SettingsResult<()> {
            if id != "demo.a" {
                return Err(SettingsError::NoBuiltinPlugin { id: id.to_owned() });
            }
            self.loaded.borrow_mut().push(id.to_owned());
            Ok(())
        }
        fn unload(&mut self, id: &str) -> SettingsResult<()> {
            self.loaded.borrow_mut().retain(|l| l != id);
            Ok(())
        }
        fn plugin_parameters(&self, _id: &str) -> Vec<om_ui::settings::PluginParam> {
            vec![om_ui::settings::PluginParam {
                id: "1".into(),
                name: "Gain".into(),
                min: 0.0,
                max: 2.0,
                default: 1.0,
                value: self.gain.get(),
            }]
        }
        fn set_plugin_parameter(
            &mut self,
            _id: &str,
            param: &str,
            value: f64,
        ) -> SettingsResult<()> {
            assert_eq!(param, "1");
            self.gain.set(value.clamp(0.0, 2.0));
            Ok(())
        }
    }

    #[test]
    fn a_built_in_plugin_loads_and_unloads_and_the_themes_are_synced_each_time() {
        let syncs = Rc::new(Cell::new(0));
        let mut s = SettingsState::new(options(
            None,
            None,
            Some(Box::new(FakePlugins::default())),
            Rc::clone(&syncs),
            Rc::new(Cell::new(0)),
        ));
        assert_eq!(
            s.lists.builtins,
            vec![("demo.a".to_owned(), "Demo A".to_owned())]
        );
        let after_start = syncs.get();

        s.plugin_load_builtin("demo.a");
        assert_eq!(s.lists.loaded_plugins, vec!["demo.a".to_owned()]);
        assert!(s.lists.builtins.is_empty());
        assert_eq!(s.notice.text, "Loaded demo.a");
        assert_eq!(syncs.get(), after_start + 1);

        s.plugin_unload("demo.a");
        assert!(s.lists.loaded_plugins.is_empty());
        assert_eq!(s.lists.builtins.len(), 1);
        s.plugin_load_builtin("missing");
        assert!(s.notice.error);
    }

    #[test]
    fn the_generic_panel_steps_resets_and_closes() {
        let plugins = FakePlugins::default();
        plugins.gain.set(1.0);
        let mut s = SettingsState::new(options(
            None,
            None,
            Some(Box::new(plugins)),
            Rc::new(Cell::new(0)),
            Rc::new(Cell::new(0)),
        ));
        s.plugin_load_builtin("demo.a");
        s.plugin_toggle_params("demo.a");
        assert_eq!(s.lists.open_params.as_deref(), Some("demo.a"));
        assert_eq!(s.lists.params[0].display(), "1");

        s.plugin_step_param("demo.a", "1", 1);
        assert_eq!(s.lists.params[0].display(), "1.1");
        s.plugin_step_param("demo.a", "1", -1);
        s.plugin_step_param("demo.a", "1", -1);
        assert_eq!(s.lists.params[0].display(), "0.9");
        s.plugin_reset_param("demo.a", "1");
        assert_eq!(s.lists.params[0].display(), "1");
        s.plugin_step_param("demo.a", "missing", 1);
        assert!(!s.notice.error, "an unknown parameter is ignored");

        s.plugin_toggle_params("demo.a");
        assert!(s.lists.open_params.is_none() && s.lists.params.is_empty());
    }
}

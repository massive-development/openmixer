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
        let press = id.clone();
        loaded = loaded.child(
            div()
                .flex()
                .justify_between()
                .items_center()
                .mb(px(4.))
                .child(div().flex_1().overflow_hidden().child(id.clone()))
                .child(chip(t, "Unload", false, cx, move |this| {
                    let id = press.clone();
                    this.settings_mut(|s| s.plugin_unload(&id));
                    this.refresh_theme();
                })),
        );
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

impl SettingsState {
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
}

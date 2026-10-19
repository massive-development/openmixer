//! The appearance page: the theme picker.

use super::{set_if_changed, SettingsError, SettingsResult};
use crate::preferences::Preferences;
use crate::theme::ThemeRegistry;

/// Appearance page: lists the registry's themes and stores the chosen id.
pub struct AppearanceSettingsPage<'a> {
    prefs: &'a mut Preferences,
    themes: &'a ThemeRegistry,
}

impl<'a> AppearanceSettingsPage<'a> {
    pub fn new(prefs: &'a mut Preferences, themes: &'a ThemeRegistry) -> Self {
        Self { prefs, themes }
    }

    /// `(id, display name)` for every registered theme.
    pub fn choices(&self) -> Vec<(String, String)> {
        self.themes
            .themes()
            .into_iter()
            .map(|e| (e.info.id.clone(), e.info.name.clone()))
            .collect()
    }

    /// Stores the id; `false` when it was already stored. Unknown ids are refused.
    pub fn pick(&mut self, id: &str) -> SettingsResult {
        if self.themes.find(id).is_none() {
            return Err(SettingsError::UnknownTheme { id: id.to_owned() });
        }
        Ok(set_if_changed(&mut self.prefs.theme, id.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preferences::Preferences;
    use crate::theme::ThemeRegistry;

    #[test]
    fn appearance_lists_the_registry_and_stores_the_pick() {
        let mut prefs = Preferences::default();
        let mut themes = ThemeRegistry::new();
        themes.load_builtins();
        let mut page = AppearanceSettingsPage::new(&mut prefs, &themes);
        assert!(page.choices().iter().any(|(id, _)| id == "openmixer.dark"));
        assert!(page.pick("no.such.theme").is_err());
        let other = page
            .choices()
            .into_iter()
            .map(|(id, _)| id)
            .find(|id| id != "openmixer.dark")
            .expect("the built-ins ship more than one theme");
        assert_eq!(page.pick(&other).ok(), Some(true));
        assert_eq!(page.pick(&other).ok(), Some(false));
        assert_eq!(prefs.theme, other);
    }

    #[test]
    fn a_theme_added_later_appears_in_the_picker() {
        let mut prefs = Preferences::default();
        let mut themes = ThemeRegistry::new();
        themes.load_builtins();
        let before = AppearanceSettingsPage::new(&mut prefs, &themes)
            .choices()
            .len();
        themes
            .add_json(
                r#"{"id":"user.later","name":"Later"}"#,
                crate::theme::Source::User,
                "later.json",
            )
            .expect("user theme");
        let choices = AppearanceSettingsPage::new(&mut prefs, &themes).choices();
        assert_eq!(choices.len(), before + 1);
        assert!(choices
            .iter()
            .any(|(id, name)| id == "user.later" && name == "Later"));
    }
}

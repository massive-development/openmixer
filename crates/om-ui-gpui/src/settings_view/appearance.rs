//! The Appearance tab: the theme picker.

use super::chips::*;
use super::*;

pub(super) fn appearance_page(
    t: &ThemeTokens,
    settings: &mut SettingsState,
    cx: &mut Context<Shell>,
) -> Div {
    let choices = AppearanceSettingsPage::new(&mut settings.prefs, &settings.themes).choices();
    let stored = settings.prefs.theme.clone();
    let mut list = div().flex().flex_col();
    for (id, name) in choices {
        let lit = id == stored;
        let pick = id.clone();
        let theme_id = id.clone();
        list = list.child(
            div()
                .flex()
                .items_center()
                .child(
                    div()
                        .w(px(180.))
                        .child(chip(t, &name, lit, cx, move |this| {
                            let id = pick.clone();
                            this.settings_mut(|s| {
                                s.pick_theme(&id);
                            });
                            // The live switch: the window takes the new theme on the next frame.
                            this.refresh_theme();
                        })),
                )
                .child(
                    div()
                        .w(px(200.))
                        .text_color(paint(t.text_secondary))
                        .text_size(px(t.type_caption))
                        .child(theme_id),
                )
                .child(
                    div()
                        .text_color(paint(accent_of(t)))
                        .text_size(px(t.type_caption))
                        .child(if lit { "current" } else { "" }),
                ),
        );
    }
    div()
        .flex()
        .flex_col()
        .child(
            div()
                .text_color(paint(t.text_secondary))
                .text_size(px(t.type_caption))
                .mb(px(4.))
                .child("Theme"),
        )
        .child(list)
        .child(note(
            t,
            "The theme applies at once and is saved with the other preferences. Plugin themes are listed here while their plugin is loaded.",
        ))
}

impl SettingsState {
    /// Stores the theme and selects it in this window's registry (the live switch). `false` when
    /// the id is unknown, in which case nothing changes.
    pub fn pick_theme(&mut self, id: &str) -> bool {
        let result = {
            let mut page = AppearanceSettingsPage::new(&mut self.prefs, &self.themes);
            page.pick(id)
        };
        match result {
            Ok(changed) => {
                self.themes.select(id);
                if changed {
                    self.changed();
                }
                true
            }
            Err(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::settings_view::fixtures::*;

    use std::cell::RefCell;

    use std::rc::Rc;

    #[test]
    fn picking_a_theme_selects_it_live_and_an_unknown_id_changes_nothing() {
        let changes = Rc::new(RefCell::new(0));
        let mut s = state(None, Rc::clone(&changes));
        let before = s.themes.active_id().map(str::to_owned);
        assert!(!s.pick_theme("no.such.theme"));
        assert_eq!(s.themes.active_id().map(str::to_owned), before);
        assert_eq!(*changes.borrow(), 0);

        assert!(s.pick_theme("openmixer.daylight"));
        assert_eq!(s.themes.active_id(), Some("openmixer.daylight"));
        assert_eq!(s.prefs.theme, "openmixer.daylight");
        assert_eq!(*changes.borrow(), 1);
    }
}

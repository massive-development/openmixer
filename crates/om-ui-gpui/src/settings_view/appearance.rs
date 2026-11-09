//! The Appearance tab: the theme picker and the density (Automatic, Roomy, Compact).

use super::chips::*;
use super::*;
use crate::font::TypeExt as _;
use om_ui::theme::{DensityChoice, TypeRole};

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
                        .type_role(t, TypeRole::Label)
                        .child(theme_id),
                )
                .child(
                    div()
                        .text_color(paint(accent_of(t)))
                        .type_role(t, TypeRole::Label)
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
                .type_role(t, TypeRole::Label)
                .mb(px(4.))
                .child("Theme"),
        )
        .child(list)
        .child(note(
            t,
            "The theme applies at once and is saved with the other preferences. Plugin themes are listed here while their plugin is loaded.",
        ))
        .child(density_row(t, settings, cx))
}

/// The density chips: Automatic picks Roomy when the window is tall enough and Compact otherwise.
fn density_row(t: &ThemeTokens, settings: &mut SettingsState, cx: &mut Context<Shell>) -> Div {
    let stored = settings.prefs.density;
    let mut chips = div().flex();
    for choice in DensityChoice::ALL {
        chips = chips.child(chip(t, choice.label(), choice == stored, cx, move |this| {
            this.settings_mut(|s| {
                s.pick_density(choice);
            });
        }));
    }
    div()
        .mt(px(10.))
        .child(choice_row(t, "Density", chips))
        .child(note(
            t,
            "Roomy has bigger targets and more air; Compact gives the waveforms the height. Automatic uses Roomy when the window is tall enough for it.",
        ))
}

impl SettingsState {
    /// Stores the density choice; the window lays out with it on the next frame.
    pub fn pick_density(&mut self, choice: DensityChoice) {
        let changed =
            AppearanceSettingsPage::new(&mut self.prefs, &self.themes).pick_density(choice);
        if changed {
            self.changed();
        }
    }

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

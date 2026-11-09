//! Shared widgets for the settings cards: chips, choice rows, notes and the text field. The chips
//! are `gpui-component` buttons and the text field an input; what a page tells the user after an
//! action is a toast (see `Shell::push_notice`).

use super::*;
use crate::controls::push_button;
use crate::font::TypeExt as _;
use gpui_kit::component::input::Input;
use om_ui::style::LookAndFeel;
use om_ui::theme::TypeRole;

/// Height of a chip, px.
const CHIP_H: f32 = 28.0;

/// A flow-layout button: a labelled chip that runs `on_press` on the Shell when clicked. Lit
/// chips are switches on; the rest are raised buttons.
pub(crate) fn chip(
    t: &ThemeTokens,
    label: &str,
    lit: bool,
    cx: &mut Context<Shell>,
    on_press: impl Fn(&mut Shell) + 'static,
) -> Div {
    chip_with_window(t, label, lit, cx, move |this, _, _| on_press(this))
}

/// A [`chip`] whose press also gets the window and context, for actions that open another window.
pub(crate) fn chip_with_window(
    t: &ThemeTokens,
    label: &str,
    lit: bool,
    cx: &mut Context<Shell>,
    on_press: impl Fn(&mut Shell, &mut Window, &mut Context<Shell>) + 'static,
) -> Div {
    let button = push_button(t, label, lit.then(|| accent_of(t)), cx, on_press)
        .h_full()
        .px(px(10.));
    div()
        .h(px(CHIP_H))
        .mr(px(6.))
        .mb(px(6.))
        .flex_none()
        .child(button)
}

/// A chip that opens the platform's file dialog for the text field `kind`; the picked path is
/// applied as if it had been typed and confirmed.
pub(crate) fn browse_chip(t: &ThemeTokens, kind: EntryKind, cx: &mut Context<Shell>) -> Div {
    debug_assert!(kind.can_browse(), "{kind:?} is not a path field");
    let button = push_button(t, "Browse…", None, cx, move |this, _, cx| {
        this.browse(kind, cx)
    })
    .h_full()
    .px(px(10.));
    div()
        .h(px(CHIP_H))
        .mr(px(6.))
        .mb(px(6.))
        .flex_none()
        .child(button)
}

/// A labelled row of chips.
pub(super) fn choice_row(t: &ThemeTokens, title: &str, chips: Div) -> Div {
    let caption = LookAndFeel::from_tokens(t).panel_caption;
    div()
        .flex()
        .flex_col()
        .mb(px(10.))
        .child(
            div()
                .text_color(paint(caption.text))
                .type_role(t, TypeRole::Label)
                .mb(px(4.))
                .child(title.to_owned()),
        )
        .child(div().flex().flex_wrap().child(chips))
}

/// A sentence under a row: body size, in the caption colour.
pub(super) fn note(t: &ThemeTokens, text: &str) -> Div {
    let caption = LookAndFeel::from_tokens(t).panel_caption;
    div()
        .text_color(paint(caption.text))
        .type_role(t, TypeRole::Body)
        .mb(px(8.))
        .child(text.to_owned())
}

/// A row of chips.
pub(super) fn row_of(chips: Vec<Div>) -> Div {
    chips
        .into_iter()
        .fold(div().flex().flex_wrap(), |row, c| row.child(c))
}

/// The text field, when the open field is `kind`: an input with Apply and Cancel. The input
/// itself is made by `Shell::sync_entry_input` when the field opens.
pub(super) fn entry_field(
    t: &ThemeTokens,
    settings: &SettingsState,
    kind: EntryKind,
    hint: &str,
    cx: &mut Context<Shell>,
) -> Option<Div> {
    settings.entry.as_ref().filter(|e| e.kind == kind)?;
    let (_, input) = settings.entry_input.as_ref().filter(|(k, _)| *k == kind)?;
    let line = div()
        .flex()
        .items_center()
        .mb(px(6.))
        .child(div().flex_1().mr(px(6.)).child(Input::new(input)))
        .child(chip(t, "Apply", false, cx, |this| {
            this.settings_mut(|s| s.submit_entry())
        }))
        .child(chip(t, "Cancel", false, cx, |this| {
            this.settings_mut(|s| s.cancel_entry())
        }));
    Some(
        div()
            .flex()
            .flex_col()
            .child(note(
                t,
                &format!("Type the {hint}, then Enter. Escape cancels."),
            ))
            .child(line),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lit_chip_is_neutral_never_a_deck_hue() {
        let mut themes = ThemeRegistry::new();
        themes.load_builtins();
        assert!(!themes.themes().is_empty());
        for entry in themes.themes() {
            let (id, tokens) = (&entry.info.id, &entry.tokens);
            assert_eq!(accent_of(tokens), tokens.text_primary, "{id}");
            assert!(!tokens.accents.contains(&accent_of(tokens)), "{id}");
        }
    }
}

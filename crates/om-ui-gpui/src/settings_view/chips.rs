//! Shared widgets for the settings cards: chips, choice rows, notes, status lines and the text field.

use super::*;
use crate::paint::skin;
use om_ui::style::LookAndFeel;

/// A flow-layout button: a labelled chip that runs `on_press` on the Shell when clicked. Lit
/// chips are switches on; the rest are raised buttons.
pub(crate) fn chip(
    t: &ThemeTokens,
    label: &str,
    lit: bool,
    cx: &mut Context<Shell>,
    on_press: impl Fn(&mut Shell) + 'static,
) -> Div {
    let look = LookAndFeel::from_tokens(t);
    let style = if lit {
        look.toggle(true, accent_of(t))
    } else {
        look.button.idle
    };
    skin(
        div()
            .flex()
            .items_center()
            .justify_center()
            .px(px(10.))
            .py(px(4.))
            .mr(px(6.))
            .mb(px(6.)),
        &style,
    )
    .child(label.to_owned())
    .on_mouse_down(
        MouseButton::Left,
        cx.listener(move |this, _: &MouseDownEvent, _, cx| {
            on_press(this);
            cx.notify();
        }),
    )
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
                .text_size(px(caption.font_size))
                .mb(px(4.))
                .child(title.to_owned()),
        )
        .child(div().flex().flex_wrap().child(chips))
}

pub(super) fn note(t: &ThemeTokens, text: &str) -> Div {
    let caption = LookAndFeel::from_tokens(t).panel_caption;
    div()
        .text_color(paint(caption.text))
        .text_size(px(caption.font_size))
        .mb(px(8.))
        .child(text.to_owned())
}

/// The last action's text, in the error colour when it failed.
pub(super) fn status_line(t: &ThemeTokens, notice: &Notice) -> Div {
    if notice.text.is_empty() {
        return div();
    }
    let look = LookAndFeel::from_tokens(t);
    let colour = if notice.error {
        look.panel.text
    } else {
        look.panel_caption.text
    };
    let text = if notice.error {
        format!("Error: {}", notice.text)
    } else {
        notice.text.clone()
    };
    div()
        .text_color(paint(colour))
        .text_size(px(look.panel_caption.font_size))
        .mt(px(6.))
        .child(text)
}

/// A row of chips.
pub(super) fn row_of(chips: Vec<Div>) -> Div {
    chips
        .into_iter()
        .fold(div().flex().flex_wrap(), |row, c| row.child(c))
}

/// The text field, when the open field is `kind`: the typed path with Apply and Cancel.
pub(super) fn entry_field(
    t: &ThemeTokens,
    settings: &SettingsState,
    kind: EntryKind,
    hint: &str,
    cx: &mut Context<Shell>,
) -> Option<Div> {
    let entry = settings.entry.as_ref().filter(|e| e.kind == kind)?;
    let field = skin(
        div()
            .flex_1()
            .overflow_hidden()
            .px(px(8.))
            .py(px(4.))
            .mr(px(6.)),
        &LookAndFeel::from_tokens(t).text_field.focus,
    )
    .child(format!("{}|", entry.text));
    let line = div()
        .flex()
        .items_center()
        .mb(px(6.))
        .child(field)
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
    fn the_accent_falls_back_to_the_surface_when_the_theme_has_none() {
        let mut themes = ThemeRegistry::new();
        themes.load_builtins();
        themes.select(ThemeRegistry::DEFAULT_ID);
        let mut tokens = themes.current().clone();
        tokens.accents.clear();
        assert_eq!(accent_of(&tokens), tokens.surface_raised);
    }
}

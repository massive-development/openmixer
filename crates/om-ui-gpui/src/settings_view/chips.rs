//! Shared widgets for the settings cards: chips, choice rows, notes, status lines and the text field.

use super::*;

/// The accent for lit chips: the theme's first accent colour.
pub(super) fn accent_of(t: &ThemeTokens) -> om_ui::theme::Colour {
    t.accents.first().copied().unwrap_or(t.surface_raised)
}

/// A flow-layout button: a labelled chip that runs `on_press` on the Shell when clicked.
pub(super) fn chip(
    t: &ThemeTokens,
    label: &str,
    lit: bool,
    cx: &mut Context<Shell>,
    on_press: impl Fn(&mut Shell) + 'static,
) -> Div {
    div()
        .flex()
        .items_center()
        .justify_center()
        .px(px(10.))
        .py(px(4.))
        .mr(px(6.))
        .mb(px(6.))
        .rounded(px(t.control_radius))
        .border_1()
        .border_color(paint(t.border))
        .bg(paint(if lit { accent_of(t) } else { t.surface_raised }))
        .text_color(paint(if lit { t.on_accent } else { t.text_primary }))
        .text_size(px(t.type_small))
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
    div()
        .flex()
        .flex_col()
        .mb(px(10.))
        .child(
            div()
                .text_color(paint(t.text_secondary))
                .text_size(px(t.type_caption))
                .mb(px(4.))
                .child(title.to_owned()),
        )
        .child(div().flex().flex_wrap().child(chips))
}

pub(super) fn note(t: &ThemeTokens, text: &str) -> Div {
    div()
        .text_color(paint(t.text_secondary))
        .text_size(px(t.type_caption))
        .mb(px(8.))
        .child(text.to_owned())
}

/// The last action's text, in the error colour when it failed.
pub(super) fn status_line(t: &ThemeTokens, notice: &Notice) -> Div {
    if notice.text.is_empty() {
        return div();
    }
    let colour = if notice.error {
        t.text_primary
    } else {
        t.text_secondary
    };
    let text = if notice.error {
        format!("Error: {}", notice.text)
    } else {
        notice.text.clone()
    };
    div()
        .text_color(paint(colour))
        .text_size(px(t.type_caption))
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
    let field = div()
        .flex_1()
        .overflow_hidden()
        .px(px(8.))
        .py(px(4.))
        .mr(px(6.))
        .border_1()
        .border_color(paint(t.border))
        .rounded(px(t.control_radius))
        .text_color(paint(t.text_primary))
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

//! The library browser in the bottom area: the sidebar tree, the search box, the "Load to deck"
//! buttons, the sortable track table with the Camelot tint, and the drag source for rows. Every
//! click and key calls [`LibraryPanel`], which owns the behaviour; this file only draws it.
//!
//! The table is virtualised by hand: [`LibraryPanel::table_top`] and [`LibraryPanel::visible_rows`]
//! pick the rows drawn, and the wheel scrolls them. Decks accept the drag (see `deck_view`).

use crate::app::Shell;
use crate::library::{KeyOutcome, LibraryPanel, RowView};
use crate::paint::{paint, place, skin};
use gpui::{
    div, prelude::*, px, AnyElement, App, Context, Div, FocusHandle, KeyDownEvent, MouseButton,
    MouseDownEvent, Render, ScrollWheelEvent, Window,
};
use om_ui::geometry::Rect;
use om_ui::library::{columns, SEARCH_PLACEHOLDER};
use om_ui::sidebar::SidebarKind;
use om_ui::style::{ControlStyle, LookAndFeel};
use om_ui::theme::ThemeTokens;

/// Height of one table row and one sidebar line.
pub const ROW_HEIGHT: f32 = 18.0;
/// Width of the sidebar tree.
const SIDEBAR_WIDTH: f32 = 170.0;
/// Height of the toolbar (search box and buttons).
const TOOLBAR_HEIGHT: f32 = 26.0;
/// Height of the table's column header.
const HEADER_HEIGHT: f32 = 18.0;
/// Inner padding of the library card.
const PAD: f32 = 6.0;

/// The payload of a library-track drag: the description that [`om_ui::library::parse_track_drag`]
/// reads on the drop side, and the label the drag preview shows.
#[derive(Clone)]
pub struct TrackDragPayload {
    pub description: String,
    pub label: String,
    /// The style the drag preview is drawn in (the theme's tooltip style).
    pub preview: ControlStyle,
}

/// The floating label shown under the pointer while a track is dragged.
pub struct DragPreview {
    label: String,
    style: ControlStyle,
}

impl Render for DragPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        skin(div().px(px(8.)).py(px(2.)), &self.style).child(self.label.clone())
    }
}

/// Runs `f` on the library panel when there is one.
fn with_library<R>(this: &mut Shell, f: impl FnOnce(&mut LibraryPanel) -> R) -> Option<R> {
    this.library.as_mut().map(f)
}

/// Draws the library card in `bounds` (window coordinates, the bottom layout slot).
pub fn library_card(shell: &mut Shell, bounds: Rect, cx: &mut Context<Shell>) -> Div {
    let t: ThemeTokens = shell.theme.clone();
    let focus: FocusHandle = shell
        .library_focus
        .get_or_insert_with(|| cx.focus_handle())
        .clone();
    let root = place(bounds)
        .bg(paint(t.surface))
        .border_1()
        .border_color(paint(t.border))
        .rounded(px(t.control_radius))
        .text_color(paint(t.text_primary))
        .text_size(px(t.type_small))
        .overflow_hidden()
        .track_focus(&focus)
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _: &MouseDownEvent, window, _| {
                if let Some(focus) = this.library_focus.clone() {
                    focus.focus(window);
                }
            }),
        )
        .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _, cx| {
            let keys = &ev.keystroke;
            if keys.modifiers.control || keys.modifiers.platform {
                return;
            }
            let typed = keys.key_char.clone();
            let outcome = with_library(this, |p| p.handle_key(&keys.key, typed.as_deref()))
                .unwrap_or(KeyOutcome::Ignored);
            if outcome != KeyOutcome::Ignored {
                cx.notify();
            }
        }));

    let Some(panel) = shell.library.as_mut() else {
        return root.child(
            div()
                .p(px(PAD))
                .text_color(paint(t.text_secondary))
                .child("Library not available (see the terminal for the reason)"),
        );
    };

    let rows_h = bounds.h as f32 - TOOLBAR_HEIGHT - HEADER_HEIGHT - 2.0 * PAD;
    panel.visible_rows = (rows_h / ROW_HEIGHT).floor().max(1.0) as usize;
    let count = panel.row_count();
    let visible = panel.visible_rows;
    let top = panel.table_top.min(count.saturating_sub(visible));
    panel.table_top = top;
    let rows: Vec<RowView> = (top..(top + visible).min(count))
        .filter_map(|row| panel.row(row))
        .collect();
    let sidebar = panel.sidebar_lines();
    let search_text = panel.browser.search_text().to_owned();
    let name_prompt = panel.name_entry.as_ref().map(|entry| entry.prompt());
    let search_active = panel.search_active || name_prompt.is_some();
    let status = panel.browser.status().to_owned();
    let status_error = panel.browser.status_is_error();
    let num_decks = panel.browser.num_decks();
    let sort = panel.sort_indicator();

    // Sidebar tree.
    let mut tree = div()
        .absolute()
        .left(px(0.))
        .top(px(0.))
        .w(px(SIDEBAR_WIDTH))
        .h(px(bounds.h as f32))
        .flex()
        .flex_col()
        .p(px(4.))
        .bg(paint(t.well))
        .overflow_hidden();
    let drop_target = {
        let mut c = paint(t.master);
        c.a = 0.35;
        c
    };
    for line in sidebar {
        let node = line.node.clone();
        let lit = line.selected;
        let playlist = (node.kind == SidebarKind::Playlist).then_some(node.playlist_id);
        tree = tree.child(
            div()
                .h(px(ROW_HEIGHT))
                .flex()
                .items_center()
                .pl(px(4. + 12. * line.depth as f32))
                .rounded(px(3.))
                .bg(paint(if lit { t.surface_raised } else { t.well }))
                .text_color(paint(if lit {
                    t.text_primary
                } else {
                    t.text_secondary
                }))
                .child(line.node.label.clone())
                // Tracks dropped on a playlist are added to it.
                .when_some(playlist, |el, id| {
                    el.drag_over::<TrackDragPayload>(move |style, _, _, _| style.bg(drop_target))
                        .on_drop(cx.listener(move |this, payload: &TrackDragPayload, _, cx| {
                            with_library(this, |p| p.drop_on_playlist(id, &payload.description));
                            cx.notify();
                        }))
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                        with_library(this, |p| p.select_sidebar(&node));
                        cx.notify();
                    }),
                ),
        );
    }
    // The playlist actions sit under the tree.
    let playlist_actions = div()
        .absolute()
        .left(px(0.))
        .bottom(px(0.))
        .w(px(SIDEBAR_WIDTH))
        .p(px(4.))
        .flex()
        .flex_wrap()
        .gap(px(4.))
        .child(action_button(&t, "New", cx, |p| p.begin_new_playlist()))
        .child(action_button(&t, "Rename", cx, |p| {
            p.begin_rename_playlist()
        }))
        .child(action_button(&t, "Delete", cx, |p| p.delete_playlist()))
        .child(action_button(&t, "Remove tracks", cx, |p| {
            p.remove_selected_from_playlist();
        }));

    // Toolbar: search box, load buttons, analyse, status line.
    let search_label = if let Some(prompt) = name_prompt.clone() {
        prompt
    } else if search_text.is_empty() && !search_active {
        SEARCH_PLACEHOLDER.to_owned()
    } else if search_active {
        format!("{search_text}|")
    } else {
        search_text.clone()
    };
    let search_box = div()
        .flex_1()
        .h(px(TOOLBAR_HEIGHT - 6.))
        .px(px(6.))
        .flex()
        .items_center()
        .overflow_hidden()
        .rounded(px(3.))
        .border_1()
        .border_color(paint(if search_active { t.highlight } else { t.border }))
        .bg(paint(t.surface_raised))
        .text_color(paint(
            if search_text.is_empty() && !search_active && name_prompt.is_none() {
                t.text_muted.with_multiplied_alpha(0.8)
            } else {
                t.text_primary
            },
        ))
        // The hint is dim and italic so it never reads as a typed query.
        .when(search_text.is_empty() && !search_active, |el| el.italic())
        .child(search_label)
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _: &MouseDownEvent, window, cx| {
                with_library(this, |p| p.search_active = true);
                if let Some(focus) = this.library_focus.clone() {
                    focus.focus(window);
                }
                cx.notify();
            }),
        );
    let mut toolbar = div()
        .absolute()
        .left(px(SIDEBAR_WIDTH + PAD))
        .top(px(PAD / 2.))
        .w(px(bounds.w as f32 - SIDEBAR_WIDTH - 2.0 * PAD))
        .h(px(TOOLBAR_HEIGHT))
        .flex()
        .items_center()
        .gap(px(4.))
        .child(search_box);
    for deck in 0..num_decks {
        let label = format!("Load to {}", (b'A' + deck as u8) as char);
        toolbar = toolbar.child(action_button(&t, &label, cx, move |p| {
            p.load_selected(deck)
        }));
    }
    toolbar = toolbar.child(action_button(&t, "Analyse", cx, |p| {
        p.prep_selected();
    }));
    toolbar = toolbar.child(action_button(&t, "Prepare stems", cx, |p| {
        p.prepare_stems();
    }));
    toolbar = toolbar.child(action_button(&t, "Cancel stems", cx, |p| {
        p.cancel_stems_selected();
    }));

    let status_colour = if status_error {
        t.highlight
    } else {
        t.text_secondary
    };
    let status_line = div()
        .absolute()
        .left(px(SIDEBAR_WIDTH + PAD))
        .top(px(PAD / 2. + TOOLBAR_HEIGHT))
        .w(px(bounds.w as f32 - SIDEBAR_WIDTH - 2.0 * PAD))
        .h(px(HEADER_HEIGHT))
        .flex()
        .items_center()
        .text_color(paint(status_colour))
        .overflow_hidden()
        .child(format!("{status}   {count} in view"));

    // Column header: a click sorts by the column.
    let mut header = div()
        .absolute()
        .left(px(SIDEBAR_WIDTH + PAD))
        .top(px(PAD / 2. + TOOLBAR_HEIGHT + HEADER_HEIGHT))
        .w(px(bounds.w as f32 - SIDEBAR_WIDTH - 2.0 * PAD))
        .h(px(HEADER_HEIGHT))
        .flex()
        .items_center()
        .overflow_hidden()
        .text_color(paint(t.text_secondary));
    for column in columns() {
        let id = column.id;
        let mut title = column.title.to_owned();
        if let Some((sorted, ascending)) = sort {
            if sorted == id {
                title.push_str(if ascending { " ^" } else { " v" });
            }
        }
        header = header.child(
            div()
                .w(px(column.width as f32))
                .flex_none()
                .overflow_hidden()
                .child(title)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                        with_library(this, |p| p.sort_by_column(id));
                        cx.notify();
                    }),
                ),
        );
    }

    // Track rows.
    let mut table = div()
        .absolute()
        .left(px(SIDEBAR_WIDTH + PAD))
        .top(px(PAD / 2. + TOOLBAR_HEIGHT + HEADER_HEIGHT * 2.))
        .w(px(bounds.w as f32 - SIDEBAR_WIDTH - 2.0 * PAD))
        .h(px(rows_h))
        .flex()
        .flex_col()
        .overflow_hidden()
        .on_scroll_wheel(cx.listener(|this, ev: &ScrollWheelEvent, _, cx| {
            let dy = f32::from(ev.delta.pixel_delta(px(ROW_HEIGHT)).y);
            let lines = (-dy / ROW_HEIGHT).round() as isize;
            if lines != 0 {
                with_library(this, |p| p.scroll_rows(lines));
                cx.notify();
            }
        }));
    if count == 0 {
        table = table.child(
            div()
                .h(px(ROW_HEIGHT))
                .text_color(paint(t.text_secondary))
                .child("No tracks match"),
        );
    }
    for row in rows {
        table = table.child(track_row(&t, row, cx));
    }

    root.child(tree)
        .child(playlist_actions)
        .child(toolbar)
        .child(status_line)
        .child(header)
        .child(table)
}

/// One table row: click selects (shift extends), double-click loads, and the row drags as a track.
fn track_row(t: &ThemeTokens, row: RowView, cx: &mut Context<Shell>) -> AnyElement {
    let row_index = row.row;
    let label = row.cells.first().cloned().unwrap_or_default();
    let background = if row.selected {
        paint(t.surface_raised)
    } else if row.compatible {
        // The Camelot tint: the master colour, translucent, over the row.
        let mut tint = paint(t.master);
        tint.a = 0.22;
        tint
    } else {
        paint(t.surface)
    };
    let mut line = div()
        .h(px(ROW_HEIGHT))
        .flex_none()
        .flex()
        .items_center()
        .overflow_hidden()
        .bg(background)
        .text_color(paint(t.text_primary));
    for (column, cell) in columns().iter().zip(row.cells.iter()) {
        line = line.child(
            div()
                .w(px(column.width as f32))
                .flex_none()
                .overflow_hidden()
                .child(cell.clone()),
        );
    }
    let payload = TrackDragPayload {
        description: row.drag.clone(),
        label,
        preview: LookAndFeel::from_tokens(t).tooltip,
    };
    line.id(("library-row", row_index))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                let shift = ev.modifiers.shift;
                let double = ev.click_count >= 2;
                with_library(this, |p| {
                    if double {
                        let _ = p.activate_row(row_index);
                    } else if shift {
                        p.extend_selection(row_index);
                    } else {
                        p.select_row(row_index);
                    }
                });
                cx.notify();
            }),
        )
        .on_drag(payload, |payload: &TrackDragPayload, _, _, cx: &mut App| {
            cx.new(|_| DragPreview {
                label: payload.label.clone(),
                style: payload.preview,
            })
        })
        .into_any_element()
}

/// A toolbar button that runs `on_press` on the panel.
fn action_button(
    t: &ThemeTokens,
    label: &str,
    cx: &mut Context<Shell>,
    on_press: impl Fn(&mut LibraryPanel) + 'static,
) -> Div {
    div()
        .h(px(TOOLBAR_HEIGHT - 6.))
        .px(px(8.))
        .flex()
        .items_center()
        .flex_none()
        .rounded(px(t.control_radius))
        .border_1()
        .border_color(paint(t.border))
        .bg(paint(t.surface_raised))
        .text_color(paint(t.text_primary))
        .child(label.to_owned())
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                with_library(this, |p| on_press(p));
                if with_library(this, |p| p.take_licence_request()).unwrap_or(false) {
                    this.show_stems_licence();
                }
                cx.notify();
            }),
        )
}

//! The library browser in the bottom area: the sidebar tree, the search box, the Analyse and Stems
//! buttons, the sortable track table with the Camelot tint, and the drag source for rows. A track
//! loads by dragging its row onto a deck or by double-clicking it (into the suggested deck), so
//! there are no per-deck load buttons. Every
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
use om_ui::library::{column_shown, columns, sort_glyph, MixLevel, SortGlyph, SEARCH_PLACEHOLDER};
use om_ui::main_layout::{header_height, library_metrics};
use om_ui::sidebar::SidebarKind;
use om_ui::style::{gutter_width, ControlStyle, LookAndFeel};
use om_ui::theme::ThemeTokens;

/// Inner padding of the library card.
const PAD: f32 = 6.0;

/// The payload of a library-track drag: the description that [`om_ui::library::parse_track_drag`]
/// reads on the drop side, and the label the drag preview shows.
#[derive(Clone)]
pub struct TrackDragPayload {
    pub description: String,
    /// The table row the drag started on (told apart from other rows of the same track).
    pub row: usize,
    pub label: String,
    /// The style the drag preview is drawn in (the theme's tooltip style).
    pub preview: ControlStyle,
}

/// The payload of a sidebar playlist or folder drag: dropped on a folder, it moves in.
#[derive(Clone)]
pub struct PlaylistDragPayload {
    pub id: i64,
    pub label: String,
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
    let (window_w, window_h) = shell.window_size;
    let metrics = library_metrics(window_w, window_h);
    let (row_h, header_h, toolbar_h, sidebar_w) = (
        metrics.row_h as f32,
        metrics.header_h as f32,
        metrics.toolbar_h as f32,
        metrics.sidebar_w as f32,
    );
    // Toolbar buttons are as tall as the header's controls.
    let button_h = (header_height(window_h) - 6) as f32;
    // The playlist buttons: four rows of one table row's height, under the tree.
    let actions_h = 4.0 * row_h + 3.0 * 4.0 + 8.0;
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
        .text_size(px(t.type_caption.max(12.0)))
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
                .child("Library could not open. Open Settings > Library."),
        );
    };

    let rows_h = bounds.h as f32 - toolbar_h - header_h - 2.0 * PAD;
    panel.visible_rows = (rows_h / row_h).floor().max(1.0) as usize;
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
    let stems_running = panel.stems_in_flight() > 0;
    let sort = panel.sort_indicator();

    // Sidebar tree.
    let mut tree = div()
        .absolute()
        .left(px(0.))
        .top(px(0.))
        .w(px(sidebar_w))
        .h(px((bounds.h as f32 - actions_h).max(0.)))
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
    let preview = LookAndFeel::from_tokens(&t).tooltip;
    for (index, line) in sidebar.into_iter().enumerate() {
        let node = line.node.clone();
        let look = sidebar_row_look(&t, line.selected);
        let playlist = (node.kind == SidebarKind::Playlist).then_some(node.playlist_id);
        // Playlists and folders drag; folders (`Some`) and the Playlists heading (`None`, the top
        // level) accept the drop.
        let draggable =
            matches!(node.kind, SidebarKind::Playlist | SidebarKind::Folder).then(|| {
                PlaylistDragPayload {
                    id: node.playlist_id,
                    label: node.label.clone(),
                    preview,
                }
            });
        let folder_target = match node.kind {
            SidebarKind::Folder => Some(Some(node.playlist_id)),
            SidebarKind::PlaylistGroup => Some(None),
            _ => None,
        };
        tree = tree.child(
            div()
                .id(("sidebar-row", index))
                .h(px(row_h))
                .flex()
                .items_center()
                .pl(px(4. + 12. * line.depth as f32))
                .rounded(px(3.))
                .relative()
                .bg(paint(look.fill))
                .text_color(paint(look.text))
                .when(look.bar.is_some(), |el| {
                    el.child(
                        div()
                            .absolute()
                            .left(px(0.))
                            .top(px(2.))
                            .bottom(px(2.))
                            .w(px(SIDEBAR_BAR_W))
                            .rounded(px(1.5))
                            .bg(paint(look.bar.unwrap_or(look.fill))),
                    )
                })
                .child(line.node.label.clone())
                // Tracks dropped on a playlist are added to it.
                .when_some(playlist, |el, id| {
                    el.drag_over::<TrackDragPayload>(move |style, _, _, _| style.bg(drop_target))
                        .on_drop(cx.listener(move |this, payload: &TrackDragPayload, _, cx| {
                            with_library(this, |p| p.drop_on_playlist(id, &payload.description));
                            cx.notify();
                        }))
                })
                // Playlists and folders dropped on a folder move into it.
                .when_some(folder_target, |el, folder| {
                    el.drag_over::<PlaylistDragPayload>(move |style, _, _, _| style.bg(drop_target))
                        .on_drop(
                            cx.listener(move |this, payload: &PlaylistDragPayload, _, cx| {
                                with_library(this, |p| p.drop_playlist_on(payload.id, folder));
                                cx.notify();
                            }),
                        )
                })
                .when_some(draggable, |el, payload| {
                    el.on_drag(
                        payload,
                        |payload: &PlaylistDragPayload, _, _, cx: &mut App| {
                            cx.new(|_| DragPreview {
                                label: payload.label.clone(),
                                style: payload.preview,
                            })
                        },
                    )
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
        .w(px(sidebar_w))
        .p(px(4.))
        .flex()
        .flex_col()
        .gap(px(4.))
        .child(
            div()
                .flex()
                .gap(px(4.))
                .child(action_button(&t, row_h, "New", cx, |p| {
                    p.begin_new_playlist()
                }))
                .child(action_button(&t, row_h, "Rename", cx, |p| {
                    p.begin_rename_playlist()
                })),
        )
        .child(action_button(
            &t,
            row_h,
            om_ui::labels::NEW_FOLDER,
            cx,
            |p| p.begin_new_folder(),
        ))
        .child(action_button(
            &t,
            row_h,
            om_ui::labels::DELETE_PLAYLIST,
            cx,
            |p| p.delete_playlist(),
        ))
        .child(action_button(
            &t,
            row_h,
            om_ui::labels::REMOVE_FROM_PLAYLIST,
            cx,
            |p| {
                p.remove_selected_from_playlist();
            },
        ));

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
        .h(px(button_h))
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
        .left(px(sidebar_w + PAD))
        .top(px(PAD / 2.))
        .w(px(bounds.w as f32 - sidebar_w - 2.0 * PAD))
        .h(px(toolbar_h))
        .flex()
        .items_center()
        .gap(px(4.))
        .child(search_box);
    toolbar = toolbar.child(action_button(&t, button_h, "Analyse", cx, |p| {
        p.prep_selected();
    }));
    toolbar = toolbar.child(action_button(
        &t,
        button_h,
        om_ui::labels::SPLIT_STEMS,
        cx,
        |p| {
            p.prepare_stems();
        },
    ));
    // Cancel is only worth a button while stem jobs are queued or running.
    if stems_running {
        toolbar = toolbar.child(action_button(
            &t,
            button_h,
            om_ui::labels::CANCEL_STEMS,
            cx,
            |p| {
                p.cancel_stems_selected();
            },
        ));
    }

    let status_colour = if status_error {
        t.highlight
    } else {
        t.text_secondary
    };
    let status_line = div()
        .absolute()
        .left(px(sidebar_w + PAD))
        .top(px(PAD / 2. + toolbar_h))
        .w(px(bounds.w as f32 - sidebar_w - 2.0 * PAD))
        .h(px(header_h))
        .flex()
        .items_center()
        .text_color(paint(status_colour))
        .overflow_hidden()
        .child(om_ui::labels::library_status(&status, count));

    let look = LookAndFeel::from_tokens(&t);
    // While the list scrolls, header and rows both leave a gutter for the scroll bar.
    let thumb = scroll_thumb(count, visible, top, rows_h);
    let gutter = gutter_width(thumb.is_some(), look.scroll_bar.width);
    let list_w = bounds.w as f32 - sidebar_w - 2.0 * PAD;
    // Column header: a click sorts by the column.
    let mut header = div()
        .absolute()
        .left(px(sidebar_w + PAD))
        .top(px(PAD / 2. + toolbar_h + header_h))
        .w(px(list_w - gutter))
        .h(px(header_h))
        .flex()
        .items_center()
        .overflow_hidden()
        .bg(paint(look.table.header.fill))
        .text_color(paint(look.table.header.text))
        .text_size(px(look.table.header.font_size))
        .border_b(px(look.table.divider.thickness.round().max(1.0)))
        .border_color(paint(look.table.divider.colour));
    for column in columns().iter().filter(|c| column_shown(c, window_w)) {
        let id = column.id;
        let mut title = column.title.to_owned();
        match sort_glyph(sort, id) {
            SortGlyph::Up => title.push_str(" \u{25B2}"),
            SortGlyph::Down => title.push_str(" \u{25BC}"),
            SortGlyph::None => {}
        }
        header = header.child(
            sized_cell(column.width, column.numeric)
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
        .left(px(sidebar_w + PAD))
        .top(px(PAD / 2. + toolbar_h + header_h * 2.))
        .w(px(list_w))
        .h(px(rows_h))
        .pr(px(gutter))
        .flex()
        .flex_col()
        .overflow_hidden()
        .on_scroll_wheel(cx.listener(move |this, ev: &ScrollWheelEvent, _, cx| {
            let dy = f32::from(ev.delta.pixel_delta(px(row_h)).y);
            let lines = (-dy / row_h).round() as isize;
            if lines != 0 {
                with_library(this, |p| p.scroll_rows(lines));
                cx.notify();
            }
        }));
    if count == 0 {
        table = table.child(
            div()
                .h(px(row_h))
                .text_color(paint(t.text_secondary))
                .child("No tracks match"),
        );
    }
    for row in rows {
        table = table.child(track_row(&t, row, row_h, window_w, cx));
    }
    if let Some((offset, length)) = thumb {
        let bar = look.scroll_bar;
        let thumb = bar.thumb;
        table = table.child(
            div()
                .absolute()
                .top(px(0.))
                .right(px(0.))
                .w(px(bar.width))
                .h(px(rows_h))
                .bg(paint(bar.track.fill))
                .rounded(px(bar.track.radius))
                .child(
                    // Display-only: the wheel and keys scroll; the thumb is not draggable.
                    div()
                        .absolute()
                        .top(px(offset))
                        .left(px(0.))
                        .w(px(bar.width))
                        .h(px(length))
                        .bg(paint(thumb.fill))
                        .rounded(px(thumb.radius)),
                ),
        );
    }

    root.child(tree)
        .child(playlist_actions)
        .child(toolbar)
        .child(status_line)
        .child(header)
        .child(table)
}

/// Width of the selection bar at a sidebar row's left edge (px). Unsourced; small.
const SIDEBAR_BAR_W: f32 = 3.0;
/// Share of the focus colour mixed into a selected sidebar row's fill. Unsourced; small.
const SIDEBAR_SELECT_TINT: f32 = 0.18;

/// How a sidebar row is drawn. Selection is a tinted fill plus a bar in the focus colour (so it
/// does not rest on tone alone, labels-layout.md item 5), and primary text against secondary.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SidebarRowLook {
    pub fill: om_ui::theme::Colour,
    pub text: om_ui::theme::Colour,
    /// The selection indicator, drawn on the left edge; `None` when not selected.
    pub bar: Option<om_ui::theme::Colour>,
}

pub(crate) fn sidebar_row_look(t: &ThemeTokens, selected: bool) -> SidebarRowLook {
    if !selected {
        return SidebarRowLook {
            fill: t.well,
            text: t.text_secondary,
            bar: None,
        };
    }
    SidebarRowLook {
        fill: om_ui::theme::Colour::mix(&[
            (t.well, 1.0 - SIDEBAR_SELECT_TINT),
            (t.focus_ring, SIDEBAR_SELECT_TINT),
        ]),
        text: t.text_primary,
        bar: Some(t.focus_ring),
    }
}

/// One table row: click selects (shift extends), double-click loads, and the row drags as a track.
fn track_row(
    t: &ThemeTokens,
    row: RowView,
    row_h: f32,
    window_w: i32,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let row_index = row.row;
    let label = row.cells.first().cloned().unwrap_or_default();
    let look = LookAndFeel::from_tokens(t);
    let background = if row.selected {
        paint(look.table.row_selected.fill)
    } else if let Some(deck) = row.on_deck {
        // The track on a deck: that deck's accent at 18% (plan).
        paint(t.accent_for(deck as i32).with_alpha(PLAYING_ROW_ALPHA))
    } else if row.compatible {
        // The Camelot tint: the master colour, translucent, over the row. Key and tempo both
        // fitting is stronger than the key alone.
        let mut tint = paint(t.master);
        tint.a = if row.mix == MixLevel::Strong {
            0.34
        } else {
            0.16
        };
        tint
    } else if row_index % 2 == 1 {
        paint(look.table.row_alt.fill)
    } else {
        paint(look.table.row.fill)
    };
    let mut line = div()
        .h(px(row_h))
        .flex_none()
        .flex()
        .items_center()
        .overflow_hidden()
        .bg(background)
        .text_color(paint(look.table.row.text))
        .text_size(px(look.table.row.font_size))
        .when(!row.selected, |el| {
            let hover = t.surface_hover;
            el.hover(move |style| style.bg(paint(hover)))
        });
    for (column, cell) in columns()
        .iter()
        .zip(row.cells.iter())
        .filter(|(c, _)| column_shown(c, window_w))
    {
        let text = if column.title == "Rating" {
            cell.replace('*', "\u{2605}").replace('-', "\u{2606}")
        } else {
            cell.clone()
        };
        line = line.child(sized_cell(column.width, column.numeric).child(text));
    }
    let payload = TrackDragPayload {
        description: row.drag.clone(),
        row: row_index,
        label,
        preview: LookAndFeel::from_tokens(t).tooltip,
    };
    let drop_target = {
        let mut c = paint(t.master);
        c.a = 0.35;
        c
    };
    // A track dropped on another row of the open playlist takes that row's place (the browser
    // refuses it anywhere else).
    line.id(("library-row", row_index))
        .drag_over::<TrackDragPayload>(move |style, _, _, _| style.bg(drop_target))
        .on_drop(cx.listener(move |this, payload: &TrackDragPayload, _, cx| {
            with_library(this, |p| p.drop_on_row(payload.row, row_index));
            cx.notify();
        }))
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

/// Smallest thumb length (px) so a long list still leaves something to see.
const MIN_THUMB: f32 = 16.0;

/// Where the scroll thumb sits in a track of `track_h` px: `(offset, length)`, or `None` when the
/// whole list fits.
pub(crate) fn scroll_thumb(
    count: usize,
    visible: usize,
    top: usize,
    track_h: f32,
) -> Option<(f32, f32)> {
    if count <= visible || visible == 0 || track_h <= 0.0 {
        return None;
    }
    let length = (track_h * visible as f32 / count as f32)
        .max(MIN_THUMB)
        .min(track_h);
    let max_top = (count - visible) as f32;
    let offset = (track_h - length) * (top as f32 / max_top).clamp(0.0, 1.0);
    Some((offset, length))
}

#[cfg(test)]
mod scroll_thumb_tests {
    use super::scroll_thumb;

    #[test]
    fn no_thumb_when_the_list_fits() {
        assert_eq!(scroll_thumb(5, 10, 0, 100.0), None);
    }

    #[test]
    fn thumb_spans_the_track_from_top_to_bottom() {
        let (o0, len) = scroll_thumb(100, 10, 0, 200.0).unwrap();
        assert_eq!(o0, 0.0);
        assert_eq!(len, 20.0);
        let (o1, _) = scroll_thumb(100, 10, 90, 200.0).unwrap();
        assert!((o1 + len - 200.0).abs() < 1e-3);
    }

    #[test]
    fn thumb_never_shrinks_below_the_minimum() {
        let (_, len) = scroll_thumb(100_000, 10, 0, 200.0).unwrap();
        assert!(len >= 16.0);
    }
}

/// Opacity of the deck accent behind the track loaded on a deck (plan, Task 5).
const PLAYING_ROW_ALPHA: f32 = 0.18;

/// A table cell: fixed `width` px, or flexible when 0; numbers are right-aligned.
fn sized_cell(width: i32, numeric: bool) -> Div {
    let cell = div().overflow_hidden().whitespace_nowrap().px(px(4.));
    let cell = if width > 0 {
        cell.w(px(width as f32)).flex_none()
    } else {
        cell.flex_1().min_w(px(0.))
    };
    if numeric {
        cell.flex().justify_end()
    } else {
        cell
    }
}

/// A toolbar button that runs `on_press` on the panel.
fn action_button(
    t: &ThemeTokens,
    button_h: f32,
    label: &str,
    cx: &mut Context<Shell>,
    on_press: impl Fn(&mut LibraryPanel) + 'static,
) -> Div {
    div()
        .h(px(button_h))
        .px(px(10.))
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

#[cfg(test)]
mod sidebar_tests {
    use super::*;
    use om_ui::style::{contrast_ratio, over};
    use om_ui::theme::{parse_theme, BUILTIN_THEMES};

    fn themes() -> [ThemeTokens; 2] {
        ["dark", "daylight"].map(|id| {
            let (_, json) = BUILTIN_THEMES
                .iter()
                .find(|(file, _)| file.contains(id))
                .expect("built-in theme");
            parse_theme(json).expect("theme parses").tokens
        })
    }

    #[test]
    fn selection_reads_in_both_themes_at_wcag() {
        for t in themes() {
            let ground = over(t.well, over(t.surface, t.backdrop_bottom));
            let sel = sidebar_row_look(&t, true);
            let un = sidebar_row_look(&t, false);
            let sel_fill = over(sel.fill, ground);
            let bar = sel.bar.expect("a selected row has a bar");
            assert!(contrast_ratio(bar, sel_fill) >= 3.0, "indicator 3:1");
            assert!(contrast_ratio(bar, over(un.fill, ground)) >= 3.0);
            assert!(contrast_ratio(sel.text, sel_fill) >= 4.5, "selected text");
            assert!(
                contrast_ratio(un.text, over(un.fill, ground)) >= 4.5,
                "plain text"
            );
            assert!(un.bar.is_none());
            // The selected text is the stronger one, so selection never reads inverted.
            assert!(
                contrast_ratio(sel.text, sel_fill)
                    >= contrast_ratio(un.text, over(un.fill, ground))
            );
        }
    }
}

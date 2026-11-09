//! The library browser in the bottom area: the sidebar tree, the search box, the Analyse and Stems
//! buttons, the sortable track table with the Camelot tint, and the drag source for rows. A track
//! loads by dragging its row onto a deck or by double-clicking it (into the suggested deck), so
//! there are no per-deck load buttons. Every click and key calls [`LibraryPanel`], which owns the
//! behaviour; this file only draws it.
//!
//! The pieces are `gpui-component` widgets: [`Button`]s, an [`Input`] for the search box (which
//! also takes a playlist name), [`ListItem`] rows for the sidebar tree and a virtualised
//! [`DataTable`] for the tracks ([`LibraryTable`] is its delegate). Decks accept the track drag
//! (see `deck_view`).

use crate::app::Shell;
use crate::controls::push_button;
use crate::font::{numeral, Numeral, TypeExt as _};
use crate::library::{KeyOutcome, LibraryPanel, RowView};
use crate::paint::{paint, place, skin};
use gpui::{
    div, prelude::*, px, App, Context, Div, Entity, FocusHandle, Focusable as _, Hsla,
    KeyDownEvent, MouseButton, MouseDownEvent, Render, WeakEntity, Window,
};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::list::ListItem;
use gpui_kit::component::table::{Column, ColumnSort, DataTable, TableDelegate, TableState};
use gpui_kit::component::{Sizable as _, Size};
use om_ui::geometry::Rect;
use om_ui::library::{column_shown, columns, MixLevel, SEARCH_PLACEHOLDER};
use om_ui::main_layout::library_metrics;
use om_ui::sidebar::SidebarKind;
use om_ui::style::{ControlStyle, LookAndFeel};
use om_ui::theme::{Colour, ThemeTokens, TypeRole};
use std::collections::HashMap;

/// Inner padding of the library card.
const PAD: f32 = 6.0;
/// Narrowest the flexible (Title, Artist) columns get, px.
const MIN_FLEX_W: f32 = 120.0;
/// The selected row: the slate hue (deck B's) as an 18% tint with a 55% edge (DESIGN.md "Library
/// rows").
const SELECTED_TINT: f32 = 0.18;
const SELECTED_EDGE: f32 = 0.55;
/// A row loaded on a deck: that deck's hue at 40% as its edge, beside its letter.
const LOADED_EDGE: f32 = 0.40;
/// Rows the table delegate keeps built before it starts over (a screenful is well under this).
const ROW_CACHE_LIMIT: usize = 512;
/// The hue a selection takes: slate, deck B's colour.
const SELECTION_HUE: i32 = 1;
/// Width of the selection bar at a sidebar row's left edge (px). Unsourced; small.
const SIDEBAR_BAR_W: f32 = 3.0;

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

/// A translucent colour over a row for a drag that may drop on it.
fn drop_tint(master: Colour) -> gpui::Rgba {
    let mut c = paint(master);
    c.a = 0.35;
    c
}

// The search box.

impl Shell {
    /// The toolbar's text box, made on first use. Typing in it runs the query (or edits the name
    /// being typed), Enter applies it and Escape drops it.
    fn search_input(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<InputState> {
        if let Some(input) = &self.library_search {
            return input.clone();
        }
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(SEARCH_PLACEHOLDER));
        cx.subscribe_in(
            &input,
            window,
            |this, input, event: &InputEvent, window, cx| {
                match event {
                    InputEvent::Change => {
                        let text = input.read(cx).value();
                        with_library(this, |p| {
                            if p.name_entry.is_some() {
                                p.set_name_text(&text);
                            } else {
                                p.set_query(&text);
                            }
                        });
                    }
                    InputEvent::PressEnter { .. } => {
                        let named = with_library(this, |p| {
                            if p.name_entry.is_some() {
                                p.submit_name();
                                true
                            } else {
                                p.commit_query();
                                false
                            }
                        });
                        // A refused name keeps the box open so it can be fixed.
                        if named == Some(false) {
                            if let Some(focus) = this.library_focus.clone() {
                                focus.focus(window, cx);
                            }
                        }
                    }
                    InputEvent::Focus | InputEvent::Blur => {}
                }
                cx.notify();
            },
        )
        .detach();
        self.library_search = Some(input.clone());
        input
    }

    /// Escape in the box: a name is dropped, a query is cleared, and the keyboard goes back to
    /// the library card.
    fn search_escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let was_named = with_library(self, |p| {
            let named = p.name_entry.is_some();
            if named {
                p.cancel_name();
            } else {
                p.clear_query();
            }
            named
        });
        if was_named == Some(false) {
            if let Some(input) = self.library_search.clone() {
                input.update(cx, |input, cx| input.set_value("", window, cx));
            }
        }
        if let Some(focus) = self.library_focus.clone() {
            focus.focus(window, cx);
        }
    }

    /// Puts the box in step with the panel: while a playlist name is being typed it shows that
    /// prompt and name, and takes the keyboard; afterwards the query comes back.
    fn sync_search_input(
        &mut self,
        input: &Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let naming = self
            .library
            .as_ref()
            .and_then(|p| p.name_entry.as_ref())
            .map(|entry| (entry.prompt(), entry.text.clone()));
        match (naming, self.search_names) {
            (Some((prompt, text)), false) => {
                self.search_names = true;
                input.update(cx, |input, cx| {
                    input.set_placeholder(prompt, window, cx);
                    input.set_value(text, window, cx);
                    input.focus(window, cx);
                });
            }
            (None, true) => {
                self.search_names = false;
                let query = self
                    .library
                    .as_ref()
                    .map(|p| p.browser.search_text().to_owned())
                    .unwrap_or_default();
                input.update(cx, |input, cx| {
                    input.set_placeholder(SEARCH_PLACEHOLDER, window, cx);
                    input.set_value(query, window, cx);
                });
            }
            _ => {}
        }
    }

    /// Whether the search box has the keyboard.
    fn search_has_focus(&self, window: &Window, cx: &App) -> bool {
        self.library_search
            .as_ref()
            .is_some_and(|input| input.focus_handle(cx).is_focused(window))
    }

    /// The track table, made on first use.
    fn track_table(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TableState<LibraryTable>> {
        if let Some(table) = &self.library_table {
            return table.clone();
        }
        let delegate = LibraryTable {
            shell: cx.weak_entity(),
            window_w: self.window_size.0,
            list_w: 0.0,
            sort: None,
            rows: HashMap::new(),
        };
        let table = cx.new(|cx| {
            TableState::new(delegate, window, cx)
                .row_selectable(false)
                .col_movable(false)
                .col_resizable(false)
                .row_header(false)
        });
        self.library_table = Some(table.clone());
        table
    }
}

// The track table.

/// The delegate of the track table: it reads the rows from the shell's [`LibraryPanel`] as they
/// scroll into view, and hands clicks, drags and drops back to it.
pub struct LibraryTable {
    shell: WeakEntity<Shell>,
    window_w: i32,
    /// Width of the table, which the flexible columns share.
    list_w: f32,
    /// Column and direction of the last sort, shown on the header.
    sort: Option<(i32, bool)>,
    /// The rows `render_tr` built, by index, so their cells need not ask the panel again. The table
    /// builds every visible row before any of their cells (the cells sit in a per-row list laid
    /// out later), so one cached row is not enough.
    rows: HashMap<usize, RowView>,
}

impl LibraryTable {
    fn shown(&self) -> impl Iterator<Item = (usize, &'static om_ui::library::Column)> {
        let window_w = self.window_w;
        columns()
            .iter()
            .enumerate()
            .filter(move |(_, c)| column_shown(c, window_w))
    }

    /// Width of a flexible column: what the fixed columns leave, shared by the flexible ones.
    fn flex_width(&self) -> f32 {
        let (fixed, flexible) =
            self.shown()
                .fold((0, 0), |(fixed, flexible), (_, c)| match c.width {
                    0 => (fixed, flexible + 1),
                    width => (fixed + width, flexible),
                });
        if flexible == 0 {
            return MIN_FLEX_W;
        }
        ((self.list_w - fixed as f32) / flexible as f32).max(MIN_FLEX_W)
    }
}

impl TableDelegate for LibraryTable {
    fn columns_count(&self, _: &App) -> usize {
        self.shown().count()
    }

    fn rows_count(&self, cx: &App) -> usize {
        self.shell
            .read_with(cx, |shell, _| {
                shell.library.as_ref().map_or(0, LibraryPanel::row_count)
            })
            .unwrap_or(0)
    }

    fn column(&self, col_ix: usize, _: &App) -> Column {
        let (_, spec) = self.shown().nth(col_ix).expect("column in range");
        let width = match spec.width {
            0 => self.flex_width(),
            width => width as f32,
        };
        let mut column = Column::new(spec.id.to_string(), spec.title).width(px(width));
        column.resizable = false;
        column.movable = false;
        if spec.numeric {
            column = column.text_right();
        }
        if spec.sort_field.is_some() {
            column = match self.sort {
                Some((id, true)) if id == spec.id => column.ascending(),
                Some((id, false)) if id == spec.id => column.descending(),
                _ => column.sortable(),
            };
        }
        column
    }

    fn perform_sort(
        &mut self,
        col_ix: usize,
        _: ColumnSort,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) {
        // The panel owns the order (a second click on a column flips it).
        let Some((_, spec)) = self.shown().nth(col_ix) else {
            return;
        };
        let id = spec.id;
        let sort = self
            .shell
            .update(cx, |shell, cx| {
                cx.notify();
                with_library(shell, |p| {
                    p.sort_by_column(id);
                    p.sort_indicator()
                })
                .flatten()
            })
            .ok()
            .flatten();
        self.sort = sort;
        cx.notify();
    }

    fn render_tr(
        &mut self,
        row_ix: usize,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> gpui::Stateful<Div> {
        let drawn = self
            .shell
            .update(cx, |shell, _| {
                let look = RowLook::from_tokens(&shell.theme);
                let preview = LookAndFeel::from_tokens(&shell.theme).tooltip;
                let row = with_library(shell, |p| p.row(row_ix)).flatten();
                (row, look, preview)
            })
            .ok();
        let line = div().id(("library-row", row_ix));
        let Some((Some(row), look, preview)) = drawn else {
            self.rows.remove(&row_ix);
            return line;
        };
        let (background, edge) = look.fill_and_edge(&row);
        let dimmed = row.error.is_some();
        let (master, size) = (look.master, look.size);
        let label = row.cells.first().cloned().unwrap_or_default();
        let payload = TrackDragPayload {
            description: row.drag.clone(),
            row: row_ix,
            label,
            preview,
        };
        if self.rows.len() > ROW_CACHE_LIMIT {
            self.rows.clear();
        }
        self.rows.insert(row_ix, row);
        let drop_target = drop_tint(master);
        let (shell, shell_drop) = (self.shell.clone(), self.shell.clone());
        // A track dropped on another row of the open playlist takes that row's place (the browser
        // refuses it anywhere else).
        line.text_size(px(size))
            .when_some(background, |el, colour| el.bg(colour))
            .when_some(edge, |el, colour| el.border_1().border_color(colour))
            .when(dimmed, |el| {
                el.border_dashed().text_color(paint(look.dimmed_text))
            })
            .drag_over::<TrackDragPayload>(move |style, _, _, _| style.bg(drop_target))
            .on_drop(move |payload: &TrackDragPayload, _, cx| {
                shell_drop
                    .update(cx, |this, cx| {
                        with_library(this, |p| p.drop_on_row(payload.row, row_ix));
                        cx.notify();
                    })
                    .ok();
            })
            .on_mouse_down(MouseButton::Left, move |ev: &MouseDownEvent, _, cx| {
                let (shift, double) = (ev.modifiers.shift, ev.click_count >= 2);
                shell
                    .update(cx, |this, cx| {
                        with_library(this, |p| {
                            if double {
                                let _ = p.activate_row(row_ix);
                            } else if shift {
                                p.extend_selection(row_ix);
                            } else {
                                p.select_row(row_ix);
                            }
                        });
                        cx.notify();
                    })
                    .ok();
            })
            .on_drag(payload, |payload: &TrackDragPayload, _, _, cx: &mut App| {
                cx.new(|_| DragPreview {
                    label: payload.label.clone(),
                    style: payload.preview,
                })
            })
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let Some((index, spec)) = self.shown().nth(col_ix) else {
            return div();
        };
        let cell = self
            .rows
            .get(&row_ix)
            .and_then(|row| row.cells.get(index))
            .cloned()
            .unwrap_or_default();
        let text = if spec.title == "Rating" {
            cell.replace('*', "\u{2605}").replace('-', "\u{2606}")
        } else {
            cell
        };
        if col_ix == 0 {
            let marks = self.rows.get(&row_ix).map(|row| (row.on_deck, row.error));
            if let Some((deck, error)) = marks.filter(|(d, e)| d.is_some() || e.is_some()) {
                return self
                    .shell
                    .read_with(cx, |shell, _| title_cell(&shell.theme, &text, deck, error))
                    .unwrap_or_else(|_| div().child(text));
            }
        }
        // BPM, key and length are numbers: mono, so a column of them lines up (the Mono Numbers Rule).
        let el = if spec.numeric {
            self.shell
                .read_with(cx, |shell, _| {
                    numeral(&shell.theme, Numeral::Value, text.clone())
                })
                .unwrap_or_else(|_| div().child(text))
        } else {
            div().child(text)
        };
        el.overflow_hidden().whitespace_nowrap()
    }

    fn render_empty(
        &mut self,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .px(px(8.))
            .child("No tracks match")
    }
}

/// How the library rows are tinted and edged, from the theme.
#[derive(Clone, Copy)]
struct RowLook {
    master: Colour,
    size: f32,
    compat: f32,
    selection: Colour,
    accents: [Colour; 4],
    dimmed_text: Colour,
    error_edge: Colour,
}

impl RowLook {
    fn from_tokens(t: &ThemeTokens) -> Self {
        Self {
            master: t.master,
            size: t.type_body,
            compat: t.library_compat_alpha,
            selection: t.accent_for(SELECTION_HUE),
            accents: std::array::from_fn(|i| t.accent_for(i as i32)),
            dimmed_text: t.text_muted,
            error_edge: t.border_strong,
        }
    }

    /// The row's fill and 1px edge: selected (slate tint and edge), loaded on a deck (the deck's
    /// edge), unplayable (the dashed graphic edge), or the key-compatibility tint, stronger when
    /// the tempo fits too.
    fn fill_and_edge(&self, row: &RowView) -> (Option<Hsla>, Option<Hsla>) {
        let shade = |c: Colour| Hsla::from(paint(c));
        if row.selected {
            return (
                Some(shade(self.selection.with_alpha(SELECTED_TINT))),
                Some(shade(self.selection.with_alpha(SELECTED_EDGE))),
            );
        }
        if row.error.is_some() {
            return (None, Some(shade(self.error_edge)));
        }
        let fill = row.compatible.then(|| {
            let alpha = if row.mix == MixLevel::Strong {
                self.compat
            } else {
                self.compat / 2.0
            };
            shade(self.master.with_alpha(alpha))
        });
        let edge = row
            .on_deck
            .map(|d| shade(self.accents[d % self.accents.len()].with_alpha(LOADED_EDGE)));
        (fill, edge)
    }
}

/// The title cell of a marked row: the letter of the deck it is loaded on, in that deck's hue, and
/// the reason in words when it would not load.
fn title_cell(t: &ThemeTokens, title: &str, deck: Option<usize>, error: Option<&str>) -> Div {
    let m = crate::controls::metrics(t);
    let mut el = div()
        .flex()
        .items_center()
        .gap(px(m.space[2]))
        .overflow_hidden()
        .whitespace_nowrap();
    if let Some(d) = deck {
        el = el.child(
            crate::font::mark(
                t,
                crate::font::MarkSize::Word,
                om_ui::widgets::deck_header::deck_letter(d).to_string(),
            )
            .text_color(paint(t.accent_for(d as i32))),
        );
    }
    el = el.child(
        div()
            .overflow_hidden()
            .text_ellipsis()
            .child(title.to_owned()),
    );
    if let Some(reason) = error {
        el = el.child(
            div()
                .flex_none()
                .type_role(t, TypeRole::Label)
                .text_color(paint(t.text_secondary))
                .child(reason.to_owned()),
        );
    }
    el
}

/// Draws the library card in `bounds` (window coordinates, the bottom layout slot).
pub fn library_card(
    shell: &mut Shell,
    bounds: Rect,
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> Div {
    let t: ThemeTokens = shell.theme.clone();
    let window_w = shell.window_size.0;
    let m = crate::controls::metrics(&t);
    let metrics = library_metrics(&m);
    let (row_h, header_h, toolbar_h, sidebar_w) = (
        metrics.row_h as f32,
        metrics.header_h as f32,
        metrics.toolbar_h as f32,
        metrics.sidebar_w as f32,
    );
    // Toolbar buttons are one control high, as everywhere on the faceplate.
    let button_h = m.control_height;
    // The sidebar's lines and the playlist buttons are one control high (comp sidebar).
    let line_h = m.control_height;
    let focus: FocusHandle = shell
        .library_focus
        .get_or_insert_with(|| cx.focus_handle())
        .clone();
    // The band sits on the faceplate under one etched line (comp library band), no card.
    let root = place(bounds)
        .border_t_1()
        .border_color(paint(t.border))
        .text_color(paint(t.text_primary))
        .type_role(&t, TypeRole::Body)
        .overflow_hidden()
        .track_focus(&focus)
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _: &MouseDownEvent, window, cx| {
                if let Some(focus) = this.library_focus.clone() {
                    focus.focus(window, cx);
                }
            }),
        )
        .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
            let keys = &ev.keystroke;
            if keys.modifiers.control || keys.modifiers.platform {
                // Ctrl or Cmd C copies the selected tracks, unless the search box is editing text.
                if keys.key == "c" && !this.search_has_focus(window, cx) {
                    this.copy_selected_tracks(window, cx);
                    cx.notify();
                }
                return;
            }
            // Keys typed in the search box are the box's own, except Escape.
            if this.search_has_focus(window, cx) {
                if keys.key == "escape" {
                    this.search_escape(window, cx);
                    cx.notify();
                }
                return;
            }
            let outcome =
                with_library(this, |p| p.handle_key(&keys.key)).unwrap_or(KeyOutcome::Ignored);
            match outcome {
                KeyOutcome::Search => {
                    // After this key, so the "/" is not typed into the box.
                    cx.defer_in(window, |this, window, cx| {
                        if let Some(input) = this.library_search.clone() {
                            input.update(cx, |input, cx| input.focus(window, cx));
                        }
                    });
                }
                KeyOutcome::Moved => {
                    let selected = this.library.as_ref().and_then(|p| p.browser.selected_row());
                    if let (Some(table), Some(row)) = (this.library_table.clone(), selected) {
                        table.update(cx, |table, cx| table.scroll_to_row(row, cx));
                    }
                }
                KeyOutcome::Ignored | KeyOutcome::Activated | KeyOutcome::Edited => {}
            }
            if outcome != KeyOutcome::Ignored {
                cx.notify();
            }
        }));

    let input = shell.search_input(window, cx);
    shell.sync_search_input(&input, window, cx);
    let table = shell.track_table(window, cx);
    let Some(panel) = shell.library.as_mut() else {
        return root.child(
            div()
                .p(px(PAD))
                .text_color(paint(t.text_secondary))
                .child("Library could not open. Open Settings > Library."),
        );
    };

    let count = panel.row_count();
    let sidebar = panel.sidebar_lines();
    let status = panel.browser.status().to_owned();
    let status_error = panel.browser.status_is_error();
    let stems_running = panel.stems_in_flight() > 0;
    let sort = panel.sort_indicator();

    // The table fills what the toolbar and the status line leave.
    let list_w = bounds.w as f32 - sidebar_w - 2.0 * PAD;
    let list_top = PAD / 2. + toolbar_h + header_h;
    let list_h = (bounds.h as f32 - list_top - PAD).max(0.);
    table.update(cx, |table, cx| {
        let delegate = table.delegate_mut();
        let moved = delegate.window_w != window_w
            || (delegate.list_w - list_w).abs() > 0.5
            || delegate.sort != sort;
        delegate.window_w = window_w;
        delegate.list_w = list_w;
        delegate.sort = sort;
        if moved {
            table.refresh(cx);
        }
    });

    // Sidebar tree.
    let mut tree = div()
        .absolute()
        .left(px(0.))
        .top(px(0.))
        .w(px(sidebar_w))
        .h(px(bounds.h as f32))
        .flex()
        .flex_col()
        .p(px(4.))
        .overflow_hidden();
    let drop_target = drop_tint(t.master);
    let preview = LookAndFeel::from_tokens(&t).tooltip;
    for (index, line) in sidebar.into_iter().enumerate() {
        let node = line.node.clone();
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
        let (text, bar) = if line.selected {
            (t.text_primary, Some(t.focus_ring))
        } else {
            (t.text_secondary, None)
        };
        tree = tree.child(
            ListItem::new(("sidebar-row", index))
                .selected(line.selected)
                .h(px(line_h))
                .py(px(0.))
                .pl(px(4. + 12. * line.depth as f32))
                .rounded(px(3.))
                .type_role(&t, TypeRole::Body)
                .text_color(paint(text))
                .when_some(bar, |el, colour| {
                    // Selection is a fill and a bar in the focus colour, so it does not rest on
                    // tone alone (labels-layout.md item 5).
                    el.child(
                        div()
                            .absolute()
                            .left(px(0.))
                            .top(px(2.))
                            .bottom(px(2.))
                            .w(px(SIDEBAR_BAR_W))
                            .rounded(px(1.5))
                            .bg(paint(colour)),
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

    // Toolbar: search box, analyse, stems, status line.
    let mut toolbar = div()
        .absolute()
        .left(px(sidebar_w + PAD))
        .top(px(PAD / 2.))
        .w(px(list_w))
        .h(px(toolbar_h))
        .flex()
        .items_center()
        .gap(px(4.))
        .child(
            div()
                .flex_1()
                .min_w(px(t.library_search_min_width))
                .h(px(button_h))
                .child(Input::new(&input).with_size(search_size(button_h))),
        );
    toolbar = toolbar.child(
        div().h(px(button_h)).flex_none().child(
            push_button(&t, "Copy", None, cx, |this, window, cx| {
                this.copy_selected_tracks(window, cx)
            })
            .h_full()
            .px(px(10.)),
        ),
    );
    toolbar = toolbar.child(action_button(&t, button_h, "Analyse", false, cx, |p| {
        p.prep_selected();
    }));
    toolbar = toolbar.child(action_button(
        &t,
        button_h,
        om_ui::labels::SPLIT_STEMS,
        false,
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
            false,
            cx,
            |p| {
                p.cancel_stems_selected();
            },
        ));
    }

    // The playlist actions follow the track actions on the toolbar, so the sidebar's tree keeps the
    // band's whole height (the faceplate's band is short in Compact).
    toolbar = toolbar
        .child(action_button(&t, button_h, "New", false, cx, |p| {
            p.begin_new_playlist()
        }))
        .child(action_button(&t, button_h, "Rename", false, cx, |p| {
            p.begin_rename_playlist()
        }))
        .child(action_button(
            &t,
            button_h,
            om_ui::labels::NEW_FOLDER,
            false,
            cx,
            |p| p.begin_new_folder(),
        ))
        .child(action_button(
            &t,
            button_h,
            om_ui::labels::DELETE_PLAYLIST,
            false,
            cx,
            |p| p.delete_playlist(),
        ))
        .child(action_button(
            &t,
            button_h,
            om_ui::labels::REMOVE_FROM_PLAYLIST,
            false,
            cx,
            |p| {
                p.remove_selected_from_playlist();
            },
        ));

    // An error states its reason in words at full text strength; `highlight` is now the top edge.
    let status_colour = if status_error {
        t.text_primary
    } else {
        t.text_secondary
    };
    let status_line = div()
        .absolute()
        .left(px(sidebar_w + PAD))
        .top(px(PAD / 2. + toolbar_h))
        .w(px(list_w))
        .h(px(header_h))
        .flex()
        .items_center()
        .text_color(paint(status_colour))
        .overflow_hidden()
        .child(om_ui::labels::library_status(&status, count));

    let list = div()
        .absolute()
        .left(px(sidebar_w + PAD))
        .top(px(list_top))
        .w(px(list_w))
        .h(px(list_h))
        .child(
            DataTable::new(&table)
                .with_size(Size::Size(px(row_h)))
                .header_row_height(px(header_h))
                .stripe(true)
                .bordered(false),
        );

    root.child(tree)
        .child(toolbar)
        .child(status_line)
        .child(list)
}

/// The search box's component size: the one whose height fits a control `button_h` px high.
fn search_size(button_h: f32) -> Size {
    if button_h >= 26.0 {
        Size::Small
    } else {
        Size::XSmall
    }
}

/// A toolbar button that runs `on_press` on the panel. `half` makes two of them share a row.
fn action_button(
    t: &ThemeTokens,
    button_h: f32,
    label: &str,
    half: bool,
    cx: &mut Context<Shell>,
    on_press: impl Fn(&mut LibraryPanel) + 'static,
) -> Div {
    let button = push_button(t, label, None, cx, move |this: &mut Shell, _, _| {
        with_library(this, |p| on_press(p));
        if with_library(this, |p| p.take_licence_request()).unwrap_or(false) {
            this.show_stems_licence();
        }
    })
    .h_full()
    .px(px(10.));
    let cell = div().h(px(button_h)).flex_none();
    if half {
        cell.flex_1().child(button.w_full())
    } else {
        cell.child(button)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(selected: bool, on_deck: Option<usize>, error: Option<&'static str>) -> RowView {
        RowView {
            row: 0,
            track_id: 1,
            cells: Vec::new(),
            compatible: true,
            mix: MixLevel::Strong,
            selected,
            on_deck,
            error,
            drag: String::new(),
        }
    }

    fn shade(c: Colour) -> Hsla {
        Hsla::from(paint(c))
    }

    #[test]
    fn a_selected_row_is_the_slate_tint_with_its_edge() {
        let t = ThemeTokens::builtin_dark();
        let (fill, edge) = RowLook::from_tokens(&t).fill_and_edge(&row(true, Some(0), None));
        let slate = t.accent_for(1);
        assert_eq!(fill, Some(shade(slate.with_alpha(0.18))));
        assert_eq!(edge, Some(shade(slate.with_alpha(0.55))));
    }

    #[test]
    fn a_loaded_row_takes_its_decks_edge_and_an_unplayable_one_the_graphic_edge() {
        let t = ThemeTokens::builtin_dark();
        let look = RowLook::from_tokens(&t);
        let (_, edge) = look.fill_and_edge(&row(false, Some(1), None));
        assert_eq!(edge, Some(shade(t.accent_for(1).with_alpha(0.40))));
        let (fill, edge) = look.fill_and_edge(&row(false, None, Some("can't read file")));
        assert_eq!((fill, edge), (None, Some(shade(t.border_strong))));
        assert_ne!(t.border_strong, t.playhead, "errors are never red");
    }
}

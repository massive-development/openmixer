//! The Tauri commands of the library browser. Each hands one closure to the control thread (see
//! `library`), which owns the hub; the answer is the browser's whole view, so the front end redraws
//! from one reply and never keeps a second copy of the library's state. Commands that change
//! something answer with the view of the same window the front end last asked for.

use crate::commands::ShellState;
use crate::library::{self, LibraryView};
use om_ui::library::hub::LibraryPanel;
use serde::Deserialize;
use tauri::State;

/// The rows the front end draws: from `first`, `count` of them.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Window {
    pub first: usize,
    pub count: usize,
}

/// Runs `act` on the panel, then answers with the view of `window`.
fn act(
    state: &ShellState,
    window: Window,
    act: impl FnOnce(&mut LibraryPanel) + Send + 'static,
) -> Result<LibraryView, String> {
    state.library.call(move |panel| {
        act(panel);
        library::view(panel, window.first, window.count)
    })
}

/// The browser as it is now.
#[tauri::command(async)]
pub fn library_view(window: Window, state: State<'_, ShellState>) -> Result<LibraryView, String> {
    act(&state, window, |_| {})
}

/// The search box's text changed (`commit` is Enter: the query runs now; else it runs shortly).
#[tauri::command(async)]
pub fn library_search(
    text: String,
    commit: bool,
    window: Window,
    state: State<'_, ShellState>,
) -> Result<LibraryView, String> {
    act(&state, window, move |panel| {
        panel.set_query(&text);
        if commit {
            panel.commit_query();
        }
    })
}

/// Escape in the search box: the query is dropped.
#[tauri::command(async)]
pub fn library_clear_search(
    window: Window,
    state: State<'_, ShellState>,
) -> Result<LibraryView, String> {
    act(&state, window, |panel| panel.clear_query())
}

/// A click on sidebar line `index` (a heading does nothing).
#[tauri::command(async)]
pub fn library_select_sidebar(
    index: usize,
    window: Window,
    state: State<'_, ShellState>,
) -> Result<LibraryView, String> {
    act(&state, window, move |panel| {
        if let Some(line) = panel.sidebar_lines().into_iter().nth(index) {
            panel.select_sidebar(&line.node);
        }
    })
}

/// A header click: sorts by the column, flipping direction on a second click.
#[tauri::command(async)]
pub fn library_sort(
    column: i32,
    window: Window,
    state: State<'_, ShellState>,
) -> Result<LibraryView, String> {
    act(&state, window, move |panel| {
        panel.sort_by_column(column);
    })
}

/// A click on a row; `extend` (shift) selects from the first selected row to it.
#[tauri::command(async)]
pub fn library_select_row(
    row: usize,
    extend: bool,
    window: Window,
    state: State<'_, ShellState>,
) -> Result<LibraryView, String> {
    act(&state, window, move |panel| {
        if extend {
            panel.extend_selection(row);
        } else {
            panel.select_row(row);
        }
    })
}

/// A key while no text box has the keyboard: `up`, `down`, `enter`, `p`, `/`. Answers with what it
/// did (`search`, `moved`, `activated`, `edited`, `ignored`) beside the view.
#[tauri::command(async)]
pub fn library_key(
    key: String,
    window: Window,
    state: State<'_, ShellState>,
) -> Result<(String, LibraryView), String> {
    state.library.call(move |panel| {
        let outcome = library::key(panel, &key).to_owned();
        (outcome, library::view(panel, window.first, window.count))
    })
}

/// A double-click: loads the row into the suggested deck.
#[tauri::command(async)]
pub fn library_activate(
    row: usize,
    window: Window,
    state: State<'_, ShellState>,
) -> Result<LibraryView, String> {
    act(&state, window, move |panel| {
        let _ = panel.activate_row(row);
    })
}

/// Loads library track `track_id` into `deck` (a drop on a deck, or a button); the reason it did
/// not load is the browser's status line.
#[tauri::command(async)]
pub fn library_load(
    deck: usize,
    track_id: i64,
    window: Window,
    state: State<'_, ShellState>,
) -> Result<LibraryView, String> {
    act(&state, window, move |panel| {
        let _ = panel.load_track_id(deck, track_id);
    })
}

/// "Analyse": queues the selected rows for analysis.
#[tauri::command(async)]
pub fn library_analyse(
    window: Window,
    state: State<'_, ShellState>,
) -> Result<LibraryView, String> {
    act(&state, window, |panel| {
        panel.prep_selected();
    })
}

/// Splits stems for the selection, or cancels it (`cancel`); the toolbar offers both only with stems.
#[tauri::command(async)]
pub fn library_stems(
    cancel: bool,
    window: Window,
    state: State<'_, ShellState>,
) -> Result<LibraryView, String> {
    act(&state, window, move |panel| {
        if cancel {
            panel.cancel_stems_selected();
        } else {
            panel.prepare_stems();
        }
    })
}

/// The playlist name box: `new-playlist`, `new-folder`, `rename`, `text`, `submit` or `cancel`.
#[tauri::command(async)]
pub fn library_name(
    action: String,
    text: Option<String>,
    window: Window,
    state: State<'_, ShellState>,
) -> Result<LibraryView, String> {
    state.library.call(move |panel| {
        library::name_action(panel, &action, text.as_deref());
        library::view(panel, window.first, window.count)
    })
}

/// "Delete playlist": asks on the first press and deletes on the second (see the status line).
#[tauri::command(async)]
pub fn library_delete_playlist(
    window: Window,
    state: State<'_, ShellState>,
) -> Result<LibraryView, String> {
    act(&state, window, |panel| panel.delete_playlist())
}

/// "Remove from playlist": takes the selected rows out of the playlist shown.
#[tauri::command(async)]
pub fn library_remove_from_playlist(
    window: Window,
    state: State<'_, ShellState>,
) -> Result<LibraryView, String> {
    act(&state, window, |panel| {
        panel.remove_selected_from_playlist();
    })
}

/// Track `track_id` dropped on playlist `playlist_id`: it, or the whole selection when it is part
/// of it, joins the playlist.
#[tauri::command(async)]
pub fn library_drop_on_playlist(
    playlist_id: i64,
    track_id: i64,
    window: Window,
    state: State<'_, ShellState>,
) -> Result<LibraryView, String> {
    act(&state, window, move |panel| {
        let description = om_ui::library::describe_track_drag(track_id);
        panel.drop_on_playlist(playlist_id, &description);
    })
}

/// A playlist or folder dropped on a folder (`folder`), or on the Playlists heading (none).
#[tauri::command(async)]
pub fn library_drop_playlist(
    playlist_id: i64,
    folder: Option<i64>,
    window: Window,
    state: State<'_, ShellState>,
) -> Result<LibraryView, String> {
    act(&state, window, move |panel| {
        panel.drop_playlist_on(playlist_id, folder);
    })
}

/// The track on `source` dropped on row `target` of the open playlist: reorders it.
#[tauri::command(async)]
pub fn library_drop_on_row(
    source: usize,
    target: usize,
    window: Window,
    state: State<'_, ShellState>,
) -> Result<LibraryView, String> {
    act(&state, window, move |panel| {
        panel.drop_on_row(source, target);
    })
}

/// The selected tracks as text for the clipboard, one per line.
#[tauri::command(async)]
pub fn library_copy(state: State<'_, ShellState>) -> Result<String, String> {
    state.library.call(|panel| panel.selected_summary())
}

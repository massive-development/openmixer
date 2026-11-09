//! The waveform stage under the deck header row: deck A's and deck B's zoomed waveforms stacked,
//! full width (comp 4a/4b). The strips are `deck_view::zoom`, placed by `om_ui::widgets::stage`.

use crate::app::Shell;
use crate::deck_view::zoom;
use crate::paint::place;
use gpui::{prelude::*, Context, Div};
use om_ui::geometry::Rect;
use om_ui::widgets::stage::stage_layout;

/// The stage, drawn at `bounds` (window coordinates).
pub fn stage_card(shell: &Shell, bounds: Rect, cx: &mut Context<Shell>) -> Div {
    let strips = stage_layout(Rect::sized(bounds.w, bounds.h), shell.engine.num_decks());
    place(bounds).children(
        strips
            .into_iter()
            .enumerate()
            .map(|(index, strip)| zoom(shell, index, strip, cx)),
    )
}

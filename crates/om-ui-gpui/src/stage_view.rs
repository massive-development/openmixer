//! The waveform stage across the top of the window: each deck's header and overview side by side,
//! with both zoomed waveforms under them, full width and stacked. The parts are the deck card's own
//! (`deck_view`), placed by `om_ui::widgets::stage`; the controls stay in the deck cards below. A
//! library track dropped on a deck's header loads into that deck.

use crate::app::Shell;
use crate::deck_view::{header, overview, zoom};
use crate::library_view::TrackDragPayload;
use crate::mixer_view::fx_page;
use crate::paint::{paint, place};
use gpui::{prelude::*, px, Context, Div};
use om_ui::geometry::Rect;
use om_ui::widgets::stage::stage_layout;

/// Width of the deck-colour bar at the left end of a zoomed waveform.
const ACCENT_WIDTH: i32 = 3;

/// The stage, drawn at `bounds` (window coordinates).
pub fn stage_card(shell: &Shell, bounds: Rect, cx: &mut Context<Shell>) -> Div {
    let t = &shell.theme;
    let layouts = stage_layout(Rect::sized(bounds.w, bounds.h), t, shell.engine.num_decks());
    let drop_target = t.drop_target;

    let mut root = place(bounds)
        .overflow_hidden()
        .bg(paint(t.surface))
        .border_1()
        .border_color(paint(t.border))
        .rounded(px(t.control_radius))
        .text_color(paint(t.text_primary));

    // The effects page takes the stage's place while it is open; the strips and the module stay.
    if shell.mixer.fx_visible() {
        let body = Rect::sized(bounds.w, bounds.h).reduced(t.pad, t.pad);
        return root.children(fx_page(shell, body, bounds, cx));
    }

    for (index, l) in layouts.iter().enumerate() {
        let head = header(shell, index, l)
            .drag_over::<TrackDragPayload>(move |style, _, _, _| style.bg(paint(drop_target)))
            .on_drop(cx.listener(move |this, payload: &TrackDragPayload, _, cx| {
                if let Some(panel) = this.library.as_mut() {
                    let _ = panel.drop_on_deck(index, &payload.description);
                }
                cx.notify();
            }));
        root = root.child(head);
        if !l.overview.is_empty() {
            root = root.child(overview(shell, index, bounds, l, cx));
        }
        if l.zoom_visible {
            root = root.child(zoom(shell, index, bounds, l, cx));
            root = root.child(
                place(Rect::new(l.zoom.x, l.zoom.y, ACCENT_WIDTH, l.zoom.h))
                    .bg(paint(t.accent_for(index as i32))),
            );
        }
    }
    root
}

//! Application shell: the themed main window. Lays the decks, the mixer and the library placeholder
//! out with om-ui's `MainLayout` rectangles, owns the om-ui widget models, and ticks the engine's
//! control side once per frame.

use crate::deck_view::deck_card;
use crate::paint::{paint, place};
use gpui::{
    div, prelude::*, px, size, App, Application, Bounds, Context, Window, WindowBounds, WindowOptions,
};
use om_core::{ParameterRouter, ParameterTarget};
use om_engine::DjEngine;
use om_ui::binding::DeckBinding;
use om_ui::geometry::Rect;
use om_ui::main_layout::{visible_cells, CellKind, LayoutSpec, PlacedCell, LIBRARY_SLOT, MIXER_SLOT};
use om_ui::theme::{ThemeRegistry, ThemeTokens};
use om_ui::widgets::deck::DeckWidget;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// Height of the banner strip shown above the layout (no audio device, and so on).
const BANNER_HEIGHT: i32 = 24;

/// What the caller decides before the window opens.
pub struct RunOptions {
    /// Quit after this many rendered frames and return (`--smoke`). `None` runs until closed.
    pub smoke_frames: Option<u64>,
    /// Visible notice above the layout, such as the mock-device fallback.
    pub banner: Option<String>,
}

/// The root view. Owns the widget models and the handles they read from.
pub struct Shell {
    pub(crate) engine: Arc<DjEngine>,
    pub(crate) theme: ThemeTokens,
    /// One card per engine deck, in deck order.
    pub(crate) decks: Vec<DeckWidget>,
    options: RunOptions,
    started: Instant,
    last_tick: f64,
    frames: u64,
    frames_out: Arc<AtomicU64>,
}

/// The theme the window uses: the active theme of a registry loaded with the built-ins.
pub fn load_theme() -> ThemeTokens {
    let mut registry = ThemeRegistry::new();
    registry.load_builtins();
    registry.select(ThemeRegistry::DEFAULT_ID);
    registry.current().clone()
}

/// Cells for a window `area` (banner excluded), in draw order.
pub fn cells_for(engine_decks: usize, area: Rect, gap: i32) -> Vec<PlacedCell> {
    visible_cells(&LayoutSpec::standard(engine_decks), engine_decks, area, gap)
}

impl Shell {
    fn new(
        engine: Arc<DjEngine>,
        router: Arc<ParameterRouter>,
        theme: ThemeTokens,
        options: RunOptions,
        frames_out: Arc<AtomicU64>,
    ) -> Self {
        let target: Arc<dyn ParameterTarget> = router;
        let decks = (0..engine.num_decks())
            .map(|index| {
                DeckWidget::new(DeckBinding::new(
                    Arc::clone(&engine),
                    Arc::clone(&target),
                    index,
                ))
            })
            .collect();
        Self {
            engine,
            theme,
            decks,
            options,
            started: Instant::now(),
            last_tick: 0.0,
            frames: 0,
            frames_out,
        }
    }

    /// Frames rendered so far.
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// One frame of upkeep: engine control tick, then the widgets follow the engine.
    fn tick(&mut self) {
        let now = self.started.elapsed().as_secs_f64();
        self.engine.update_control(now - self.last_tick);
        self.last_tick = now;
        for deck in &mut self.decks {
            deck.refresh(false);
        }
        self.frames += 1;
        self.frames_out.store(self.frames, Ordering::Relaxed);
    }

    fn cell_view(&self, cell: &PlacedCell, cx: &mut Context<Self>) -> gpui::Div {
        let t = &self.theme;
        match &cell.kind {
            CellKind::Deck(index) => deck_card(self, *index, cell.bounds, cx),
            CellKind::Slot(name) => {
                let (title, accent) = if name == MIXER_SLOT {
                    ("Mixer".to_owned(), t.master)
                } else if name == LIBRARY_SLOT {
                    ("Library (analysis not built yet)".to_owned(), t.border)
                } else {
                    (name.clone(), t.border)
                };
                place(cell.bounds)
                    .bg(paint(t.surface))
                    .border_1()
                    .border_color(paint(accent))
                    .rounded(px(t.control_radius))
                    .text_color(paint(t.text_primary))
                    .p(px(t.pad as f32))
                    .child(title)
            }
        }
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.tick();
        window.request_animation_frame();
        if self.options.smoke_frames.is_some_and(|limit| self.frames >= limit) {
            cx.quit();
        }

        let viewport = window.viewport_size();
        let width = f32::from(viewport.width) as i32;
        let height = f32::from(viewport.height) as i32;
        let banner = self.options.banner.clone();
        let top = if banner.is_some() { BANNER_HEIGHT } else { 0 };
        let area = Rect::new(0, top, width, (height - top).max(0));
        let gap = self.theme.gap;

        let mut root = div()
            .size_full()
            .relative()
            .bg(paint(self.theme.backdrop_top))
            .text_color(paint(self.theme.text_primary));
        if let Some(text) = banner {
            root = root.child(
                place(Rect::new(0, 0, width, BANNER_HEIGHT))
                    .bg(paint(self.theme.popup))
                    .text_color(paint(self.theme.text_secondary))
                    .px(px(8.))
                    .child(text),
            );
        }
        for cell in cells_for(self.engine.num_decks(), area, gap) {
            root = root.child(self.cell_view(&cell, cx));
        }
        root
    }
}

/// Opens the main window and runs the GPUI event loop until it closes (or `smoke_frames` pass).
/// Returns the number of frames rendered.
pub fn run(engine: Arc<DjEngine>, router: Arc<ParameterRouter>, options: RunOptions) -> u64 {
    let theme = load_theme();
    let frames_out = Arc::new(AtomicU64::new(0));
    let counter = Arc::clone(&frames_out);
    Application::new().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1280.), px(800.)), cx);
        let mut slot = Some((engine, router, theme, options, counter));
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            move |_, cx| {
                let (engine, router, theme, options, counter) =
                    slot.take().expect("window opened once");
                cx.new(|_| Shell::new(engine, router, theme, options, counter))
            },
        )
        .expect("open main window");
        cx.activate(true);
    });
    frames_out.load(Ordering::Relaxed)
}

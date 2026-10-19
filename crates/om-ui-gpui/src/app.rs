//! Application shell: the themed main window. Lays the decks, the mixer and the library placeholder
//! out with om-ui's `MainLayout` rectangles, owns the om-ui widget models, and ticks the engine's
//! control side once per frame.

use crate::controls::release_on;
use crate::debug_panel::{device_rows, parameter_rows, DeviceStatus};
use crate::deck_view::deck_card;
use crate::drag::Drag;
use crate::library::{LibraryPanel, SharedHub};
use crate::library_view::library_card;
use crate::mixer_view::mixer_card;
use crate::paint::{paint, place};
use crate::settings_view::{SettingsOptions, SettingsState, HEADER_HEIGHT};
use gpui::{
    div, prelude::*, px, size, App, Application, Bounds, Context, FocusHandle, Window,
    WindowBounds, WindowOptions,
};
use om_analysis::WaveformData;
use om_core::ParameterRouter;
use om_engine::DjEngine;
use om_ui::binding::DeckBinding;
use om_ui::binding::MixerBinding;
use om_ui::geometry::Rect;
use om_ui::main_layout::{
    visible_cells, CellKind, LayoutSpec, PlacedCell, LIBRARY_SLOT, MIXER_SLOT,
};
use om_ui::meter::meter_params;
use om_ui::theme::{ThemeRegistry, ThemeTokens};
use om_ui::widgets::deck::{DeckWidget, TrackInfo, WaveformMeta};
use om_ui::widgets::mixer::MixerPanel;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// Height of the banner strip shown above the layout (no audio device, and so on).
const BANNER_HEIGHT: i32 = 24;

/// Reads the device's figures for the debug panel. Called on the UI thread once per frame.
pub type DeviceStatusFn = Arc<dyn Fn() -> DeviceStatus + Send + Sync>;

/// Width of the `--debug-panel` column on the right.
const DEBUG_WIDTH: i32 = 340;

/// What the caller decides before the window opens.
pub struct RunOptions {
    /// Quit after this many rendered frames and return (`--smoke`). `None` runs until closed.
    pub smoke_frames: Option<u64>,
    /// Visible notice above the layout, such as the mock-device fallback.
    pub banner: Option<String>,
    /// Shows the debug column (`--debug-panel`) and reads the device through this.
    pub debug: Option<DeviceStatusFn>,
    /// The settings overlay (Settings button in the header). `None` leaves the header out.
    pub settings: Option<SettingsOptions>,
    /// The library (services and loader), shared with the caller. `None` shows a placeholder in
    /// the library slot.
    pub library: Option<SharedHub>,
    /// Receives the number of rows in the library table after each frame (the `--smoke` check).
    pub library_rows_out: Option<Arc<AtomicUsize>>,
}

/// The root view. Owns the widget models and the handles they read from.
pub struct Shell {
    pub(crate) engine: Arc<DjEngine>,
    router: Arc<ParameterRouter>,
    pub(crate) theme: ThemeTokens,
    /// One card per engine deck, in deck order.
    pub(crate) decks: Vec<DeckWidget>,
    /// The mixer page: channel strips, master, crossfader, cue and FX.
    pub(crate) mixer: MixerPanel,
    /// The control being dragged, if any.
    pub(crate) drag: Option<Drag>,
    /// The settings overlay's state, when the window was given settings.
    pub(crate) settings: Option<SettingsState>,
    /// The library browser, when the window was given the library.
    pub(crate) library: Option<LibraryPanel>,
    /// Focus for the library card: its keys (search, selection) go here. Made on the first draw.
    pub(crate) library_focus: Option<FocusHandle>,
    /// Focus for the settings card: its keys (text fields, Escape) go here while it is open.
    pub(crate) settings_focus: Option<FocusHandle>,
    /// What each deck card last showed, so the widgets only change on a real change.
    shown_info: Vec<TrackInfo>,
    shown_waveform: Vec<Option<Arc<WaveformData>>>,
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
    /// The window's root state. Creates the widget models over the engine's control surface.
    pub(crate) fn new(
        engine: Arc<DjEngine>,
        router: Arc<ParameterRouter>,
        theme: ThemeTokens,
        mut options: RunOptions,
        frames_out: Arc<AtomicU64>,
    ) -> Self {
        let settings = options.settings.take().map(SettingsState::new);
        let library = options
            .library
            .take()
            .map(|hub| LibraryPanel::new(hub, Arc::clone(&engine)));
        let deck_count = engine.num_decks();
        let mixer = MixerPanel::new(
            MixerBinding::typed(Arc::clone(&engine), Arc::clone(&router)),
            meter_params(&theme),
        );
        let decks = (0..engine.num_decks())
            .map(|index| {
                DeckWidget::new(DeckBinding::typed(
                    Arc::clone(&engine),
                    Arc::clone(&router),
                    index,
                ))
            })
            .collect();
        Self {
            engine,
            router,
            theme,
            decks,
            mixer,
            drag: None,
            settings,
            library,
            library_focus: None,
            settings_focus: None,
            shown_info: vec![TrackInfo::default(); deck_count],
            shown_waveform: vec![None; deck_count],
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

    /// One frame of upkeep: the widgets follow the engine and the meters advance by the frame
    /// time. The engine's control clock runs on its own thread (see the composition), so it keeps
    /// going when no frames are drawn.
    fn tick(&mut self) {
        let now = self.started.elapsed().as_secs_f64();
        let dt = (now - self.last_tick) as f32;
        self.last_tick = now;
        let dragging = self.dragging_channel_fader();
        if let Some(panel) = self.library.as_mut() {
            panel.tick();
        }
        // Each deck card follows the library loader: header metadata and the analysed waveform.
        // Widgets are only changed when the loaded track or its analysis changed.
        for (index, deck) in self.decks.iter_mut().enumerate() {
            let Some(panel) = self.library.as_ref() else {
                break;
            };
            let info = panel.track_info(index);
            if self.shown_info[index] != info {
                deck.header.set_track(info.clone());
                self.shown_info[index] = info;
            }
            let wave = panel.waveform(index);
            let changed = match (&wave, &self.shown_waveform[index]) {
                (None, None) => false,
                (Some(now), Some(shown)) => !Arc::ptr_eq(now, shown),
                _ => true,
            };
            if changed {
                deck.zoom.set_waveform(wave.as_ref().map(|w| WaveformMeta {
                    sample_rate: w.sample_rate(),
                }));
                self.shown_waveform[index] = wave;
            }
        }
        for deck in &mut self.decks {
            deck.refresh(false);
        }
        self.mixer.refresh(dt, self.engine.mixer(), dragging);
        self.frames += 1;
        self.frames_out.store(self.frames, Ordering::Relaxed);
        if let (Some(out), Some(panel)) = (&self.options.library_rows_out, &self.library) {
            out.store(panel.row_count(), Ordering::Relaxed);
        }
    }

    /// The `--debug-panel` column: device figures, then the raw control-surface values.
    fn debug_column(&self, status: &DeviceStatus, bounds: Rect) -> gpui::Div {
        let t = &self.theme;
        let mut lines = vec![("Device".to_owned(), String::new())];
        lines.extend(device_rows(status));
        lines.push((String::new(), String::new()));
        lines.push(("Parameters".to_owned(), String::new()));
        lines.extend(parameter_rows(&self.engine, self.router.unknown_count()));
        let mut body = div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .text_size(px(t.type_caption));
        for (label, value) in lines {
            let row = div()
                .flex()
                .justify_between()
                .child(div().text_color(paint(t.text_secondary)).child(label))
                .child(div().text_color(paint(t.text_primary)).child(value));
            body = body.child(row);
        }
        place(bounds)
            .bg(paint(t.popup))
            .border_1()
            .border_color(paint(t.border))
            .p(px(6.))
            .overflow_hidden()
            .child(body)
    }

    fn cell_view(&mut self, cell: &PlacedCell, cx: &mut Context<Self>) -> gpui::Div {
        let t = &self.theme;
        match &cell.kind {
            CellKind::Deck(index) => deck_card(self, *index, cell.bounds, cx),
            CellKind::Slot(name) if name == MIXER_SLOT => mixer_card(self, cell.bounds, cx),
            CellKind::Slot(name) if name == LIBRARY_SLOT && self.library.is_some() => {
                library_card(self, cell.bounds, cx)
            }
            CellKind::Slot(name) => {
                let title = if name == LIBRARY_SLOT {
                    "Library not available (see the terminal for the reason)".to_owned()
                } else {
                    name.clone()
                };
                place(cell.bounds)
                    .bg(paint(t.surface))
                    .border_1()
                    .border_color(paint(t.border))
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
        if self
            .options
            .smoke_frames
            .is_some_and(|limit| self.frames >= limit)
        {
            cx.quit();
        }

        let viewport = window.viewport_size();
        let width = f32::from(viewport.width) as i32;
        let height = f32::from(viewport.height) as i32;
        let banner = self.options.banner.clone();
        let banner_h = if banner.is_some() { BANNER_HEIGHT } else { 0 };
        let header_h = if self.settings.is_some() {
            HEADER_HEIGHT
        } else {
            0
        };
        let top = banner_h + header_h;
        let debug_w = if self.options.debug.is_some() {
            DEBUG_WIDTH
        } else {
            0
        };
        let area = Rect::new(0, top, (width - debug_w).max(0), (height - top).max(0));
        let gap = self.theme.gap;

        let mut root = release_on(
            div()
                .size_full()
                .relative()
                .bg(paint(self.theme.backdrop_top))
                .text_color(paint(self.theme.text_primary))
                .on_mouse_move(cx.listener(|this, ev: &gpui::MouseMoveEvent, _, cx| {
                    this.drag_move(ev.position);
                    cx.notify();
                })),
            cx,
        );
        if let Some(header) = self.header(banner_h, width, cx) {
            root = root.child(header);
        }
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
        if let Some(status) = self.options.debug.clone() {
            root = root.child(self.debug_column(
                &status(),
                Rect::new(width - debug_w, top, debug_w, (height - top).max(0)),
            ));
        }
        if let Some(overlay) = self.settings_overlay(Rect::new(0, 0, width, height), window, cx) {
            root = root.child(overlay);
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

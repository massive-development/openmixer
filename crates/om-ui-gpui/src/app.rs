//! Application shell: the themed main window. Lays the decks, the mixer and the library placeholder
//! out with om-ui's `MainLayout` rectangles, owns the om-ui widget models, and ticks the engine's
//! control side once per frame.

use crate::controls::release_on;
use crate::debug_panel::{decks_section, device_rows, parameter_rows, DeckControls, DeviceStatus};
use crate::deck_view::deck_card;
use crate::drag::Drag;
use crate::library::{LibraryPanel, SharedHub};
use crate::library_view::library_card;
use crate::mixer_view::{channel_card, mixer_card};
use crate::paint::{paint, place};
use crate::sampler_view::sampler_card;
use crate::settings_view::{SettingsOptions, SettingsState};
use crate::stage_view::stage_card;
use gpui::{
    div, prelude::*, px, size, App, Application, AsyncApp, Bounds, Context, FocusHandle,
    KeyDownEvent, Window, WindowBounds, WindowOptions,
};
use om_analysis::WaveformData;
use om_core::ParameterRouter;
use om_core::{CommandSink, ParameterTarget};
use om_engine::DjEngine;
use om_ui::artwork_binding::{ArtworkLoader, ArtworkView, DeckArtwork};
use om_ui::binding::DeckBinding;
use om_ui::binding::MixerBinding;
use om_ui::geometry::Rect;
use om_ui::main_layout::{
    channel_of_slot, page_for, visible_cells, CellKind, LayoutPlan, LayoutState, PlacedCell,
    LIBRARY_SLOT, MIN_WINDOW, MIXER_SLOT, SAMPLER_SLOT, STAGE_SLOT,
};
use om_ui::meter::meter_params;
use om_ui::sample_bank::SampleBank;
use om_ui::sampler_binding::SamplerBinding;
use om_ui::style::LookAndFeel;
use om_ui::theme::{ThemeRegistry, ThemeTokens};
use om_ui::widgets::deck::{DeckWidget, TrackInfo, WaveformMeta};
use om_ui::widgets::mixer::MixerPanel;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// Reads the device's figures for the debug panel. Called on the UI thread once per frame.
pub type DeviceStatusFn = Arc<dyn Fn() -> DeviceStatus + Send + Sync>;

/// Width of the `--debug-panel` column on the right.
const DEBUG_WIDTH: i32 = 300;

/// DEMO / TEST ONLY (`--demo-scene`): window state staged for screenshots. All off by default.
#[derive(Debug, Clone, Default)]
pub struct SceneOptions {
    /// Start with the sampler drawer open.
    pub sampler_open: bool,
    /// Start with deck A on its STEMS tab.
    pub stems_tab: bool,
    /// Switch deck A's platter on.
    pub platter: bool,
    /// Press this sampler pad once its sample has loaded.
    pub press_pad: Option<usize>,
}

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
    /// Demo only: a fixed-tempo beat grid (BPM) put on deck A each time its analysis lands.
    pub demo_grid_bpm: Option<f64>,
    /// The sample bank behind the sampler strip (loads and per-pad settings). `None` leaves loads
    /// refused; the pads still play what the engine holds.
    pub samples: Option<Arc<SampleBank>>,
    /// Called on the window thread every [`CONTROL_TICK`] by a foreground timer, independent of
    /// rendering (a minimised or occluded window still ticks). The app steps what must run on this
    /// thread (automix, whose host reads the library).
    pub on_frame: Option<Box<dyn FnMut()>>,
    /// The automix bar under the header: reads the automix snapshot. `None` leaves the bar out.
    pub automix: Option<crate::automix_bar::AutomixSnapshotFn>,
    /// Shows the Record button in the header and the `r` key (the window sends `record.toggle`).
    pub record: bool,
    /// Demo only: staged window state for screenshots.
    pub scene: SceneOptions,
}

/// The root view. Owns the widget models and the handles they read from.
pub struct Shell {
    pub(crate) engine: Arc<DjEngine>,
    pub(crate) router: Arc<ParameterRouter>,
    pub(crate) theme: ThemeTokens,
    /// One card per engine deck, in deck order.
    pub(crate) decks: Vec<DeckWidget>,
    /// The mixer page: channel strips, master, crossfader, cue and FX.
    pub(crate) mixer: MixerPanel,
    /// The control being dragged, if any.
    pub(crate) drag: Option<Drag>,
    /// The knob under the pointer, if any: it shows its value in a popup.
    pub(crate) hovered: Option<crate::drag::Target>,
    /// The settings overlay's state, when the window was given settings.
    pub(crate) settings: Option<SettingsState>,
    /// The library browser, when the window was given the library.
    pub(crate) library: Option<LibraryPanel>,
    /// Focus for the library card: its keys (search, selection) go here. Made on the first draw.
    pub(crate) library_focus: Option<FocusHandle>,
    /// Focus for the settings card: its keys (text fields, Escape) go here while it is open.
    pub(crate) settings_focus: Option<FocusHandle>,
    /// Focus for the debug column: its path fields take the keys while one is open.
    pub(crate) debug_focus: Option<FocusHandle>,
    /// The debug column's per-deck path fields and its status line.
    pub(crate) deck_controls: DeckControls,
    /// What each deck card last showed, so the widgets only change on a real change.
    shown_info: Vec<TrackInfo>,
    /// Cover art per deck, loaded on a worker; present with the library.
    deck_art: Option<DeckArtwork>,
    /// The cover each deck card draws, built once per change (gpui hashes the bytes of a new one).
    pub(crate) art_images: Vec<Option<Arc<gpui::Image>>>,
    /// The cover bytes each `art_images` entry was built from (compared by pointer).
    art_sources: Vec<Option<Arc<[u8]>>>,
    shown_waveform: Vec<Option<Arc<WaveformData>>>,
    /// The last stems action's failure per deck, shown in the stems caption until the next action.
    pub(crate) stem_errors: Vec<Option<String>>,
    /// The sampler strip's view-model: pads, selection, path fields and the sample bank.
    pub(crate) sampler: SamplerBinding,
    /// Focus for the sampler strip: its digit keys press pads while it has focus.
    pub(crate) sampler_focus: Option<FocusHandle>,
    /// True while the selected pad's path field takes typed keys.
    pub(crate) sampler_editing: bool,
    /// The last sampler action's outcome: its text, and whether it is an error.
    pub(crate) sampler_status: Option<(String, bool)>,
    /// Performance mode: the library band is hidden and the waveform stage grows.
    pub(crate) performance: bool,
    /// The sampler drawer is open (collapsed it is a narrow handle beside the library).
    pub(crate) sampler_open: bool,
    /// Demo only: a sampler pad to press once its sample has loaded.
    pending_pad: Option<usize>,
    /// The header's notice chip was dismissed (it stays gone for the session).
    pub(crate) notice_dismissed: bool,
    /// The window's size in px, as the last frame saw it (the library sizes itself from it).
    pub(crate) window_size: (i32, i32),
    pub(crate) options: RunOptions,
    started: Instant,
    last_tick: f64,
    frames: u64,
    frames_out: Arc<AtomicU64>,
    /// Frame-time measurement, on when `OPENMIXER_FRAME_STATS` is set.
    frame_stats: Option<crate::frame_stats::FrameStats>,
}

/// The gpui image for a deck's cover, keeping `current` while the view still holds the same bytes
/// (the same `Arc`, remembered in `source`), so the image is not rebuilt and rehashed every tick.
fn cover_image(
    view: &ArtworkView,
    source: &mut Option<Arc<[u8]>>,
    current: Option<Arc<gpui::Image>>,
) -> Option<Arc<gpui::Image>> {
    let ArtworkView::Image { data, .. } = view else {
        *source = None;
        return None;
    };
    if let (Some(image), Some(known)) = (current, source.as_ref()) {
        if Arc::ptr_eq(known, data) {
            return Some(image);
        }
    }
    let format = if view.is_png() {
        gpui::ImageFormat::Png
    } else {
        gpui::ImageFormat::Jpeg
    };
    *source = Some(Arc::clone(data));
    Some(Arc::new(gpui::Image::from_bytes(format, data.to_vec())))
}

/// The theme the window uses: the active theme of a registry loaded with the built-ins.
pub fn load_theme() -> ThemeTokens {
    let mut registry = ThemeRegistry::new();
    registry.load_builtins();
    registry.select(ThemeRegistry::DEFAULT_ID);
    registry.current().clone()
}

/// Cells for a window of `window` size whose layout area (banner and top bar excluded) is `area`,
/// in draw order. A window under [`MIN_WINDOW`] shows two decks and no sampler rather than
/// shrinking its controls (see [`LayoutPlan::for_window`]).
pub fn cells_for(
    engine_decks: usize,
    window: (i32, i32),
    area: Rect,
    gap: i32,
    state: LayoutState,
) -> Vec<PlacedCell> {
    let plan = LayoutPlan::for_window(engine_decks, window, gap, state);
    visible_cells(&plan.spec, plan.decks, area, gap)
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
        // The settings registry holds the session's theme (preference or OPENMIXER_THEME).
        let theme = settings
            .as_ref()
            .map_or(theme, |s| s.themes.current().clone());
        let library = options
            .library
            .take()
            .map(|hub| LibraryPanel::new(hub, Arc::clone(&engine)));
        let deck_count = engine.num_decks();
        let deck_art = library.as_ref().map(|panel| {
            let path = panel.hub().borrow().services.db_path().to_owned();
            DeckArtwork::new(ArtworkLoader::for_library_file(&path), deck_count)
        });
        let scene = options.scene.clone();
        let sampler = SamplerBinding::new(
            Arc::clone(&engine),
            Some(router.clone() as Arc<dyn CommandSink>),
            router.clone() as Arc<dyn ParameterTarget>,
            options.samples.clone(),
        );
        let mixer = MixerPanel::new(
            MixerBinding::typed(Arc::clone(&engine), Arc::clone(&router)),
            meter_params(&theme),
        );
        let decks = (0..engine.num_decks())
            .map(|index| {
                let mut deck = DeckWidget::new(DeckBinding::typed(
                    Arc::clone(&engine),
                    Arc::clone(&router),
                    index,
                ));
                if scene.platter && index == 0 {
                    crate::deck_view::stage_platter(0);
                }
                if scene.stems_tab && index == 0 {
                    deck.pad_mode = om_ui::widgets::deck::PadMode::Stems;
                }
                deck
            })
            .collect();
        Self {
            engine,
            router,
            theme,
            decks,
            mixer,
            drag: None,
            hovered: None,
            settings,
            library,
            library_focus: None,
            settings_focus: None,
            debug_focus: None,
            deck_controls: DeckControls::new(deck_count),
            shown_info: vec![TrackInfo::default(); deck_count],
            deck_art,
            art_images: vec![None; deck_count],
            art_sources: vec![None; deck_count],
            shown_waveform: vec![None; deck_count],
            stem_errors: vec![None; deck_count],
            sampler,
            sampler_focus: None,
            sampler_editing: false,
            sampler_status: None,
            performance: false,
            sampler_open: scene.sampler_open,
            pending_pad: scene.press_pad,
            notice_dismissed: false,
            window_size: (0, 0),
            options,
            started: Instant::now(),
            last_tick: 0.0,
            frames: 0,
            frames_out,
            frame_stats: std::env::var_os("OPENMIXER_FRAME_STATS")
                .map(|_| crate::frame_stats::FrameStats::default()),
        }
    }

    /// The layout state the user has chosen: performance mode and the sampler drawer.
    pub(crate) fn layout_state(&self) -> LayoutState {
        LayoutState {
            performance: self.performance,
            sampler_open: self.sampler_open,
            sampler_visible: true,
        }
    }

    /// The deck's 'Load stems' / 'Stems off' action. A failure is kept for the stems caption.
    pub(crate) fn toggle_deck_stems(&mut self, deck: usize) {
        let result = match self.library.as_mut() {
            Some(panel) => panel.toggle_stems(deck),
            None => Err(om_ui::library::LoadError::Unavailable),
        };
        self.stem_errors[deck] = result.err().map(|error| error.to_string());
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
        // The mapping editor's learn is polled here, not only while the overlay is drawn, so a
        // learn started and then closed is still recorded (cheap when idle).
        if let Some(settings) = self.settings.as_mut() {
            settings.mapping_tick();
        }
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
            if let Some(art) = self.deck_art.as_mut() {
                art.follow(index, panel.loaded_track_id(index));
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
                if let (Some(bpm), 0, Some(w)) = (self.options.demo_grid_bpm, index, &wave) {
                    let grid = om_engine::grid::FixedTempoGrid::new(bpm, w.sample_rate(), 0);
                    deck.set_grid(Some(Arc::new(grid)));
                }
                self.shown_waveform[index] = wave;
            }
        }
        if let Some(art) = self.deck_art.as_mut() {
            art.pump();
            for (index, slot) in self.art_images.iter_mut().enumerate() {
                *slot = cover_image(art.view(index), &mut self.art_sources[index], slot.take());
            }
        }
        for deck in &mut self.decks {
            // Before refresh: the zoomed view follows the smoothed playhead, not the raw
            // once-per-audio-buffer position.
            deck.zoom.advance_playhead(now);
            deck.refresh(false);
        }
        self.mixer.refresh(dt, self.engine.mixer(), dragging);
        self.sampler.prune_pending();
        if let Some(pad) = self.pending_pad {
            if self.sampler.pads().get(pad).is_some_and(|p| p.loaded) {
                self.sampler.press(pad, 1.0);
                self.pending_pad = None;
            }
        }
        self.frames += 1;
        self.frames_out.store(self.frames, Ordering::Relaxed);
        if let (Some(out), Some(panel)) = (&self.options.library_rows_out, &self.library) {
            out.store(panel.row_count(), Ordering::Relaxed);
        }
    }

    /// The `--debug-panel` column: the per-deck load and play controls, device figures, then the raw
    /// control-surface values. Keys go to the column (`focus`), which passes them to an open path field.
    fn debug_column(
        &self,
        status: &DeviceStatus,
        bounds: Rect,
        focus: FocusHandle,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let t = &self.theme;
        let look = LookAndFeel::from_tokens(t);
        let playing: Vec<bool> = (0..self.engine.num_decks())
            .map(|deck| self.engine.deck(deck).is_playing())
            .collect();
        let blank = |label: &str| (label.to_owned(), String::new(), String::new());
        let mut lines = vec![blank("Developer"), blank("Device")];
        lines.extend(
            device_rows(status)
                .into_iter()
                .map(|(label, value)| (label, String::new(), value)),
        );
        lines.push(blank(""));
        lines.push(blank("Parameters"));
        lines.extend(parameter_rows(&self.engine, self.router.unknown_count()));
        let mut body = div()
            .id("debug-scroll")
            .h(px((bounds.h - 12 - 14).max(0) as f32))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(2.))
            .text_size(px(t.type_caption))
            .child(decks_section(t, &self.deck_controls, &playing, cx));
        for (label, id, value) in lines {
            let row = div()
                .flex()
                .gap(px(6.))
                .child(
                    div()
                        .text_color(paint(look.panel_caption.text))
                        .child(label),
                )
                .child(
                    div()
                        .flex_1()
                        .overflow_hidden()
                        .text_color(paint(look.popup_hint.text))
                        .child(id),
                )
                .child(div().text_color(paint(look.popup.text)).child(value));
            body = body.child(row);
        }
        place(bounds)
            .bg(paint(look.popup.fill))
            .border_1()
            .border_color(paint(look.popup.border))
            .p(px(6.))
            .overflow_hidden()
            .track_focus(&focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _, cx| {
                let keys = &ev.keystroke;
                if !(keys.modifiers.control || keys.modifiers.platform) {
                    this.debug_key(&keys.key, keys.key_char.as_deref());
                    cx.notify();
                }
            }))
            .child(body)
            .child(
                div()
                    .absolute()
                    .bottom(px(0.))
                    .left(px(0.))
                    .w_full()
                    .h(px(14.))
                    .px(px(6.))
                    .bg(paint(look.popup_hint.fill))
                    .text_size(px(look.popup_hint.font_size))
                    .text_color(paint(look.popup_hint.text))
                    .child("scroll the list for more  v"),
            )
    }

    /// One placed cell. Tagged with a debug selector (`deck:N`, `slot:NAME`) so headless tests can
    /// read back the bounds it was laid out at; the tag is a no-op without gpui's `test-support`.
    fn cell_view(&mut self, cell: &PlacedCell, cx: &mut Context<Self>) -> gpui::Div {
        let tag = match &cell.kind {
            CellKind::Deck(index) => format!("deck:{index}"),
            CellKind::Slot(name) => format!("slot:{name}"),
            CellKind::Rows(_) => "rows".to_owned(),
        };
        self.cell_content(cell, cx).debug_selector(|| tag)
    }

    fn cell_content(&mut self, cell: &PlacedCell, cx: &mut Context<Self>) -> gpui::Div {
        let t = &self.theme;
        match &cell.kind {
            CellKind::Deck(index) => deck_card(self, *index, cell.bounds, cx),
            CellKind::Slot(name) if name == STAGE_SLOT => stage_card(self, cell.bounds, cx),
            CellKind::Slot(name) if name == MIXER_SLOT => mixer_card(self, cell.bounds, cx),
            CellKind::Slot(name)
                if channel_of_slot(name).is_some_and(|ch| ch < self.mixer.strips.len()) =>
            {
                channel_card(self, channel_of_slot(name).unwrap_or(0), cell.bounds, cx)
            }
            CellKind::Slot(name) if name == SAMPLER_SLOT => {
                let focus = self
                    .sampler_focus
                    .get_or_insert_with(|| cx.focus_handle())
                    .clone();
                sampler_card(self, cell.bounds, focus, cx)
            }
            CellKind::Slot(name) if name == LIBRARY_SLOT && self.library.is_some() => {
                library_card(self, cell.bounds, cx)
            }
            // `compute_layout` flattens groups, so none reaches the view.
            CellKind::Rows(_) => place(cell.bounds),
            CellKind::Slot(name) => {
                let title = if name == LIBRARY_SLOT {
                    "Library could not open. Open Settings > Library.".to_owned()
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
        let frame_start = Instant::now();
        self.tick();
        window.request_animation_frame();
        if !window.is_window_active() {
            // The key-up or mouse-up of a held sampler pad never reaches an inactive window.
            self.sampler.release_held();
        }
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
        let debug_w = if self.options.debug.is_some() {
            DEBUG_WIDTH
        } else {
            0
        };
        let gap = self.theme.gap;
        self.window_size = (width, height);
        let page = page_for(
            self.engine.num_decks(),
            (width, height),
            debug_w,
            gap,
            self.layout_state(),
            self.header_needs(),
        );
        let top = page.area.y;

        let mut root = release_on(
            div()
                .size_full()
                .relative()
                .font(crate::font::interface_font(&self.theme))
                .bg(paint(self.theme.backdrop_top))
                .text_color(paint(self.theme.text_primary))
                .on_key_down(cx.listener(|this, ev: &gpui::KeyDownEvent, _, cx| {
                    // `r` toggles recording unless a text field or the settings card takes the key.
                    let keys = &ev.keystroke;
                    let plain =
                        !(keys.modifiers.control || keys.modifiers.platform || keys.modifiers.alt);
                    if keys.key == "r" && plain && !this.typing() {
                        this.press_record();
                        cx.notify();
                    } else if keys.key == "tab" && plain && !this.typing() {
                        // Tab toggles performance mode, like the PERFORM button.
                        this.performance = !this.performance;
                        cx.notify();
                    }
                }))
                .on_mouse_move(cx.listener(|this, ev: &gpui::MouseMoveEvent, _, cx| {
                    this.drag_move(ev.position);
                    cx.notify();
                })),
            cx,
        );
        root = root.child(self.header_strip(&page.header, cx));
        for cell in &page.cells {
            root = root.child(self.cell_view(cell, cx));
        }
        if let Some(status) = self.options.debug.clone() {
            let focus = self
                .debug_focus
                .get_or_insert_with(|| cx.focus_handle())
                .clone();
            root = root.child(self.debug_column(
                &status(),
                Rect::new(width - debug_w, top, debug_w, (height - top).max(0)),
                focus,
                cx,
            ));
        }
        if let Some(overlay) = self.settings_overlay(Rect::new(0, 0, width, height), window, cx) {
            root = root.child(overlay);
        }
        if let Some(stats) = self.frame_stats.as_mut() {
            if let Some(line) = stats.frame(frame_start, frame_start.elapsed()) {
                eprintln!("frame stats: {line}");
            }
        }
        root
    }
}

/// The period of the control timer (the automix step rate): 33 ms, about 30 Hz.
pub const CONTROL_TICK: std::time::Duration = std::time::Duration::from_millis(33);

/// Opens the main window and runs the GPUI event loop until it closes (or `smoke_frames` pass).
/// Returns the number of frames rendered.
pub fn run(engine: Arc<DjEngine>, router: Arc<ParameterRouter>, mut options: RunOptions) -> u64 {
    let theme = load_theme();
    let frames_out = Arc::new(AtomicU64::new(0));
    let counter = Arc::clone(&frames_out);
    Application::new().run(move |cx: &mut App| {
        crate::font::register(cx);
        let requested = std::env::var("OPENMIXER_WINDOW")
            .ok()
            .and_then(|text| crate::window_size::parse_size(&text));
        let display = cx.primary_display().map(|d| {
            let b = d.bounds().size;
            (f32::from(b.width), f32::from(b.height))
        });
        let (width, height) = crate::window_size::initial_size(requested, display);
        let bounds = Bounds::centered(None, size(px(width), px(height)), cx);
        if let Some(mut hook) = options.on_frame.take() {
            // A foreground timer, not the render loop: it keeps running while the window is hidden.
            cx.spawn(async move |cx: &mut AsyncApp| loop {
                cx.background_executor().timer(CONTROL_TICK).await;
                if cx.update(|_| hook()).is_err() {
                    break;
                }
            })
            .detach();
        }
        let mut slot = Some((engine, router, theme, options, counter));
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                // Below this the deck controls would overlap; see `main_layout::MIN_WINDOW`.
                window_min_size: Some(size(px(MIN_WINDOW.0 as f32), px(MIN_WINDOW.1 as f32))),
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

//! One deck: track header, overview and zoomed waveforms, and the composition of the deck card.
//! Models only; drawing and drag-and-drop are the gpui layer's job. State and geometry only.

use super::deck_header::{self, KeyNotation};
use super::{
    hotcues::HotcuePads,
    loops::{LoopControls, LoopFrame},
    pitch::PitchFader,
    px,
    transport::TransportPanel,
};
use crate::binding::DeckBinding;
use crate::format;
use crate::geometry::Rect;
use crate::stem_binding::StemBinding;
use crate::theme::ThemeTokens;
use crate::waveform::{self, StripCache, StripPlan, View};
use om_core::param_id::DeckParam;
use om_engine::grid::BeatGrid;
use std::sync::Arc;

/// Track metadata shown in the header. A beat grid on the deck takes precedence for the BPM.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrackInfo {
    pub title: String,
    pub artist: String,
    pub key: String,
    pub camelot: String,
    /// 0 = unknown.
    pub bpm: f64,
}

/// `1:05.3`, or `-0:12.0` for remaining time.
pub fn format_time(seconds: f64, remaining: bool) -> String {
    let text = format::clock(seconds, true);
    if remaining {
        format!("-{text}")
    } else {
        text
    }
}

/// `128.0`, or `--` when unknown.
pub fn format_bpm(bpm: f64) -> String {
    if bpm > 0.0 {
        format!("{bpm:.1}")
    } else {
        "--".to_owned()
    }
}

/// Text of every header label at one moment.
#[derive(Debug, Clone, PartialEq)]
pub struct HeaderText {
    /// A, B, C, ...: the deck's identity that does not rely on colour.
    pub letter: char,
    pub title: String,
    pub artist: String,
    /// One notation only, see [`KeyNotation`].
    pub key: String,
    pub bpm: String,
    pub elapsed: String,
    /// Time left with its minus sign.
    pub remaining: String,
    /// The countdown is under the warning threshold (drawn red).
    pub remaining_warn: bool,
    pub sync: bool,
    pub master: bool,
    /// Length of the active loop (`1/4`, `8`); `None` while not looping.
    pub loop_label: Option<String>,
}

#[derive(Clone)]
pub struct TrackHeader {
    binding: DeckBinding,
    info: TrackInfo,
    key_notation: KeyNotation,
}

impl TrackHeader {
    pub fn new(binding: DeckBinding) -> Self {
        Self {
            binding,
            info: TrackInfo::default(),
            key_notation: KeyNotation::default(),
        }
    }

    /// Chooses the key spelling the header shows.
    pub fn set_key_notation(&mut self, notation: KeyNotation) {
        self.key_notation = notation;
    }

    pub fn set_track(&mut self, info: TrackInfo) {
        self.info = info;
    }

    /// Grid BPM (or metadata BPM) scaled by the playback rate.
    pub fn effective_bpm(&self) -> f64 {
        let base = match self.binding.control().grid() {
            Some(grid) => grid.bpm(),
            None => self.info.bpm,
        };
        base * self.binding.state().rate
    }

    /// Labels for the current state. Elapsed and remaining come from the file's sample rate.
    pub fn text(&self) -> HeaderText {
        let deck = self.binding.state();
        let file_rate = deck.file_sample_rate;
        let (length, position) = if file_rate > 0.0 {
            (
                deck.length_frames as f64 / file_rate,
                deck.position_frames as f64 / file_rate,
            )
        } else {
            (0.0, 0.0)
        };
        let loop_frame = LoopFrame::from_state(&deck, self.binding.control().grid().as_deref());
        let loop_label = (deck.looping && loop_frame.length_beats > 0.0)
            .then(|| deck_header::loop_label(loop_frame.length_beats));
        HeaderText {
            letter: deck_header::deck_letter(self.binding.index),
            title: if self.info.title.is_empty() {
                "No track loaded".to_owned()
            } else {
                self.info.title.clone()
            },
            artist: self.info.artist.clone(),
            key: deck_header::format_key(&self.info, self.key_notation),
            bpm: format_bpm(self.effective_bpm()),
            elapsed: format_time(position, false),
            remaining: format_time(length - position, true),
            remaining_warn: deck_header::remaining_warning(length - position, length),
            sync: deck.sync,
            master: deck.sync_master,
            loop_label,
        }
    }
}

/// The overview strip: a click or drag seeks to the matching fraction of the track.
#[derive(Clone)]
pub struct OverviewWaveform {
    binding: DeckBinding,
}

impl OverviewWaveform {
    pub fn new(binding: DeckBinding) -> Self {
        Self { binding }
    }

    /// Seeks to the fraction under `x` (clamped) and returns it.
    pub fn seek_at(&self, x: f64, width: f64) -> f64 {
        let fraction = waveform::x_to_fraction(x, width);
        self.binding.send(DeckParam::Seek, fraction);
        fraction
    }

    /// Playhead position in pixels.
    pub fn playhead_x(&self, width: f64) -> f64 {
        let deck = self.binding.state();
        waveform::frame_to_x(deck.position_frames, deck.length_frames, width)
    }

    /// Playhead, hot cues, loop and bar ticks across a strip `width` px wide, from one deck
    /// state.
    pub fn markers(&self, width: f64) -> waveform::OverviewMarkers {
        let deck = self.binding.state();
        let x = |frame: i64| waveform::frame_to_x(frame, deck.length_frames, width);
        let cues = deck
            .hotcues
            .iter()
            .enumerate()
            .filter_map(|(i, cue)| cue.map(|f| (x(f), waveform::cue_colour(i))))
            .collect();
        let bar_ticks = self
            .binding
            .control()
            .grid()
            .map(|g| waveform::overview_bar_ticks(g.as_ref(), deck.length_frames, width))
            .unwrap_or_default();
        waveform::OverviewMarkers {
            playhead_x: x(deck.position_frames),
            cues,
            loop_span: deck
                .loop_valid()
                .then(|| (x(deck.loop_start), x(deck.loop_end), deck.looping)),
            bar_ticks,
        }
    }
}

/// What the zoomed view needs from a loaded waveform: the sample rate it was analysed at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaveformMeta {
    pub sample_rate: f64,
}

/// The deck state the zoomed view repaints for. A change in any field is a repaint.
#[derive(Debug, Clone, PartialEq)]
pub struct ZoomShown {
    pub position: i64,
    pub loop_start: i64,
    pub loop_end: i64,
    pub looping: bool,
    pub cues: [i64; HotcuePads::COUNT],
}

/// A vertical beat or bar line in the zoomed view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridLine {
    pub x: i32,
    pub bar: bool,
    /// Bar number to print at this line (bars count from 1); only on bars with room for a label.
    pub bar_number: Option<i64>,
}

/// Thinner than this and the beat lines turn to noise: only bars are drawn.
pub const MIN_GRID_SPACING_PX: f32 = 8.0;

/// The zoomed waveform: a view centred on the playhead, with a cached strip that is only redrawn when
/// the view leaves its margin or the scale, size or waveform changes.
pub struct ZoomWaveform {
    binding: DeckBinding,
    waveform: Option<WaveformMeta>,
    zoom_step: usize,
    cache: Option<StripCache>,
    shown: Option<ZoomShown>,
    clock: waveform::PlayheadClock,
    /// Repaints requested by [`ZoomWaveform::refresh`] (state changes only).
    pub repaint_requests: u64,
    /// Strips rendered into the cache.
    pub cache_renders: u64,
}

impl ZoomWaveform {
    pub fn new(binding: DeckBinding) -> Self {
        Self {
            binding,
            waveform: None,
            zoom_step: waveform::DEFAULT_ZOOM_STEP,
            cache: None,
            shown: None,
            clock: waveform::PlayheadClock::default(),
            repaint_requests: 0,
            cache_renders: 0,
        }
    }

    /// Index into [`waveform::ZOOM_SECONDS`].
    pub fn zoom_step(&self) -> usize {
        self.zoom_step
    }

    /// Seconds of track across the strip at the current step.
    pub fn zoom_seconds(&self) -> f64 {
        waveform::seconds_at(self.zoom_step)
    }

    /// Sets the zoom step (clamped). The caller applies the same step to every deck so they share
    /// one time scale.
    pub fn set_zoom_step(&mut self, step: usize) {
        self.zoom_step = waveform::clamp_step(step);
        self.cache = None;
    }

    /// Mouse wheel: scrolling down zooms in, up zooms out, one step per notch. Returns the new
    /// step.
    pub fn wheel(&mut self, delta_y: f32) -> usize {
        self.set_zoom_step(waveform::step_after_wheel(self.zoom_step, delta_y));
        self.zoom_step
    }

    /// Replaces the loaded waveform (or drops it). Invalidates the strip.
    pub fn set_waveform(&mut self, waveform: Option<WaveformMeta>) {
        self.waveform = waveform;
        self.cache = None;
    }

    /// Advances the smoothed playhead to `now` (seconds on a monotonic clock). Call once per frame,
    /// before the view is built.
    pub fn advance_playhead(&mut self, now: f64) {
        let deck = self.binding.state();
        let fps = deck.rate * deck.file_sample_rate;
        self.clock
            .advance(deck.position_frames, now, deck.playing, fps);
    }

    /// The playhead frame the zoomed view is centred on: smoothed once [`advance_playhead`] has
    /// run, else the engine's published position.
    ///
    /// [`advance_playhead`]: ZoomWaveform::advance_playhead
    pub fn playhead_frame(&self) -> i64 {
        match self.clock.current() {
            Some(frame) => frame.round() as i64,
            None => self.binding.state().position_frames,
        }
    }

    /// The view for a widget `width` px wide, centred on the playhead.
    pub fn view(&self, width: f64) -> View {
        let deck = self.binding.state();
        let rate = if deck.file_sample_rate > 0.0 {
            deck.file_sample_rate
        } else {
            self.waveform.map_or(0.0, |w| w.sample_rate)
        };
        View {
            centre_frame: self.playhead_frame(),
            frames_per_pixel: waveform::frames_per_pixel(self.zoom_seconds(), rate, width),
            width,
        }
    }

    /// What the zoomed view shows now, from one deck state.
    fn shown_now(&self) -> ZoomShown {
        let deck = self.binding.state();
        ZoomShown {
            position: deck.position_frames,
            loop_start: deck.loop_start,
            loop_end: deck.loop_end,
            looping: deck.looping,
            cues: std::array::from_fn(|i| deck.hotcues[i].unwrap_or(-1)),
        }
    }

    /// One frame tick: counts a repaint when the position, loop or cues changed since the last one.
    /// Returns whether it did.
    pub fn refresh(&mut self) -> bool {
        let now = self.shown_now();
        if self.shown.as_ref() == Some(&now) {
            return false;
        }
        self.shown = Some(now);
        self.repaint_requests += 1;
        true
    }

    /// Plans one paint of the strip for a `width` x `height` widget, rendering it when the cache is
    /// stale. `None` when no waveform is loaded (nothing to draw).
    pub fn paint_strip(&mut self, width: i32, height: i32) -> Option<StripPlan> {
        self.waveform?;
        let view = self.view(f64::from(width));
        let plan = waveform::plan_strip(self.cache.as_ref(), &view, height);
        if plan == StripPlan::Render {
            self.cache = Some(waveform::strip_for(&view, height));
            self.cache_renders += 1;
        }
        Some(plan)
    }

    /// Converts a deck-file frame to a waveform sample, as the strip is indexed by samples.
    pub fn frame_to_sample(&self, frame: f64) -> f64 {
        let file_rate = self.binding.state().file_sample_rate;
        match self.waveform {
            Some(w) if file_rate > 0.0 => frame * w.sample_rate / file_rate,
            _ => frame,
        }
    }

    /// Beat and bar lines inside `view`. Empty without a grid. Beat lines are dropped when they would
    /// be closer than [`MIN_GRID_SPACING_PX`].
    pub fn grid_lines(&self, view: &View) -> Vec<GridLine> {
        let Some(grid) = self.binding.control().grid() else {
            return Vec::new();
        };
        grid_lines_for(grid.as_ref(), view)
    }

    /// The loop in pixels, when one is set, with its length in beats when the deck has a grid.
    pub fn loop_region(&self, view: &View) -> Option<waveform::LoopRegion> {
        let deck = self.binding.state();
        deck.loop_valid().then(|| {
            let label = self
                .binding
                .control()
                .grid()
                .map(|g| {
                    waveform::loop_label(waveform::loop_beats(
                        g.as_ref(),
                        deck.loop_start,
                        deck.loop_end,
                    ))
                })
                .unwrap_or_default();
            waveform::LoopRegion {
                x0: view.x_of_frame(deck.loop_start),
                x1: view.x_of_frame(deck.loop_end),
                looping: deck.looping,
                label,
            }
        })
    }

    /// Cue marker x positions, `None` for unset cues.
    pub fn cue_xs(&self, view: &View) -> [Option<f64>; HotcuePads::COUNT] {
        let cues = self.binding.state().hotcues;
        std::array::from_fn(|i| cues[i].map(|f| view.x_of_frame(f)))
    }
}

/// Beat and bar lines for a grid inside a view; see [`ZoomWaveform::grid_lines`].
pub fn grid_lines_for(grid: &dyn BeatGrid, view: &View) -> Vec<GridLine> {
    let per_bar = grid.beats_per_bar().max(1) as i64;
    let first = grid.beat_at_frame(view.first_frame() as i64).ceil();
    let last = grid.beat_at_frame(view.last_frame() as i64);
    let beat_px =
        (grid.frame_at_beat(1.0) - grid.frame_at_beat(0.0)) as f32 / view.frames_per_pixel as f32;
    let bars_only = beat_px.abs() < MIN_GRID_SPACING_PX;
    let label_step = waveform::bar_label_step(beat_px * per_bar as f32);
    let mut lines = Vec::new();
    let mut beat = first;
    while beat <= last {
        let bar = (beat.round() as i64).rem_euclid(per_bar) == 0;
        if !bars_only || bar {
            let x = view.x_of_frame(grid.frame_at_beat(beat));
            let bar_number = if bar {
                waveform::bar_number(beat, per_bar).filter(|n| (n - 1) % label_step == 0)
            } else {
                None
            };
            lines.push(GridLine {
                x: x.round() as i32,
                bar,
                bar_number,
            });
        }
        beat += 1.0;
    }
    lines
}

/// Rectangles of one deck card, from the theme metrics.
#[derive(Debug, Clone, PartialEq)]
pub struct DeckLayout {
    pub header: Rect,
    pub overview: Rect,
    pub pitch: Rect,
    pub zoom: Rect,
    /// Hidden on a short deck so it does not overlap the controls.
    pub zoom_visible: bool,
    pub hotcues: Rect,
    pub loops: Rect,
    pub transport: Rect,
    /// Tab strip under the drawer: Cues / Stems / More.
    pub tabs: Rect,
    /// The stems drawer. Only set while [`PadMode::Stems`] is open, then it takes the place of
    /// the hotcue pads and the loop row.
    pub stems: Rect,
}

impl DeckLayout {
    /// Every rectangle empty and the zoom hidden: the start of a layout that sets only some parts.
    pub fn empty() -> Self {
        Self {
            header: Rect::default(),
            overview: Rect::default(),
            pitch: Rect::default(),
            zoom: Rect::default(),
            zoom_visible: false,
            hotcues: Rect::default(),
            loops: Rect::default(),
            transport: Rect::default(),
            tabs: Rect::default(),
            stems: Rect::default(),
        }
    }
}

// Deck control metrics (px), local until they move into theme tokens. Touch-size floors: the
// transport and the hotcue pads are at least 44 px, every other clickable at least 32 px.
// Gap 6 = inner gap 8 / (4/3); the 4 px stem row gap is the half-step below it.
pub const TRANSPORT_HEIGHT: i32 = 44;
pub const CUE_PAD_MIN: i32 = 44;
pub const CUE_PAD_MAX: i32 = 56;
pub const LOOP_ROW_HEIGHT: i32 = 36;
pub const TAB_HEIGHT: i32 = 32;
pub const STEM_ROW_HEIGHT: i32 = 32;
pub const STEM_ROW_GAP: i32 = 4;
pub const CONTROL_GAP: i32 = 6;
pub const PITCH_WIDTH: i32 = 64;
/// Height of the controls column in its default (Cues) mode: transport, two pad rows, loop row and
/// tab strip with the gaps between them.
pub const CONTROLS_HEIGHT: i32 =
    TRANSPORT_HEIGHT + CUE_PAD_MIN * 2 + LOOP_ROW_HEIGHT + TAB_HEIGHT + CONTROL_GAP * 5;

/// What the drawer under the transport shows. The default is the hotcue pads; the others are one
/// tap away, so a deck is one row of essentials until the DJ asks for more.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PadMode {
    #[default]
    Cues,
    Stems,
    /// Master, key lock, quantize and slip.
    More,
}

impl PadMode {
    pub const ALL: [PadMode; 3] = [PadMode::Cues, PadMode::Stems, PadMode::More];

    pub const fn label(self) -> &'static str {
        match self {
            PadMode::Cues => "CUES",
            PadMode::Stems => "STEMS",
            PadMode::More => "MORE",
        }
    }

    pub const fn tooltip(self) -> &'static str {
        match self {
            PadMode::Cues => "Hotcue pads",
            PadMode::Stems => "Stem mute, solo, volume and filter",
            PadMode::More => "Master, key lock, quantize and slip",
        }
    }
}

/// Equal cells across the tab strip.
pub fn tab_rects(tabs: Rect) -> [Rect; 3] {
    let mut area = tabs;
    let cell = (area.w - CONTROL_GAP * 2) / 3;
    std::array::from_fn(|_| {
        let r = area.remove_left(cell);
        area.remove_left(CONTROL_GAP);
        r
    })
}

/// The controls column inside `area`: transport on top, the tab strip at the bottom, between them
/// the drawer. In [`PadMode::Stems`] the drawer is the stems; otherwise it is the pads (hotcues or
/// the More toggles) over the loop row.
fn controls_in(mut area: Rect, mode: PadMode) -> DeckLayout {
    let transport = area.remove_top(TRANSPORT_HEIGHT);
    area.remove_top(CONTROL_GAP);
    let tabs = area.remove_bottom(TAB_HEIGHT);
    area.remove_bottom(CONTROL_GAP);
    if mode == PadMode::Stems {
        return DeckLayout {
            transport,
            tabs,
            stems: area,
            ..DeckLayout::empty()
        };
    }
    let loops = area.remove_bottom(LOOP_ROW_HEIGHT);
    area.remove_bottom(CONTROL_GAP);
    DeckLayout {
        transport,
        tabs,
        loops,
        hotcues: area,
        ..DeckLayout::empty()
    }
}

/// The deck card of a stage deck in its default mode; see [`deck_controls_layout_for`].
pub fn deck_controls_layout(bounds: Rect, theme: &ThemeTokens) -> DeckLayout {
    deck_controls_layout_for(bounds, theme, PadMode::Cues)
}

/// The deck card of a stage deck: no header, overview or zoomed waveform (the stage above draws
/// those), only the controls: the transport on top, the drawer for `mode` under it and the tab
/// strip at the bottom, with the pitch column on the right.
pub fn deck_controls_layout_for(bounds: Rect, theme: &ThemeTokens, mode: PadMode) -> DeckLayout {
    let mut area = bounds.reduced(theme.pad, theme.pad);
    let pitch = area.remove_right(PITCH_WIDTH);
    area.remove_right(theme.inner_gap);
    DeckLayout {
        pitch,
        ..controls_in(area, mode)
    }
}

/// Places the deck card's parts inside `bounds` (the card itself). Mirrors `DeckWidget::resized`.
pub fn deck_layout(bounds: Rect, theme: &ThemeTokens) -> DeckLayout {
    deck_layout_for(bounds, theme, PadMode::Cues)
}

/// [`deck_layout`] with the drawer for `mode`.
pub fn deck_layout_for(bounds: Rect, theme: &ThemeTokens, mode: PadMode) -> DeckLayout {
    let gap = theme.inner_gap;
    let mut area = bounds.reduced(theme.pad, theme.pad);
    let header = area.remove_top(px(theme.deck_header_height).max(deck_header::HEADER_HEIGHT));
    let overview = area.remove_top(px(theme.deck_overview_height));
    area.remove_top(gap);
    let pitch = area.remove_right(PITCH_WIDTH);
    area.remove_right(gap);
    let controls = controls_in(area.remove_bottom(CONTROLS_HEIGHT.min(area.h)), mode);
    area.remove_bottom(gap);
    let zoom_visible = area.h >= px(theme.deck_min_zoom_height);
    let zoom = if area.h > 0 { area } else { Rect::default() };
    DeckLayout {
        header,
        overview,
        pitch,
        zoom,
        zoom_visible,
        ..controls
    }
}

/// One deck: the models of its parts, wired to the same engine deck.
pub struct DeckWidget {
    binding: DeckBinding,
    pub header: TrackHeader,
    pub overview: OverviewWaveform,
    pub zoom: ZoomWaveform,
    pub transport: TransportPanel,
    pub pitch: PitchFader,
    pub hotcues: HotcuePads,
    pub loops: LoopControls,
    pub stems: StemBinding,
    /// Which drawer is open under the transport.
    pub pad_mode: PadMode,
}

impl DeckWidget {
    pub fn new(binding: DeckBinding) -> Self {
        Self {
            stems: StemBinding::for_deck(&binding),
            header: TrackHeader::new(binding.clone()),
            overview: OverviewWaveform::new(binding.clone()),
            zoom: ZoomWaveform::new(binding.clone()),
            transport: TransportPanel::new(binding.clone()),
            pitch: PitchFader::new(binding.clone()),
            hotcues: HotcuePads::new(binding.clone()),
            loops: LoopControls::new(binding.clone()),
            pad_mode: PadMode::default(),
            binding,
        }
    }

    pub fn index(&self) -> usize {
        self.binding.index
    }

    /// Sets (or clears) the deck's beat grid in the engine. The zoomed view reads it back.
    pub fn set_grid(&self, grid: Option<Arc<dyn BeatGrid>>) {
        self.binding.engine.set_grid(self.binding.index, grid);
    }

    /// One frame tick: follows engine-side pitch changes. The zoom repaint counter advances in
    /// [`ZoomWaveform::refresh`], which the caller drives.
    pub fn refresh(&mut self, dragging_pitch: bool) {
        self.zoom.refresh();
        self.pitch.refresh(dragging_pitch);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_and_bpm_read_as_the_header_shows_them() {
        assert_eq!(format_time(65.34, false), "1:05.3");
        assert_eq!(format_time(12.0, true), "-0:12.0");
        assert_eq!(format_time(-3.0, false), "0:00.0");
        assert_eq!(format_bpm(0.0), "--");
        assert_eq!(format_bpm(127.96), "128.0");
    }

    #[test]
    fn deck_layout_hides_the_zoom_on_a_short_deck() {
        let theme = ThemeTokens::builtin_dark();
        let short = deck_layout(Rect::sized(560, 330), &theme);
        assert!(!short.zoom_visible);
        assert!(!short.zoom.intersects(&short.transport));
        assert!(!short.hotcues.intersects(&short.loops));

        let tall = deck_layout(Rect::sized(560, 520), &theme);
        assert!(tall.zoom_visible);
        assert!(!tall.zoom.intersects(&tall.transport));
    }

    #[test]
    fn every_pad_mode_fits_without_overlap_at_the_smallest_controls_height() {
        let theme = ThemeTokens::builtin_dark();
        // 290 px wide and as tall as the layout guarantees the controls row (CONTROLS_MIN_HEIGHT).
        let card = Rect::sized(290, 262);
        for mode in PadMode::ALL {
            let l = deck_controls_layout_for(card, &theme, mode);
            let parts = [l.pitch, l.transport, l.tabs, l.hotcues, l.loops, l.stems];
            for (i, a) in parts.iter().enumerate() {
                assert!(card.contains(a), "{mode:?}: {a:?} escapes the card");
                for b in &parts[i + 1..] {
                    assert!(!a.intersects(b), "{mode:?}: {a:?} overlaps {b:?}");
                }
            }
            assert!(l.transport.h >= 44 && l.tabs.h >= 32);
            if mode == PadMode::Stems {
                assert!(l.stems.h >= STEM_ROW_HEIGHT * 4 + STEM_ROW_GAP * 3, "{l:?}");
            } else {
                assert!(l.hotcues.h >= CUE_PAD_MIN * 2 + CONTROL_GAP, "{l:?}");
                assert!(l.loops.h >= 32);
            }
        }
    }

    #[test]
    fn deck_parts_fit_inside_the_card_at_the_default_size() {
        let theme = ThemeTokens::builtin_dark();
        // 16px card padding and 40px control rows need ~560px of height for every part.
        let card = Rect::sized(560, 600);
        let l = deck_layout(card, &theme);
        for part in [
            l.header,
            l.overview,
            l.pitch,
            l.zoom,
            l.hotcues,
            l.loops,
            l.transport,
            l.tabs,
        ] {
            assert!(!part.is_empty(), "{part:?}");
            assert!(card.contains(&part), "{part:?} escapes the card");
        }
    }
}

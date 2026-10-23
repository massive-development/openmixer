//! One deck: track header, overview and zoomed waveforms, and the composition of the deck card.
//! Models only; drawing and drag-and-drop are the gpui layer's job. State and geometry only.

use super::{
    hotcues::HotcuePads, loops::LoopControls, pitch::PitchFader, px, stem_pads,
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

/// Musical key and Camelot code, separated by two spaces; whichever is empty is left out.
pub fn format_key(info: &TrackInfo) -> String {
    [info.key.as_str(), info.camelot.as_str()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("  ")
}

/// Text of every header label at one moment.
#[derive(Debug, Clone, PartialEq)]
pub struct HeaderText {
    pub title: String,
    pub artist: String,
    pub key: String,
    pub bpm: String,
    pub elapsed: String,
    pub remaining: String,
}

#[derive(Clone)]
pub struct TrackHeader {
    binding: DeckBinding,
    info: TrackInfo,
}

impl TrackHeader {
    pub fn new(binding: DeckBinding) -> Self {
        Self {
            binding,
            info: TrackInfo::default(),
        }
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
        HeaderText {
            title: if self.info.title.is_empty() {
                "No track loaded".to_owned()
            } else {
                self.info.title.clone()
            },
            artist: self.info.artist.clone(),
            key: format_key(&self.info),
            bpm: format_bpm(self.effective_bpm()),
            elapsed: format_time(position, false),
            remaining: format_time(length - position, true),
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
}

/// Thinner than this and the beat lines turn to noise: only bars are drawn.
pub const MIN_GRID_SPACING_PX: f32 = 6.0;

/// The zoomed waveform: a view centred on the playhead, with a cached strip that is only redrawn when
/// the view leaves its margin or the scale, size or waveform changes.
pub struct ZoomWaveform {
    binding: DeckBinding,
    waveform: Option<WaveformMeta>,
    frames_per_pixel: f64,
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
            frames_per_pixel: 256.0,
            cache: None,
            shown: None,
            clock: waveform::PlayheadClock::default(),
            repaint_requests: 0,
            cache_renders: 0,
        }
    }

    pub fn frames_per_pixel(&self) -> f64 {
        self.frames_per_pixel
    }

    /// Sets the scale, clamped to the supported range.
    pub fn set_frames_per_pixel(&mut self, frames_per_pixel: f64) {
        self.frames_per_pixel = waveform::clamp_zoom(frames_per_pixel);
        self.cache = None;
    }

    pub fn zoom_by(&mut self, factor: f64) {
        self.set_frames_per_pixel(self.frames_per_pixel * factor);
    }

    /// Mouse wheel: scrolling down zooms in, up zooms out, one notch per step.
    pub fn wheel(&mut self, delta_y: f32) {
        let factor = if delta_y > 0.0 {
            1.0 / waveform::WHEEL_ZOOM_STEP
        } else {
            waveform::WHEEL_ZOOM_STEP
        };
        self.zoom_by(factor);
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
        View {
            centre_frame: self.playhead_frame(),
            frames_per_pixel: self.frames_per_pixel,
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

    /// Loop span in pixels, when a loop is set.
    pub fn loop_span(&self, view: &View) -> Option<(f64, f64, bool)> {
        let deck = self.binding.state();
        deck.loop_valid().then(|| {
            (
                view.x_of_frame(deck.loop_start),
                view.x_of_frame(deck.loop_end),
                deck.looping,
            )
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
    let mut lines = Vec::new();
    let mut beat = first;
    while beat <= last {
        let bar = (beat.round() as i64).rem_euclid(per_bar) == 0;
        if !bars_only || bar {
            let x = view.x_of_frame(grid.frame_at_beat(beat));
            lines.push(GridLine {
                x: x.round() as i32,
                bar,
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
    /// The stem pads row, full width under the pitch column and the loops.
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
            stems: Rect::default(),
        }
    }
}

/// The deck card of a stage deck: no header, overview or zoomed waveform (the stage above draws
/// those), only the controls, from the top: transport, hotcues, loops, with the pitch column on the
/// right and the stems row along the bottom.
pub fn deck_controls_layout(bounds: Rect, theme: &ThemeTokens) -> DeckLayout {
    let gap = theme.inner_gap;
    let mut area = bounds.reduced(theme.pad, theme.pad);
    let stems = area.remove_bottom(stem_pads::stems_row_height(theme));
    area.remove_bottom(gap);
    let pitch = area.remove_right(px(theme.deck_pitch_width));
    area.remove_right(gap);
    let row = px(theme.deck_row_height);
    let transport = area.remove_top(px(theme.deck_transport_height));
    area.remove_top(gap);
    let hotcues = area.remove_top(row);
    area.remove_top(gap);
    let loops = area.remove_top(row * 2 + gap);
    DeckLayout {
        pitch,
        hotcues,
        loops,
        transport,
        stems,
        ..DeckLayout::empty()
    }
}

/// Places the deck card's parts inside `bounds` (the card itself). Mirrors `DeckWidget::resized`.
pub fn deck_layout(bounds: Rect, theme: &ThemeTokens) -> DeckLayout {
    let gap = theme.inner_gap;
    let mut area = bounds.reduced(theme.pad, theme.pad);
    let header = area.remove_top(px(theme.deck_header_height));
    let overview = area.remove_top(px(theme.deck_overview_height));
    area.remove_top(gap);
    // The stems row spans the whole card, under the pitch column, so its pads have the width.
    let stems = area.remove_bottom(stem_pads::stems_row_height(theme));
    area.remove_bottom(gap);
    let pitch = area.remove_right(px(theme.deck_pitch_width));
    area.remove_right(gap);
    let row = px(theme.deck_row_height);
    let loops = area.remove_bottom(row * 2 + gap);
    area.remove_bottom(gap);
    let hotcues = area.remove_bottom(row);
    area.remove_bottom(gap);
    let transport = area.remove_bottom(px(theme.deck_transport_height));
    area.remove_bottom(gap);
    let zoom_visible = area.h >= px(theme.deck_min_zoom_height);
    let zoom = if area.h > 0 { area } else { Rect::default() };
    DeckLayout {
        header,
        overview,
        pitch,
        zoom,
        zoom_visible,
        hotcues,
        loops,
        transport,
        stems,
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
    fn time_bpm_and_key_read_as_the_header_shows_them() {
        assert_eq!(format_time(65.34, false), "1:05.3");
        assert_eq!(format_time(12.0, true), "-0:12.0");
        assert_eq!(format_time(-3.0, false), "0:00.0");
        assert_eq!(format_bpm(0.0), "--");
        assert_eq!(format_bpm(127.96), "128.0");
        assert_eq!(
            format_key(&TrackInfo {
                key: "Am".into(),
                camelot: "8A".into(),
                ..Default::default()
            }),
            "Am  8A"
        );
        assert_eq!(
            format_key(&TrackInfo {
                camelot: "8A".into(),
                ..Default::default()
            }),
            "8A"
        );
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
    fn deck_parts_fit_inside_the_card_at_the_default_size() {
        let theme = ThemeTokens::builtin_dark();
        let card = Rect::sized(560, 420);
        let l = deck_layout(card, &theme);
        for part in [
            l.header,
            l.overview,
            l.pitch,
            l.zoom,
            l.hotcues,
            l.loops,
            l.transport,
        ] {
            assert!(!part.is_empty(), "{part:?}");
            assert!(card.contains(&part), "{part:?} escapes the card");
        }
    }
}

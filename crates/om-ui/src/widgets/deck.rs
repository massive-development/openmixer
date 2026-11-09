//! One deck: track header, overview and zoomed waveforms, and the composition of the deck card.
//! Models only; drawing and drag-and-drop are the gpui layer's job. State and geometry only.

use super::deck_header::{self, KeyNotation};
use super::{
    hotcues::HotcuePads,
    loops::{LoopControls, LoopFrame},
    pitch::PitchFader,
    transport::TransportPanel,
};
use crate::binding::DeckBinding;
use crate::format;
use crate::geometry::Rect;
use crate::stem_binding::StemBinding;
use crate::theme::DensityMetrics;
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

/// The master tempo as the deck header row shows it: two decimals (comp "120.00"), or `--`.
pub fn format_tempo(bpm: f64) -> String {
    if bpm > 0.0 {
        format!("{bpm:.2}")
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

/// Rectangles of one deck's controls in the controls row (comp 4a/4b): the pitch fader along the
/// outside edge, then the transport row, the Hot cues / Loop / Stems / FX tabs, the 4 x 2 pad grid
/// and the options row under it. Deck B's are mirrored so each deck reads from its outside edge.
#[derive(Debug, Clone, PartialEq)]
pub struct DeckLayout {
    /// The pitch fader, the full height of the controls, at the outside edge.
    pub pitch: Rect,
    /// Play, Cue, then Sync and Key lock at the far end.
    pub transport: Rect,
    /// The segmented tabs that pick what the drawer shows.
    pub tabs: Rect,
    /// The 4 x 2 pad grid (hot cues or loop sizes).
    pub pads: Rect,
    /// The row of toggles under the pads.
    pub options: Rect,
    /// Everything under the tabs: the stems rows, the effects and the platter take it whole.
    pub drawer: Rect,
    /// Deck B: rows start at the right and run leftward.
    pub mirrored: bool,
}

/// What the drawer under the tabs shows (the comp's segmented control).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PadMode {
    #[default]
    Cues,
    Loop,
    Stems,
    Fx,
}

impl PadMode {
    pub const ALL: [PadMode; 4] = [PadMode::Cues, PadMode::Loop, PadMode::Stems, PadMode::Fx];

    pub const fn label(self) -> &'static str {
        match self {
            PadMode::Cues => "Hot cues",
            PadMode::Loop => "Loop",
            PadMode::Stems => "Stems",
            PadMode::Fx => "FX",
        }
    }

    pub const fn tooltip(self) -> &'static str {
        match self {
            PadMode::Cues => "Hot cue pads, tempo leader, snap, slip and platter",
            PadMode::Loop => "Loop sizes, halve, double and loop on or off",
            PadMode::Stems => "Stem mute, solo, volume and filter",
            PadMode::Fx => "The effect rack of this deck's channel",
        }
    }

    /// Whether the pad grid and the options row are shown (else the drawer is one area).
    pub const fn has_pads(self) -> bool {
        matches!(self, PadMode::Cues | PadMode::Loop)
    }
}

/// Pads per row of the grid.
pub const PAD_COLUMNS: usize = 4;
/// Pads in the grid: two rows.
pub const PAD_COUNT: usize = 8;

fn space(m: &DensityMetrics, step: usize) -> i32 {
    m.space[step].round() as i32
}

/// Width of the pitch fader column: one knob wide.
pub fn pitch_width(m: &DensityMetrics) -> i32 {
    m.knob_size.round() as i32
}

/// Height of the pad grid: two pads and the gap between them.
pub fn pad_grid_height(m: &DensityMetrics) -> i32 {
    2 * m.pad_size.round() as i32 + space(m, 1)
}

/// Height the deck controls need: transport and tabs rows, the pad grid and the options row, `s3`
/// apart.
pub fn controls_height(m: &DensityMetrics) -> i32 {
    let h = m.control_height.round() as i32;
    3 * h + pad_grid_height(m) + 3 * space(m, 2)
}

/// Lays out deck controls in `bounds` (the deck cell); `mirrored` for deck B.
pub fn deck_controls_layout(bounds: Rect, m: &DensityMetrics, mirrored: bool) -> DeckLayout {
    let h = m.control_height.round() as i32;
    let (gap, row_gap) = (space(m, 3), space(m, 2));
    let mut area = bounds;
    let pitch = if mirrored {
        area.remove_right(pitch_width(m))
    } else {
        area.remove_left(pitch_width(m))
    };
    if mirrored {
        area.remove_right(gap);
    } else {
        area.remove_left(gap);
    }
    let transport = area.remove_top(h);
    area.remove_top(row_gap);
    let tabs = area.remove_top(h);
    area.remove_top(row_gap);
    let drawer = area;
    let pads = area.remove_top(pad_grid_height(m));
    area.remove_top(row_gap);
    let options = area.remove_top(h);
    DeckLayout {
        pitch,
        transport,
        tabs,
        pads,
        options,
        drawer,
        mirrored,
    }
}

/// The eight pads of the grid in `area`: four columns of equal width, two rows, `gap` apart, in
/// reading order (deck B's grid is not mirrored: pad 1 stays top left, like the comp).
pub fn pad_grid(area: Rect, gap: i32) -> [Rect; PAD_COUNT] {
    let cols = PAD_COLUMNS as i32;
    let w = ((area.w - gap * (cols - 1)) / cols).max(0);
    let h = ((area.h - gap) / 2).max(0);
    std::array::from_fn(|i| {
        let (col, row) = ((i % PAD_COLUMNS) as i32, (i / PAD_COLUMNS) as i32);
        let x = area.x + (w + gap) * col;
        let width = if col + 1 == cols {
            (area.right() - x).max(0)
        } else {
            w
        };
        Rect::new(x, area.y + (h + gap) * row, width, h)
    })
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
    use crate::theme::{Density, ThemeTokens};

    #[test]
    fn time_and_bpm_read_as_the_header_shows_them() {
        assert_eq!(format_time(65.34, false), "1:05.3");
        assert_eq!(format_time(12.0, true), "-0:12.0");
        assert_eq!(format_time(-3.0, false), "0:00.0");
        assert_eq!(format_bpm(0.0), "--");
        assert_eq!(format_bpm(127.96), "128.0");
        assert_eq!(format_tempo(120.0), "120.00");
        assert_eq!(format_tempo(0.0), "--");
    }

    fn metric_sets() -> [DensityMetrics; 2] {
        let t = ThemeTokens::builtin_dark();
        [*t.metrics(Density::Roomy), *t.metrics(Density::Compact)]
    }

    #[test]
    fn the_controls_hold_every_part_without_overlap_in_both_densities() {
        for m in metric_sets() {
            let card = Rect::new(0, 0, 400, controls_height(&m));
            for mirrored in [false, true] {
                let l = deck_controls_layout(card, &m, mirrored);
                let parts = [l.pitch, l.transport, l.tabs, l.pads, l.options];
                for (i, a) in parts.iter().enumerate() {
                    assert!(card.contains(a), "{a:?} escapes the card");
                    assert!(!a.is_empty(), "{a:?} is empty");
                    for b in &parts[i + 1..] {
                        assert!(!a.intersects(b), "{a:?} overlaps {b:?}");
                    }
                }
                assert!(card.contains(&l.drawer) && !l.drawer.intersects(&l.tabs));
                assert_eq!(l.transport.h, m.control_height as i32);
                assert_eq!(l.pads.h, pad_grid_height(&m));
            }
        }
    }

    #[test]
    fn deck_b_mirrors_deck_a() {
        for m in metric_sets() {
            let card = Rect::new(500, 0, 400, controls_height(&m));
            let a = deck_controls_layout(card, &m, false);
            let b = deck_controls_layout(card, &m, true);
            assert_eq!(
                a.pitch.x, card.x,
                "A's pitch sits at its outside (left) edge"
            );
            assert_eq!(b.pitch.right(), card.right(), "B's at its right edge");
            assert_eq!(a.transport.w, b.transport.w);
            assert_eq!(a.transport.right(), card.right());
            assert_eq!(b.transport.x, card.x);
        }
    }

    #[test]
    fn pads_are_the_density_height_in_a_four_by_two_grid() {
        for m in metric_sets() {
            let area = Rect::new(10, 20, 403, pad_grid_height(&m));
            let gap = m.space[1] as i32;
            let pads = pad_grid(area, gap);
            for (i, p) in pads.iter().enumerate() {
                assert_eq!(p.h, m.pad_size as i32, "pad {i}");
                assert!(area.contains(p));
                for q in &pads[i + 1..] {
                    assert!(!p.intersects(q));
                }
            }
            assert_eq!(pads[3].right(), area.right());
            assert_eq!(pads[4].y, pads[0].bottom() + gap);
        }
    }
}

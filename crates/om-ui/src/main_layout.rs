//! The main window's rect contract: the faceplate of `docs/design-system/Design System.dc.html`
//! sections 4a (Roomy) and 4b (Compact). Top to bottom: the header strip; the deck header row (deck
//! A's header, the master tempo over the mixer column, deck B's header mirrored); the stage (both
//! zoomed waveforms, stacked, full width); the controls row (deck A, the mixer, deck B mirrored);
//! the library band with the sampler drawer beside it. Both densities go through this one code
//! path; only the metric set differs. Pure geometry.
//!
//! The generic grid ([`LayoutSpec`], [`compute_layout`]) is kept for data-driven layouts. Its JSON
//! form:
//!
//! ```json
//! {"rows":[{"height":0.66,"cells":[{"deck":0},{"slot":"mixer","weight":0.4},{"deck":1}]},
//!          {"height":0.34,"cells":[{"slot":"library"}]}]}
//! ```

use crate::geometry::Rect;
use crate::header_layout::{header_height, header_rects, HeaderNeeds, HeaderRects};
use crate::theme::{Density, DensityChoice, DensityMetrics, ThemeTokens};
use serde_json::{json, Map, Value};

/// The mixer column in the controls row: EQ and filter for both channels, the faders, the
/// crossfader and the master section.
pub const MIXER_SLOT: &str = "mixer";
/// The library band.
pub const LIBRARY_SLOT: &str = "library";
/// The waveform stage: both zoomed waveforms, stacked, full width.
pub const STAGE_SLOT: &str = "stage";
/// The master tempo, over the mixer column in the deck header row.
pub const TEMPO_SLOT: &str = "tempo";
/// The sampler drawer, beside the library in the same band.
pub const SAMPLER_SLOT: &str = "sampler";
/// Prefix of a deck header's slot name; the deck number follows (`head:0`).
pub const HEAD_SLOT_PREFIX: &str = "head:";
/// Decks the faceplate shows: A and B (plan D3). Further engine decks are not placed.
pub const FACEPLATE_DECKS: usize = 2;
/// Smallest window the layout is designed for. Below it the layout degrades (the sampler drawer
/// goes) instead of shrinking its controls.
pub const MIN_WINDOW: (i32, i32) = (1280, 720);
/// Share of the height the stage and the library split that the library takes. Unsourced: about
/// the comp's split at 1440x900.
pub const LIBRARY_SHARE: f32 = 0.4;
/// Rows the library band shows at least (the column header counts as one). Unsourced.
pub const LIBRARY_MIN_ROWS: i32 = 4;
/// Space between the two stacked waveforms (comp 4a: 2 px).
pub const WAVE_GAP: i32 = 2;
/// A zoomed waveform shorter than this is not drawn.
pub const MIN_WAVE_HEIGHT: i32 = 32;
/// Automatic density picks Roomy only while each waveform keeps at least this height. Unsourced:
/// the Fibonacci step above Roomy's 55 px pad.
pub const ROOMY_MIN_WAVE: i32 = 89;
/// Width of the sampler drawer while collapsed, in px: a handle that opens it.
pub const COLLAPSED_DRAWER_WIDTH: i32 = 56;
/// Share of the library band's width the open sampler drawer takes.
pub const SAMPLER_WEIGHT: f32 = 0.34;

/// Slot name of deck `index`'s header.
pub fn head_slot(index: usize) -> String {
    format!("{HEAD_SLOT_PREFIX}{index}")
}

/// The deck number in a slot name made by [`head_slot`].
pub fn deck_of_head(name: &str) -> Option<usize> {
    name.strip_prefix(HEAD_SLOT_PREFIX)?.parse().ok()
}

/// Deck B, and every second deck after it, is drawn mirrored so it reads from its outside edge.
pub fn is_mirrored(deck: usize) -> bool {
    deck % 2 == 1
}

/// What the user has opened or closed, as far as the page layout goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutState {
    /// The library band is hidden (header LIBRARY toggle, Tab) and the stage grows.
    pub performance: bool,
    /// The sampler drawer is open (collapsed it is a narrow handle beside the library).
    pub sampler_open: bool,
    /// The window has room for the sampler at all.
    pub sampler_visible: bool,
}

impl Default for LayoutState {
    /// Library shown, sampler collapsed.
    fn default() -> Self {
        Self {
            performance: false,
            sampler_open: false,
            sampler_visible: true,
        }
    }
}

impl LayoutState {
    pub fn with_sampler_open(mut self, open: bool) -> Self {
        self.sampler_open = open;
        self
    }
}

/// The spacing of the faceplate for one metric set (DESIGN.md Layout): the window keeps `s4` above
/// the header and `s5` at the sides and bottom; rows are `s4` apart and columns `s5`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spacing {
    pub margin: i32,
    pub row_gap: i32,
    pub column_gap: i32,
}

impl Spacing {
    pub fn of(m: &DensityMetrics) -> Self {
        Self {
            margin: m.space[4].round() as i32,
            row_gap: m.space[3].round() as i32,
            column_gap: m.space[4].round() as i32,
        }
    }
}

/// Sizes of the library browser's parts, in px: rows of the density's row height (34 Roomy, 26
/// Compact), the column header and the toolbar one control high, and the comp's 160 px sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LibraryMetrics {
    pub row_h: i32,
    pub header_h: i32,
    pub toolbar_h: i32,
    pub sidebar_w: i32,
    /// Space around and between the parts.
    pub pad: i32,
}

/// Width of the library sidebar (comp 4a).
pub const LIBRARY_SIDEBAR_WIDTH: i32 = 160;

impl LibraryMetrics {
    /// The band's height with the toolbar, the column header and `rows` rows.
    pub fn min_height(&self, rows: i32) -> i32 {
        self.toolbar_h + self.header_h + rows * self.row_h + 3 * self.pad
    }
}

/// The library's sizes for one metric set.
pub fn library_metrics(m: &DensityMetrics) -> LibraryMetrics {
    let h = m.control_height.round() as i32;
    LibraryMetrics {
        row_h: m.row_height.round() as i32,
        header_h: h,
        toolbar_h: h,
        sidebar_w: LIBRARY_SIDEBAR_WIDTH,
        pad: m.space[1].round() as i32,
    }
}

/// Heights of the rows that do not stretch, for one theme and density.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bands {
    pub header: i32,
    /// The deck header row: title block, readouts and the overview strip.
    pub head: i32,
    /// The controls row: the taller of the deck controls and the mixer column.
    pub controls: i32,
    /// The library band's floor: toolbar and [`LIBRARY_MIN_ROWS`] rows.
    pub library_min: i32,
}

impl Bands {
    pub fn of(t: &ThemeTokens, m: &DensityMetrics) -> Self {
        let deck = crate::widgets::deck::controls_height(m);
        let mixer = crate::widgets::mixer::column_height(m);
        Self {
            header: header_height(m),
            head: crate::widgets::deck_header::head_height(t, m),
            controls: deck.max(mixer),
            library_min: library_metrics(m).min_height(LIBRARY_MIN_ROWS),
        }
    }
}

/// What fills a cell.
#[derive(Debug, Clone, PartialEq)]
pub enum CellKind {
    /// A deck's controls by index.
    Deck(usize),
    /// A named slot, filled by the app or a later task.
    Slot(String),
    /// Rows laid out inside the cell (generic grid only). The placed result holds the inner cells,
    /// never the group itself.
    Rows(Vec<RowSpec>),
}

impl CellKind {
    pub fn is_deck(&self) -> bool {
        matches!(self, CellKind::Deck(_))
    }

    /// The slot named `name`.
    pub fn slot(name: &str) -> Self {
        CellKind::Slot(name.to_owned())
    }
}

/// A cell placed inside the window.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedCell {
    pub kind: CellKind,
    pub bounds: Rect,
}

/// The whole main window: the header strip's parts and every placed cell.
#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    pub header: HeaderRects,
    /// The metric set the page was laid out with.
    pub density: Density,
    /// The decks and slots, in draw order, all inside [`Page::area`].
    pub cells: Vec<PlacedCell>,
    /// The window under the header strip and left of the debug column.
    pub area: Rect,
    /// The window is smaller than [`MIN_WINDOW`]: the sampler drawer is left out.
    pub degraded: bool,
}

impl Page {
    /// The bounds of the cell of `kind`, if the page has one.
    pub fn cell(&self, kind: &CellKind) -> Option<Rect> {
        self.cells
            .iter()
            .find(|c| &c.kind == kind)
            .map(|c| c.bounds)
    }
}

/// The density a window lays out with: for `Automatic` Roomy while each waveform keeps
/// [`ROOMY_MIN_WAVE`] px with the library shown, and Compact below that or under [`MIN_WINDOW`].
/// An explicit Roomy is kept while the library still shows [`LIBRARY_MIN_ROWS`] rows; in a
/// shorter window it falls back to Compact rather than leave a band with no rows.
pub fn resolve_density(choice: DensityChoice, t: &ThemeTokens, window: (i32, i32)) -> Density {
    match choice {
        DensityChoice::Roomy if roomy_keeps_the_library(t, window) => Density::Roomy,
        DensityChoice::Automatic if roomy_fits(t, window) => Density::Roomy,
        _ => Density::Compact,
    }
}

/// Whether the Roomy metric set leaves both waveforms [`ROOMY_MIN_WAVE`] tall in `window`.
fn roomy_fits(t: &ThemeTokens, window: (i32, i32)) -> bool {
    !is_degraded(window)
        && roomy_page(t, window)
            .cell(&CellKind::slot(STAGE_SLOT))
            .is_some_and(|stage| stage.h >= 2 * ROOMY_MIN_WAVE + WAVE_GAP)
}

/// Whether the Roomy library band in `window` holds its floor of [`LIBRARY_MIN_ROWS`] rows.
fn roomy_keeps_the_library(t: &ThemeTokens, window: (i32, i32)) -> bool {
    let floor = Bands::of(t, t.metrics(Density::Roomy)).library_min;
    !is_degraded(window)
        && roomy_page(t, window)
            .cell(&CellKind::slot(LIBRARY_SLOT))
            .is_some_and(|band| band.h >= floor)
}

fn roomy_page(t: &ThemeTokens, window: (i32, i32)) -> Page {
    page_for(
        FACEPLATE_DECKS,
        window,
        0,
        LayoutState::default(),
        HeaderNeeds::default(),
        (t, Density::Roomy),
    )
}

fn is_degraded(window: (i32, i32)) -> bool {
    window.0 < MIN_WINDOW.0 || window.1 < MIN_WINDOW.1
}

/// The three columns of the deck header and controls rows: deck A, the mixer, deck B. The mixer
/// keeps its density width ([`DensityMetrics::mixer_width`]) while the decks get at least as much;
/// in a narrower window it gives way first.
pub fn columns(content: Rect, m: &DensityMetrics, gap: i32) -> [Rect; 3] {
    let usable = (content.w - 2 * gap).max(0);
    let mixer_w = (m.mixer_width.round() as i32).min(usable / 3);
    let deck_w = (usable - mixer_w) / 2;
    let a = Rect::new(content.x, content.y, deck_w, content.h);
    let mixer = Rect::new(a.right() + gap, content.y, mixer_w, content.h);
    let b_x = mixer.right() + gap;
    let b = Rect::new(b_x, content.y, (content.right() - b_x).max(0), content.h);
    [a, mixer, b]
}

/// Heights of the stage and the library band from the `flexible` height they share: the library
/// takes [`LIBRARY_SHARE`] but at least its floor, unless that would leave the stage less than two
/// readable waveforms (a window below the design size), when the share alone applies.
fn split_stage_and_library(flexible: i32, library_min: i32) -> (i32, i32) {
    let flexible = flexible.max(0);
    let share = (flexible as f32 * LIBRARY_SHARE).round() as i32;
    let wanted = share.max(library_min);
    let stage_floor = 2 * MIN_WAVE_HEIGHT + WAVE_GAP;
    let library = if flexible - wanted >= stage_floor {
        wanted
    } else {
        share
    };
    (flexible - library, library)
}

/// Lays out a `window` (width, height) with `engine_decks` decks for `state`: the header strip
/// across the top and, under it, the faceplate rows, with `debug_w` px kept free at the right for
/// the debug column. `look` is the theme and the density to lay out with (see
/// [`resolve_density`]). A deck the engine lacks is not placed.
pub fn page_for(
    engine_decks: usize,
    window: (i32, i32),
    debug_w: i32,
    state: LayoutState,
    header: HeaderNeeds,
    (t, density): (&ThemeTokens, Density),
) -> Page {
    let m = t.metrics(density);
    let space = Spacing::of(m);
    let bands = Bands::of(t, m);
    let degraded = is_degraded(window);
    let right = (window.0 - debug_w).max(0);
    let area = Rect::new(0, bands.header, right, (window.1 - bands.header).max(0));
    let content = Rect::new(
        space.margin,
        bands.header + space.row_gap,
        (right - 2 * space.margin).max(0),
        (window.1 - bands.header - space.row_gap - space.margin).max(0),
    );
    let mut body = content;
    let head_row = body.remove_top(bands.head);
    body.remove_top(space.row_gap);
    let library_shown = !state.performance;
    let fixed = bands.controls + space.row_gap * if library_shown { 2 } else { 1 };
    let (stage_h, library_h) = if library_shown {
        split_stage_and_library(body.h - fixed, bands.library_min)
    } else {
        ((body.h - fixed).max(0), 0)
    };
    let stage = body.remove_top(stage_h);
    body.remove_top(space.row_gap);
    let controls_row = body.remove_top(bands.controls);
    body.remove_top(space.row_gap);
    let band = body.remove_top(library_h);

    let decks = engine_decks.min(FACEPLATE_DECKS);
    let [head_a, tempo, head_b] = columns(head_row, m, space.column_gap);
    let [deck_a, mixer, deck_b] = columns(controls_row, m, space.column_gap);
    let mut cells = vec![
        (CellKind::Slot(head_slot(0)), head_a, decks > 0),
        (CellKind::slot(TEMPO_SLOT), tempo, true),
        (CellKind::Slot(head_slot(1)), head_b, decks > 1),
        (CellKind::slot(STAGE_SLOT), stage, true),
        (CellKind::Deck(0), deck_a, decks > 0),
        (CellKind::slot(MIXER_SLOT), mixer, true),
        (CellKind::Deck(1), deck_b, decks > 1),
    ];
    if library_shown {
        let sampler = state.sampler_visible && !degraded;
        let (library, drawer) = band_cells(band, state.sampler_open, sampler, space.column_gap);
        cells.push((CellKind::slot(LIBRARY_SLOT), library, true));
        if let Some(drawer) = drawer {
            cells.push((CellKind::slot(SAMPLER_SLOT), drawer, true));
        }
    }
    Page {
        header: header_rects(window.0, m, header),
        density,
        cells: cells
            .into_iter()
            .filter(|(_, _, shown)| *shown)
            .map(|(kind, bounds, _)| PlacedCell { kind, bounds })
            .collect(),
        area,
        degraded,
    }
}

/// The library band split into the library and, when `sampler`, the sampler drawer at its right:
/// a [`COLLAPSED_DRAWER_WIDTH`] handle, or [`SAMPLER_WEIGHT`] of the band while open.
fn band_cells(band: Rect, open: bool, sampler: bool, gap: i32) -> (Rect, Option<Rect>) {
    if !sampler {
        return (band, None);
    }
    let usable = (band.w - gap).max(0);
    let drawer_w = if open {
        (usable as f32 * SAMPLER_WEIGHT).round() as i32
    } else {
        COLLAPSED_DRAWER_WIDTH.min(usable)
    };
    let mut library = band;
    let drawer = library.remove_right(drawer_w);
    library.remove_right(gap);
    (library, Some(drawer))
}

// ---- The generic grid --------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct CellSpec {
    pub kind: CellKind,
    /// Share of the row width.
    pub weight: f32,
    /// Smallest width in px (0 = none). A cell that its share would leave narrower takes this
    /// instead, and the cells without a floor give up the difference.
    pub min_width: i32,
}

impl CellSpec {
    pub fn deck(index: usize) -> Self {
        Self {
            kind: CellKind::Deck(index),
            weight: 1.0,
            min_width: 0,
        }
    }

    pub fn slot(name: &str, weight: f32) -> Self {
        Self {
            kind: CellKind::Slot(name.to_owned()),
            weight,
            min_width: 0,
        }
    }

    /// The same cell with a floor of `min_width` px.
    pub fn with_min_width(mut self, min_width: i32) -> Self {
        self.min_width = min_width;
        self
    }

    /// A cell holding stacked rows.
    pub fn rows(rows: Vec<RowSpec>, weight: f32) -> Self {
        Self {
            kind: CellKind::Rows(rows),
            weight,
            min_width: 0,
        }
    }

    pub fn is_deck(&self) -> bool {
        matches!(self.kind, CellKind::Deck(_))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RowSpec {
    /// Share of the window height.
    pub height: f32,
    /// Smallest height in px. A row that its share would leave shorter takes this instead, and the
    /// rows without a floor give up the difference.
    pub min_height: i32,
    pub cells: Vec<CellSpec>,
}

/// A data-driven arrangement: rows of cells, each a deck or a named slot.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LayoutSpec {
    pub rows: Vec<RowSpec>,
}

/// Why a layout document was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LayoutError {
    #[error("layout is not valid JSON: {0}")]
    Json(String),
    #[error("layout is malformed: {0}")]
    Malformed(&'static str),
}

impl LayoutSpec {
    /// Parses the JSON form. Every row needs cells and a positive height; every cell needs exactly
    /// one of `deck` (a non-negative integer) or `slot` (a name), and a positive `weight` (default 1).
    pub fn from_json(json: &str) -> Result<Self, LayoutError> {
        let root: Value =
            serde_json::from_str(json).map_err(|e| LayoutError::Json(e.to_string()))?;
        let rows = root
            .get("rows")
            .and_then(Value::as_array)
            .filter(|rows| !rows.is_empty())
            .ok_or(LayoutError::Malformed("rows must be a non-empty array"))?;
        let rows = rows.iter().map(parse_row).collect::<Result<Vec<_>, _>>()?;
        Ok(Self { rows })
    }

    /// The JSON form, compact.
    pub fn to_json(&self) -> String {
        json!({ "rows": rows_json(&self.rows) }).to_string()
    }

    /// One more than the highest deck index used; 0 with no decks.
    pub fn deck_count(&self) -> usize {
        self.cells()
            .filter_map(|c| match &c.kind {
                CellKind::Deck(d) => Some(d + 1),
                CellKind::Slot(_) => None,
                CellKind::Rows(rows) => Some(Self { rows: rows.clone() }.deck_count()),
            })
            .max()
            .unwrap_or(0)
    }

    /// Slot names in order of first use, without repeats.
    pub fn slot_names(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for c in self.cells() {
            let found = match &c.kind {
                CellKind::Slot(name) => vec![name.clone()],
                CellKind::Rows(rows) => Self { rows: rows.clone() }.slot_names(),
                CellKind::Deck(_) => Vec::new(),
            };
            for name in found {
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        }
        names
    }

    /// The cells of every row, outermost level only.
    fn cells(&self) -> impl Iterator<Item = &CellSpec> {
        self.rows.iter().flat_map(|r| r.cells.iter())
    }
}

fn rows_json(rows: &[RowSpec]) -> Value {
    Value::Array(
        rows.iter()
            .map(|row| {
                let cells: Vec<Value> = row.cells.iter().map(cell_json).collect();
                let mut obj = json!({ "height": f64::from(row.height), "cells": cells });
                if row.min_height > 0 {
                    obj["minHeight"] = json!(row.min_height);
                }
                obj
            })
            .collect(),
    )
}

fn parse_row(row: &Value) -> Result<RowSpec, LayoutError> {
    let height = match row.get("height") {
        None => 1.0,
        Some(v) => v
            .as_f64()
            .ok_or(LayoutError::Malformed("row height must be a number"))?,
    };
    if height.is_nan() || height <= 0.0 {
        return Err(LayoutError::Malformed("row height must be positive"));
    }
    let cells = row
        .get("cells")
        .and_then(Value::as_array)
        .filter(|c| !c.is_empty())
        .ok_or(LayoutError::Malformed("row needs a non-empty cells array"))?;
    let cells = cells
        .iter()
        .map(parse_cell)
        .collect::<Result<Vec<_>, _>>()?;
    let min_height = non_negative(
        row.get("minHeight"),
        "row minHeight must be a non-negative integer",
    )?;
    Ok(RowSpec {
        height: height as f32,
        min_height,
        cells,
    })
}

/// An optional non-negative integer field; 0 when absent.
fn non_negative(value: Option<&Value>, error: &'static str) -> Result<i32, LayoutError> {
    match value {
        None => Ok(0),
        Some(v) => v
            .as_u64()
            .and_then(|m| i32::try_from(m).ok())
            .ok_or(LayoutError::Malformed(error)),
    }
}

fn parse_cell(value: &Value) -> Result<CellSpec, LayoutError> {
    let Value::Object(obj) = value else {
        return Err(LayoutError::Malformed("cell must be an object"));
    };
    let weight = match obj.get("weight") {
        None => 1.0,
        Some(v) => v
            .as_f64()
            .ok_or(LayoutError::Malformed("cell weight must be a number"))?,
    };
    if weight.is_nan() || weight <= 0.0 {
        return Err(LayoutError::Malformed("cell weight must be positive"));
    }
    let kind = match (obj.get("deck"), obj.get("slot"), obj.get("rows")) {
        (None, None, Some(r)) => {
            let rows = r
                .as_array()
                .filter(|rows| !rows.is_empty())
                .ok_or(LayoutError::Malformed("rows must be a non-empty array"))?;
            CellKind::Rows(rows.iter().map(parse_row).collect::<Result<Vec<_>, _>>()?)
        }
        (Some(d), None, None) => {
            let index = d.as_u64().ok_or(LayoutError::Malformed(
                "deck must be a non-negative integer",
            ))?;
            CellKind::Deck(index as usize)
        }
        (None, Some(s), None) => {
            let name = s
                .as_str()
                .filter(|n| !n.is_empty())
                .ok_or(LayoutError::Malformed("slot must be a non-empty name"))?;
            CellKind::Slot(name.to_owned())
        }
        _ => {
            return Err(LayoutError::Malformed(
                "a cell has exactly one of deck, slot or rows",
            ))
        }
    };
    let min_width = non_negative(
        obj.get("minWidth"),
        "cell minWidth must be a non-negative integer",
    )?;
    Ok(CellSpec {
        kind,
        weight: weight as f32,
        min_width,
    })
}

fn cell_json(cell: &CellSpec) -> Value {
    let mut obj = Map::new();
    match &cell.kind {
        CellKind::Deck(d) => obj.insert("deck".to_owned(), json!(d)),
        CellKind::Slot(s) => obj.insert("slot".to_owned(), json!(s)),
        CellKind::Rows(rows) => obj.insert("rows".to_owned(), rows_json(rows)),
    };
    obj.insert("weight".to_owned(), json!(f64::from(cell.weight)));
    if cell.min_width > 0 {
        obj.insert("minWidth".to_owned(), json!(cell.min_width));
    }
    Value::Object(obj)
}

/// Splits `length` by weights with `gap` between parts: a part that its share leaves below its
/// floor takes the floor, and the rest is shared out again among the others. Pixels left over by
/// rounding go to the parts that lost the most (so equal weights differ by at most 1 px). Returns
/// `(position, size)` per part. Single precision.
fn split_with_floors(length: i32, weights: &[f32], floors: &[i32], gap: i32) -> Vec<(i32, i32)> {
    let n = weights.len();
    let usable = (length - gap * (n as i32 - 1)).max(0);
    let mut fixed: Vec<Option<i32>> = vec![None; n];
    let ideal = loop {
        let taken: i32 = fixed.iter().flatten().sum();
        let free_weight: f32 = (0..n)
            .filter(|&i| fixed[i].is_none())
            .map(|i| weights[i])
            .sum();
        if free_weight <= 0.0 {
            break fixed
                .iter()
                .map(|f| f.unwrap_or(0) as f32)
                .collect::<Vec<f32>>();
        }
        let share = |i: usize| (usable - taken).max(0) as f32 * weights[i] / free_weight;
        match (0..n).find(|&i| fixed[i].is_none() && share(i) < floors[i] as f32) {
            Some(i) => fixed[i] = Some(floors[i]),
            None => {
                break (0..n)
                    .map(|i| fixed[i].map_or_else(|| share(i), |f| f as f32))
                    .collect::<Vec<f32>>()
            }
        }
    };
    let mut sizes: Vec<i32> = ideal.iter().map(|v| v.floor() as i32).collect();
    let mut leftover = usable - sizes.iter().sum::<i32>();
    let mut by_loss: Vec<usize> = (0..n).filter(|&i| fixed[i].is_none()).collect();
    by_loss.sort_by(|&a, &b| {
        let (fa, fb) = (ideal[a].fract(), ideal[b].fract());
        fb.partial_cmp(&fa).unwrap_or(std::cmp::Ordering::Equal)
    });
    for i in by_loss.into_iter().cycle().take(n * 2) {
        if leftover <= 0 {
            break;
        }
        sizes[i] += 1;
        leftover -= 1;
    }
    if leftover != 0 {
        // Only floors were placed (or the floors overshoot): the last part takes the difference.
        if let Some(last) = sizes.last_mut() {
            *last = (*last + leftover).max(0);
        }
    }
    let mut pos = 0;
    sizes
        .into_iter()
        .map(|size| {
            let part = (pos, size);
            pos += size + gap;
            part
        })
        .collect()
}

/// Splits `area` by row heights and cell weights with `gap` between neighbours.
pub fn compute_layout(spec: &LayoutSpec, area: Rect, gap: i32) -> Vec<PlacedCell> {
    if spec.rows.is_empty() {
        return Vec::new();
    }
    let heights: Vec<f32> = spec.rows.iter().map(|r| r.height).collect();
    let floors: Vec<i32> = spec.rows.iter().map(|r| r.min_height).collect();
    let row_parts = split_with_floors(area.h, &heights, &floors, gap);
    let mut placed = Vec::new();
    for (r, row) in spec.rows.iter().enumerate() {
        let weights: Vec<f32> = row.cells.iter().map(|c| c.weight).collect();
        let min_widths: Vec<i32> = row.cells.iter().map(|c| c.min_width).collect();
        for (c, (x, w)) in split_with_floors(area.w, &weights, &min_widths, gap)
            .into_iter()
            .enumerate()
        {
            let (y, h) = row_parts[r];
            let bounds = Rect::new(area.x + x, area.y + y, w, h);
            match &row.cells[c].kind {
                CellKind::Rows(inner) => {
                    let inner = LayoutSpec {
                        rows: inner.clone(),
                    };
                    placed.extend(compute_layout(&inner, bounds, gap));
                }
                kind => placed.push(PlacedCell {
                    kind: kind.clone(),
                    bounds,
                }),
            }
        }
    }
    placed
}

/// Cells of a generic layout the view can show with `engine_decks` decks: a deck the engine lacks
/// is skipped rather than failing a bad layout file.
pub fn visible_cells(
    spec: &LayoutSpec,
    engine_decks: usize,
    area: Rect,
    gap: i32,
) -> Vec<PlacedCell> {
    compute_layout(spec, area, gap)
        .into_iter()
        .filter(|p| match p.kind {
            CellKind::Deck(d) => d < engine_decks,
            CellKind::Slot(_) | CellKind::Rows(_) => true,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> ThemeTokens {
        ThemeTokens::builtin_dark()
    }

    fn page(window: (i32, i32), density: Density, state: LayoutState) -> Page {
        let needs = HeaderNeeds {
            automix: true,
            record: true,
            settings: true,
            notice: false,
        };
        page_for(2, window, 0, state, needs, (&theme(), density))
    }

    #[test]
    fn rows_need_cells_and_a_positive_height() {
        assert!(LayoutSpec::from_json(r#"{"rows":[{"height":-1,"cells":[{"deck":0}]}]}"#).is_err());
        assert!(LayoutSpec::from_json(r#"{"rows":[{"cells":[]}]}"#).is_err());
        assert!(LayoutSpec::from_json(r#"{"rows":[]}"#).is_err());
    }

    #[test]
    fn a_cell_is_exactly_a_deck_or_a_slot_with_positive_weight() {
        assert!(LayoutSpec::from_json(r#"{"rows":[{"cells":[{"deck":0,"slot":"x"}]}]}"#).is_err());
        assert!(LayoutSpec::from_json(r#"{"rows":[{"cells":[{}]}]}"#).is_err());
        assert!(LayoutSpec::from_json(r#"{"rows":[{"cells":[{"deck":0,"weight":0}]}]}"#).is_err());
        assert!(LayoutSpec::from_json(r#"{"rows":[{"cells":[{"deck":-1}]}]}"#).is_err());
    }

    #[test]
    fn head_slot_names_round_trip() {
        assert_eq!(deck_of_head(&head_slot(1)), Some(1));
        assert_eq!(deck_of_head("mixer"), None);
        assert_eq!(deck_of_head("head:x"), None);
        assert!(!is_mirrored(0) && is_mirrored(1));
    }

    #[test]
    fn the_mixer_column_keeps_its_density_width_and_the_decks_share_the_rest() {
        let t = theme();
        for density in [Density::Roomy, Density::Compact] {
            let m = t.metrics(density);
            let [a, mixer, b] = columns(Rect::new(21, 0, 1398, 10), m, 21);
            assert_eq!(mixer.w, m.mixer_width as i32, "{density:?}");
            assert!((a.w - b.w).abs() <= 1);
            assert_eq!(b.right(), 21 + 1398);
            assert_eq!((a.right() + 21, mixer.right() + 21), (mixer.x, b.x));
        }
    }

    #[test]
    fn the_tempo_sits_over_the_mixer_and_the_heads_over_their_decks() {
        for density in [Density::Roomy, Density::Compact] {
            let p = page((1440, 900), density, LayoutState::default());
            let at = |k: CellKind| p.cell(&k).expect("cell");
            let tempo = at(CellKind::slot(TEMPO_SLOT));
            let mixer = at(CellKind::slot(MIXER_SLOT));
            assert_eq!((tempo.x, tempo.w), (mixer.x, mixer.w));
            for d in 0..2 {
                let (head, deck) = (at(CellKind::Slot(head_slot(d))), at(CellKind::Deck(d)));
                assert_eq!((head.x, head.w), (deck.x, deck.w));
            }
        }
    }

    #[test]
    fn performance_mode_drops_the_band_and_grows_the_stage() {
        let normal = page((1440, 900), Density::Roomy, LayoutState::default());
        let perf = page(
            (1440, 900),
            Density::Roomy,
            LayoutState {
                performance: true,
                ..LayoutState::default()
            },
        );
        let stage = |p: &Page| p.cell(&CellKind::slot(STAGE_SLOT)).expect("stage").h;
        assert!(stage(&perf) > stage(&normal) + 100);
        assert!(perf.cell(&CellKind::slot(LIBRARY_SLOT)).is_none());
        assert!(perf.cell(&CellKind::slot(SAMPLER_SLOT)).is_none());
    }

    #[test]
    fn the_sampler_drawer_is_a_handle_or_its_share_of_the_band() {
        let closed = page((1440, 900), Density::Roomy, LayoutState::default());
        let drawer = closed.cell(&CellKind::slot(SAMPLER_SLOT)).expect("drawer");
        assert_eq!(drawer.w, COLLAPSED_DRAWER_WIDTH);
        let open = page(
            (1440, 900),
            Density::Roomy,
            LayoutState::default().with_sampler_open(true),
        );
        let drawer = open.cell(&CellKind::slot(SAMPLER_SLOT)).expect("drawer");
        let band = 1440 - 2 * 21 - 21;
        assert!((drawer.w as f32 / band as f32 - SAMPLER_WEIGHT).abs() < 0.01);
        let small = page((1024, 640), Density::Compact, LayoutState::default());
        assert!(small.degraded);
        assert!(small.cell(&CellKind::slot(SAMPLER_SLOT)).is_none());
    }

    #[test]
    fn automatic_density_is_roomy_on_tall_windows_and_compact_on_short_ones() {
        let t = theme();
        let auto = |w: (i32, i32)| resolve_density(DensityChoice::Automatic, &t, w);
        assert_eq!(auto((1440, 900)), Density::Roomy);
        assert_eq!(auto((1920, 1080)), Density::Roomy);
        assert_eq!(auto((1280, 720)), Density::Compact);
        assert_eq!(auto((1280, 800)), Density::Compact);
        assert_eq!(auto((900, 1000)), Density::Compact, "degraded is Compact");
        let roomy = |w: (i32, i32)| resolve_density(DensityChoice::Roomy, &t, w);
        assert_eq!(
            roomy((1280, 800)),
            Density::Roomy,
            "an explicit choice wins where it fits"
        );
        assert_eq!(
            roomy((1280, 720)),
            Density::Compact,
            "but never empties the library"
        );
        let compact = resolve_density(DensityChoice::Compact, &t, (1920, 1080));
        assert_eq!(compact, Density::Compact);
    }

    #[test]
    fn every_density_choice_keeps_the_library_rows_from_the_minimum_window_up() {
        let t = theme();
        let choices = [
            DensityChoice::Automatic,
            DensityChoice::Roomy,
            DensityChoice::Compact,
        ];
        for w in [MIN_WINDOW.0, 1440, 1920] {
            for h in (MIN_WINDOW.1..=1200).step_by(8) {
                for choice in choices {
                    let density = resolve_density(choice, &t, (w, h));
                    let floor = Bands::of(&t, t.metrics(density)).library_min;
                    let band = page((w, h), density, LayoutState::default())
                        .cell(&CellKind::slot(LIBRARY_SLOT))
                        .expect("library band");
                    assert!(
                        band.h >= floor,
                        "{choice:?} {w}x{h}: band {} < {floor}",
                        band.h
                    );
                }
            }
        }
    }

    #[test]
    fn the_debug_column_is_kept_out_of_the_area() {
        let needs = HeaderNeeds::default();
        let p = page_for(
            2,
            (1920, 1080),
            300,
            LayoutState::default(),
            needs,
            (&theme(), Density::Roomy),
        );
        assert_eq!(p.area.w, 1620);
        assert!(p.cells.iter().all(|c| c.bounds.right() <= 1620));
        assert_eq!(p.header.bar.w, 1920, "the header spans the window");
    }

    #[test]
    fn split_shares_the_rounding_and_never_loses_a_pixel() {
        let parts = split_with_floors(100, &[1.0, 1.0, 1.0], &[0, 0, 0], 0);
        assert_eq!(parts, vec![(0, 34), (34, 33), (67, 33)]);
        let parts = split_with_floors(101, &[1.0, 1.0], &[0, 0], 1);
        assert_eq!(parts.iter().map(|p| p.1).sum::<i32>(), 100);
        assert_eq!(parts[1].0 + parts[1].1, 101);
    }

    #[test]
    fn a_floor_wins_over_a_small_share() {
        let parts = split_with_floors(1000, &[0.1, 1.0, 0.1], &[112, 0, 112], 8);
        assert_eq!(parts[0].1, 112);
        assert_eq!(parts[2].1, 112);
        assert_eq!(parts[1].1, 1000 - 16 - 224);
    }
}

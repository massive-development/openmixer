//! Data-driven arrangement of the main window: rows of cells, each a deck or a named slot (mixer,
//! library, or anything a plugin registers). Pure geometry, so adding decks later is a data change.
//! JSON form:
//!
//! ```json
//! {"rows":[{"height":0.66,"cells":[{"deck":0},{"slot":"mixer","weight":0.4},{"deck":1}]},
//!          {"height":0.34,"cells":[{"slot":"library"}]}]}
//! ```

use crate::geometry::Rect;
use crate::header_layout::{header_rects, HeaderNeeds, HeaderRects};
use serde_json::{json, Map, Value};

/// Name of the mixer module slot: page tabs, master, crossfader and headphone cue, between the decks'
/// controls. The channel strips are separate slots, one beside each deck (see [`channel_slot`]).
pub const MIXER_SLOT: &str = "mixer";
/// Name of the library slot.
pub const LIBRARY_SLOT: &str = "library";
/// Prefix of a channel strip's slot name; the channel number follows (`channel:0`).
pub const CHANNEL_SLOT_PREFIX: &str = "channel:";
/// Share of the deck rows' width one channel strip takes in the standard layout (13% of the
/// window's width, never below [`CHANNEL_MIN_WIDTH`]).
pub const CHANNEL_WEIGHT: f32 = 0.13;
/// A channel strip is never narrower than this, in px.
pub const CHANNEL_MIN_WIDTH: i32 = 112;
/// Share of the deck rows' width the stage and the controls below it take.
pub const CENTRE_WEIGHT: f32 = 1.0;
/// Name of the waveform stage: the decks' headers, overviews and zoomed waveforms, across the top.
pub const STAGE_SLOT: &str = "stage";
/// Share of the deck rows' height the waveform stage takes in the standard layout.
pub const STAGE_SHARE: f32 = 0.50;
/// The same share in performance mode, when the library band is gone and the stage grows.
pub const PERFORMANCE_STAGE_SHARE: f32 = 0.62;
/// Height the deck controls row needs (transport, hotcues, loops and the stems row), in px. On a
/// short window the stage and the library band give up height before the controls do.
pub const CONTROLS_MIN_HEIGHT: i32 = 262;
/// Smallest window the layout is designed for. Below it the layout degrades (see
/// [`LayoutPlan::for_window`]) instead of shrinking its controls.
pub const MIN_WINDOW: (i32, i32) = (1280, 720);
/// Height of the header strip in a window at least 900 px tall, in px.
pub const HEADER_HEIGHT_TALL: i32 = 40;
/// Height of the header strip in a shorter window, in px.
pub const HEADER_HEIGHT_SHORT: i32 = 36;

/// The header strip's height for a window `window_h` px tall.
pub fn header_height(window_h: i32) -> i32 {
    if window_h >= 900 {
        HEADER_HEIGHT_TALL
    } else {
        HEADER_HEIGHT_SHORT
    }
}
/// Smallest the library band may get, in px: toolbar, header and a handful of rows.
pub const LIBRARY_MIN_HEIGHT: i32 = 150;
/// Width of the sampler drawer while collapsed, in px: a handle that opens it.
pub const COLLAPSED_DRAWER_WIDTH: i32 = 56;
/// Name of the sampler strip: eight pads beside the library, in the same band.
pub const SAMPLER_SLOT: &str = "sampler";
/// Share of the library band's width the sampler strip takes.
pub const SAMPLER_WEIGHT: f32 = 0.34;
/// Share of the window height the deck rows take in the standard layout; the library band gets
/// the rest (about 28% of the window).
pub const DECK_ROWS_SHARE: f32 = 0.71;
/// Share of a deck card's width the mixer module takes between the decks' controls (deck cards
/// weigh 1.0).
pub const MIXER_WEIGHT: f32 = 0.72;

/// Slot name of channel `index`'s strip.
pub fn channel_slot(index: usize) -> String {
    format!("{CHANNEL_SLOT_PREFIX}{index}")
}

/// The channel number in a slot name made by [`channel_slot`].
pub fn channel_of_slot(name: &str) -> Option<usize> {
    name.strip_prefix(CHANNEL_SLOT_PREFIX)?.parse().ok()
}

/// The weight, against the library's 1.0, that makes the collapsed sampler drawer
/// [`COLLAPSED_DRAWER_WIDTH`] px wide in a band of `(width, gap)`.
pub fn collapsed_drawer_weight(band: Option<(i32, i32)>) -> f32 {
    let Some((width, gap)) = band else {
        return SAMPLER_WEIGHT / (1.0 - SAMPLER_WEIGHT);
    };
    let usable = (width - gap).max(COLLAPSED_DRAWER_WIDTH + 1) as f32;
    let handle = COLLAPSED_DRAWER_WIDTH as f32;
    handle / (usable - handle)
}

/// What the user has opened or closed, as far as the page layout goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutState {
    /// Performance mode: the library band is gone and the stage grows.
    pub performance: bool,
    /// The sampler drawer is open (collapsed it is a narrow handle beside the library).
    pub sampler_open: bool,
    /// The window has room for the sampler at all.
    pub sampler_visible: bool,
}

impl Default for LayoutState {
    /// Library mode, sampler collapsed.
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

/// The layout for one window size: the spec, how many decks it can honestly show, and what was
/// given up to fit.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutPlan {
    pub spec: LayoutSpec,
    /// Decks the plan shows: all of them in a window of at least [`MIN_WINDOW`], two below it.
    pub decks: usize,
    /// True when the window is smaller than [`MIN_WINDOW`].
    pub degraded: bool,
    pub sampler_visible: bool,
}

impl LayoutPlan {
    /// Plans a `window` (width, height) with `engine_decks` decks. At or above [`MIN_WINDOW`]
    /// everything is shown. Below it the layout keeps its controls at full size and gives up
    /// things instead: decks beyond the second, and the sampler drawer.
    pub fn for_window(
        engine_decks: usize,
        window: (i32, i32),
        gap: i32,
        mut state: LayoutState,
    ) -> Self {
        let degraded = window.0 < MIN_WINDOW.0 || window.1 < MIN_WINDOW.1;
        let decks = if degraded {
            engine_decks.min(2)
        } else {
            engine_decks
        };
        state.sampler_visible = !degraded;
        let spec = LayoutSpec::build(decks, &state, Some((window.0, gap)));
        Self {
            spec,
            decks,
            degraded,
            sampler_visible: state.sampler_visible && !state.performance,
        }
    }
}

/// What fills a cell.
#[derive(Debug, Clone, PartialEq)]
pub enum CellKind {
    /// A deck by index.
    Deck(usize),
    /// A named slot, filled by the app or a later task.
    Slot(String),
    /// Rows laid out inside the cell: how a column of the window holds stacked parts. The placed
    /// result holds the inner cells, never the group itself.
    Rows(Vec<RowSpec>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct CellSpec {
    pub kind: CellKind,
    /// Share of the row width.
    pub weight: f32,
    /// Smallest width in px (0 = none). A cell that its share would leave narrower takes this
    /// instead, and the cells without a floor give up the difference.
    pub min_width: i32,
}

impl CellKind {
    pub fn is_deck(&self) -> bool {
        matches!(self, CellKind::Deck(_))
    }
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

/// A cell placed inside the window.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedCell {
    pub kind: CellKind,
    pub bounds: Rect,
}

impl LayoutSpec {
    /// Channel strips at the window's left and right edges, the stage over the decks' controls
    /// between them, and the library and the open sampler strip across the bottom. Further decks
    /// (and their strips) get a row of full cards below the first.
    pub fn standard(num_decks: usize) -> Self {
        Self::build(
            num_decks,
            &LayoutState::default().with_sampler_open(true),
            None,
        )
    }

    /// The layout for a UI `state`. `band` is the library band's `(width, gap)`, needed to size the
    /// collapsed sampler drawer in px; `None` gives the open drawer its usual share.
    pub fn build(num_decks: usize, state: &LayoutState, band: Option<(i32, i32)>) -> Self {
        let stage_share = if state.performance {
            PERFORMANCE_STAGE_SHARE
        } else {
            STAGE_SHARE
        };
        let centre = CellSpec::rows(
            vec![
                RowSpec {
                    height: stage_share,
                    min_height: 0,
                    cells: vec![CellSpec::slot(STAGE_SLOT, 1.0)],
                },
                RowSpec {
                    height: 1.0 - stage_share,
                    min_height: CONTROLS_MIN_HEIGHT,
                    cells: match num_decks {
                        0 | 1 => vec![CellSpec::deck(0), CellSpec::slot(MIXER_SLOT, MIXER_WEIGHT)],
                        _ => vec![
                            CellSpec::deck(0),
                            CellSpec::slot(MIXER_SLOT, MIXER_WEIGHT),
                            CellSpec::deck(1),
                        ],
                    },
                },
            ],
            CENTRE_WEIGHT,
        );
        let mut top = vec![
            CellSpec::slot(&channel_slot(0), CHANNEL_WEIGHT).with_min_width(CHANNEL_MIN_WIDTH),
            centre,
        ];
        if num_decks > 1 {
            top.push(
                CellSpec::slot(&channel_slot(1), CHANNEL_WEIGHT).with_min_width(CHANNEL_MIN_WIDTH),
            );
        }
        let deck_share = if state.performance {
            1.0
        } else {
            DECK_ROWS_SHARE
        };
        let mut rows = vec![RowSpec {
            height: deck_share,
            min_height: 0,
            cells: top,
        }];
        if num_decks > 2 {
            rows.push(RowSpec {
                height: deck_share,
                min_height: 0,
                cells: (2..num_decks)
                    .flat_map(|d| {
                        [
                            CellSpec::deck(d),
                            CellSpec::slot(&channel_slot(d), CHANNEL_WEIGHT)
                                .with_min_width(CHANNEL_MIN_WIDTH),
                        ]
                    })
                    .collect(),
            });
        }
        if !state.performance {
            let mut band_cells = vec![CellSpec::slot(LIBRARY_SLOT, 1.0)];
            if state.sampler_visible {
                let weight = if state.sampler_open {
                    SAMPLER_WEIGHT / (1.0 - SAMPLER_WEIGHT)
                } else {
                    collapsed_drawer_weight(band)
                };
                band_cells.push(CellSpec::slot(SAMPLER_SLOT, weight));
            }
            rows.push(RowSpec {
                height: 1.0 - DECK_ROWS_SHARE,
                min_height: LIBRARY_MIN_HEIGHT,
                cells: band_cells,
            });
        }
        Self { rows }
    }

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
    let min_height = match row.get("minHeight") {
        None => 0,
        Some(v) => v
            .as_u64()
            .and_then(|m| i32::try_from(m).ok())
            .ok_or(LayoutError::Malformed(
                "row minHeight must be a non-negative integer",
            ))?,
    };
    Ok(RowSpec {
        height: height as f32,
        min_height,
        cells,
    })
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
    let deck = obj.get("deck");
    let slot = obj.get("slot");
    let rows = obj.get("rows");
    let kind = match (deck, slot, rows) {
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
    let min_width = match obj.get("minWidth") {
        None => 0,
        Some(v) => v
            .as_u64()
            .and_then(|m| i32::try_from(m).ok())
            .ok_or(LayoutError::Malformed(
                "cell minWidth must be a non-negative integer",
            ))?,
    };
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

/// Cells the main view can show with `engine_decks` decks: a deck the engine lacks is skipped
/// rather than failing a bad layout file.
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

/// The whole main window: the header strip's parts and every placed cell.
#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    pub header: HeaderRects,
    /// The decks and slots, in draw order, all inside [`Page::area`].
    pub cells: Vec<PlacedCell>,
    /// The window under the header strip and left of the debug column. The cells end a `gap`
    /// above its bottom edge, so no card touches the window's border.
    pub area: Rect,
}

/// Lays out a `window` (width, height): the header strip (see [`header_height`]) and, under it, the
/// layout for `state`, with `engine_decks` decks, `gap` between cells and `debug_w` px kept free
/// at the right for the debug column. A deck the engine lacks is skipped.
pub fn page_for(
    engine_decks: usize,
    window: (i32, i32),
    debug_w: i32,
    gap: i32,
    state: LayoutState,
    header: HeaderNeeds,
) -> Page {
    let header_h = header_height(window.1);
    let rects = header_rects(window.0, header_h, header);
    let area = Rect::new(
        0,
        header_h,
        (window.0 - debug_w).max(0),
        (window.1 - header_h).max(0),
    );
    // The bottom margin keeps the library and sampler cards off the window's edge.
    let layout_area = Rect::new(area.x, area.y, area.w, (area.h - gap).max(0));
    let plan = LayoutPlan::for_window(engine_decks, (area.w, window.1), gap, state);
    Page {
        header: rects,
        cells: visible_cells(&plan.spec, plan.decks, layout_area, gap),
        area,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    const SIZES: [(i32, i32); 4] = [(1280, 720), (1280, 800), (1920, 1080), (2560, 1440)];
    const GAP: i32 = 12;
    const FULL: HeaderNeeds = HeaderNeeds {
        automix: true,
        record: true,
        settings: true,
        notice: false,
    };

    /// Every state the page has: library or performance mode, sampler open or collapsed.
    fn states() -> Vec<LayoutState> {
        let mut states = Vec::new();
        for performance in [false, true] {
            for sampler_open in [false, true] {
                states.push(LayoutState {
                    performance,
                    sampler_open,
                    sampler_visible: true,
                });
            }
        }
        states
    }

    fn page(decks: usize, size: (i32, i32), state: LayoutState) -> Page {
        page_for(decks, size, 0, GAP, state, FULL)
    }

    fn cell(page: &Page, kind: CellKind) -> Rect {
        page.cells
            .iter()
            .find(|c| c.kind == kind)
            .unwrap_or_else(|| panic!("no {kind:?} cell"))
            .bounds
    }

    fn slot(name: &str) -> CellKind {
        CellKind::Slot(name.into())
    }

    #[test]
    fn cells_sit_under_the_header_inside_the_area_and_never_overlap() {
        for (w, h) in SIZES {
            for decks in [2, 4] {
                for state in states() {
                    let page = page(decks, (w, h), state);
                    let header_h = header_height(h);
                    assert_eq!(page.area, Rect::new(0, header_h, w, h - header_h));
                    let bar = page.header.all();
                    for (i, a) in page.cells.iter().enumerate() {
                        let b = a.bounds;
                        assert!(
                            page.area.contains(&b),
                            "{w}x{h} {state:?}: {:?} leaves the area",
                            a.kind
                        );
                        assert!(b.y >= header_h);
                        for part in &bar {
                            assert!(!overlaps(b, *part), "{w}x{h}: {b:?} over {part:?}");
                        }
                        for other in &page.cells[i + 1..] {
                            assert!(
                                !overlaps(b, other.bounds),
                                "{w}x{h} {state:?}: {:?} vs {:?}",
                                a.kind,
                                other.kind
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn strips_stage_and_cards_line_up() {
        for (w, h) in SIZES {
            for state in states() {
                let page = page(2, (w, h), state);
                let stage = cell(&page, slot(STAGE_SLOT));
                let (s0, s1) = (
                    cell(&page, slot(&channel_slot(0))),
                    cell(&page, slot(&channel_slot(1))),
                );
                let (d0, d1) = (
                    cell(&page, CellKind::Deck(0)),
                    cell(&page, CellKind::Deck(1)),
                );
                let mixer = cell(&page, slot(MIXER_SLOT));
                for strip in [s0, s1] {
                    assert_eq!(strip.y, stage.y, "{w}x{h}: strip top is the stage top");
                    assert_eq!(strip.bottom(), d0.bottom(), "{w}x{h}: strip bottom");
                    assert!(strip.w >= CHANNEL_MIN_WIDTH, "{w}x{h}: strip {}", strip.w);
                }
                assert_eq!((d0.bottom(), mixer.bottom()), (d1.bottom(), d0.bottom()));
                assert!((s0.w - s1.w).abs() <= 1, "{w}x{h}: strips differ");
                assert!(
                    (d0.w - d1.w).abs() <= 1,
                    "{w}x{h}: decks {} vs {}",
                    d0.w,
                    d1.w
                );
            }
        }
    }

    #[test]
    fn the_library_keeps_a_margin_above_the_window_bottom() {
        for (w, h) in SIZES {
            for decks in [2, 4] {
                for open in [false, true] {
                    let state = LayoutState::default().with_sampler_open(open);
                    let page = page(decks, (w, h), state);
                    assert_eq!(cell(&page, slot(LIBRARY_SLOT)).bottom(), h - GAP);
                    assert_eq!(cell(&page, slot(SAMPLER_SLOT)).bottom(), h - GAP);
                }
            }
        }
    }

    #[test]
    fn a_short_window_keeps_the_controls_and_the_library_and_squeezes_the_stage() {
        let short = page(2, (1280, 720), LayoutState::default());
        assert!(cell(&short, CellKind::Deck(0)).h >= CONTROLS_MIN_HEIGHT);
        assert!(cell(&short, slot(LIBRARY_SLOT)).h >= LIBRARY_MIN_HEIGHT);
        let tall = page(2, (1280, 800), LayoutState::default());
        assert!(
            cell(&tall, slot(STAGE_SLOT)).h > cell(&short, slot(STAGE_SLOT)).h,
            "the stage gives up height first"
        );
    }

    #[test]
    fn the_standard_proportions_follow_the_design() {
        for (w, h) in [(1280, 800), (1920, 1080), (2560, 1440)] {
            let page = page(2, (w, h), LayoutState::default());
            let stage = cell(&page, slot(STAGE_SLOT));
            let deck = cell(&page, CellKind::Deck(0));
            let rows = (deck.bottom() - stage.y) as f32;
            let stage_share = stage.h as f32 / rows;
            assert!(
                (0.45..=0.55).contains(&stage_share),
                "{w}x{h}: stage {stage_share}"
            );
            let mixer = cell(&page, slot(MIXER_SLOT)).w as f32 / w as f32;
            assert!((0.18..=0.26).contains(&mixer), "{w}x{h}: mixer {mixer}");
            let strip = cell(&page, slot(&channel_slot(0))).w as f32 / w as f32;
            assert!((0.07..=0.13).contains(&strip), "{w}x{h}: strip {strip}");
        }
        // At 720 the controls floor takes the height, so the stage is a little under half.
        let short = page(2, (1280, 720), LayoutState::default());
        let stage = cell(&short, slot(STAGE_SLOT));
        let rows = (cell(&short, CellKind::Deck(0)).bottom() - stage.y) as f32;
        assert!((0.40..=0.55).contains(&(stage.h as f32 / rows)));
    }

    #[test]
    fn performance_mode_grows_the_stage_and_drops_the_band() {
        let normal = page(2, (1920, 1080), LayoutState::default());
        let perf = page(
            2,
            (1920, 1080),
            LayoutState {
                performance: true,
                ..LayoutState::default()
            },
        );
        assert!(cell(&perf, slot(STAGE_SLOT)).h > cell(&normal, slot(STAGE_SLOT)).h + 100);
        assert!(perf
            .cells
            .iter()
            .all(|c| c.kind != slot(LIBRARY_SLOT) && c.kind != slot(SAMPLER_SLOT)));
        assert_eq!(cell(&perf, CellKind::Deck(0)).bottom(), 1080 - GAP);
    }

    #[test]
    fn the_debug_column_is_kept_out_of_the_area() {
        let page = page_for(2, (1920, 1080), 300, GAP, LayoutState::default(), FULL);
        assert_eq!(page.area.w, 1620);
        assert!(page.cells.iter().all(|c| c.bounds.right() <= 1620));
        assert_eq!(page.header.bar.w, 1920, "the header spans the window");
    }

    #[test]
    fn a_cell_floor_round_trips_through_json() {
        let spec = LayoutSpec::standard(2);
        let parsed = LayoutSpec::from_json(&spec.to_json()).expect("round trip");
        assert_eq!(parsed, spec);
        assert!(spec.to_json().contains("\"minWidth\":112"));
        let plain = LayoutSpec::from_json(r#"{"rows":[{"cells":[{"deck":0}]}]}"#).expect("parses");
        assert_eq!(plain.rows[0].cells[0].min_width, 0);
        assert!(
            LayoutSpec::from_json(r#"{"rows":[{"cells":[{"deck":0,"minWidth":-1}]}]}"#).is_err()
        );
    }

    fn overlaps(a: Rect, b: Rect) -> bool {
        a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
    }

    fn band_cell(spec: &LayoutSpec, w: i32, slot: &str) -> Option<Rect> {
        compute_layout(spec, Rect::new(0, 0, w, 700), 12)
            .into_iter()
            .find(|c| c.kind == CellKind::Slot(slot.into()))
            .map(|c| c.bounds)
    }

    #[test]
    fn the_collapsed_sampler_drawer_is_a_narrow_handle_at_any_width() {
        for w in [1280, 1600, 1920, 2560] {
            let plan = LayoutPlan::for_window(2, (w, 800), 12, LayoutState::default());
            let drawer = band_cell(&plan.spec, w, SAMPLER_SLOT).expect("drawer");
            assert!(
                (drawer.w - COLLAPSED_DRAWER_WIDTH).abs() <= 2,
                "{w}: handle is {} px",
                drawer.w
            );
            let library = band_cell(&plan.spec, w, LIBRARY_SLOT).expect("library");
            assert!(
                library.w > w * 9 / 10 - 40,
                "{w}: the library takes the band"
            );
        }
    }

    #[test]
    fn an_open_drawer_takes_its_share_of_the_band() {
        let state = LayoutState::default().with_sampler_open(true);
        let plan = LayoutPlan::for_window(2, (1280, 800), 12, state);
        let drawer = band_cell(&plan.spec, 1280, SAMPLER_SLOT).expect("drawer");
        let share = drawer.w as f32 / (1280.0 - 12.0);
        assert!((share - SAMPLER_WEIGHT).abs() < 0.01, "share {share}");
        assert!(drawer.w >= 250, "room for the pads");
    }

    #[test]
    fn performance_mode_drops_the_library_band_and_grows_the_stage() {
        let area = Rect::new(0, 32, 1280, 688);
        let normal = LayoutPlan::for_window(2, (1280, 720), 12, LayoutState::default());
        let perf = LayoutPlan::for_window(
            2,
            (1280, 720),
            12,
            LayoutState {
                performance: true,
                ..LayoutState::default()
            },
        );
        let stage = |plan: &LayoutPlan| {
            compute_layout(&plan.spec, area, 12)
                .into_iter()
                .find(|c| c.kind == CellKind::Slot(STAGE_SLOT.into()))
                .expect("stage")
                .bounds
        };
        assert!(stage(&perf).h > stage(&normal).h + 100);
        let placed = compute_layout(&perf.spec, area, 12);
        assert!(placed
            .iter()
            .all(|c| c.kind != CellKind::Slot(LIBRARY_SLOT.into())
                && c.kind != CellKind::Slot(SAMPLER_SLOT.into())));
        assert!(!perf.sampler_visible);
        for c in &placed {
            assert!(area.contains(&c.bounds), "{c:?}");
        }
    }

    #[test]
    fn a_small_window_keeps_two_decks_and_hides_the_sampler() {
        let big = LayoutPlan::for_window(4, (1920, 1080), 12, LayoutState::default());
        assert_eq!(big.decks, 4);
        assert!(!big.degraded && big.sampler_visible);
        for window in [(1100, 720), (1280, 640), (1024, 600)] {
            let plan = LayoutPlan::for_window(4, window, 12, LayoutState::default());
            assert!(plan.degraded, "{window:?}");
            assert_eq!(plan.decks, 2, "{window:?}");
            assert_eq!(plan.spec.deck_count(), 2);
            assert!(!plan.sampler_visible);
            assert!(!plan.spec.slot_names().contains(&SAMPLER_SLOT.to_owned()));
        }
        let exact = LayoutPlan::for_window(2, MIN_WINDOW, 12, LayoutState::default());
        assert!(!exact.degraded, "the minimum itself is fully supported");
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

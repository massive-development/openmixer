//! Data-driven arrangement of the main window: rows of cells, each a deck or a named slot (mixer,
//! library, or anything a plugin registers). Pure geometry, so adding decks later is a data change.
//! JSON form:
//!
//! ```json
//! {"rows":[{"height":0.66,"cells":[{"deck":0},{"slot":"mixer","weight":0.4},{"deck":1}]},
//!          {"height":0.34,"cells":[{"slot":"library"}]}]}
//! ```

use crate::geometry::Rect;
use serde_json::{json, Map, Value};

/// Name of the mixer module slot: page tabs, master, crossfader and headphone cue, between the decks'
/// controls. The channel strips are separate slots, one beside each deck (see [`channel_slot`]).
pub const MIXER_SLOT: &str = "mixer";
/// Name of the library slot.
pub const LIBRARY_SLOT: &str = "library";
/// Prefix of a channel strip's slot name; the channel number follows (`channel:0`).
pub const CHANNEL_SLOT_PREFIX: &str = "channel:";
/// Share of the deck rows' width one channel strip takes in the standard layout.
pub const CHANNEL_WEIGHT: f32 = 0.15;
/// Share of the deck rows' width the stage and the controls below it take.
pub const CENTRE_WEIGHT: f32 = 1.0;
/// Name of the waveform stage: the decks' headers, overviews and zoomed waveforms, across the top.
pub const STAGE_SLOT: &str = "stage";
/// Share of the window height the waveform stage takes in the standard layout.
pub const STAGE_SHARE: f32 = 0.50;
/// Height the deck controls row needs (transport, hotcues, loops and the stems row), in px. On a
/// short window the stage and the library band give up height before the controls do.
pub const CONTROLS_MIN_HEIGHT: i32 = 262;
/// Smallest window the layout is designed for.
pub const MIN_WINDOW: (i32, i32) = (1024, 640);
/// Name of the sampler strip: eight pads beside the library, in the same band.
pub const SAMPLER_SLOT: &str = "sampler";
/// Share of the library band's width the sampler strip takes.
pub const SAMPLER_WEIGHT: f32 = 0.34;
/// Share of the window height the deck rows take in the standard layout.
pub const DECK_ROWS_SHARE: f32 = 0.79;
/// Share of a row's width the mixer module takes between the decks' controls.
pub const MIXER_WEIGHT: f32 = 0.6;

/// Slot name of channel `index`'s strip.
pub fn channel_slot(index: usize) -> String {
    format!("{CHANNEL_SLOT_PREFIX}{index}")
}

/// The channel number in a slot name made by [`channel_slot`].
pub fn channel_of_slot(name: &str) -> Option<usize> {
    name.strip_prefix(CHANNEL_SLOT_PREFIX)?.parse().ok()
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
        }
    }

    pub fn slot(name: &str, weight: f32) -> Self {
        Self {
            kind: CellKind::Slot(name.to_owned()),
            weight,
        }
    }

    /// A cell holding stacked rows.
    pub fn rows(rows: Vec<RowSpec>, weight: f32) -> Self {
        Self {
            kind: CellKind::Rows(rows),
            weight,
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
    /// between them, and the library and sampler strip across the bottom. Further decks (and their
    /// strips) get a row of full cards below the first.
    pub fn standard(num_decks: usize) -> Self {
        let centre = CellSpec::rows(
            vec![
                RowSpec {
                    height: STAGE_SHARE,
                    min_height: 0,
                    cells: vec![CellSpec::slot(STAGE_SLOT, 1.0)],
                },
                RowSpec {
                    height: 1.0 - STAGE_SHARE,
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
        let mut top = vec![CellSpec::slot(&channel_slot(0), CHANNEL_WEIGHT), centre];
        if num_decks > 1 {
            top.push(CellSpec::slot(&channel_slot(1), CHANNEL_WEIGHT));
        }
        let mut rows = vec![RowSpec {
            height: DECK_ROWS_SHARE,
            min_height: 0,
            cells: top,
        }];
        if num_decks > 2 {
            rows.push(RowSpec {
                height: DECK_ROWS_SHARE,
                min_height: 0,
                cells: (2..num_decks)
                    .flat_map(|d| {
                        [
                            CellSpec::deck(d),
                            CellSpec::slot(&channel_slot(d), CHANNEL_WEIGHT),
                        ]
                    })
                    .collect(),
            });
        }
        rows.push(RowSpec {
            height: 1.0 - DECK_ROWS_SHARE,
            min_height: 0,
            cells: vec![
                CellSpec::slot(LIBRARY_SLOT, 1.0 - SAMPLER_WEIGHT),
                CellSpec::slot(SAMPLER_SLOT, SAMPLER_WEIGHT),
            ],
        });
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
    Ok(CellSpec {
        kind,
        weight: weight as f32,
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
    Value::Object(obj)
}

/// Splits `length` by weights with `gap` between parts. The last part absorbs rounding. Returns
/// `(position, size)` per part. Single precision.
fn split(length: i32, weights: &[f32], gap: i32) -> Vec<(i32, i32)> {
    let total: f32 = weights.iter().sum();
    let n = weights.len();
    let usable = (length - gap * (n as i32 - 1)).max(0) as f32;
    let mut parts = Vec::with_capacity(n);
    let mut pos = 0;
    for (i, w) in weights.iter().enumerate() {
        let size = if i == n - 1 {
            (length - pos).max(0)
        } else {
            (usable * w / total) as i32
        };
        parts.push((pos, size));
        pos += size + gap;
    }
    parts
}

/// Like [`split`], with a floor per part: a part that its share leaves below its floor takes the
/// floor, and the rest is shared out again among the others.
fn split_with_floors(length: i32, weights: &[f32], floors: &[i32], gap: i32) -> Vec<(i32, i32)> {
    let n = weights.len();
    let usable = (length - gap * (n as i32 - 1)).max(0);
    let mut fixed: Vec<Option<i32>> = vec![None; n];
    loop {
        let taken: i32 = fixed.iter().flatten().sum();
        let free_weight: f32 = (0..n)
            .filter(|&i| fixed[i].is_none())
            .map(|i| weights[i])
            .sum();
        if free_weight <= 0.0 {
            break;
        }
        let share = |i: usize| ((usable - taken).max(0) as f32 * weights[i] / free_weight) as i32;
        match (0..n).find(|&i| fixed[i].is_none() && share(i) < floors[i]) {
            Some(i) => fixed[i] = Some(floors[i]),
            None => {
                return place_parts(length, gap, n, |i| fixed[i].unwrap_or_else(|| share(i)));
            }
        }
    }
    place_parts(length, gap, n, |i| fixed[i].unwrap_or(0))
}

/// Lays `n` parts of the given sizes along `length` with `gap` between them; the last part takes
/// whatever is left, so rounding never leaves a seam at the end.
fn place_parts(length: i32, gap: i32, n: usize, size: impl Fn(usize) -> i32) -> Vec<(i32, i32)> {
    let mut parts = Vec::with_capacity(n);
    let mut pos = 0;
    for i in 0..n {
        let s = if i == n - 1 {
            (length - pos).max(0)
        } else {
            size(i)
        };
        parts.push((pos, s));
        pos += s + gap;
    }
    parts
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
        for (c, (x, w)) in split(area.w, &weights, gap).into_iter().enumerate() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record_bar::{automix_width, record_rect};

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

    /// At the test sizes the cells sit under the header strip (automix bar, Record button and the
    /// Settings button, all in y `0..HEADER`), stay inside the window and never overlap each other.
    #[test]
    fn the_cells_clear_the_header_strip_and_each_other_at_the_test_sizes() {
        const HEADER: i32 = 32;
        const SETTINGS_RESERVE: i32 = 96;
        const GAP: i32 = 8;
        let spec = LayoutSpec::standard(2);
        for (w, h) in [(900, 600), (1024, 640), (1280, 800), (1920, 1080)] {
            let header = [
                Rect::new(0, 0, automix_width(w, SETTINGS_RESERVE), HEADER),
                record_rect(w, 0, HEADER, SETTINGS_RESERVE).expect("the record button fits"),
                Rect::new(w - SETTINGS_RESERVE, 0, SETTINGS_RESERVE, HEADER),
            ];
            let cells = compute_layout(&spec, Rect::new(0, HEADER, w, h - HEADER), GAP);
            for cell in &cells {
                let b = cell.bounds;
                assert!(
                    b.y >= HEADER && b.x >= 0 && b.x + b.w <= w && b.y + b.h <= h,
                    "{w}x{h}: {b:?}"
                );
                for strip in &header {
                    assert!(!overlaps(b, *strip), "{w}x{h}: {b:?} over {strip:?}");
                }
            }
            for (i, a) in cells.iter().enumerate() {
                for b in &cells[i + 1..] {
                    assert!(!overlaps(a.bounds, b.bounds), "{w}x{h}: {a:?} vs {b:?}");
                }
            }
        }
    }

    fn overlaps(a: Rect, b: Rect) -> bool {
        a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
    }

    #[test]
    fn split_gives_the_last_part_the_rounding() {
        let parts = split(100, &[1.0, 1.0, 1.0], 0);
        assert_eq!(parts, vec![(0, 33), (33, 33), (66, 34)]);
    }
}

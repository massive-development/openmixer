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

/// Name of the mixer slot.
pub const MIXER_SLOT: &str = "mixer";
/// Name of the library slot.
pub const LIBRARY_SLOT: &str = "library";
/// Share of the window height the deck rows take in the standard layout.
pub const DECK_ROWS_SHARE: f32 = 0.68;
/// Share of a row's width the mixer takes next to the decks.
pub const MIXER_WEIGHT: f32 = 0.85;

/// What fills a cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CellKind {
    /// A deck by index.
    Deck(usize),
    /// A named slot, filled by the app or a later task.
    Slot(String),
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

    pub fn is_deck(&self) -> bool {
        matches!(self.kind, CellKind::Deck(_))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RowSpec {
    /// Share of the window height.
    pub height: f32,
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
    /// Decks flank the mixer on the first row (any further decks get a row below), library at the
    /// bottom.
    pub fn standard(num_decks: usize) -> Self {
        let mut top = RowSpec {
            height: DECK_ROWS_SHARE,
            cells: vec![
                CellSpec::deck(0),
                CellSpec::slot(MIXER_SLOT, MIXER_WEIGHT),
                CellSpec::deck(1),
            ],
        };
        if num_decks < 2 {
            top.cells = vec![CellSpec::deck(0), CellSpec::slot(MIXER_SLOT, MIXER_WEIGHT)];
        }
        let mut rows = vec![top];
        if num_decks > 2 {
            rows.push(RowSpec {
                height: DECK_ROWS_SHARE,
                cells: (2..num_decks).map(CellSpec::deck).collect(),
            });
        }
        rows.push(RowSpec {
            height: 1.0 - DECK_ROWS_SHARE,
            cells: vec![CellSpec::slot(LIBRARY_SLOT, 1.0)],
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
        let rows: Vec<Value> = self
            .rows
            .iter()
            .map(|row| {
                let cells: Vec<Value> = row.cells.iter().map(cell_json).collect();
                json!({ "height": f64::from(row.height), "cells": cells })
            })
            .collect();
        json!({ "rows": rows }).to_string()
    }

    /// One more than the highest deck index used; 0 with no decks.
    pub fn deck_count(&self) -> usize {
        self.rows
            .iter()
            .flat_map(|r| r.cells.iter())
            .filter_map(|c| match c.kind {
                CellKind::Deck(d) => Some(d + 1),
                CellKind::Slot(_) => None,
            })
            .max()
            .unwrap_or(0)
    }

    /// Slot names in order of first use, without repeats.
    pub fn slot_names(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for c in self.rows.iter().flat_map(|r| r.cells.iter()) {
            if let CellKind::Slot(name) = &c.kind {
                if !names.contains(name) {
                    names.push(name.clone());
                }
            }
        }
        names
    }
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
    Ok(RowSpec {
        height: height as f32,
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
    let kind = match (deck, slot) {
        (Some(d), None) => {
            let index = d.as_u64().ok_or(LayoutError::Malformed(
                "deck must be a non-negative integer",
            ))?;
            CellKind::Deck(index as usize)
        }
        (None, Some(s)) => {
            let name = s
                .as_str()
                .filter(|n| !n.is_empty())
                .ok_or(LayoutError::Malformed("slot must be a non-empty name"))?;
            CellKind::Slot(name.to_owned())
        }
        _ => {
            return Err(LayoutError::Malformed(
                "a cell has exactly one of deck or slot",
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

/// Splits `area` by row heights and cell weights with `gap` between neighbours.
pub fn compute_layout(spec: &LayoutSpec, area: Rect, gap: i32) -> Vec<PlacedCell> {
    if spec.rows.is_empty() {
        return Vec::new();
    }
    let heights: Vec<f32> = spec.rows.iter().map(|r| r.height).collect();
    let row_parts = split(area.h, &heights, gap);
    let mut placed = Vec::new();
    for (r, row) in spec.rows.iter().enumerate() {
        let weights: Vec<f32> = row.cells.iter().map(|c| c.weight).collect();
        for (c, (x, w)) in split(area.w, &weights, gap).into_iter().enumerate() {
            let (y, h) = row_parts[r];
            placed.push(PlacedCell {
                kind: row.cells[c].kind.clone(),
                bounds: Rect::new(area.x + x, area.y + y, w, h),
            });
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
            CellKind::Slot(_) => true,
        })
        .collect()
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

    #[test]
    fn split_gives_the_last_part_the_rounding() {
        let parts = split(100, &[1.0, 1.0, 1.0], 0);
        assert_eq!(parts, vec![(0, 33), (33, 33), (66, 34)]);
    }
}

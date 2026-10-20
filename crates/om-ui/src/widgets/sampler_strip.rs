//! Geometry of the sampler strip: a caption line, a 4x2 grid of pads on the left and a control
//! column on the right (mode, load and clear, gain and pitch steppers). Sized by fractions of the
//! card, so it fits the library band at the smallest window. Geometry only; the gpui layer draws
//! the strip and wires it to [`crate::sampler_binding::SamplerBinding`].

use super::px;
use crate::geometry::Rect;
use crate::theme::ThemeTokens;

/// Pads in the grid.
pub const PADS: usize = 8;
/// Grid columns; the grid has two rows, pads 1 to 4 on top.
pub const GRID_COLS: usize = 4;

/// Where each part of the strip sits, in the parent's coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SamplerStripLayout {
    /// The caption line: the selected pad, or the path field while one is open.
    pub caption: Rect,
    pub grid: [Rect; PADS],
    pub mode: Rect,
    pub load: Rect,
    pub clear: Rect,
    pub gain_down: Rect,
    pub gain_up: Rect,
    pub pitch_down: Rect,
    pub pitch_up: Rect,
}

/// Places the strip inside `bounds` (the card's cell).
pub fn sampler_strip_layout(bounds: Rect, theme: &ThemeTokens) -> SamplerStripLayout {
    let gap = theme.inner_gap.min(6).max(2);
    let mut area = bounds.reduced(px(theme.pad as f32).max(2), px(theme.pad as f32).max(2));
    let caption_h = px(theme.type_caption * 1.5).min(area.h).max(0);
    let caption = area.remove_top(caption_h);
    area.remove_top(gap.min(area.h));

    // Control column on the right, about a third of the body; the grid takes the rest.
    let control_w = (area.w * 36 / 100).max(0);
    let mut controls = area.remove_right(control_w);
    controls.x += gap;
    controls.w = (controls.w - gap).max(0);
    area.remove_right(gap.min(area.w));
    let grid_area = area;

    let mut grid = [Rect::default(); PADS];
    let rows = PADS / GRID_COLS;
    let cell_w = ((grid_area.w - gap * (GRID_COLS as i32 - 1)) / GRID_COLS as i32).max(0);
    let cell_h = ((grid_area.h - gap * (rows as i32 - 1)) / rows as i32).max(0);
    for (index, cell) in grid.iter_mut().enumerate() {
        let (row, col) = (index / GRID_COLS, index % GRID_COLS);
        *cell = Rect::new(
            grid_area.x + col as i32 * (cell_w + gap),
            grid_area.y + row as i32 * (cell_h + gap),
            cell_w,
            cell_h,
        );
    }

    // Four control rows: mode; load and clear; gain; pitch.
    let row_h = ((controls.h - gap * 3) / 4).max(0);
    let next_row = |rows_left: &mut Rect| {
        let row = rows_left.remove_top(row_h);
        rows_left.remove_top(gap.min(rows_left.h));
        row
    };
    let mut rows_left = controls;
    let mode = next_row(&mut rows_left);
    let load_row = next_row(&mut rows_left);
    let half = ((load_row.w - gap) / 2).max(0);
    let load = Rect::new(load_row.x, load_row.y, half, load_row.h);
    let clear = Rect::new(
        load_row.x + half + gap,
        load_row.y,
        (load_row.w - half - gap).max(0),
        load_row.h,
    );
    let gain_row = next_row(&mut rows_left);
    let pitch_row = next_row(&mut rows_left);
    let stepper = |row: Rect| {
        let w = ((row.w - gap) / 2).max(0);
        (
            Rect::new(row.x, row.y, w, row.h),
            Rect::new(row.x + w + gap, row.y, (row.w - w - gap).max(0), row.h),
        )
    };
    let (gain_down, gain_up) = stepper(gain_row);
    let (pitch_down, pitch_up) = stepper(pitch_row);
    SamplerStripLayout {
        caption,
        grid,
        mode,
        load,
        clear,
        gain_down,
        gain_up,
        pitch_down,
        pitch_up,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inside(outer: &Rect, inner: &Rect) -> bool {
        inner.w >= 0
            && inner.h >= 0
            && inner.x >= outer.x
            && inner.y >= outer.y
            && inner.x + inner.w <= outer.x + outer.w
            && inner.y + inner.h <= outer.y + outer.h
    }

    fn parts(l: &SamplerStripLayout) -> Vec<Rect> {
        let mut v = vec![l.caption];
        v.extend(l.grid);
        v.extend([
            l.mode,
            l.load,
            l.clear,
            l.gain_down,
            l.gain_up,
            l.pitch_down,
            l.pitch_up,
        ]);
        v
    }

    #[test]
    fn every_part_sits_inside_the_strip_and_no_two_overlap() {
        let theme = ThemeTokens::builtin_dark();
        for (w, h) in [(306, 113), (250, 110), (420, 160), (600, 260)] {
            let bounds = Rect::sized(w, h);
            let layout = sampler_strip_layout(bounds, &theme);
            let all = parts(&layout);
            for part in &all {
                assert!(inside(&bounds, part), "{w}x{h}: {part:?} outside the strip");
            }
            for (i, a) in all.iter().enumerate() {
                for b in &all[i + 1..] {
                    if a.w > 0 && a.h > 0 && b.w > 0 && b.h > 0 {
                        assert!(!a.intersects(b), "{w}x{h}: {a:?} overlaps {b:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn pads_read_left_to_right_top_to_bottom() {
        let theme = ThemeTokens::builtin_dark();
        let layout = sampler_strip_layout(Rect::sized(306, 113), &theme);
        assert!(layout.grid[0].x < layout.grid[1].x);
        assert!(layout.grid[3].x > layout.grid[2].x);
        assert!(layout.grid[4].y > layout.grid[0].y);
        assert_eq!(layout.grid[4].x, layout.grid[0].x);
    }
}

//! Integer layout rectangles: `remove_*` takes a slice
//! off one edge and shrinks the rectangle, clamping to what is left. Pixels are whole numbers; the
//! waveform code uses `f64` rather than `f32`.

/// A rectangle in whole pixels. `x`/`y` are relative to whatever owns the layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    /// A rectangle at the origin with the given size.
    pub const fn sized(w: i32, h: i32) -> Self {
        Self::new(0, 0, w, h)
    }

    pub const fn right(&self) -> i32 {
        self.x + self.w
    }

    pub const fn bottom(&self) -> i32 {
        self.y + self.h
    }

    pub const fn centre_x(&self) -> i32 {
        self.x + self.w / 2
    }

    pub const fn centre_y(&self) -> i32 {
        self.y + self.h / 2
    }

    pub const fn is_empty(&self) -> bool {
        self.w <= 0 || self.h <= 0
    }

    /// Takes `amount` off the top (clamped to the height) and returns it.
    pub fn remove_top(&mut self, amount: i32) -> Self {
        let taken = amount.clamp(0, self.h.max(0));
        let out = Self::new(self.x, self.y, self.w, taken);
        self.y += taken;
        self.h -= taken;
        out
    }

    /// Takes `amount` off the bottom (clamped to the height) and returns it.
    pub fn remove_bottom(&mut self, amount: i32) -> Self {
        let taken = amount.clamp(0, self.h.max(0));
        let out = Self::new(self.x, self.bottom() - taken, self.w, taken);
        self.h -= taken;
        out
    }

    /// Takes `amount` off the left (clamped to the width) and returns it.
    pub fn remove_left(&mut self, amount: i32) -> Self {
        let taken = amount.clamp(0, self.w.max(0));
        let out = Self::new(self.x, self.y, taken, self.h);
        self.x += taken;
        self.w -= taken;
        out
    }

    /// Takes `amount` off the right (clamped to the width) and returns it.
    pub fn remove_right(&mut self, amount: i32) -> Self {
        let taken = amount.clamp(0, self.w.max(0));
        let out = Self::new(self.right() - taken, self.y, taken, self.h);
        self.w -= taken;
        out
    }

    /// Insets each side by `dx` horizontally and `dy` vertically; never negative in size.
    pub fn reduced(&self, dx: i32, dy: i32) -> Self {
        Self::new(
            self.x + dx,
            self.y + dy,
            (self.w - 2 * dx).max(0),
            (self.h - 2 * dy).max(0),
        )
    }

    /// Grows each side by `dx` and `dy`.
    pub fn expanded(&self, dx: i32, dy: i32) -> Self {
        Self::new(self.x - dx, self.y - dy, self.w + 2 * dx, self.h + 2 * dy)
    }

    /// Same centre, new size.
    pub fn with_size_keeping_centre(&self, w: i32, h: i32) -> Self {
        Self::new(self.centre_x() - w / 2, self.centre_y() - h / 2, w, h)
    }

    /// Drops `amount` from the left edge without changing the right edge.
    pub fn with_trimmed_left(&self, amount: i32) -> Self {
        let taken = amount.clamp(0, self.w.max(0));
        Self::new(self.x + taken, self.y, self.w - taken, self.h)
    }

    /// True when `other` lies fully inside `self` (an empty `other` counts as inside).
    pub fn contains(&self, other: &Rect) -> bool {
        other.is_empty()
            || (other.x >= self.x
                && other.y >= self.y
                && other.right() <= self.right()
                && other.bottom() <= self.bottom())
    }

    /// True when the two rectangles share any area.
    pub fn intersects(&self, other: &Rect) -> bool {
        !self.is_empty()
            && !other.is_empty()
            && self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }
}

/// Placement of one caption strip above a control: the caption takes `caption_height` off the top of
/// `cell` and the control gets what is left. Returns `(caption, control)`.
pub fn place_captioned(cell: Rect, caption_height: i32) -> (Rect, Rect) {
    let mut rest = cell;
    let caption = rest.remove_top(caption_height);
    (caption, rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removals_clamp_and_shrink() {
        let mut r = Rect::new(10, 20, 100, 50);
        assert_eq!(r.remove_top(10), Rect::new(10, 20, 100, 10));
        assert_eq!(r, Rect::new(10, 30, 100, 40));
        assert_eq!(r.remove_left(500), Rect::new(10, 30, 100, 40));
        assert!(r.is_empty() || r.w == 0);
        let mut s = Rect::new(0, 0, 10, 10);
        assert_eq!(s.remove_bottom(4), Rect::new(0, 6, 10, 4));
        assert_eq!(s.remove_right(3), Rect::new(7, 0, 3, 6));
    }

    #[test]
    fn centring_and_insets() {
        let r = Rect::new(0, 0, 100, 40);
        assert_eq!(
            r.with_size_keeping_centre(40, 10),
            Rect::new(30, 15, 40, 10)
        );
        assert_eq!(r.reduced(5, 3), Rect::new(5, 3, 90, 34));
        assert_eq!(Rect::sized(4, 4).reduced(10, 10).w, 0);
    }

    #[test]
    fn containment_and_overlap() {
        let outer = Rect::sized(100, 100);
        assert!(outer.contains(&Rect::new(0, 0, 100, 100)));
        assert!(!outer.contains(&Rect::new(90, 0, 20, 10)));
        assert!(
            !Rect::new(0, 0, 10, 10).intersects(&Rect::new(10, 0, 10, 10)),
            "touching is not overlap"
        );
        assert!(Rect::new(0, 0, 10, 10).intersects(&Rect::new(9, 9, 5, 5)));
    }

    #[test]
    fn captions_take_the_top() {
        let (caption, control) = place_captioned(Rect::new(0, 0, 50, 60), 16);
        assert_eq!(caption, Rect::new(0, 0, 50, 16));
        assert_eq!(control, Rect::new(0, 16, 50, 44));
    }
}

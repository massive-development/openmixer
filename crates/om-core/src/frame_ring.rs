/// Fixed-capacity ring of stereo frames tagged with a source index. Single-owner (audio thread);
/// allocates only in [`FrameRing::new`]; pushing into a full ring drops the oldest frame.
#[derive(Debug, Clone)]
pub struct FrameRing {
    frames: Vec<[f32; 2]>,
    indices: Vec<i64>,
    head: usize,
    count: usize,
}

impl FrameRing {
    /// Creates an empty ring. Panics if `capacity` is zero.
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "FrameRing capacity must be non-zero");
        Self {
            frames: vec![[0.0; 2]; capacity],
            indices: vec![0; capacity],
            head: 0,
            count: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn capacity(&self) -> usize {
        self.indices.len()
    }

    pub fn clear(&mut self) {
        self.head = 0;
        self.count = 0;
    }

    pub fn push(&mut self, frame: [f32; 2], index: i64) {
        if self.count == self.capacity() {
            self.drop_oldest();
        }
        let slot = (self.head + self.count) % self.capacity();
        self.frames[slot] = frame;
        self.indices[slot] = index;
        self.count += 1;
    }

    /// Removes and returns the oldest frame with its index.
    pub fn pop(&mut self) -> Option<([f32; 2], i64)> {
        if self.count == 0 {
            return None;
        }
        let item = (self.frames[self.head], self.indices[self.head]);
        self.drop_oldest();
        Some(item)
    }

    /// Replaces `dst`'s contents with the newest `n` frames, oldest first.
    pub fn copy_newest_to(&self, dst: &mut FrameRing, n: usize) {
        dst.clear();
        let n = n.min(self.count);
        for i in (self.count - n)..self.count {
            let slot = (self.head + i) % self.capacity();
            dst.push(self.frames[slot], self.indices[slot]);
        }
    }

    fn drop_oldest(&mut self) {
        self.head = (self.head + 1) % self.capacity();
        self.count -= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::FrameRing;

    #[test]
    fn full_ring_drops_oldest_frame() {
        let mut ring = FrameRing::new(2);
        ring.push([1.0, 1.0], 1);
        ring.push([2.0, 2.0], 2);
        ring.push([3.0, 3.0], 3);
        assert_eq!(ring.len(), 2);
        assert_eq!(ring.pop(), Some(([2.0, 2.0], 2)));
        assert_eq!(ring.pop(), Some(([3.0, 3.0], 3)));
        assert_eq!(ring.pop(), None);
    }

    #[test]
    fn copy_newest_keeps_the_tail_oldest_first() {
        let mut src = FrameRing::new(4);
        for i in 0..4 {
            src.push([i as f32, 0.0], i);
        }
        let mut dst = FrameRing::new(4);
        src.copy_newest_to(&mut dst, 2);
        assert_eq!(dst.pop(), Some(([2.0, 0.0], 2)));
        assert_eq!(dst.pop(), Some(([3.0, 0.0], 3)));
        assert!(dst.is_empty());
    }
}

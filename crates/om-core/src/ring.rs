//! Single-producer single-consumer ring of `Copy` items for handing data between two threads.
//! Writes that do not fit are truncated, never overwritten; reads that find too little return less.

/// Producer half. Owned by exactly one writer thread.
pub struct SpscProducer<T: Copy>(rtrb::Producer<T>);

/// Consumer half. Owned by exactly one reader thread.
pub struct SpscConsumer<T: Copy>(rtrb::Consumer<T>);

/// Creates a ring whose capacity is `min_capacity` rounded up to a power of two (minimum 2).
pub fn spsc<T: Copy>(min_capacity: usize) -> (SpscProducer<T>, SpscConsumer<T>) {
    let capacity = min_capacity.max(2).next_power_of_two();
    let (producer, consumer) = rtrb::RingBuffer::new(capacity);
    (SpscProducer(producer), SpscConsumer(consumer))
}

impl<T: Copy> SpscProducer<T> {
    /// Total number of slots.
    pub fn capacity(&self) -> usize {
        self.0.buffer().capacity()
    }

    /// Slots currently free for writing.
    pub fn writable_count(&self) -> usize {
        self.0.slots()
    }

    /// Writes as many items of `src` as fit and returns how many were written.
    pub fn write(&mut self, src: &[T]) -> usize {
        let (written, _rest) = self.0.push_partial_slice(src);
        written.len()
    }
}

impl<T: Copy> SpscConsumer<T> {
    /// Items currently available for reading.
    pub fn readable_count(&self) -> usize {
        self.0.slots()
    }

    /// Reads up to `dst.len()` items and returns how many were read.
    pub fn read(&mut self, dst: &mut [T]) -> usize {
        let (read, _rest) = self.0.pop_partial_slice(dst);
        read.len()
    }

    /// Discards up to `n` items and returns how many were discarded.
    pub fn skip(&mut self, n: usize) -> usize {
        let count = n.min(self.0.slots());
        if count > 0 {
            if let Ok(chunk) = self.0.read_chunk(count) {
                chunk.commit_all();
            }
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use super::spsc;

    #[test]
    fn capacity_rounds_up_to_power_of_two() {
        let (producer, consumer) = spsc::<f32>(100);
        assert_eq!(producer.capacity(), 128);
        assert_eq!(producer.writable_count(), 128);
        assert_eq!(consumer.readable_count(), 0);
    }

    #[test]
    fn basic_write_read() {
        let (mut producer, mut consumer) = spsc::<i32>(8);
        let input = [1, 2, 3, 4, 5];
        let mut output = [0; 5];
        assert_eq!(producer.write(&input), 5);
        assert_eq!(consumer.readable_count(), 5);
        assert_eq!(consumer.read(&mut output), 5);
        assert_eq!(output, input);
    }

    #[test]
    fn overflow_is_truncated_not_overwritten() {
        let (mut producer, mut consumer) = spsc::<i32>(8);
        let input: Vec<i32> = (0..12).collect();
        assert_eq!(producer.write(&input), 8);
        assert_eq!(producer.write(&input[..1]), 0);
        let mut output = [0; 8];
        assert_eq!(consumer.read(&mut output), 8);
        assert_eq!(output.to_vec(), (0..8).collect::<Vec<i32>>());
    }

    #[test]
    fn underrun_returns_fewer_items() {
        let (mut producer, mut consumer) = spsc::<i32>(8);
        producer.write(&[7, 8, 9]);
        let mut output = [0; 8];
        assert_eq!(consumer.read(&mut output), 3);
        assert_eq!(consumer.read(&mut output), 0);
    }

    #[test]
    fn wraparound_preserves_order() {
        let (mut producer, mut consumer) = spsc::<i32>(8);
        let mut counter = 0;
        let mut expected = 0;
        for _ in 0..100 {
            let input: Vec<i32> = (0..5)
                .map(|_| {
                    counter += 1;
                    counter - 1
                })
                .collect();
            assert_eq!(producer.write(&input), 5);
            let mut output = [0; 5];
            assert_eq!(consumer.read(&mut output), 5);
            for v in output {
                assert_eq!(v, expected);
                expected += 1;
            }
        }
    }

    #[test]
    fn skip_discards_items() {
        let (mut producer, mut consumer) = spsc::<i32>(8);
        producer.write(&[0, 1, 2, 3, 4, 5]);
        assert_eq!(consumer.skip(4), 4);
        let mut output = [0; 2];
        assert_eq!(consumer.read(&mut output), 2);
        assert_eq!(output, [4, 5]);
        assert_eq!(consumer.skip(10), 0);
    }

    #[test]
    fn threaded_producer_consumer_delivers_every_item_in_order() {
        const TOTAL: i32 = 2_000_000;
        let (mut producer, mut consumer) = spsc::<i32>(1024);

        let consumed_in_order = std::thread::scope(|s| {
            s.spawn(move || {
                let mut next = 0;
                let mut block = [0; 37];
                while next < TOTAL {
                    let n = (TOTAL - next).min(37) as usize;
                    for (i, slot) in block[..n].iter_mut().enumerate() {
                        *slot = next + i as i32;
                    }
                    let written = producer.write(&block[..n]) as i32;
                    next += written;
                    if written == 0 {
                        std::thread::yield_now();
                    }
                }
            });
            let consumer_handle = s.spawn(move || {
                let mut expected = 0;
                let mut block = [0; 53];
                while expected < TOTAL {
                    let read = consumer.read(&mut block);
                    for &v in &block[..read] {
                        if v != expected {
                            return false;
                        }
                        expected += 1;
                    }
                    if read == 0 {
                        std::thread::yield_now();
                    }
                }
                consumer.readable_count() == 0
            });
            consumer_handle.join().unwrap()
        });
        assert!(
            consumed_in_order,
            "items must arrive in order without loss or duplication"
        );
    }
}

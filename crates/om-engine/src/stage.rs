/// A stereo in-place audio stage that can be inserted into a channel or the master bus.
///
/// `prepare` runs on the control thread before insertion; `process` runs on the audio thread and
/// must not allocate, lock or perform I/O. `left` and `right` always have the same length.
pub trait ProcessStage: Send {
    fn prepare(&mut self, _sample_rate: f64, _max_block_size: usize) {}
    fn process(&mut self, left: &mut [f32], right: &mut [f32]);
}

//! The real-time callback an audio device drives. Shape of C++ `AudioCallback`; the device layer
//! itself lands with the audio task.

/// `process` runs on the audio thread: no allocation, locks or blocking. Every slice in `inputs`
/// and `outputs` has the same length, which is the frame count of the block.
pub trait AudioCallback: Send {
    fn prepare(&mut self, _sample_rate: f64, _max_block_size: usize) {}

    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]);

    fn release(&mut self) {}
}

//! `StreamResampler` allocates nothing while resampling once the caller's output has room.
//! Run in its own binary because it installs the allocation-guard global allocator.

use assert_no_alloc::{assert_no_alloc, AllocDisabler};
use om_stems::StreamResampler;

#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

#[test]
fn push_and_finish_do_not_allocate_after_construction() {
    let mut resampler = StreamResampler::new(48_000.0, 44_100.0).expect("valid rates");
    let block: Vec<f32> = (0..700).map(|n| (n as f32 * 0.05).sin() * 0.5).collect();
    let blocks = 60;
    let mut out: Vec<f32> = Vec::with_capacity(blocks * block.len() + 4096);
    // Warm-up block outside the guard, so lazily sized state is not counted.
    resampler.push(&block, &mut out);
    assert_no_alloc(|| {
        for _ in 1..blocks {
            resampler.push(&block, &mut out);
        }
        resampler.finish(&mut out);
    });
    assert_eq!(
        out.len() as u64,
        StreamResampler::output_len(48_000.0, 44_100.0, (blocks * block.len()) as u64)
    );
}

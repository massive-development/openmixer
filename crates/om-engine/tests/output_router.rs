//! Port of tests/unit/OutputRouterTest.cpp (the engine-backed cases follow with the deck engine).

use om_engine::{AudioCallback, OutputRouter, NO_PAIR};

/// Engine stand-in: output channel c carries the constant (c + 1) so routing is easy to read.
struct ConstantSource {
    blocks: usize,
}

impl AudioCallback for ConstantSource {
    fn process(&mut self, _: &[&[f32]], outputs: &mut [&mut [f32]]) {
        for (c, out) in outputs.iter_mut().enumerate() {
            out.fill((c + 1) as f32);
        }
        self.blocks += 1;
        assert_eq!(outputs.len(), 4);
    }
}

struct Device {
    buffers: Vec<Vec<f32>>,
}

impl Device {
    fn new(channels: usize, samples: usize) -> Self {
        Self {
            buffers: vec![vec![-9.0; samples]; channels],
        }
    }

    fn outputs(&mut self) -> Vec<&mut [f32]> {
        self.buffers.iter_mut().map(Vec::as_mut_slice).collect()
    }

    fn at(&self, channel: usize, i: usize) -> f32 {
        self.buffers[channel][i]
    }
}

#[test]
fn output_router_test_defaults_send_master_to_pair_zero_and_cue_to_pair_one() {
    let (mut router, _pairs) = OutputRouter::new();
    router.prepare(64);
    let mut src = ConstantSource { blocks: 0 };
    let mut dev = Device::new(4, 64);
    router.process(&mut src, &mut dev.outputs(), 64);
    assert_eq!(dev.at(0, 0), 1.0);
    assert_eq!(dev.at(1, 0), 2.0);
    assert_eq!(dev.at(2, 0), 3.0);
    assert_eq!(dev.at(3, 0), 4.0);
}

#[test]
fn output_router_test_pairs_can_be_swapped_and_moved_to_higher_outputs() {
    let (mut router, pairs) = OutputRouter::new();
    router.prepare(64);
    pairs.set_master_pair(2);
    pairs.set_cue_pair(0);
    let mut src = ConstantSource { blocks: 0 };
    let mut dev = Device::new(6, 64);
    router.process(&mut src, &mut dev.outputs(), 64);
    assert_eq!(dev.at(0, 0), 3.0);
    assert_eq!(dev.at(1, 0), 4.0); // cue on 1+2
    assert_eq!(dev.at(2, 0), 0.0);
    assert_eq!(dev.at(3, 0), 0.0); // unrouted pair silenced
    assert_eq!(dev.at(4, 0), 1.0);
    assert_eq!(dev.at(5, 0), 2.0); // master on 5+6
}

#[test]
fn output_router_test_no_cue_pair_leaves_only_the_master() {
    let (mut router, pairs) = OutputRouter::new();
    router.prepare(32);
    pairs.set_cue_pair(NO_PAIR);
    let mut src = ConstantSource { blocks: 0 };
    let mut dev = Device::new(4, 32);
    router.process(&mut src, &mut dev.outputs(), 32);
    assert_eq!(dev.at(0, 0), 1.0);
    assert_eq!(dev.at(2, 0), 0.0);
    assert_eq!(dev.at(3, 0), 0.0);
}

#[test]
fn output_router_test_out_of_range_pairs_are_ignored_without_writing_past_the_device() {
    let (mut router, pairs) = OutputRouter::new();
    router.prepare(32);
    pairs.set_master_pair(7);
    pairs.set_cue_pair(1); // a 2-channel device has no pair 1
    let mut src = ConstantSource { blocks: 0 };
    let mut dev = Device::new(2, 32);
    router.process(&mut src, &mut dev.outputs(), 32);
    assert_eq!(dev.at(0, 0), 0.0);
    assert_eq!(dev.at(1, 0), 0.0);
}

#[test]
fn output_router_test_same_master_and_cue_pair_keeps_the_master() {
    let (mut router, pairs) = OutputRouter::new();
    router.prepare(16);
    pairs.set_cue_pair(0);
    let mut src = ConstantSource { blocks: 0 };
    let mut dev = Device::new(2, 16);
    router.process(&mut src, &mut dev.outputs(), 16);
    assert_eq!(dev.at(0, 0), 1.0);
    assert_eq!(dev.at(1, 0), 2.0);
}

#[test]
fn output_router_test_long_device_blocks_are_rendered_in_chunks() {
    let (mut router, _pairs) = OutputRouter::new();
    router.prepare(16);
    let mut src = ConstantSource { blocks: 0 };
    let mut dev = Device::new(2, 50);
    router.process(&mut src, &mut dev.outputs(), 50);
    assert_eq!(src.blocks, 4); // 16 + 16 + 16 + 2
    for i in 0..50 {
        assert_eq!(dev.at(0, i), 1.0, "{i}");
    }
}

#[test]
fn output_router_test_mono_devices_go_straight_to_the_source() {
    struct Probe {
        seen: usize,
    }
    impl AudioCallback for Probe {
        fn process(&mut self, _: &[&[f32]], outputs: &mut [&mut [f32]]) {
            self.seen = outputs.len();
        }
    }
    let (mut router, _pairs) = OutputRouter::new();
    router.prepare(16);
    let mut probe = Probe { seen: 0 };
    let mut dev = Device::new(1, 16);
    router.process(&mut probe, &mut dev.outputs(), 16);
    assert_eq!(probe.seen, 1);
}

#[test]
fn output_router_test_process_before_prepare_is_silent_and_terminates() {
    let (mut router, _pairs) = OutputRouter::new(); // no prepare(): scratch has no room
    let mut src = ConstantSource { blocks: 0 };
    let mut dev = Device::new(2, 32);
    router.process(&mut src, &mut dev.outputs(), 32);
    assert_eq!(src.blocks, 0);
    assert_eq!(dev.at(0, 0), 0.0);
    assert_eq!(dev.at(1, 31), 0.0);
}

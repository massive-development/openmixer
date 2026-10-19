//! Port of tests/unit/DeckLoopTest.cpp: gapless loop wrap, loop exit, long loops past the prefetch
//! slot, resizing, seeking out of a loop and hotcues. Same numbers and tolerances as the C++.

use super::tests::{pump_until_buffered, wait_for, write_ramp, Fixture, FRAMES, RATE};
use super::{Deck, DeckAudio};
use crate::quantizer::Loop;
use om_core::ParameterTarget;
use std::path::PathBuf;
use std::time::Duration;

/// Loads the ramp fixture and pumps the decoder until 4096 frames sit in the ring at frame 0.
fn start_at_zero(fixture: &Fixture) -> (Deck, DeckAudio, PathBuf) {
    let wav = fixture.path("ramp.wav");
    write_ramp(&wav, 2);
    let (deck, mut audio) = Deck::new(1 << 16);
    audio.prepare(RATE, 64);
    deck.load(&wav).expect("ramp loads");
    assert!(pump_until_buffered(&deck, &mut audio, 4096, 0));
    (deck, audio, wav)
}

fn frame_index(sample: f32) -> i64 {
    (sample * FRAMES as f32).round() as i64
}

/// Renders device-paced 64-frame blocks and returns the source frame index of every left sample.
/// `before_block` runs ahead of each block.
fn render_indices(
    audio: &mut DeckAudio,
    num_blocks: usize,
    mut before_block: impl FnMut(usize),
) -> Vec<i64> {
    let mut out = Vec::new();
    let mut l = [0.0_f32; 64];
    let mut r = [0.0_f32; 64];
    for b in 0..num_blocks {
        before_block(b);
        audio.render_stereo(&mut l, &mut r);
        out.extend(l.iter().map(|&v| frame_index(v)));
        std::thread::sleep(Duration::from_millis(1));
    }
    out
}

/// Asserts `got[i] == expected(i)` for the first `count` samples.
fn expect_sequence(got: &[i64], expected: impl Fn(i64) -> i64, count: usize) {
    assert!(
        got.len() >= count,
        "got {} samples, need {count}",
        got.len()
    );
    for (i, &g) in got.iter().take(count).enumerate() {
        let e = expected(i as i64);
        assert_eq!(g, e, "sample {i}: got frame {g}, expected {e}");
    }
}

#[test]
fn set_loop_validates_and_reports_state() {
    let fixture = Fixture::new();
    let wav = fixture.path("ramp.wav");
    write_ramp(&wav, 2);
    let (deck, _audio) = Deck::new(1 << 16);
    deck.load(&wav).unwrap();
    assert!(!deck.set_loop(Loop {
        start: 100,
        end: 101
    }));
    assert!(!deck.is_looping());
    assert!(deck.set_loop(Loop {
        start: 100,
        end: FRAMES + 500
    }));
    assert_eq!(deck.get_loop().end, FRAMES, "clamped to the track");
    assert!(deck.is_looping());
    deck.exit_loop();
    assert!(!deck.is_looping());
    assert!(deck.get_loop().valid());
    deck.reloop();
    assert!(deck.is_looping());
    deck.load(&wav).unwrap();
    assert!(
        !deck.get_loop().valid(),
        "a new track starts without a loop"
    );
}

#[test]
fn loop_wraps_sample_accurate_without_underruns() {
    let fixture = Fixture::new();
    let (deck, mut audio, _) = start_at_zero(&fixture);
    assert!(deck.set_loop(Loop {
        start: 1000,
        end: 1500
    }));
    assert!(wait_for(
        || deck.is_loop_prefetched(),
        Duration::from_secs(10)
    ));
    deck.play();
    let got = render_indices(&mut audio, 60, |_| {});
    expect_sequence(
        &got,
        |i| if i < 1500 { i } else { 1000 + (i - 1500) % 500 },
        got.len(),
    );
    assert_eq!(deck.underrun_count(), 0);
}

#[test]
fn loop_longer_than_the_prefetch_slot_falls_back_to_the_ring() {
    let fixture = Fixture::new();
    let wav = fixture.path("ramp.wav");
    write_ramp(&wav, 2);
    let (deck, mut audio) = Deck::with_prefetch(1 << 16, 2048);
    audio.prepare(RATE, 64);
    deck.load(&wav).unwrap();
    assert!(pump_until_buffered(&deck, &mut audio, 4096, 0));
    assert!(deck.set_loop(Loop {
        start: 1000,
        end: 9000
    }));
    assert!(wait_for(
        || deck.is_loop_prefetched(),
        Duration::from_secs(10)
    ));
    deck.play();
    let got = render_indices(&mut audio, 400, |_| {});
    expect_sequence(
        &got,
        |i| {
            if i < 9000 {
                i
            } else {
                1000 + (i - 9000) % 8000
            }
        },
        got.len(),
    );
    assert_eq!(deck.underrun_count(), 0);
}

#[test]
fn exiting_a_loop_continues_linearly() {
    let fixture = Fixture::new();
    let (deck, mut audio, _) = start_at_zero(&fixture);
    assert!(deck.set_loop(Loop {
        start: 1000,
        end: 1500
    }));
    assert!(wait_for(
        || deck.is_loop_prefetched(),
        Duration::from_secs(10)
    ));
    deck.play();
    let got = render_indices(&mut audio, 120, |block| {
        if block == 40 {
            deck.exit_loop();
        }
    });

    let mut wraps = 0;
    for i in 1..got.len() {
        let wrap = got[i - 1] == 1499 && got[i] == 1000;
        if wrap {
            wraps += 1;
        }
        assert!(
            wrap || got[i] == got[i - 1] + 1,
            "discontinuity at sample {i}: {} -> {}",
            got[i - 1],
            got[i]
        );
    }
    assert!(wraps >= 2, "wraps {wraps}");
    assert!(
        *got.last().unwrap() > 1500,
        "playback should leave the loop"
    );
    assert_eq!(deck.underrun_count(), 0);
}

#[test]
fn hotcue_parameters_set_jump_and_clear() {
    let fixture = Fixture::new();
    let wav = fixture.path("ramp.wav");
    write_ramp(&wav, 2);
    let (deck, _audio) = Deck::new(1 << 16);
    deck.load(&wav).unwrap();
    deck.set_hotcue(2, 5000);
    assert_eq!(deck.hotcue(2), Some(5000));
    assert!(deck.set_parameter("hotcue2.clear", 1.0));
    assert_eq!(deck.hotcue(2), None);
    assert!(
        deck.set_parameter("hotcue3", 1.0),
        "empty: stores the playhead"
    );
    assert_eq!(deck.hotcue(3), Some(0));
    assert!(!deck.set_parameter("hotcue8", 1.0));
    assert!(!deck.set_parameter("hotcuex", 1.0));
    assert!(!deck.set_parameter("hotcue1.bogus", 1.0));
    assert!(deck.set_parameter("loop.exit", 1.0));
}

#[test]
fn seeking_outside_a_loop_exits_it() {
    let fixture = Fixture::new();
    let wav = fixture.path("ramp.wav");
    write_ramp(&wav, 2);
    let (deck, _audio) = Deck::new(1 << 16);
    deck.load(&wav).unwrap();
    assert!(deck.set_loop(Loop {
        start: 1000,
        end: 2000
    }));
    deck.seek(1500);
    assert!(deck.is_looping());
    deck.seek(5000);
    assert!(!deck.is_looping());
}

#[test]
fn hotcue_jump_repositions_playback() {
    let fixture = Fixture::new();
    let (deck, mut audio, _) = start_at_zero(&fixture);
    deck.set_hotcue(0, 12_000);
    deck.trigger_hotcue(0);
    let mut l = [0.0_f32; 64];
    let mut r = [0.0_f32; 64];
    assert!(wait_for(
        || {
            audio.render_stereo(&mut l, &mut r);
            deck.position_frames() == 12_000 && deck.buffered_frames() > 256
        },
        Duration::from_secs(10)
    ));
    deck.play();
    audio.render_stereo(&mut l, &mut r);
    assert_eq!(frame_index(l[0]), 12_000);
}

#[test]
fn resizing_a_loop_while_looping_stays_continuous() {
    let fixture = Fixture::new();
    let (deck, mut audio, _) = start_at_zero(&fixture);
    assert!(deck.set_loop(Loop {
        start: 1000,
        end: 1800
    }));
    assert!(wait_for(
        || deck.is_loop_prefetched(),
        Duration::from_secs(10)
    ));
    deck.play();
    let got = render_indices(&mut audio, 100, |block| {
        if block == 50 {
            deck.set_loop(Loop {
                start: 1000,
                end: 1400,
            });
        }
    });
    for i in 1..got.len() {
        let wraps_to = |len: i64| got[i] == 1000 + (got[i - 1] + 1 - 1000).rem_euclid(len);
        let wrap = got[i - 1] >= 1399 && (wraps_to(800) || wraps_to(400));
        assert!(
            wrap || got[i] == got[i - 1] + 1,
            "discontinuity at sample {i}: {} -> {}",
            got[i - 1],
            got[i]
        );
    }
    let tail_max = got[got.len() - 2000..].iter().copied().max().unwrap();
    assert!(tail_max < 1400, "tail max {tail_max}");
    assert_eq!(deck.underrun_count(), 0);
}

/// Loop bounds set from one control thread while another toggles `enabled`. The toggles must
/// never put back bounds the writer has already replaced. The race window is small, so each
/// round is repeated.
#[test]
fn toggling_the_loop_never_reverts_newer_bounds() {
    use std::sync::atomic::{AtomicBool, Ordering::{Acquire, Release}};
    use std::sync::Arc;
    let fixture = crate::deck::tests::Fixture::new();
    let wav = fixture.path("toggle.wav");
    crate::deck::tests::write_ramp(&wav, 2);
    let (deck, _audio) = Deck::new(1 << 12);
    deck.load(&wav).expect("fixture loads");
    let deck = Arc::new(deck);
    const ROUNDS: usize = 20;
    const SETS: i64 = 100_000;
    // The fixture is FRAMES long, so starts wrap inside it; the last start is the expected result.
    let start_of = |i: i64| i % (FRAMES - 1000);
    for _ in 0..ROUNDS {
        let done = Arc::new(AtomicBool::new(false));
        let toggler = {
            let (deck, done) = (Arc::clone(&deck), Arc::clone(&done));
            std::thread::spawn(move || {
                while !done.load(Acquire) {
                    deck.exit_loop();
                    deck.reloop();
                }
            })
        };
        for i in 0..SETS {
            let start = start_of(i);
            assert!(deck.set_loop(Loop {
                start,
                end: start + 100
            }));
        }
        done.store(true, Release);
        toggler.join().expect("toggler");
        assert_eq!(
            deck.get_loop().start,
            start_of(SETS - 1),
            "a toggle reverted the bounds"
        );
    }
}

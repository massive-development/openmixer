//! Allocation guard for the audio-thread render paths (debug builds only). The global allocator
//! panics on any heap allocation made while an `assert_no_alloc` scope is active on the current
//! thread, so the mixer and deck render loops are proven allocation-free without touching them.

use crate::deck::Deck;
use crate::mixer::{ChannelInput, Mixer};
use assert_no_alloc::{assert_no_alloc, AllocDisabler};

#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

const BLOCK: usize = 256;
const BLOCKS: usize = 200;
const RATE: f64 = 44_100.0;

#[test]
fn mixer_and_deck_render_paths_do_not_allocate() {
    let fixture = crate::deck::tests::Fixture::new();
    let wav = fixture.path("long.wav");
    let frames = BLOCK * BLOCKS + 4096;
    let mut samples = Vec::with_capacity(frames * 2);
    for n in 0..frames {
        let v = (n % 1000) as f32 / 1000.0;
        samples.push(v);
        samples.push(-v);
    }
    crate::deck::tests::write_float_wav(&wav, 2, &samples);

    let (mut mixer, _control) = Mixer::new(2);
    mixer.prepare(RATE, BLOCK);
    let (deck, mut audio) = Deck::new(1 << 16);
    audio.prepare(RATE, BLOCK);
    deck.load(&wav).expect("fixture loads");
    assert!(
        crate::deck::tests::pump_until_buffered(&deck, &mut audio, BLOCK * 4, 0),
        "decoder did not buffer in time"
    );
    deck.play();

    let mut dl = [0.0_f32; BLOCK];
    let mut dr = [0.0_f32; BLOCK];
    let mut ml = [0.0_f32; BLOCK];
    let mut mr = [0.0_f32; BLOCK];
    let mut cl = [0.0_f32; BLOCK];
    let mut cr = [0.0_f32; BLOCK];
    let mut audible_blocks = 0_usize;

    // Each block waits (unguarded) until the decoder has buffered a block, then the deck and the
    // mixer render paths run inside assert_no_alloc. Every block carries real audio.
    for _ in 0..BLOCKS {
        assert!(
            crate::deck::tests::wait_for(
                || deck.buffered_frames() >= BLOCK,
                std::time::Duration::from_secs(10)
            ),
            "decoder stalled"
        );
        assert_no_alloc(|| audio.render_stereo(&mut dl, &mut dr));
        assert_no_alloc(|| {
            let inputs = [ChannelInput {
                left: Some(&dl),
                right: Some(&dr),
            }];
            mixer.process(&inputs, &mut ml, &mut mr, &mut cl, &mut cr);
        });
        if ml.iter().any(|&v| v != 0.0) {
            audible_blocks += 1;
        }
    }
    assert_eq!(
        audible_blocks, BLOCKS,
        "every block should carry deck audio"
    );
}

#[test]
fn loop_playback_does_not_allocate() {
    use crate::quantizer::Loop;
    let fixture = crate::deck::tests::Fixture::new();
    let wav = fixture.path("loop.wav");
    let frames = BLOCK * BLOCKS + 4096;
    let mut samples = Vec::with_capacity(frames * 2);
    for n in 0..frames {
        let v = (n % 1000) as f32 / 1000.0;
        samples.push(v);
        samples.push(-v);
    }
    crate::deck::tests::write_float_wav(&wav, 2, &samples);

    let (mut mixer, _control) = Mixer::new(2);
    mixer.prepare(RATE, BLOCK);
    let (deck, mut audio) = Deck::new(1 << 16);
    audio.prepare(RATE, BLOCK);
    deck.load(&wav).expect("fixture loads");
    assert!(
        crate::deck::tests::pump_until_buffered(&deck, &mut audio, BLOCK * 4, 0),
        "decoder did not buffer in time"
    );
    assert!(deck.set_loop(Loop {
        start: 1024,
        end: 5120
    }));
    assert!(
        crate::deck::tests::wait_for(
            || deck.is_loop_prefetched(),
            std::time::Duration::from_secs(10)
        ),
        "loop start was not prefetched"
    );
    deck.play();

    let mut dl = [0.0_f32; BLOCK];
    let mut dr = [0.0_f32; BLOCK];
    let mut ml = [0.0_f32; BLOCK];
    let mut mr = [0.0_f32; BLOCK];
    let mut cl = [0.0_f32; BLOCK];
    let mut cr = [0.0_f32; BLOCK];
    let mut audible_blocks = 0_usize;

    // Playing a loop wraps every 4096 frames, so several wraps happen inside the guarded blocks.
    // The loop start is prefetched, so the wraps take the slot path (no ring waits here).
    for _ in 0..BLOCKS {
        assert_no_alloc(|| audio.render_stereo(&mut dl, &mut dr));
        assert_no_alloc(|| {
            let inputs = [ChannelInput {
                left: Some(&dl),
                right: Some(&dr),
            }];
            mixer.process(&inputs, &mut ml, &mut mr, &mut cl, &mut cr);
        });
        if ml.iter().any(|&v| v != 0.0) {
            audible_blocks += 1;
        }
    }
    assert!(deck.is_looping());
    assert_eq!(
        audible_blocks, BLOCKS,
        "every block should carry loop audio"
    );
}

#[test]
fn keylock_playback_does_not_allocate() {
    // Rubber Band's own allocations are invisible here: this allocator only sees Rust allocations.
    // The Rust side of the stretcher (pointer arrays, tee and sink buffers, lookahead and history
    // rings) is preallocated in `prepare`, so the guarded render should not allocate.
    let fixture = crate::deck::tests::Fixture::new();
    let wav = fixture.path("keylock.wav");
    // Rate 1.1 consumes more source than the output blocks, so the track must be longer.
    let frames = BLOCK * BLOCKS * 2 + 4096;
    let mut samples = Vec::with_capacity(frames * 2);
    for n in 0..frames {
        let v = (n % 1000) as f32 / 1000.0;
        samples.push(v);
        samples.push(-v);
    }
    crate::deck::tests::write_float_wav(&wav, 2, &samples);

    let (mut mixer, _control) = Mixer::new(2);
    mixer.prepare(RATE, BLOCK);
    let (deck, mut audio) = Deck::new(1 << 16);
    audio.prepare(RATE, BLOCK);
    deck.load(&wav).expect("fixture loads");
    assert!(
        crate::deck::tests::pump_until_buffered(&deck, &mut audio, BLOCK * 4, 0),
        "decoder did not buffer in time"
    );
    deck.set_keylock(true);
    deck.set_rate(1.1);
    deck.play();

    let mut dl = [0.0_f32; BLOCK];
    let mut dr = [0.0_f32; BLOCK];
    let mut ml = [0.0_f32; BLOCK];
    let mut mr = [0.0_f32; BLOCK];
    let mut cl = [0.0_f32; BLOCK];
    let mut cr = [0.0_f32; BLOCK];
    let mut audible_blocks = 0_usize;

    // Priming runs inside the first guarded blocks; the stretcher then takes over.
    for _ in 0..BLOCKS {
        assert!(
            crate::deck::tests::wait_for(
                || deck.buffered_frames() >= BLOCK * 2,
                std::time::Duration::from_secs(10)
            ),
            "decoder stalled"
        );
        assert_no_alloc(|| audio.render_stereo(&mut dl, &mut dr));
        assert_no_alloc(|| {
            let inputs = [ChannelInput {
                left: Some(&dl),
                right: Some(&dr),
            }];
            mixer.process(&inputs, &mut ml, &mut mr, &mut cl, &mut cr);
        });
        if ml.iter().any(|&v| v != 0.0) {
            audible_blocks += 1;
        }
    }
    assert!(deck.is_keylock());
    assert_eq!(
        audible_blocks, BLOCKS,
        "every block should carry keylocked deck audio"
    );
}

//! Allocation guard for the audio-thread render paths (debug builds only). The global allocator
//! panics on any heap allocation made while an `assert_no_alloc` scope is active on the current
//! thread, so the mixer and deck render loops are proven allocation-free without touching them.

use crate::deck::Deck;
use crate::mixer::{ChannelInput, Mixer};
use assert_no_alloc::{assert_no_alloc, AllocDisabler};
use om_testing::wait::{wait_until, DEFAULT_POLL};

#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

const BLOCK: usize = 256;
const BLOCKS: usize = 200;
const RATE: f64 = 44_100.0;

#[test]
fn mixer_and_deck_render_paths_do_not_allocate() {
    let fixture = om_testing::fs::TempDir::new("om_engine");
    let wav = fixture.file("long.wav");
    let frames = BLOCK * BLOCKS + 4096;
    let mut samples = Vec::with_capacity(frames * 2);
    for n in 0..frames {
        let v = (n % 1000) as f32 / 1000.0;
        samples.push(v);
        samples.push(-v);
    }
    om_testing::wav::write_float_wav(&wav, 2, 44_100, &samples);

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
            wait_until(std::time::Duration::from_secs(10), DEFAULT_POLL, || deck
                .buffered_frames()
                >= BLOCK),
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
    let fixture = om_testing::fs::TempDir::new("om_engine");
    let wav = fixture.file("loop.wav");
    let frames = BLOCK * BLOCKS + 4096;
    let mut samples = Vec::with_capacity(frames * 2);
    for n in 0..frames {
        let v = (n % 1000) as f32 / 1000.0;
        samples.push(v);
        samples.push(-v);
    }
    om_testing::wav::write_float_wav(&wav, 2, 44_100, &samples);

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
        wait_until(std::time::Duration::from_secs(10), DEFAULT_POLL, || deck
            .is_loop_prefetched()),
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
    let fixture = om_testing::fs::TempDir::new("om_engine");
    let wav = fixture.file("keylock.wav");
    // Rate 1.1 consumes more source than the output blocks, so the track must be longer.
    let frames = BLOCK * BLOCKS * 2 + 4096;
    let mut samples = Vec::with_capacity(frames * 2);
    for n in 0..frames {
        let v = (n % 1000) as f32 / 1000.0;
        samples.push(v);
        samples.push(-v);
    }
    om_testing::wav::write_float_wav(&wav, 2, 44_100, &samples);

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
            wait_until(std::time::Duration::from_secs(10), DEFAULT_POLL, || deck
                .buffered_frames()
                >= BLOCK * 2),
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

#[test]
fn dj_engine_process_does_not_allocate_with_two_decks_loop_fx_and_keylock() {
    use crate::callback::AudioCallback;
    use crate::dj_engine::DjEngine;
    use crate::quantizer::Loop;
    use om_core::ParameterTarget;

    let fixture = om_testing::fs::TempDir::new("om_engine");
    let wav = fixture.file("engine.wav");
    // Keylock at rate 1.1 consumes more source than the output, so the track is longer than the run.
    let frames = BLOCK * BLOCKS * 2 + 4096;
    let mut samples = Vec::with_capacity(frames * 2);
    for n in 0..frames {
        let v = (n % 1000) as f32 / 1000.0;
        samples.push(v);
        samples.push(-v);
    }
    om_testing::wav::write_float_wav(&wav, 2, 44_100, &samples);

    let (engine, mut audio) = DjEngine::new(2);
    audio.prepare(RATE, BLOCK);
    for d in 0..2 {
        engine.deck(d).load(&wav).expect("fixture loads");
    }

    // Unguarded pump: the decoders fill the rings and the loop prefetch before the guarded run.
    let mut pump = [[0.0_f32; BLOCK]; 4];
    let [p0, p1, p2, p3] = &mut pump;
    let mut outs: [&mut [f32]; 4] = [p0, p1, p2, p3];
    for d in 0..2 {
        assert!(
            wait_until(std::time::Duration::from_secs(10), DEFAULT_POLL, || {
                audio.process(&[], &mut outs);
                engine.deck(d).buffered_frames() >= BLOCK * 4
            }),
            "decoder {d} did not buffer in time"
        );
    }
    assert!(engine.deck(0).set_loop(Loop {
        start: 1024,
        end: 5120
    }));
    assert!(
        wait_until(std::time::Duration::from_secs(10), DEFAULT_POLL, || engine
            .deck(0)
            .is_loop_prefetched()),
        "loop start was not prefetched"
    );
    engine.deck(0).set_keylock(true);
    engine.deck(1).set_keylock(true);
    engine.deck(1).set_rate(1.1);
    // FX on master and both channels, faders up.
    for id in ["master.fx0.mix", "ch0.fx1.mix", "ch1.fx2.mix"] {
        assert!(engine.fx().set_parameter(id, 0.5));
    }
    for id in ["master.fx0.enable", "ch0.fx1.enable", "ch1.fx2.enable"] {
        assert!(engine.fx().set_parameter(id, 1.0));
    }
    engine.mixer().set_fader(0, 1.0);
    engine.mixer().set_fader(1, 1.0);
    engine.deck(0).play();
    engine.deck(1).play();

    let mut audible_blocks = 0_usize;
    for _ in 0..BLOCKS {
        for d in 0..2 {
            assert!(
                wait_until(std::time::Duration::from_secs(10), DEFAULT_POLL, || engine
                    .deck(d)
                    .buffered_frames()
                    >= BLOCK * 2),
                "decoder {d} stalled"
            );
        }
        let [a, b, c, d] = &mut pump;
        let mut outs: [&mut [f32]; 4] = [a, b, c, d];
        assert_no_alloc(|| audio.process(&[], &mut outs));
        if outs[0].iter().any(|&v| v != 0.0) {
            audible_blocks += 1;
        }
    }
    assert!(engine.deck(0).is_looping());
    assert_eq!(
        audible_blocks, BLOCKS,
        "every block should carry master audio"
    );
}

/// Live control changes between blocks, as a performer makes them: the master deck's rate moves
/// every block (so key-lock re-ratios every block), the sync timer nudges the follower's rate,
/// and FX parameters move. Only `audio.process` runs under the guard.
///
/// What the guard can and cannot see: it fails on any Rust allocation made while the guard is
/// active. Allocations made inside Rubber Band (its own `new`/`malloc` calls) bypass the Rust global
/// allocator, so they are not counted here, and neither is the CPU cost Rubber Band spends
/// re-planning when its ratio changes. See `deck/playback/keylock.rs`.
#[test]
fn keylock_with_live_rate_sync_and_fx_changes_does_not_allocate() {
    use crate::callback::AudioCallback;
    use crate::dj_engine::DjEngine;
    use crate::grid::FixedTempoGrid;
    use om_core::ParameterTarget;
    use std::sync::Arc;

    let fixture = om_testing::fs::TempDir::new("om_engine");
    let wav = fixture.file("live.wav");
    // Rates stay within 0.9..1.1, so the track is longer than the run at the slowest rate.
    let frames = BLOCK * BLOCKS * 2 + 4096;
    let mut samples = Vec::with_capacity(frames * 2);
    for n in 0..frames {
        let v = (n % 1000) as f32 / 1000.0;
        samples.push(v);
        samples.push(-v);
    }
    om_testing::wav::write_float_wav(&wav, 2, 44_100, &samples);

    let (engine, mut audio) = DjEngine::new(2);
    audio.prepare(RATE, BLOCK);
    for d in 0..2 {
        engine.deck(d).load(&wav).expect("fixture loads");
        engine.deck(d).set_keylock(true);
        engine.set_grid(d, Some(Arc::new(FixedTempoGrid::new(120.0, RATE, 0))));
    }
    engine.sync().set_leader(Some(0));
    engine.sync().set_enabled(1, true);
    engine.mixer().set_fader(0, 1.0);
    engine.mixer().set_fader(1, 1.0);
    for id in ["master.fx0.enable", "ch0.fx1.enable", "ch1.fx2.enable"] {
        assert!(engine.fx().set_parameter(id, 1.0));
    }

    let mut pump = [[0.0_f32; BLOCK]; 4];
    // Unguarded pump: decoders fill their rings and the keylock primes before the guarded run.
    for d in 0..2 {
        assert!(
            wait_until(std::time::Duration::from_secs(10), DEFAULT_POLL, || {
                let [p0, p1, p2, p3] = &mut pump;
                let mut outs: [&mut [f32]; 4] = [p0, p1, p2, p3];
                audio.process(&[], &mut outs);
                engine.deck(d).buffered_frames() >= BLOCK * 4
            }),
            "decoder {d} did not buffer in time"
        );
    }
    engine.deck(0).play();
    engine.deck(1).play();

    let mut audible_blocks = 0_usize;
    for block in 0..BLOCKS {
        let t = block as f64;
        // Control-thread work between blocks.
        engine.deck(0).set_rate(1.0 + 0.1 * (t * 0.113).sin());
        engine.update_control(BLOCK as f64 / RATE);
        engine
            .fx()
            .set_parameter("ch0.fx1.mix", 0.5 + 0.4 * (t * 0.07).sin());
        engine
            .fx()
            .set_parameter("master.fx0.mix", 0.5 + 0.4 * (t * 0.05).cos());
        for d in 0..2 {
            assert!(
                wait_until(std::time::Duration::from_secs(10), DEFAULT_POLL, || engine
                    .deck(d)
                    .buffered_frames()
                    >= BLOCK * 2),
                "decoder {d} stalled"
            );
        }
        let [a, b, c, d] = &mut pump;
        let mut outs: [&mut [f32]; 4] = [a, b, c, d];
        assert_no_alloc(|| audio.process(&[], &mut outs));
        if outs[0].iter().any(|&v| v != 0.0) {
            audible_blocks += 1;
        }
    }
    assert!(engine.deck(0).is_keylock() && engine.deck(1).is_keylock());
    assert_eq!(
        audible_blocks, BLOCKS,
        "every block should carry master audio"
    );
}

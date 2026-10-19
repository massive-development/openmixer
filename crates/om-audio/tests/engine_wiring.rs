//! FX racks, sync, quantized loops and slip reached
//! through the parameter router, on a real `DjEngine` driven by the mock device.

mod common;

use common::{sine_stereo, wait_until, write_float_wav, TempDir};
use om_audio::{AudioDevice, DjEngine, MockAudioDevice};
use om_core::{ParameterRouter, ParameterTarget};
use om_engine::grid::{BeatGrid, FixedTempoGrid};
use om_engine::sync::SyncDeckState;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

const RATE: f64 = 44_100.0;
const FRAMES: usize = 44_100 * 10;

fn grid(bpm: f64) -> Option<Arc<dyn BeatGrid>> {
    Some(Arc::new(FixedTempoGrid::new(bpm, RATE, 0)))
}

/// Engine, router and a running mock device. The device is never rendered by the fixture itself;
/// tests pump it with `dev.render_block()` when they need audio.
struct Wiring {
    dev: MockAudioDevice,
    engine: DjEngine,
    router: ParameterRouter,
    _dir: TempDir,
    file: PathBuf,
}

impl Wiring {
    fn new() -> Self {
        let dir = TempDir::new("openmixer_wiring_test");
        let file = dir.file("sine.wav");
        write_float_wav(
            &file,
            2,
            RATE as u32,
            &sine_stereo(220.0, 0.3, FRAMES, RATE),
        );
        let (engine, audio) = DjEngine::new(2);
        let router = ParameterRouter::new();
        engine.register(&router);
        let mut dev = MockAudioDevice::new(RATE, 256, 0, 2);
        dev.start(Box::new(audio)).expect("start");
        Self {
            dev,
            engine,
            router,
            _dir: dir,
            file,
        }
    }

    /// Renders until the deck sits on `target`, servicing its seek handshake. Bounded.
    fn settle_at(&self, deck: usize, target: i64) -> bool {
        wait_until(
            || {
                self.dev.render_block();
                self.engine.deck(deck).position_frames() == target
            },
            Duration::from_secs(20),
        )
    }

    /// Renders until the deck has `frames` buffered, so playback starts without underruns.
    fn buffered(&self, deck: usize, frames: usize) -> bool {
        wait_until(
            || {
                self.dev.render_block();
                self.engine.deck(deck).buffered_frames() >= frames
            },
            Duration::from_secs(20),
        )
    }
}

#[test]
fn router_reaches_master_and_channel_fx_parameters() {
    let w = Wiring::new();
    let fx = w.engine.fx();
    assert!(w.router.set_parameter("fx.master.fx1.mix", 0.3));
    assert_eq!(fx.master_rack().unit(1).unwrap().mix(), 0.3);
    assert!(w.router.set_parameter("fx.ch1.fx2.enable", 1.0));
    assert!(fx.channel_rack(1).unwrap().unit(2).unwrap().is_enabled());
    assert!(
        !fx.channel_rack(0).unwrap().unit(2).unwrap().is_enabled(),
        "channels are independent"
    );
    assert!(w.router.set_parameter("fx.ch0.bpm", 140.0));
    assert_eq!(fx.channel_rack(0).unwrap().unit(0).unwrap().tempo(), 140.0);
}

#[test]
fn router_rejects_unknown_fx_ids() {
    let w = Wiring::new();
    for id in [
        "fx.ch9.fx0.mix",
        "fx.master.fx99.mix",
        "fx.bus.fx0.mix",
        "fx.master",
        "fx.ch.fx0.mix",
        "fx.master.nope",
        "fx.ch2147483648.fx0.mix",
        "fx.ch4294967296.fx0.mix",
        "fx.master.fx2147483648.mix",
        "fx.master.fx4294967296.mix",
        "fx.ch0000.fx0.mix",
    ] {
        assert!(!w.router.set_parameter(id, 0.5), "{id}");
    }
}

#[test]
fn racks_sit_in_the_signal_path() {
    let w = Wiring::new();
    assert!(w.engine.deck(0).load(&w.file).is_ok());
    assert!(w.buffered(0, 8192));
    w.engine.deck(0).play();
    let settled_rms = || {
        for _ in 0..400 {
            wait_until(
                || w.engine.deck(0).buffered_frames() >= 2048,
                Duration::from_secs(10),
            ); // keep the pump ahead
            w.dev.render_block();
        }
        common::rms(&w.dev.last_output()[0][..256])
    };
    let dry = settled_rms();
    w.router.set_parameter("fx.master.fx0.mix", 1.0);
    w.router.set_parameter("fx.master.fx0.enable", 1.0);
    let wet = settled_rms();
    assert!(dry > 0.01, "deck reaches the master");
    assert!(
        (wet - dry).abs() > 1e-3,
        "master output changes with fx0 enabled"
    );
}

#[test]
fn rack_units_report_active() {
    let w = Wiring::new();
    w.router.set_parameter("fx.master.fx0.enable", 1.0);
    w.router.set_parameter("fx.ch1.fx3.enable", 1.0);
    w.dev.render_blocks(2);
    let fx = w.engine.fx();
    assert!(fx.master_rack().unit(0).unwrap().is_active());
    assert!(fx.channel_rack(1).unwrap().unit(3).unwrap().is_active());
    assert!(!fx.channel_rack(0).unwrap().unit(3).unwrap().is_active());
}

#[test]
fn fx_tempo_follows_deck_grid_and_rate() {
    let w = Wiring::new();
    w.engine.set_grid(0, grid(120.0));
    w.engine.set_grid(1, grid(100.0));
    w.engine.deck(0).set_rate(1.1);
    w.engine.update_control(0.03);
    let fx = w.engine.fx();
    let tempo = |rack: &om_engine::fx::FxRackControl| rack.unit(0).unwrap().tempo();
    assert!((tempo(fx.channel_rack(0).unwrap()) - 132.0).abs() < 1e-9);
    assert!((tempo(fx.channel_rack(1).unwrap()) - 100.0).abs() < 1e-9);
    assert!(
        (tempo(fx.master_rack()) - 132.0).abs() < 1e-9,
        "master follows first deck with a grid"
    );

    w.router.set_parameter("deck.1.sync.master", 1.0);
    w.engine.update_control(0.03);
    assert!(
        (tempo(fx.master_rack()) - 100.0).abs() < 1e-9,
        "master follows the sync leader"
    );
}

#[test]
fn sync_ids_select_leader_and_follower_and_match_tempo() {
    let w = Wiring::new();
    w.engine.set_grid(0, grid(120.0));
    w.engine.set_grid(1, grid(100.0));
    assert!(w.router.set_parameter("deck.0.sync.master", 1.0));
    assert!(w.router.set_parameter("deck.1.sync", 1.0));
    assert_eq!(w.engine.sync().leader(), Some(0));
    assert!(w.engine.sync().is_enabled(1));

    w.engine.deck(0).play();
    w.engine.deck(1).play(); // no audio renders: positions stay at 0, so phase is aligned
    w.engine.update_control(0.03);
    assert!(
        (w.engine.deck(1).rate() - 1.2).abs() < 1e-9,
        "100 bpm deck sped up to the 120 bpm leader"
    );
    assert_eq!(w.engine.deck(0).rate(), 1.0);

    assert!(w.router.set_parameter("deck.1.sync", 0.0));
    assert!(
        w.router.set_parameter("deck.1.sync.master", 0.0),
        "releasing a non-leader is a no-op"
    );
    assert_eq!(w.engine.sync().leader(), Some(0));
    w.router.set_parameter("deck.0.sync.master", 0.0);
    assert_eq!(w.engine.sync().leader(), None);
}

#[test]
fn deck_ids_still_reach_the_deck_and_unknown_ids_fail() {
    let w = Wiring::new();
    assert!(w.router.set_parameter("deck.0.rate", 1.05));
    assert_eq!(w.engine.deck(0).rate(), 1.05);
    assert!(!w.router.set_parameter("deck.0.bogus", 1.0));
    for id in [
        "deck.0.loop.quantized.abc",
        "deck.0.loop.quantized.",
        "deck.0.loop.quantized.0",
        "deck.0.loop.quantized.1.2.3",
        "deck.0.loop.quantized..5",
    ] {
        assert!(!w.router.set_parameter(id, 1.0), "{id}");
    }
    assert!(
        !w.router.set_parameter("deck.0.loop.quantized.2", 1.0),
        "no grid: press reports failure"
    );
}

#[test]
fn sync_aligns_two_decks_on_fixed_grids() {
    let w = Wiring::new();
    assert!(w.engine.deck(0).load(&w.file).is_ok());
    assert!(w.engine.deck(1).load(&w.file).is_ok());
    w.engine.set_grid(0, grid(120.0));
    w.engine.set_grid(1, grid(120.0));
    w.engine.deck(1).seek(2205); // follower starts 0.1 beat late
    assert!(w.settle_at(1, 2205));
    // No start-up underruns skewing the offset.
    assert!(w.buffered(0, 8192));
    assert!(w.buffered(1, 8192));
    w.router.set_parameter("deck.0.sync.master", 1.0);
    w.router.set_parameter("deck.1.sync", 1.0);
    w.engine.deck(0).play();
    w.engine.deck(1).play();

    for block in 0..1200 {
        assert!(wait_until(
            || {
                w.engine.deck(0).buffered_frames() >= 2048
                    && w.engine.deck(1).buffered_frames() >= 2048
            },
            Duration::from_secs(20),
        )); // keep the unpaced pump from underrunning
        w.dev.render_block();
        if block % 8 == 0 {
            w.engine.update_control(0.046);
        }
        std::thread::yield_now();
    }
    let states = [0, 1].map(|d| SyncDeckState {
        position: w.engine.deck(d).position_frames(),
        rate: w.engine.deck(d).rate(),
        playing: true,
    });
    let phase = w.engine.sync().phase_error(&states, 1);
    assert!(
        phase.abs() < 0.02,
        "follower converged onto the leader's beat: {phase}"
    );
}

#[test]
fn quantized_loop_bounds_come_from_the_grid() {
    let w = Wiring::new();
    assert!(w.engine.deck(0).load(&w.file).is_ok());
    w.engine.deck(0).seek(30_000); // beat 1.36 at 120 bpm: nearest beat is 22050
    assert!(w.settle_at(0, 30_000));

    assert!(
        !w.engine.deck_control(0).set_quantized_loop(2.0),
        "no grid yet"
    );
    w.engine.set_grid(0, grid(120.0));
    assert!(w.router.set_parameter("deck.0.loop.quantized.2", 1.0));
    let loop_ = w.engine.deck(0).loop_state();
    assert_eq!(loop_.start, 22_050);
    assert_eq!(loop_.end, 22_050 + 44_100);

    assert!(w.router.set_parameter("deck.0.loop.quantized.0.5", 1.0));
    let loop_ = w.engine.deck(0).loop_state();
    assert_eq!(loop_.end - loop_.start, 11_025, "half-beat loop");
}

#[test]
fn quantize_off_starts_the_loop_exactly_at_the_playhead() {
    let w = Wiring::new();
    assert!(w.engine.deck(0).load(&w.file).is_ok());
    w.engine.deck(0).seek(30_000);
    assert!(w.settle_at(0, 30_000));
    w.engine.set_grid(0, grid(120.0));

    assert!(w.router.set_parameter("deck.0.quantize", 0.0));
    assert!(!w.engine.deck_control(0).is_quantize());
    assert!(w.router.set_parameter("deck.0.loop.quantized.1", 1.0));
    assert_eq!(w.engine.deck(0).loop_state().start, 30_000);

    assert!(w.router.set_parameter("deck.0.quantize", 1.0));
    assert!(w.engine.deck_control(0).is_quantize());
}

#[test]
fn slip_resumes_where_the_track_would_have_been() {
    let w = Wiring::new();
    assert!(w.engine.deck(0).load(&w.file).is_ok());
    w.engine.deck(0).seek(1000);
    assert!(w.settle_at(0, 1000));

    assert!(w.router.set_parameter("deck.0.slip", 1.0));
    assert!(w.engine.deck_control(0).is_slipping());
    w.engine.deck(0).play(); // no audio renders: the playhead is held, the ghost moves
    w.engine.update_control(0.5);
    w.engine.deck(0).pause();
    assert!(w.router.set_parameter("deck.0.slip", 0.0));
    assert!(!w.engine.deck_control(0).is_slipping());
    assert!(w.settle_at(0, 1000 + 22_050));
}

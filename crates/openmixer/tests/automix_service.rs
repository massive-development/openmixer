//! Automix wiring in the composition: the `automix.*` ids reach the controller through the router
//! and the runner publishes its state, with no UI and no audio device.

use om_analysis::{is_harmonically_compatible, MusicalKey};
use om_automix::{AutomixHost, Camelot, DeckInfo, LoadPoll, NextTrack};
use om_core::command::AutomixCommand;
use om_core::param_id::AutomixParam;
use om_core::{AutomixStyle, Command, CommandSink, ParameterTarget};
use openmixer::automix_service::AutomixRunner;
use openmixer::composition::AppComposition;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

fn composition() -> (tempfile::TempDir, AppComposition) {
    let scratch = tempfile::tempdir().expect("tempdir");
    let c = AppComposition::new(&scratch.path().join("prefs.json"), None);
    (scratch, c)
}

#[test]
fn automix_ids_reach_the_controller_and_the_snapshot_follows() {
    let (_dir, c) = composition();
    let mut runner = AutomixRunner::new(c.automix.clone(), c.automix_sink.clone());
    assert!(!c.automix.snapshot().enabled);
    assert!(c.router.set_parameter("automix.enable", 1.0));
    assert!(c.router.set_parameter("automix.style", 2.0));
    runner.step(&c.engine, 0.0);
    let s = c.automix.snapshot();
    assert!(s.enabled, "snapshot {s:?}");
    assert_eq!(s.style, AutomixStyle::BassSwap);
    // The typed command path reaches the same target.
    c.router
        .send(Command::Automix(AutomixCommand {
            p: AutomixParam::Style,
            value: 4.0,
        }))
        .expect("sent");
    runner.step(&c.engine, 0.1);
    assert_eq!(c.automix.snapshot().style, AutomixStyle::Cut);
    assert!(c.router.set_parameter("automix.enable", 0.0));
    runner.step(&c.engine, 0.2);
    assert!(!c.automix.snapshot().enabled);
    assert_eq!(c.router.unknown_count(), 0);
}

struct CountingHost(Arc<AtomicUsize>);

impl AutomixHost for CountingHost {
    fn next_track(&mut self, _deck: u8, _tempo_gate: Option<f64>) -> Option<NextTrack> {
        self.0.fetch_add(1, Ordering::SeqCst);
        None
    }
    fn start_load(&mut self, _deck: u8, _track: &NextTrack) -> Result<(), String> {
        Err("unused".into())
    }
    fn poll_load(&mut self, _deck: u8) -> LoadPoll {
        LoadPoll::Pending
    }
    fn deck_info(&mut self, _deck: u8) -> Option<DeckInfo> {
        None
    }
}

#[test]
fn idle_decks_cost_nothing_and_the_host_is_not_asked_for_tracks() {
    let (_dir, c) = composition();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut runner = AutomixRunner::new(c.automix.clone(), c.automix_sink.clone());
    runner.set_host(Box::new(CountingHost(Arc::clone(&calls))));
    // No deck plays, so the host is never asked for a track, enabled or not.
    assert!(c.router.set_parameter("automix.enable", 1.0));
    for tick in 0..10 {
        runner.step(&c.engine, f64::from(tick) * 0.033);
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(c.automix.snapshot().state, om_core::AutomixState::Idle);
}

#[test]
fn camelot_compatibility_matches_the_analysis_crate_for_every_pair() {
    for a in 1..=12u8 {
        for a_minor in [false, true] {
            for b in 1..=12u8 {
                for b_minor in [false, true] {
                    let code = |n: u8, minor: bool| format!("{n}{}", if minor { 'A' } else { 'B' });
                    let (ca, cb) = (code(a, a_minor), code(b, b_minor));
                    let expected = is_harmonically_compatible(
                        &MusicalKey::from_camelot(&ca).unwrap(),
                        &MusicalKey::from_camelot(&cb).unwrap(),
                    );
                    let got = Camelot::parse(&ca)
                        .unwrap()
                        .compatible(Camelot::parse(&cb).unwrap());
                    assert_eq!(got, expected, "{ca} vs {cb}");
                }
            }
        }
    }
}

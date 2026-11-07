//! midir smoke tests. They need the host's MIDI system (on Linux `/dev/snd/seq`); without it
//! (CI, WSL) each test skips. Nothing here opens a port that is not already there, and nothing
//! sends.

use om_midi::{MidiBackend, MidirBackend};

fn midi_system_present() -> bool {
    if cfg!(target_os = "linux") {
        std::path::Path::new("/dev/snd/seq").exists()
    } else {
        true
    }
}

#[test]
fn construction_fails_cleanly_without_a_sequencer() {
    if midi_system_present() {
        eprintln!("skipped: the MIDI system exists, so construction is expected to succeed");
        return;
    }
    assert!(
        MidirBackend::open().is_err(),
        "midir opened without /dev/snd/seq"
    );
}

#[test]
fn enumeration_runs_and_skips_our_own_and_loopback_ports() {
    if !midi_system_present() {
        eprintln!("skipped: no /dev/snd/seq");
        return;
    }
    let mut backend = MidirBackend::open().expect("MIDI system opens");
    assert_eq!(backend.name(), "midir");
    let inputs = backend.inputs().expect("inputs enumerate");
    let outputs = backend.outputs().expect("outputs enumerate");
    for port in inputs.iter().chain(&outputs) {
        assert!(!port.id.starts_with("Midi Through"), "{}", port.id);
        assert!(!port.id.starts_with("openmixer:"), "{}", port.id);
    }
    backend.refresh().expect("refresh");
}

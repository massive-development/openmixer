//! ALSA sequencer smoke tests. They need `/dev/snd/seq`; without it (CI, WSL) each test skips.
//! Nothing here opens a port that is not already there, and nothing sends.

#![cfg(target_os = "linux")]

use om_midi::{AlsaBackend, MidiBackend, MidiError};

fn sequencer_present() -> bool {
    std::path::Path::new("/dev/snd/seq").exists()
}

#[test]
fn construction_fails_cleanly_without_a_sequencer() {
    if sequencer_present() {
        eprintln!("skipped: /dev/snd/seq exists, so construction is expected to succeed");
        return;
    }
    match AlsaBackend::open() {
        Err(MidiError::Device(_)) => {}
        Err(other) => panic!("expected a device error, got {other}"),
        Ok(_) => panic!("a sequencer opened without /dev/snd/seq"),
    }
}

#[test]
fn enumeration_runs_when_a_sequencer_is_present() {
    if !sequencer_present() {
        eprintln!("skipped: no /dev/snd/seq");
        return;
    }
    let mut backend = AlsaBackend::open().expect("sequencer opens");
    assert_eq!(backend.name(), "alsa");
    backend.inputs().expect("inputs enumerate");
    backend.outputs().expect("outputs enumerate");
    backend.refresh().expect("refresh");
}

//! The Tauri trial window (ADR 0009): `--ui=tauri --mock-device --smoke` opens the window, waits for
//! the front end's first snapshot packet and its `ready`, and exits 0. Needs a display (WSLg, or `xvfb-run` in CI) and the built
//! front end (`npm run build` in `crates/om-ui-tauri/web`).
#![cfg(feature = "gui-tauri")]

use std::process::{Command, Output};

/// Runs the binary on its own preferences file, forced onto X11 the way the docs run windows on WSL.
fn run_openmixer(args: &[&str]) -> Output {
    let prefs = tempfile::tempdir().expect("tempdir");
    Command::new(env!("CARGO_BIN_EXE_openmixer"))
        .args(args)
        .env("WAYLAND_DISPLAY", "")
        .env("OPENMIXER_PREFS", prefs.path().join("preferences.json"))
        .env_remove("OPENMIXER_THEME")
        .output()
        .expect("failed to run openmixer binary")
}

#[test]
fn tauri_window_smoke() {
    let out = run_openmixer(&["--ui=tauri", "--mock-device", "--smoke"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("tauri smoke ok"), "stdout was: {stdout}");
}

/// The front end calls `ready` only after a snapshot packet with the right magic arrived, and the
/// demo library loads its decks on the control thread meanwhile.
#[test]
fn tauri_stream_smoke() {
    let out = run_openmixer(&["--ui=tauri", "--mock-device", "--demo-library", "--smoke"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("tauri smoke ok"), "stdout was: {stdout}");
}

/// In a smoke run with a library the front end waits for deck A's track, sends `deck0.play`
/// through `control` and awaits it before `ready`; the binary then fails unless the engine reports
/// deck A playing.
#[test]
fn tauri_control_round_trip() {
    let out = run_openmixer(&["--ui=tauri", "--mock-device", "--demo-library", "--smoke"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("tauri control round trip ok"),
        "stdout was: {stdout}"
    );
}

/// A smoke run without `--mock-device` still never opens a sound card: it runs on the mock.
#[test]
fn tauri_smoke_runs_silent_without_the_mock_flag() {
    let out = run_openmixer(&["--ui=tauri", "--smoke"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("no audio device"), "stderr was: {stderr}");
}

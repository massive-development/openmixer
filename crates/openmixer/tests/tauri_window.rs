//! The Tauri trial window (ADR 0009): `--ui=tauri --mock-device --smoke` opens the window, waits for
//! the front end's `ready` and exits 0. Needs a display (WSLg, or `xvfb-run` in CI) and the built
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

#[test]
fn tauri_window_refuses_a_real_device() {
    let out = run_openmixer(&["--ui=tauri", "--smoke"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("--mock-device"), "stderr was: {stderr}");
}

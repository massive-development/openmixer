# Hardware checklist: platform refresh

Everything in the platform refresh (#218 to #229, #241, #250) was built and tested headless on a host with no display, GPU, MIDI device or audio session. This list is what a person has to confirm on real hardware. Tick a box only after you saw it work. Write what you saw next to any item that fails and open an issue per failure.

Use a scratch preferences file and library so your real data stays untouched:

```bash
export OPENMIXER_PREFS=/tmp/om-check/prefs.json
export OPENMIXER_LIBRARY_DB=/tmp/om-check/library.db
mkdir -p /tmp/om-check
```

Build the full app once (Linux build needs the GPUI dependencies):

```bash
cargo build -p openmixer --features clap
```

## 0. Automated gates nobody has run yet

- [ ] `cargo run -p openmixer -- --smoke` exits 0.
- [ ] The 10-minute gate: `cargo test -p openmixer --test ui_gate -- --ignored` passes.
- [ ] `cargo test --workspace` passes on a quiet machine (no other builds running). Note any test that still fails; several timing tests flaked under load during the refresh.
- [ ] Optional leak check of the CLAP path: `tools/clap_leak_check.sh` reports 0 definitely lost, 0 indirectly lost.

## 1. Window and look (UI rewrite: #223, #229, #222 UI, CLAP panel)

The UI moved from hand-built widgets to gpui-component. Visual parity has not been compared.

- [ ] The window opens at 1280x720 and at 1024x640 (`OPENMIXER_WINDOW=1024x640`) without clipping.
- [ ] Capture the scenes and compare them with `docs/screenshots/`: `OUT_DIR=/tmp/om-scenes tools/screenshots.sh` (needs `DISPLAY`, ImageMagick and `xwininfo`). Look for: spacing, fonts, colours, dark and daylight themes, the settings tabs, the library table, dialogs.
- [ ] Dark and daylight themes both apply to the buttons, inputs, table, tabs, dialogs and toasts (the component theme follows the om-ui tokens).
- [ ] Library: the track table scrolls smoothly with a large library, sort arrows show, row selection works, the sidebar tree selects folders and playlists.
- [ ] Buttons fire on click release (this changed from press); a drag off the button cancels.
- [ ] Keys: `/` focuses library search and does not type a slash; Escape clears it and returns focus; keys 1 to 8 trigger sampler pads only when no text field is focused, and type digits when the search box or a settings field is focused.
- [ ] Dialogs: clear stem cache, reset mapping, and leave with unsaved mapping each ask before acting and Cancel does nothing.
- [ ] Toasts: the mock-device banner and the settings status messages appear and dismiss.
- [ ] Keyboard-only: Tab reaches every control in settings and the library; focus rings are visible.

## 2. Audio

- [ ] The audio device list shows your real devices and playback works (no clicks, no xruns in the debug panel at the default buffer).
- [ ] Real-time priority (Linux, off by default): build with `cargo build -p openmixer --features om-audio/realtime-dbus` (needs `libdbus-1-dev`), run under a desktop with rtkit, and check that xruns drop under CPU load compared with the default build. Promotion has never been tried on a real desktop. See issue #244 for the open decision.
- [ ] Varispeed (#241, sinc 128): play a tonal track at rates 0.9, 1.0 and 1.08 and compare by ear with the old build (`git checkout` the commit before #246). Listen for pre-ringing at the start and end of a track, aliasing above rate 1 on bright material, and clicks when changing rate, seeking, looping or loading. The last ~130 source frames of a track are known not to play.
- [ ] Loop wrap at several rates is seamless; two decks at different rates stay in sync with sync on.
- [ ] Sampler pads and stems: samples play at the right pitch and length (the resamplers were replaced with rubato). Check a 48 kHz sample on a 44.1 kHz device and the other way round.
- [ ] Loudness meter (#222): enable the LUFS panel on the mixer card. Play a track and check momentary, short-term and integrated LUFS, LRA and true peak look plausible against another meter. Play a hot signal and confirm the true-peak clip lamp. Reset clears the values. The dropped-frames warning must stay off in normal use.
- [ ] Optional: run EBU Tech 3341 and 3342 reference files through the meter (obtain them from the EBU). Cases 6 to 8 of 3341 and 5 to 6 of 3342 are the ones not covered by the synthetic tests.

## 3. MIDI (#220, midir backend)

- [ ] With a controller plugged in, the MIDI input list shows it and input reaches the app.
- [ ] Port ids changed: a controller saved by the old backend must be re-picked once. After picking, unplug and replug it and check the saved choice still matches.
- [ ] "Midi Through" and openmixer's own port do not appear in the list.
- [ ] Two identical controllers: note which one wins (known: the first one).
- [ ] Learn mode and a controller map still work with a real device.
- [ ] If you rely on sysex from a controller, note that sysex is ignored (as before).

## 4. Desktop integration (#229)

- [ ] File pickers: the Browse buttons for watch folder, recording folder, library XML import and controller map open the system file dialog (portal on Wayland) and return the chosen path.
- [ ] Copy: the Copy button and Ctrl/Cmd+C copy the selected tracks; paste into a text editor to check the text.
- [ ] System notifications appear when a recording stops and when the stem queue empties.
- [ ] Media keys: play, pause and stop control the lead deck, and the desktop now-playing widget (MPRIS) shows title, artist and play state. With no session bus, the app still starts.
- [ ] Windows and macOS (not built by CI): `souvlaki` and the media-key module were only compiled for those targets in isolation.

## 5. CLAP plugin host (#225, ADR 0007)

Needs a build with `--features clap` and at least one real CLAP plugin (also try the fixture, `examples/plugins/clap-gain-fixture`).

- [ ] A plugin placed in your plugin folder is listed on the Plugins page and loads.
- [ ] Audio passes through the plugin; bypass works; changing a parameter in the generic parameter panel changes the sound.
- [ ] Plugin GUI on X11: the plugin window opens embedded and is not painted over by the app. This is the main unverified risk in ADR 0007.
- [ ] Plugin GUI on Wayland: a floating plugin window opens; close it and confirm the app does not crash (the plugin must be closed before its parent window is destroyed).
- [ ] A plugin that offers only an X11 GUI on Wayland shows the generic panel.
- [ ] A plugin that crashes or panics in `process` is bypassed without taking the app down (a native crash is not caught, by design).
- [ ] Quit the app with a plugin open: it exits cleanly with no hang.
- [ ] Watch memory while loading and unloading a plugin repeatedly; it should stay flat.

## 6. Library database (#250)

- [ ] Startup, scanning a large folder and analysing tracks are noticeably faster than before.
- [ ] Power-loss behaviour (accepted risk): the library database and analysis cache now use `synchronous=NORMAL` in WAL mode. After a hard power-off, the last few seconds of changes may be missing but the database must open and not be corrupt. Optional: pull the plug during a scan and check it opens.
- [ ] A library created before this change still opens.

## Reporting

For each failed item, note: the build commit, the OS and desktop (X11 or Wayland), the hardware, and what you saw. The areas most likely to need fixes are the X11 plugin embedding, the visual parity of the UI rewrite, and the varispeed path by ear.

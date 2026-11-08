# ADR 0007: CLAP Plugin Hosting with clack

## Status

Accepted

Design note only (issue #225, P7). No code and no Cargo change is part of this ADR. The maintainer signed off on 2026-11-07 (see "Decisions"); implementation follows the migration plan.

Update (2026-11-07): the loader, parameters, the plugin window and a test fixture landed behind the `clap` feature of `om-plugins` (see "Implementation notes"). Still open: the first-run prompt and warning screens, state persistence and latency compensation.

## Context

`om-plugins` loads our own plugins through a C ABI with `libloading` (`abi.rs`, `loader.rs`; ABI versions 1 and 2; `docs/spikes/DECISIONS.md`, "Plugins: C ABI dynamic loading"). It is mono, in-place, one `process` call per channel, with an optional JSON parameter list and no GUI. Only plugins written against `include/openmixer_plugin.h` can run. Most effects, instruments and utilities that users own ship as CLAP, VST3 or AU, so a user cannot bring "their" reverb into OpenMixer.

`agents.md` ("Extensibility") says users must always be able to load external plugins and that we add no allow-lists, signing or lockdown. Opt-in safety warnings are fine.

CLAP (CLever Audio Plugin) is an open, MIT-licensed C ABI plugin standard. `clack` is a set of Rust wrappers for it. This ADR decides whether and how to host CLAP plugins with `clack-host` and `clack-extensions`.

### What was verified

Checked on 2026-11-06 against crates.io, the 0.2.0 crate sources (downloaded and read) and the GitHub repository.

- `clack-host` and `clack-extensions` are at 0.2.0, published 2026-09-12. Earlier releases: 0.1.0 (2026-05-03) and 0.1.1 (2026-07-29). None is yanked. Licence is MIT OR Apache-2.0 (both already allowed in `deny.toml`). MSRV is 1.85.0.
- Repository: `github.com/prokopyl/clack`. Last push 2026-09-12 (the 0.2.0 release day). About 241 stars and 5 open issues at the time of the check. About 57k (`clack-host`) and 60k (`clack-extensions`) downloads over all versions. crates.io lists only three dependents of `clack-host` (`clack-extensions` and two small crates), so there are few other production hosts to learn from.
- The README says clack is "feature-complete" but "APIs can still have breaking changes made to them in the future", with documentation and examples incomplete. It is a low-level wrapper: "the safe APIs in this crate are safeguarding the host implementation, not the plugins it loads."
- `clack-host` features: `default = ["libloading"]`, which pulls `libloading = "=0.8"` (exact pin) and, on macOS, `objc2-foundation`. `om-plugins` already depends on `libloading = "0.8"`. The exact pin could conflict with a later bump of ours (unverified until a build is tried).
- `clack-extensions` has one feature per extension (`params`, `audio-ports`, `note-ports`, `state`, `gui`, `latency`, `tail`, `timer`, `posix-fd`, `thread-pool`, `preset-discovery`, `remote-controls` and more). The `gui` extension has optional `raw-window-handle_05` and `raw-window-handle_06` features. Our lockfile resolves `raw-window-handle` 0.6.2.
- `PluginEntry::load` is `pub unsafe fn load<P: AsRef<OsStr>>(path: P)` in 0.2.0 (`src/entry.rs`; so are `load_from` and `load_from_raw`). The `# Safety` text says loading a library can run arbitrary code, and a non-compliant entry can invalidate the safety assumptions of the rest of the API. The crate docs say users who need protection from plugin crashes "should consider using additional process isolation techniques". The README example also wraps `load` in `unsafe`. So for 0.2.0 the sources and docs agree: it needs `unsafe`. (Other versions: unverified.)
- `clack-host` has no plugin discovery of its own; its docs point to a separate `clack-finder` crate (not examined; unverified).
- Extension callbacks in `clack-host` run inside `catch_unwind` (`extensions/wrapper.rs`). That protects the host from panics in Rust code, not from a crash or UB inside a third-party binary.
- GUI window handles: with `raw-window-handle_06`, clack's `Window::from_window(&W: HasWindowHandle)` builds a CLAP parent window from Win32, AppKit, Xlib or Xcb handles and returns `None` for anything else. There is no Wayland case, although CLAP names a `wayland` API string. (Read from the 0.2.0 source.)
- `gpui-pre` 0.3.10 (the GPUI we use after #219) implements `HasWindowHandle` and `HasDisplayHandle` for `Window`, delegating to the platform window (read from source). Which raw handle its Linux X11 and Wayland windows return, and whether it names a window a foreign child can be reparented into, is unverified.

## Decision

Accepted. The maintainer's answers to the seven open questions are in "Decisions" and override anything below that reads as an option.

### 1. Host third-party CLAP plugins, not only our own

Host any CLAP plugin the user points us at, beside the own-plugin ABI. This is the "no gatekeeping" rule applied. We add an opt-in warning on first load of a third-party plugin (it runs native code in our process) and nothing else: no allow-list, no signature check.

Hosting only our own CLAP plugins would add the clack dependency and its risk for no gain over the C ABI. Rejected.

### 2. Use `clack-host` and `clack-extensions` 0.2.0, pinned exactly

Enable only the extensions we use: `audio-ports`, `params`, `state`, `latency`, `tail`, `gui`, `timer`, `posix-fd`, `thread-check`, and `raw-window-handle_06`; `note-ports` is not enabled in the first release (effects only), but see "Keeping instruments and sidechain open". Pin with `=0.2.0` in the workspace `Cargo.toml` and bump by hand after reading the changelog, because the API is pre-1.0 and may break on any release. Keep every clack type behind our own types inside `om-plugins`, so a breaking release, or a move to raw `clap-sys` (which clack itself uses), touches one module. The pre-1.0 risk is accepted because the wrapper is thin and the CLAP ABI under it is stable.

### 3. `unsafe`: one new allowed module in `om-plugins`

`PluginEntry::load` needs `unsafe`. The workspace forbids `unsafe_code`; `om-plugins` denies it and allows it only in `abi.rs` and `loader.rs` (see `lib.rs`), which exist to load shared libraries.

Decision (maintainer-approved, Decisions 3): add one module, `om-plugins/src/clap` (feature `clap`), with `#[allow(unsafe_code)]` on the loading file only, as `abi` and `loader` have it. It holds the single `unsafe { PluginEntry::load(path) }` call with a `// SAFETY:` line (the user chose to load this file; the entry is then used only through clack's safe API; clack keeps the library loaded while any instance created from the entry lives). Everything else stays safe Rust in sibling files that keep `deny(unsafe_code)`, including the GUI bridge, because `Window::from_window` is safe. Avoid `Window::borrow_handle_unchecked`, which is `unsafe`. The `unsafe` surface grows by one call site, and `agents.md` ("Unsafe") and CONTRIBUTING then name this module beside `abi` and `loader`.

Alternatives:

- Put it in `loader.rs`. That mixes two loaders in a module meant for one ABI. Rejected.
- A new crate `om-clap` with `unsafe` allowed, like `om-stretch`. A harder boundary, and clack leaves `om-plugins`' dependency tree. Costs a crate and a new entry in the policy. Not chosen (Decisions, 3).

Wherever the `unsafe` sits, third-party code can still corrupt the process. See "Crash isolation".

### 4. Audio-thread rules

CLAP `process` runs on the real-time audio thread. The rules are those of the C ABI: no allocation, no lock, no blocking, no I/O.

- Build the audio buffers, the event lists and the started processor on the control thread at activation. The audio callback only fills pre-allocated buffers and calls `process`. `assert_no_alloc` (already a dev-dependency) covers the host side in tests.
- Plugin lifecycle (`init`, `activate`, `deactivate`, `destroy`, state load and save, all GUI calls) runs on the control thread, never the audio thread.
- Parameter changes from the UI or MIDI go to the audio thread through an `rtrb` queue and become CLAP parameter events in the next `process`. Plugin-originated requests (`request_callback`, `request_restart`, parameter changes made in its GUI) come back through a lock-free queue and are handled on the control thread.
- `catch_unwind` wraps every `process` call, as `abi.rs` does. On a caught panic, or an error status, mark the instance faulted, pass the dry signal through from then on, and tell the control thread. A C++ exception or crash inside a plugin is not a Rust panic and is not caught. `catch_unwind` is not a sandbox and must not be described as one.
- A plugin that allocates or locks inside `process` is outside our control. We surface its measured cost so the user can spot it. Whether the engine's stage timing is fine-grained enough per plugin is unverified.
- Implement `thread-check`; do not offer `thread-pool` at first.
- Channel layout: the stage model is one mono buffer per call, twice per stage. A CLAP effect needs stereo in one call, so it needs a stereo stage kind in the engine (migration step 4).

### 5. GUI strategy

CLAP GUIs are native windows the plugin owns, either embedded in a host-provided parent or floating.

- X11, Win32, Cocoa: embed. Take the `HasWindowHandle` of the GPUI `Window` that shows the plugin panel, convert it with `Window::from_window`, then `gui.create` (embedded), `set_parent`, `set_scale`, `set_size`, `show`. The parent is the whole top-level native window, not a sub-rectangle, so placing the plugin's child window inside our layout, and keeping it there through scroll, resize and scale changes, is our job and is untested. Unverified: whether GPUI's X11 window can host a foreign child without GPUI painting over it.
- Wayland: no reparenting exists and clack's handle bridge has no Wayland case. Use floating mode: `gui.create` (floating), `set_transient` to our top-level where possible, then `show`. The plugin opens its own window beside OpenMixer. A plugin that supports only X11 gets the generic parameter panel on Wayland; no XWayland (Decisions, 5).
- Fallback on every platform: a generic parameter panel built from the `params` extension (names, ranges, steps) in our own UI.
- Runtime order: `get_preferred_api`, then `is_api_supported` for the platform's embed API, then floating, then the generic panel.
- Plugin timers (`timer`) and file descriptors (`posix-fd`) are serviced on the control thread. A plugin-requested resize is applied on the control thread and acknowledged.
- That `gpui-pre` implements `HasWindowHandle` for `Window` is verified; that the handle is usable for embedding is the first thing the proof of concept must settle.

### 6. Relationship to the C ABI and the example plugins

CLAP hosting is added beside the C ABI, not instead of it. The C ABI works, is tested and has example plugins (`gain-plugin`, `gain-plugin-cdylib`, `param-plugin-cdylib`); it is a few functions any language can export. CLAP is far larger. Both are kept.

- Same `Plugin` trait and `PluginRegistry`: a CLAP plugin is adapted to `Plugin` as a C ABI library is, so unique ids, duplicate-name refusal and unload semantics hold for both.
- Same plugin folder scan, with a new candidate rule: a `.clap` file goes to the CLAP loader, other shared libraries keep going to the C ABI loader. Standard CLAP search paths on Linux are `~/.clap` and `/usr/lib/clap` (per the CLAP spec; to be confirmed when implemented), plus the user's plugin folder setting.
- The example plugins stay as they are. One small CLAP example, built from `clack-plugin` as a dev-only dependency, is added later as a test fixture.
- Deprecating the C ABI later, if CLAP covers everything, is not proposed here.

### Crash isolation

A third-party plugin that crashes takes OpenMixer down mid-set. clack states that its safe API does not prevent this. Options, cheapest first:

1. In-process with a warning (proposed first step). The same exposure as the C ABI has today.
2. An out-of-process plugin host (a child process loads the plugin and shares audio through shared memory). Strong isolation, but it adds latency, a second binary, and puts the plugin window in another process. Not proposed now. The CLAP adapter sits behind our `Plugin` trait, not inside the engine, so this stays possible.

## Migration plan

1. Sign-off on this ADR: done (Decisions).
2. Proof of concept on a branch, not merged: load one CLAP plugin (a `clack-plugin` gain example), activate, process a buffer offline, and open its GUI embedded on X11 inside a GPUI window. This settles the embedding risk and the `libloading` pin.
3. Land the loader: `om-plugins/src/clap` behind feature `clap` (off by default until it passes on all CI platforms), one `unsafe` call, `cargo deny` check, workspace pin, `agents.md` and CONTRIBUTING updated for the new allowed module.
4. Stereo stage kind in the engine and `om-plugins`, so a CLAP effect gets both channels in one call, with latency reported through the `latency` extension for delay compensation. Instruments (note ports) and sidechain each need their own design and come later.
5. Parameters: map CLAP parameters to the existing parameter targets (`register_target`) and the router, honouring the router rules in `agents.md` (a slow `set_parameter` stalls dispatch, so hand work to a queue).
6. State: persist plugin state (`state` extension) in preferences, next to the rack it belongs to.
7. GUI: embedded on X11, Win32, Cocoa; floating on Wayland; generic panel fallback.
8. The plugin folder scan accepts `.clap` files; the preferences UI lists them and warns on first load.

Each step is its own PR, with `docs/progress.json`, `TODO.md` and `docs/PROGRESS.md` updated when task status changes.

## Test plan

- Fixture: one small CLAP plugin built from `clack-plugin` in the repo (gain, one parameter, a trivial GUI), built in CI. A real third-party plugin is tried by hand and is not a CI dependency.
- Load: the host finds the entry, lists the plugin, and refuses a non-CLAP file and a corrupt library with a typed `PluginError`.
- Process: activate at several sample rates and block sizes, run silence and a test tone, check the gain is applied, check there is no allocation in the audio-thread path with `assert_no_alloc`, and check a plugin that errors or panics is faulted and bypassed.
- Parameters: a UI or MIDI change reaches the plugin as a parameter event in the next block; a plugin-side change appears in the host snapshot.
- Unload: instances are destroyed before the library is released; unloading with a live stage is safe.
- GUI: on X11 under `xvfb`, create, embed, show, resize and destroy the plugin GUI without a crash (CI-runnable). Wayland floating, Win32 and Cocoa are checked by hand until runners exist. `tools/screenshots.sh` gives a before and after of the plugin panel.
- Acceptance for the proof of concept and for the feature: one CLAP plugin loads, processes audio and shows its GUI.

## Consequences

- Positive: users get the existing CLAP ecosystem, and the C ABI and its examples keep working. The `unsafe` surface grows by one call site in a module allowed to have it. clack is MIT OR Apache-2.0, accepted by `deny.toml`; we link it into an AGPLv3 work, and plugins load at run time under their own licences.
- Negative: a pre-1.0 dependency pinned and upgraded by hand; a second plugin path to maintain; a large new surface (GUI embedding on several platforms, threading rules, latency, state). A third-party plugin can crash or stall the app and in-process hosting does not protect against it. VST3 and AU users stay out, which is accepted.
- Neutral: Wayland has no reparenting, so floating windows there are permanent, not a temporary gap.

## Decisions

The maintainer answered the open questions on 2026-11-07.

1. **MVP: in-process with a first-load warning.** Third-party CLAP plugins are hosted in-process. Crash isolation through an out-of-process host is planned later. To keep that door open, all clack and CLAP types stay behind our own wrapper types (a plugin-instance trait and a small message boundary of audio blocks, parameter events and control calls), so the host can move into a child process without touching the engine.
2. **First-release platforms: Linux X11 plus Wayland floating windows.** Windows and macOS follow.
3. **`unsafe` is permitted where necessary, on a no-leak condition.** The workspace `unsafe_code = "forbid"` lint stays; exceptions stay per module. Keep `unsafe` minimal, each use with a `// SAFETY:` comment, confined to the `om-plugins` `clap` module proposed above. The CLAP load, instance and teardown path needs a leak check before it merges: a test that loads, activates, processes and drops an instance many times in a loop and checks for growth with a counting allocator, plus a valgrind (or equivalent) run of that test, documented in the PR. Leaks found there block the merge.
4. **Effects only first (stereo in, stereo out).** Wrapper types and the host handlers are designed so note ports and sidechain are additions, not a rewrite; see "Keeping instruments and sidechain open".
5. **X11-only plugins on Wayland: generic parameter panel.** No XWayland.
6. **Discovery: the user's plugin folder plus the standard CLAP paths (`~/.clap`, `/usr/lib/clap`), behind a first-run prompt**, because scanning runs plugin initialisers.
7. **clack: exact pin `=0.2.0`, our own wrapper types, upgrades bumped by hand.**
8. **Plugin delay compensation: deferred.** The engine does not compensate for a CLAP plugin's latency, and no delay lines are added. The reported latency (the `latency` extension, read at activation and again when the plugin signals a change) is shown only, as frames and milliseconds at the current sample rate, on the Plugins page rows and in the parameter panel header. Revisit when a plugin needs it.

### Keeping instruments and sidechain open

- Audio ports: the wrapper reads the `audio-ports` extension and models the port list (count, channel count, main or sidechain flag) as data, not as a fixed stereo pair. The first release accepts only one stereo input and one stereo output and refuses other layouts with a typed error. A sidechain port then becomes an extra input buffer in the same process-block type.
- Events: the process-block type carries event lists in and out (parameter events now). Note events slot into the same lists, and `note-ports` is a Cargo feature to enable later.
- Stage kind: the stereo stage kind is introduced with a descriptor (inputs, outputs, kind), so an instrument or a sidechain effect is a new descriptor value and not a new engine path.
- Routing for sidechain and the source for instrument notes (MIDI from `om-midi`) need their own design notes when they are taken up.

## Implementation notes

Where the build differs from, or settled a question in, the text above.

- **Unsafe surface is three modules, not one.** `clap/entry.rs` holds the library open. clack 0.2.0 also marks `PluginGui::set_parent` and `set_transient` `unsafe` (it cannot check the window is still valid), which the "GUI" section assumed safe: they live in `clap/gui_parent.rs`. Posix file descriptors a plugin registers are borrowed in `clap/fds.rs`. Each block has a `// SAFETY:` line; the leak check covers the path.
- **One control thread per plugin instance.** clack's `PluginInstance` is not `Send`, so each instance lives on a worker thread and the engine stage holds only the audio processor (`clap/worker.rs`). All GUI, timer and parameter-query calls run there. This is the message boundary an out-of-process host would replace.
- **No engine change.** The engine's `ProcessStage::process(left, right)` already receives both channels in one call, so the "stereo stage kind" is the existing stage plus a `StageDescriptor` in `om-plugins` that records the ports (data, one stereo in and out accepted).
- **The window is a window of its own.** CLAP gives the host no call to place the plugin's child inside a sub-rectangle, so on X11 the plugin is embedded in a dedicated GPUI window that holds nothing else. Which handle GPUI returns was read from the source: an XCB window handle (the X11 window id) on X11 and a Wayland surface handle on Wayland; clack's `Window::from_window` has no Wayland case, so Wayland floats.
- **Still unverified:** that GPUI's X11 window keeps the plugin's child visible (it may paint over it), the order of `close` against platform window teardown, and a Wayland floating window opening. No display was available. The plan logic and the CLAP call sequence are tested against the fixture's GUI stub only.
- **Discovery** is data (`ClapSearch`): the standard folders are scanned only when `scan_standard_paths` is true, which the first-run prompt sets. The registry refuses `load_clap` until the first-load warning is accepted.
- **The first-run prompt, the first-load warning and `.clap` discovery (decision 6, step 8) are built, headless-tested and not seen on a display.** Consent lives in the preferences file's `plugins` section (`clapScan`: `unasked | allowed | declined`, `clapWarningAccepted`; no version bump, both keys are optional and the section is old). `PreferencesTargets::apply_plugin_policy` pushes it before the folder is loaded, `PluginRegistry::set_clap_policy` keeps it, and `load_clap_folders` lists the `.clap` files of the plugin folder (plus `~/.clap` and `/usr/lib/clap` once the answer is `allowed`) without opening them, then opens them only when the warning is accepted; before that they are `clap_files_waiting`. The two questions are `gpui-component` alert dialogs through the settings overlay's existing confirmation path (`Ask::ClapFirstRun`, `Ask::ClapFirstLoad`), asked in that order, only in builds with the `clap` feature and not in `--smoke` or demo runs. Answering stores the choice and reloads the folder; dismissing the first-run dialog (Escape) stores nothing and asks again next start; cancelling the warning stores nothing and leaves the plugins unloaded until the Plugins tab's "Review the warning" chip re-asks. The Plugins tab shows the standard-folder switch, the `.clap` files found and whether they wait. The remaining `.clap` rows are not per-file load buttons: all files found load together once the warning is accepted.
- **State persistence (step 6) is done, in a file beside the preferences, not inside them.** `Plugin::save_state` / `restore_state` (defaulted, so other plugins are unaffected) carry the plugin's opaque bytes; for a CLAP plugin they are the `state` extension's save and load on the instance's control thread (`clap/persist.rs`, capped at `MAX_STATE_BYTES`, 32 MiB). `PluginRegistry` keeps the bytes by plugin id: it hands them to a plugin right after `on_load`, captures a plugin's state when it unloads (before its stages go), and `plugin_states()` merges that with the live instances. A plugin with no stage yet keeps the bytes and a stage loads them before it activates, then the parameter snapshot is read back from the plugin. The app stores them in `<preferences folder>/plugin-state.json` (`openmixer::plugin_state`: version 1, one base64 blob per plugin id, temp-then-rename, a corrupt file kept as `.bad`, a newer version left alone), written when a plugin is unloaded, the folder is reloaded and the window closes. A separate file because a state can be megabytes and the preferences are rewritten on every change; this is the `sampler.json` pattern. Not done: saving while the app runs without unloading (a crash loses the changes since the last unload or reload), and a state for a plugin that has several stages (the first live instance is the one saved; all live instances receive a restore).

## References

- Issue #225 (P7); #218 and #219 (gpui-kit, `gpui-pre` 0.3.10)
- `crates/om-plugins/src/abi.rs`, `loader.rs`, `lib.rs`; `docs/spikes/DECISIONS.md` ("Plugins: C ABI dynamic loading", "ABI 2")
- `agents.md` ("Unsafe", "Extensibility", "Plugin control targets")
- ADR 0004 (Rust with GPUI; plugins superseded in part)
- clack: https://github.com/prokopyl/clack, https://crates.io/crates/clack-host, https://crates.io/crates/clack-extensions, https://docs.rs/clack-host/0.2.0
- CLAP: https://github.com/free-audio/clap

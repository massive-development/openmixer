# ADR 0007: CLAP Plugin Hosting with clack

## Status

Proposed

Design note only (issue #225, P7). No code and no Cargo change is part of this ADR. Building waits for the maintainer's sign-off on the decision and on the open questions below.

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

Proposed, subject to sign-off on the open questions.

### 1. Host third-party CLAP plugins, not only our own

Host any CLAP plugin the user points us at, beside the own-plugin ABI. This is the "no gatekeeping" rule applied. We add an opt-in warning on first load of a third-party plugin (it runs native code in our process) and nothing else: no allow-list, no signature check.

Hosting only our own CLAP plugins would add the clack dependency and its risk for no gain over the C ABI. Rejected.

### 2. Use `clack-host` and `clack-extensions` 0.2.0, pinned exactly

Enable only the extensions we use: `audio-ports`, `params`, `state`, `latency`, `tail`, `gui`, `timer`, `posix-fd`, `thread-check`, and `raw-window-handle_06`; `note-ports` when instruments are in scope (open question 4). Pin with `=0.2.0` in the workspace `Cargo.toml` and bump by hand after reading the changelog, because the API is pre-1.0 and may break on any release. Keep every clack type behind our own types inside `om-plugins`, so a breaking release, or a move to raw `clap-sys` (which clack itself uses), touches one module. The pre-1.0 risk is accepted because the wrapper is thin and the CLAP ABI under it is stable.

### 3. `unsafe`: one new allowed module in `om-plugins`

`PluginEntry::load` needs `unsafe`. The workspace forbids `unsafe_code`; `om-plugins` denies it and allows it only in `abi.rs` and `loader.rs` (see `lib.rs`), which exist to load shared libraries.

Decision: add one module, `om-plugins/src/clap` (feature `clap`), with `#[allow(unsafe_code)]` on the loading file only, as `abi` and `loader` have it. It holds the single `unsafe { PluginEntry::load(path) }` call with a `// SAFETY:` line (the user chose to load this file; the entry is then used only through clack's safe API; clack keeps the library loaded while any instance created from the entry lives). Everything else stays safe Rust in sibling files that keep `deny(unsafe_code)`, including the GUI bridge, because `Window::from_window` is safe. Avoid `Window::borrow_handle_unchecked`, which is `unsafe`. The `unsafe` surface grows by one call site, and `agents.md` ("Unsafe"), CONTRIBUTING and the handoff text then name this module beside `abi` and `loader`.

Alternatives:

- Put it in `loader.rs`. That mixes two loaders in a module meant for one ABI. Rejected.
- A new crate `om-clap` with `unsafe` allowed, like `om-stretch`. A harder boundary, and clack leaves `om-plugins`' dependency tree. Costs a crate and a new entry in the policy. Reasonable; see open question 3.

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
- Wayland: no reparenting exists and clack's handle bridge has no Wayland case. Use floating mode: `gui.create` (floating), `set_transient` to our top-level where possible, then `show`. The plugin opens its own window beside OpenMixer. A plugin that only supports X11 might run through XWayland (open question 5).
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

1. Sign-off on this ADR and the open questions. Status moves to Accepted.
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

## Open questions

1. Sign-off: host third-party CLAP plugins now, in-process with a warning, or wait for an out-of-process host design?
2. Which platforms must the first release cover: Linux X11 only, Linux with Wayland floating, Windows, macOS?
3. `unsafe` placement: a module in `om-plugins` (proposed) or a new crate `om-clap` with its own exception, like `om-stretch`?
4. First-release scope: effects only (stereo in, stereo out), or also instruments (note ports) and sidechain?
5. Wayland: when a plugin supports only X11, run it through XWayland, or show the generic panel only?
6. Discovery: scan only the user's plugin folder, or also the standard CLAP paths? Scanning runs plugin initialisers, so a first-run prompt may be wanted.
7. Is the pre-1.0 risk acceptable with an exact pin, our own wrapper types and hand-bumped upgrades, or do you want raw `clap-sys` from the start?

## References

- Issue #225 (P7); #218 and #219 (gpui-kit, `gpui-pre` 0.3.10); the platform refresh handoff (`docs/superpowers/plans/2026-11-06-platform-refresh-handoff.md`, local, untracked)
- `crates/om-plugins/src/abi.rs`, `loader.rs`, `lib.rs`; `docs/spikes/DECISIONS.md` ("Plugins: C ABI dynamic loading", "ABI 2")
- `agents.md` ("Unsafe", "Extensibility", "Plugin control targets")
- ADR 0004 (Rust with GPUI; plugins superseded in part)
- clack: https://github.com/prokopyl/clack, https://crates.io/crates/clack-host, https://crates.io/crates/clack-extensions, https://docs.rs/clack-host/0.2.0
- CLAP: https://github.com/free-audio/clap

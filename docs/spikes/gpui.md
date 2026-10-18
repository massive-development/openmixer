# Spike A: GPUI window (go/no-go gate)

THROWAWAY spike code in `spikes/gpui-window/` (own `[workspace]`; not built by the root workspace or Rust CI). Date: 2026-10-18.

## Verdict

| Platform | Verdict | Basis |
|---|---|---|
| Linux / WSL2 (X11 via WSLg) | **PROVISIONAL GO** | Compiled and ran; window created, 60 fps counter in release. Rendered pixels not visually confirmed by the agent (headless session); Vulkan device in use not confirmed (likely CPU llvmpipe). |
| Linux / WSL2 (Wayland via WSLg) | Fails at startup (dev-loop issue) | Panic `UnsupportedVersion` in gpui `wayland/client.rs:151`. Workaround: `WAYLAND_DISPLAY= ` (forces X11). Per the design this does not fail the gate on its own. |
| Windows | **pending user** | Not run. |
| macOS | **pending user** | Not run. |

The gate is NOT closed: Windows and macOS are unrun. If either fails, the fallback is switching toolkit: iced first, then Slint (not dropping the platform).

## Step 1: Vulkan on the dev WSL

- `vulkaninfo` is not installed (`vulkan-tools` missing), so `vulkaninfo --summary` could not be recorded. **pending user: `sudo apt install vulkan-tools`, then paste `vulkaninfo --summary` here.**
- `/usr/share/vulkan/icd.d`: asahi, gfxstream_vk, intel_hasvk, intel, lvp, nouveau, radeon, virtio ICD json files. No `dzn` (Dozen/D3D12) ICD.
- `ldconfig -p | grep -i vulkan`: `libvulkan.so.1` plus libvulkan_{virtio,radeon,nouveau,lvp,intel_hasvk,intel,gfxstream,asahi}.so. No `libvulkan_dzn.so`.
- `/dev/dri` does not exist; `/dev/dxg` does (WSL GPU paravirtualisation). With no dzn driver and no `/dev/dri`, the only usable Vulkan device here is most likely `lvp` (llvmpipe, software). This matches the observed ~114% CPU while rendering. Not confirmed without vulkaninfo.

## How GPUI is obtained

- crates.io has `gpui` **0.2.2** (`cargo search gpui`, `cargo info gpui`; repo zed-industries/zed, Apache-2.0, edition 2024). So no git pin of Zed was needed; the earlier design note ("no authoritative crates.io status") is superseded. Pinned exactly with `gpui = "=0.2.2"`.
- Many third-party `gpui-*` forks/helpers also exist on crates.io; ignore them, use the official crate.
- No Zed git SHA recorded because no git dependency is used.
- Default features: font-kit, wayland, x11, windows-manifest.

## Build

Toolchain: repo `rust-toolchain.toml` (1.91.1) applies inside `spikes/` and was sufficient.

Problems hit, in order:

1. **libc/xattr incompatibility (dependency resolution).** A fresh resolve picked `libc 0.2.191`, which breaks `xattr 0.2.3` (pulled by `zed-async-tar` via `gpui_http_client`): `error[E0425]: cannot find value ENOATTR in crate libc`. Fix: `cargo update -p libc --precise 0.2.188` (0.2.182 and 0.2.184 also fail; 0.2.186 and 0.2.188 work; `rustix` needs >= 0.2.182). The committed `spikes/gpui-window/Cargo.lock` carries this pin. A future gpui or xattr release may remove the need.
2. **Link error: `unable to find library -lxkbcommon-x11`.** The runtime lib `libxkbcommon-x11.so.0` is installed but the dev symlink is not. **pending user: `sudo apt install libxkbcommon-x11-dev`** (also likely `libxkbcommon-dev`, `libwayland-dev`, `libfontconfig-dev`, `libfreetype-dev` for a clean machine; those linked fine here). Workaround used (no install, no sudo): `ln -s /lib/x86_64-linux-gnu/libxkbcommon-x11.so.0 /tmp/libshim/libxkbcommon-x11.so` and build with `RUSTFLAGS="-L /tmp/libshim"`.
3. **API friction:** `Pixels.0` is private in 0.2.2; use `f32::from(px_value)`.

Build times (12 cores, WSL2, registry cache warm after first download):

- Debug, full (all deps) with RUSTFLAGS: **43 s** (binary 467 MB debug).
- Release, full: **1 m 16 s**.
- First attempt (download + compile until the xattr failure): 30 s.
- Incremental rebuild of the spike crate: ~2 s.

Compile-only check for CI (no display needed) is not added: it needs the system -dev packages above on the runner and the libc pin, and the root `rust.yml` deliberately does not build the spike. Suggested later command: `cargo check --manifest-path spikes/gpui-window/Cargo.toml` on a runner with `libxkbcommon-x11-dev libwayland-dev libfontconfig-dev libxcb1-dev`.

## Run results (WSL2, Ubuntu 24.04, DISPLAY=:0, WAYLAND_DISPLAY=wayland-0)

| Run | Result |
|---|---|
| `./target/release/gpui-window-spike` (Wayland default) | panic `UnsupportedVersion` (gpui wayland client `Globals::new`, a Wayland global bound at a version WSLg's compositor does not offer) |
| `WAYLAND_DISPLAY= ./target/debug/gpui-window-spike` (X11) | runs, **30.0 fps**, ~115% CPU, RSS 166 MB |
| `WAYLAND_DISPLAY= ./target/release/gpui-window-spike` (X11) | runs, **60.0 fps**, ~114% CPU, RSS 143 MB; `xwininfo` shows a 1000x400 top-level window |

Caveats (honest): the session is non-interactive, so I did not see the window; fps is the in-app frame counter printed to stdout. 30/60 exact values look like a vsync or compositor cap, so this shows "keeps up with 60" but not the headroom. GPU vs llvmpipe unconfirmed (see Step 1).

## API friction (gpui 0.2.2)

- Docs are thin (docs.rs coverage is low); I read the crate source for `canvas`, `Window::paint_quad`, `fill`, `request_animation_frame`.
- Custom painting is easy: `canvas(prepaint, paint)` plus `window.paint_quad(fill(bounds, color))`; 2000 quads per frame was fine. Text is the `div().child(String)` path.
- Continuous animation needs `window.request_animation_frame()` in `render`; no frame callback or timing info is exposed, so the fps counter is hand-rolled.
- `Pixels` internals private, edition 2024 dependency, and fragile transitive pinning (libc/xattr) suggest budgeting for dependency churn; pin `=version` and commit `Cargo.lock`.
- Wayland backend panics (unwrap) instead of falling back to X11 when the compositor lacks a protocol version.

## Run on Windows/macOS (user)

Prereqs: Rust 1.91.1 (rustup picks it up from `rust-toolchain.toml`). Windows: MSVC build tools + Windows SDK. macOS: Xcode (full app, for the Metal toolchain) and `xcode-select`.

```
cd spikes/gpui-window
cargo build --release
cargo run --release
```

If the build fails with the xattr/ENOATTR error on macOS or Linux, run `cargo update -p libc --precise 0.2.188` (the committed Cargo.lock should already hold it). The window prints `fps N` to the console each second and shows it in the top-left. Let it run 60 s, resize the window, then also test dragging between monitors if available.

Fill in:

| Metric | Windows | macOS | WSL2 (done) |
|---|---|---|---|
| OS / version | | | Ubuntu 24.04 on WSL2 (X11/WSLg) |
| CPU / GPU | | | CPU only likely (lvp) |
| Rust / gpui | | | 1.91.1 / 0.2.2 |
| Cold release build time | | | 1 m 16 s |
| Builds without extra steps? (errors) | | | no: libc pin, xkbcommon-x11 dev symlink |
| Window opens, waveform scrolls smoothly? | | | fps counter ok; not visually checked |
| fps (release, steady) | | | 60.0 |
| fps (debug) | | | 30.0 |
| CPU % while running | | | ~114% |
| Resize / HiDPI / focus issues | | | not tested |
| Verdict GO / NO-GO | | | PROVISIONAL GO |

## What the user must run (checklist)

- [ ] WSL: `sudo apt install vulkan-tools libxkbcommon-x11-dev`, then `vulkaninfo --summary` and paste the output under Step 1 (confirm whether the device is llvmpipe or a real GPU via dzn; optionally `mesa-vulkan-drivers` is already present).
- [ ] WSL: `cd spikes/gpui-window && cargo build --release && WAYLAND_DISPLAY= ./target/release/gpui-window-spike`, look at the window, confirm the waveform visibly scrolls and the overlay reads ~60 fps.
- [ ] Windows: run the commands above, fill the table column.
- [ ] macOS: run the commands above, fill the table column.
- [ ] Decide GO / NO-GO per platform. NO-GO on Windows or macOS means: switch toolkit to iced, then Slint if iced also fails, and stop for input per the plan. WSL-only trouble (Wayland panic, software rendering) does not fail the gate by itself.

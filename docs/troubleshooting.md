# Common issues

## cpal fails to build on Linux (missing `alsa.pc`)
**Solution:** Install `libasound2-dev pkg-config`

## `rubberband-sys` fails with a missing `libclang`
**Solution:** Install `libclang-dev` (bindgen needs it)

## `libdbus-1` not found
**Solution:** Install `libdbus-1-dev`; the default `realtime-dbus` feature of `openmixer` needs it. Build with `--no-default-features` to go without it (the souvlaki media keys use zbus and do not need it)

## `cargo deny check` fails on a licence
**Solution:** Find the crate with `cargo deny check licenses`; replace it or confirm the licence is AGPL-compatible before adding it to `deny.toml`

## GPUI fails to start on WSL
**Solution:** GPUI renders through Vulkan, so check the driver with `vulkaninfo --summary` (from `vulkan-tools`) before debugging the app. On Windows 11, make sure WSLg is on: set `[guiApplications]` `enabled=true` in `/etc/wsl.conf`, then run `wsl --shutdown`. On Windows 10, run an X server such as VcXsrv and `export DISPLAY=127.0.0.1:0`. WSL-only trouble is a dev-loop issue, not a gate failure (ADR 0004)

## The Tauri window opens blank, or its `--smoke` fails with "never called ready"
**Solution:** The front end was not built, so `build.rs` put a placeholder page in `crates/om-ui-tauri/web/dist`. Run `npm ci && npm run build` in `crates/om-ui-tauri/web`, then build the binary again: the page is embedded at compile time. See `docs/features/tauri-trial.md`

## `npm` fails under WSL, or the front end's tools refuse the Node version
**Solution:** `npm` may resolve to a Windows install (`/mnt/c/...` on `PATH`), and the distribution's `node` may be too old (Vite 8 and Vitest 5 need 22.12 or newer). Put a Linux Node 22 or 24 first on `PATH`, for example from nvm: `export PATH=$HOME/.nvm/versions/node/<v22.x>/bin:$PATH`, and check with `which npm node` and `node --version`

## A window (GPUI or Tauri) does not open under WSLg
**Solution:** Run it with `WAYLAND_DISPLAY=` (empty) so it uses X11, as the build guide, the Tauri window tests and `tools/ui-bench.sh` do

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
**Solution:** WSL-only trouble is a dev-loop issue, not a gate failure (ADR 0004). Check the GPU/Vulkan setup with `vulkaninfo --summary` and record the result in `openmixer-workbench/docs/spikes/DECISIONS.md`


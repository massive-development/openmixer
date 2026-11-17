# Dependencies added in the platform refresh

- `gpui-pre` (renamed to `gpui`) with `gpui-kit` (which brings `gpui-component`), the GPUI base for `om-ui-gpui`.
- `rubato`: sample-rate conversion (sampler load, stems) and deck varispeed.
- `ebur128`: EBU R128 loudness metering in `om-engine`.
- `midir`: MIDI I/O in `om-midi`.
- `souvlaki`: OS media controls in `om-ui-gpui`.
- `walkdir`: library folder scans in `om-library`; `bytemuck`: sample casts in the optional ffmpeg decode path of `om-engine`.
- `divan` (dev-dependency): benches in `om-engine` and `om-analysis`.
- Not adopted after evaluation: `rfd`, `arboard`, `tray-icon` (GPUI's file picker and clipboard cover the first two, the third cannot work under GPUI's loop; `openmixer-workbench/docs/spikes/DECISIONS.md`).
- `clack-host`, `clack-extensions`, `clack-plugin`: CLAP hosting, behind the optional `clap` feature of `om-plugins`.
- `cpal` `realtime-dbus`: `om-audio` feature (needs `libdbus-1-dev`) for Linux real-time priority; `openmixer` forwards it and enables it by default.

Added for the Tauri trial (ADR 0009, `om-ui-tauri`, the `gui-tauri` feature of `openmixer`; removed again if the trial fails):

- `tauri` 2 and `tauri-build` 2 (build dependency), with `serde` for the command payloads; Linux needs `libwebkit2gtk-4.1-dev`.
- Front-end toolchain in `crates/om-ui-tauri/web` (npm, pinned in `package-lock.json`): `@tauri-apps/api` 2.12.4; dev only: `svelte` 5.57.3, `@sveltejs/vite-plugin-svelte` 7.3.1, `vite` 8.3.5, `vitest` 5.0.5, `typescript` 5.9.3, `svelte-check` 4.7.7. Needs Node 22.12 or newer.
- Lucide icons (ISC), five glyphs copied into `web/src/lib/icons/icons.ts` with their licence.

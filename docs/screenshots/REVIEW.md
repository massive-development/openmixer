# Screenshots

The PNGs in this folder are captured from the real window with `tools/screenshots.sh` (debug build, X11, 1280x800, `--mock-device`, generated demo tracks). Capture with a neutral `TMPDIR`: the MIDI, Library and Mapping tabs print temporary paths, so a home folder name would end up in the picture.

The captures were last checked by eye on 2026-11-12. Some of them predate the latest fixes, so a few of the issues below are fixed in code but still visible in the images.

## Known visual issues

Each of these needs a design decision:

- Meters and the LUFS readout turn red above -3 dB, while DESIGN.md reserves red for the playhead, record and clip.
- The deck title is 13 px in both densities, while the design canvas uses 16 px in Roomy.
- Selection uses deck B's slate, and daylight control edges are about 1.5:1 against the panel.
- The selected-deck outline and the splash loader from DESIGN.md are not built.
- An empty library says "No tracks match" even when nothing has been typed.
- Compact deck controls leave an empty strip under the options row.

Not started:

- The library toolbar is cut rather than reflowed when the sampler or debug column is open.
- The FX panel, sampler strip and loudness panel use fixed sizes in both densities.
- The deck head has no bar or beat counter, and the master meter and fader have no captions.
- The settings card changes height between tabs.
- During an Automix transition at 1280 px the next-track name is cut short.
- There is no Harbour Night scene, and the demo tracks are synthetic, so waveform shapes and colours are not representative.

# Screenshot review

Captured with `tools/screenshots.sh` (debug build, X11 on WSLg, llvmpipe-class software GL, 1280x800, `--mock-device`, demo library of 3 generated 2 s sine WAVs, deck A loaded). Looked at by eye on 2026-10-19. The window renders (not blank/black). Scenes: `main-dark`, `main-daylight`, `debug-panel`, `settings-audio`, `settings-appearance`.

## Fixed while capturing
- Theme was not applied at start-up: `OPENMIXER_THEME` and the saved theme never reached the window (it always used `openmixer.dark`). Now the window starts on the env theme, else the saved one.

## Defects, most severe first
1. **Library panel overlaps the deck controls (all main scenes).** The library sidebar (left, "All tracks / Playlists / ...") is drawn over deck A's button row: Play, Cue, Sync, Mstr are dimmed/cut by the sidebar's translucent block (x 0-170, y 570-800); deck B's row is fully visible. The deck button rows sit inside the library's area instead of under the decks.
2. **Deck buttons sit below the library table, far from their deck.** Play/Cue/Sync/Mstr/Key/Qnt/Slip are at y 740-765, under the track list, detached from the deck cards above. Looks unfinished.
3. **Settings overlay hides the whole window (both settings scenes).** The scrim is opaque; the main window is not visible behind the card, so it is not clear it is an overlay. Card is not centred vertically on content: the Appearance page is mostly empty space (y 310-620).
4. **Audio page: "Master output" label has no options under it** (settings-audio, y 503): the pair choices are missing (mock device lists no stereo pair), only the heading is shown, then "Headphone output" follows immediately.
5. **Text overlaps in the deck header (deck A, all main scenes).** "Am 8A -- BPM" is drawn over the time readout "0:00.0 -0:02.0" at y 120-150, and the overview strip (green bars) cuts through the text. Deck B has the same layout clash ("-- BPM" over "0:00.0 -0:00.0").
6. **Knob/slider labels are overdrawn by their fills (mixer, both channels).** Trim, Filter, Hi, Mid, Lo: the name is drawn in the same area as the blue fill and the value ("0 dB"), so "Hi", "Mid", "Lo" are barely readable (low contrast, grey on blue). Trim/Filter show filled bars with no clear indication of the centre/zero.
7. **Daylight theme: low-contrast disabled/inactive controls.** Deck button row (Play, Cue, Sync, Mstr, Key, Qnt, Slip) is washed out to near invisibility on the white backdrop (partly defect 1). Meters and sliders are light grey on light grey (channel meters at x 530-555 and 655-680 are barely visible). Headline text in the library header is fine.
8. **Waveform is a static comb, not a real waveform.** Deck A shows a flat band of uniform vertical lines for the sine track (plausible for a pure tone, but the overview strip is a plain green bar with no shape and the left half of the scroller is empty). No beat grid or cue markers. BPM shows "--" and key "8A" only for smoke_0 (BPM analysis did not run or find a tempo on a sine tone), so this scene does not show analysed data.
9. **Meters stuck at zero.** Channel and master meters are empty in every scene: expected with the silent mock device, but there is no way to see them move in a screenshot.
10. **Library browser: search box shows its syntax hint as text** ("Search: artist:name bpm:120-128 ...") in the same colour as typed text; reads like a stuck query rather than a placeholder. Artist, Genre and BPM columns are empty or "--" for the demo files. Rating column shows plain "0". Selected row (smoke_0) highlight is subtle in dark theme.
11. **Debug panel:** the file path fields read "click to type a file path" (placeholder, fine) but the parameter list is cut off at the window bottom without a scrollbar or a scroll indicator ("mixer.ch0.fader" last visible). The mixer and decks are squeezed (decks lose width, "Hi/Mid/Lo" values wrap to 0 / 0 / 0 dB).
12. **Header:** the banner "Mock device: silent, no audio output" is flush at the window edge with no padding and is in a dim colour; the Settings button floats at the far right of an otherwise empty strip.
13. **Crossfader and Cue mix/Phones are bare dark bars** with no handle visible (x 465-675, y 478-492), so their position is unreadable.

## Not captured
Settings MIDI, Library and Plugins tabs (one process per scene, kept to the README's set); anything with real audio or hardware (never used).

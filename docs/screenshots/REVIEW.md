# Screenshot review

Captured with `tools/screenshots.sh` (debug build, X11 on WSLg, llvmpipe-class software GL, 1280x800, `--mock-device`, demo library of 3 generated 2 s sine WAVs, deck A loaded). Looked at by eye on 2026-10-19. The window renders (not blank/black). Scenes: `main-dark`, `main-daylight`, `main-stems`, `debug-panel`, `settings-audio`, `settings-appearance`, `settings-midi`, `settings-library`, `main-automix` (see the Automix section below). The sampler strip (Wave 7 P4) appears in the main scenes from 2026-10-20 on; the PNGs of the earlier captures do not show it.

## Fixed while capturing
- Theme was not applied at start-up: `OPENMIXER_THEME` and the saved theme never reached the window (it always used `openmixer.dark`). Now the window starts on the env theme, else the saved one.

## Defects: before and after (second pass, 2026-10-19)

The first review listed 13 defects. A fix pass followed with `tools/screenshots.sh` as the loop (the PNGs here are the "after"). Root cause of 1, 2, 5 and 13: the deck rows and the slider thumbs were children of an already positioned box but were given card-absolute rectangles, so their offset was applied twice and they landed below the card (over the library band) or off the track.

| # | Defect | Status | What changed / why not |
|---|---|---|---|
| 1 | Library sidebar overlapped deck A's button row | fixed | Rows now sit inside their deck card; cards clip their content. `main_layout` test: no overlap, all rects inside the window, deck parts inside the card, library band below the decks, at 1280x800, 1024x640, 1920x1080, 900x600. |
| 2 | Deck buttons detached below the library table | fixed | Same cause and fix as 1. Hotcue pads, loop row and transport are now inside the card. |
| 3 | Settings overlay hid the window; Appearance page empty | fixed | Scrim is black at the theme's `dimAlpha` (0.55) so the window shows through. Appearance lists each theme with its id and a "current" marker, and its card is shortened to fit. |
| 4 | Audio "Master output" heading with no options | fixed | With no stereo pairs the two headings are replaced by one note. |
| 5 | Deck header text overlap | fixed | Title and time on the first line, artist and key/BPM on the second, clipped to the header. |
| 6 | Knob labels overdrawn / low contrast | fixed | Value fill is translucent (45%) and drawn first; label and readout use the primary text colour above it. Bipolar knobs get a centre line. A narrow knob (debug layout) drops the " dB" unit so it stays on one line. |
| 7 | Daylight: near-invisible controls | fixed | Daylight tokens: darker border (`#77839B`), dark highlight (playhead, thumb edge), darker muted text and meter colours, `disabledAlpha` 0.85; meter wells get a border. New test: every control edge, fader thumb, disabled label and meter colour is >= 3:1 (daylight) / >= 1.2:1 (dark, visually fine, not tightened). |
| 8 | Waveform: no grid or cues | partially | Beat and bar lines are drawn in the zoomed view; the demo puts a 120 BPM grid on deck A (header shows "120.0 BPM"). Hotcue markers were already drawn but the demo sets no cues, so none show. The waveform shape is still a flat comb (sine tones). |
| 9 | Meters stuck at zero | not fixed | Silent mock device: nothing to show. Out of scope. |
| 10 | Search hint style; empty Artist/Genre/BPM | partially | Hint is dim and italic-styled (the UI font may not render italics, it is only dimmer). Demo WAVs carry artist/genre/title RIFF INFO tags that the ingest reads, so Artist and Genre columns fill. BPM stays "--" in the table: sine tones have no tempo, and the library table only shows analysed BPM. Rating "0" unchanged. |
| 11 | Debug panel clipping and squeeze | partially | The parameter list scrolls with the wheel and a "scroll the list for more" footer marks the clip; column narrowed 340 to 300 px. Decks and mixer are still narrower than in a normal run. |
| 12 | Banner / header | fixed | Banner is taller, has padding, primary text colour and a bottom border; the header strip has an "OpenMixer" title and a border. |
| 13 | Crossfader / cue bars had no handle | fixed | Thumb from `LookAndFeel` fader is drawn on the track (its offset was doubled before, which put it off the bar). |

## Wave 7 stem pads (`main-stems`, 2026-10-19)

Scene: `tools/screenshots.sh main-stems` runs `--demo-library --mock-stems`: deck A's track is separated by the mock separator and its stems are loaded, so the pads are live. Deck B has no track. Three capture iterations; `main-dark` and `main-daylight` were recaptured because every deck card gained the stems row.

| Found in the first capture | Status | What changed |
|---|---|---|
| Pads about 55 px wide: "Drums" and "Vocals" clipped, the solo button ate the name | fixed | The stems row spans the whole card width under the pitch column, so four pads get about 78 px each. |
| Greyed deck B: "no stems - Prepare stems" wrapped to three lines in the control column | fixed | The greyed state is one caption across the pads; the control column holds only the Load stems button. |
| Zoomed waveform hidden at 900x600 (the fixed rows left 8 px) | fixed | Deck rows take 0.79 of the window (was 0.68) and the stems row is 83 px. The `deck_card` test asserts the zoom is shown at 900x600, 1280x800, 1024x640 and 1920x1080. The library band is shorter at 900x600 as a result. |

Remaining observations (not fixed):
- The volume slider is about 45 px tall (the pad body). It is usable, but short; a taller stems row would need the zoom to shrink again.
- The activity glow is a translucent fill over the mute header, subtle at low levels in the mock (sine tones at 0.25 amplitude).
- Not verified by ear: the mock device plays nothing, so stem mixing is checked through the engine tests only.

## Sampler strip (Wave 7 P4, 2026-10-20)

The sampler strip sits in the library band, right of the library, in every main-window scene. `--demo-library` loads eight generated samples so the pads are full. Two captures were checked by eye (`sampler-dark.png` and `sampler-daylight.png` beside this file): pad names, the caption, the mode and gain/pitch controls all fit and read at 1280x800, the selected pad carries the accent border, and daylight contrast is fine.

| Item | Status | Notes |
|------|--------|-------|
| Overlaps or clipping in the strip | none seen | Both captures show every pad, the caption and all six controls inside the strip. The layout test checks the same at four window sizes. |
| `main-dark` and `main-daylight` predate the strip | not recaptured | `tools/screenshots.sh` takes at least two captures per scene; regenerate those two scenes to refresh them. |
| Lit (playing) pad and gate hold | not captured | A capture needs a pad pressed on screen; the press path is covered by the UI gate. |

## Automix bar (Wave 7, `main-automix`, 2026-10-20)

Scene: `tools/screenshots.sh main-automix` runs `--demo-automix`: three generated 24 s click tracks at 120, 122 and 124 BPM (broadband hits, so the BPM analysis finds a grid), all analysed before deck A starts playing on the mock device (silent). Automix is on in memory only (the demo's preferences file is temporary). The capture waits 19 s after the window opens (`delays` in the script). Two captures were checked by eye: 14 s (state Armed: "Next: Click 122 · 122.0 BPM · 1B", "18 beats left", Start now and Cancel lit) and 19 s (state Transitioning: progress bar about 40 %, "8 beats left", deck B playing, crossfader moving). `main-automix.png` is the 19 s capture.

| Found | Status | What changed / why |
|---|---|---|
| Bar stayed "Idle" with no grid although the analysis had landed | fixed | A deck loaded before its analysis finished kept a grid-less `DeckInfo` in the controller's cache, so it never armed. The cache now keeps only analysed decks (`controller/state.rs`, test `a_grid_that_lands_after_the_load_is_read_on_a_later_tick`). |
| "no track in the queue" error and the track dropped | fixed in the demo | The queue picked before the other tracks were analysed (`NotAnalysed`), so the run failed and that deck was suppressed. The demo now waits for every track to have a grid before deck A plays. The product gap is still there (see the agents.md notes). |
| Bar and Settings button in one strip | by design | The bar takes the header strip left of the 96 px Settings button, so the deck rows keep their height (a separate strip took 36 px and failed the 900x600 deck-zoom test). |
| Bar at narrow widths | not verified | The script captures 1280x800 only. At 900 px the "Next" text gets about 80 px and truncates; the fixed parts are covered by the layout test only, not by a picture. |
| "Skip" button | not offered | The controller has start and cancel but no skip-to-next command. Cancel drops the plan and keeps the loaded track. |
| Automix label and "Idle" text | low contrast | They use `text_muted` on the header surface. Readable in the capture; not tightened. |

## Remaining observations
- Settings Library page: the watch-folder path runs right up to its Remove button.
- Settings cards other than Appearance keep a fixed 540 px height with some empty space at the bottom.
- Waveforms are only as detailed as the 2 s sine demo tracks allow.

## Not captured
Settings Plugins tab; anything with real audio or hardware (never used).

## Recording (Wave 8, `main-record` and `settings-recording`, 2026-10-20)

Scenes: `tools/screenshots.sh main-record` runs `--demo-record`: the demo library, then a recording started at once into a scratch folder (`--demo-library` scratch dir, never the music folder), on the mock device (silent). The capture waits 4 s. `settings-recording` opens the Recording tab (`--settings=recording`). Two captures, checked by eye.

| Item | Status | Notes |
|---|---|---|
| Record button while recording (`main-record.png`) | fits, no overlap | Red outline and dot, `00:05`, `1.6 MB`, the L/R peak tracks, between the automix bar's Start now / Cancel chips and the Settings button. Peaks are flat because the mock device is silent. |
| Record button while idle | not captured | Drawn as an outline with the label `Record`; the capture budget was spent on the recording state. |
| Recording settings tab (`settings-recording.png`) | fits | Folder (Change folder / Use default, the path field opens below), the three format chips, the low-disk toggle and the status line. Nothing overlaps at 1280x800. |
| Dropped-frames badge | not captured | Shown as `!N` in the button when frames were dropped; the mock device drops none. |
| Narrow windows (900x600) | layout test only | The header strip and the cells are checked by `the_cells_clear_the_header_strip_and_each_other_at_the_test_sizes` (om-ui) and the button's rect test in `record_bar.rs`. Not looked at in a picture. |

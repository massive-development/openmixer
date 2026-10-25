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

## Mapping (Todo 36, `settings-mapping`, 2026-10-20)

Scene: `tools/screenshots.sh settings-mapping` runs `--demo-map --settings=mapping` on the mock device. The three fixture controller maps are merged into a temporary map file (`<scratch>/map/demo-map.json`), never the user's map. One capture, checked by eye.

| Item | Status | Notes |
|---|---|---|
| Mapping tab (`settings-mapping.png`) | fits, no overlap | Tab chips, the toolbar (Add to Reset), the map path line, the filter chips, the binding table with the selected-row highlight and the `!` conflict mark, the "Select a binding" hint, and the conflicts row with Keep first / Keep second. Nothing overlaps at 1280x800. |
| Below the fold | scrolls, not visible in the capture | The edit form, the Apply and Learn buttons, the monitor panel and the status line sit below the table inside the scrolling page. The capture stops at the conflicts row. |
| Narrow window (900x600) | not captured | The card is 720 px wide and shrinks to the window; the page scrolls. Not looked at in a picture. |
| Confirm steps (Reset, close with unsaved edits) | not captured | Covered by state tests in `settings_view/mapping.rs`. |

## Staged-state scenes (redesign states never seen on screen, 2026-10-25)

These scenes use `--demo-scene=NAME` (DEMO / TEST ONLY: `flags::DemoScene`, `demo_scene.rs`, `SceneOptions` in `app.rs`). They stage state the other demo flags cannot: a drawer, a tab, a pad press, cues, a loop, playback, a hot trim. All run on `--mock-device`, the scratch library and temporary preferences. The mock device still renders the engine into memory, so a playing deck lights the meters even though nothing is audible. The script writes to `$OUT_DIR` (default `/tmp/om-scenes`, never `docs/screenshots` unless `OUT_DIR=docs/screenshots` is set) and captures the newest window that appeared after it started its own process (checked with a second 1280x800 window open). PNGs are not committed; regenerate with the commands below. Captures reviewed from `/tmp/claude-1000/-home-brandon-projects-openmixer/f729f710-31a7-4d1e-a553-72f1f7b40305/scratchpad/scene-out/`.

| Scene (dark and daylight unless noted) | Run | Shows |
|---|---|---|
| `sampler-open-dark`, `-daylight` | `tools/screenshots.sh sampler-open-dark` | Drawer open, 8 pads loaded, pad 8 Bass (a latching loop pad) playing, pad 1 selected |
| `cues-loop-dark`, `-daylight` | `tools/screenshots.sh cues-loop-dark` | Deck A: hot cues 1 to 5 set (6 to 8 empty), playing from cue 3's area, a one-beat loop on the grid, LOOP chip and loop edges on the waveforms |
| `meters-lit-dark`, `-daylight` | `tools/screenshots.sh meters-lit-dark` | Deck A looped over the whole track at trim 4.5 (demo tracks peak at 0.25): channel and master meters lit, clip lamps latched |
| `stems-page` (dark) | `tools/screenshots.sh stems-page` | Deck A on its STEMS tab, mock separator's stems loaded |
| `track-preview` (dark) | `OPENMIXER_DEMO_TRACK=/path/to/song.flac tools/screenshots.sh track-preview` | Deck A plays your file from 25 percent; waits 15 s (`SHOT_DELAY=N` to change) for analysis and the waveform. The file is symlinked into the scratch folder (never copied) and shows as `0-demo-track` in the library; untagged files show no artist |

Not made: the platter scene (d). This branch has no platter (it is in the deck-controls work, `platter_paint.rs` on the integration branch). Add `--demo-scene` handling for it when that merges: the deck-side plan (`deck_plan`, playing on a loop) already fits, the toggle is one more `SceneOptions` field.

### Defects seen (judged against `docs/design/research/*.md`)

Not fixed: each is outside the files this change touches (`app.rs`, `drag.rs` test helper, `gui.rs`, `flags.rs`, `library_smoke.rs`, `demo_scene.rs`, the script) or is a design choice for the user. Image names are in the scene folder above.

| # | Where (image) | Defect | Research |
|---|---|---|---|
| 1 | Zoomed lane, `cues-loop-dark/daylight` | The loop band is painted under the waveform in `waveform_paint.rs::paint_zoom`, so over the waveform it is invisible; only the two 2 px edge lines show. No hatch or top bar. The overview lane does show the tint. Likely fix: paint the fill after the waveform paths | waveform.md Sizes ("loop as a 0.18-alpha band") and Markers ("loops should use a distinct hatch or a top bar") |
| 2 | Pads 1 to 5, `cues-loop-dark` | No ACTIVE pad state: the pad the deck plays from looks like the other set pads (no glow). Set pads have no name and no top tab; empty pads read "1".."8", not "Set", with a solid, not dashed, border | deck-controls.md States ("Active: full fill plus glow", "Empty pad: dashed border, Set") |
| 3 | Waveforms, all `*-daylight` | Band colours are the dark-theme ones. The high band (near-white `#F5F7FA`) is about 1:1 on the light lane, so the right third of the demo sweep vanishes (outline only) | waveform.md Colour stops: daylight stops and at least 4.5:1 on a light strip |
| 4 | Zoom lane bar numbers, `cues-loop-daylight`, `sampler-open-daylight` | Bar numbers ("2", "3") are white at 0.85 alpha on the light unplayed lane, about 1.2:1 (measured). Readable only on the dark played side | waveform.md Markers; labels-layout.md item 5 (4.5:1 text) |
| 5 | Empty deck B lane, `*-daylight` | The played-dimming overlay is drawn on a lane with no track: a heavy slate block over the left half. In dark it is black and goes unnoticed | waveform.md (played at 0.45 brightness applies to a waveform) |
| 6 | Channel GAIN knob, `meters-lit-dark` | Muted label on the lit fill: about 2.9:1 (measured, label pixel against fill). Daylight is fine (4.8:1). The value is not shown | labels-layout.md item 5 (4.5:1, 3:1 large); knobs.md |
| 7 | Meters, all `meters-lit-*` | Continuous bars, not segmented LEDs (the `meter_segment` tokens are unused). No numeric peak readout under the master. Clip lamp works (red, latched, both sides) | faders-meters.md Meters; section 1 notes the unused tokens |
| 8 | Fader scale, `meters-lit-dark` | The "0 -6 -12 -24 -48" labels belong to the meter but sit between the fader and the meter; on the master "-12" and "-24" nearly touch the fader well (about 3 px). The channel fader has no ticks of its own | faders-meters.md Channel fader (ticks beside the slot, unity marked) |
| 9 | Deck B with no track, all scenes | Transport, pads and loop row look enabled (full contrast, hover) with no track loaded | deck-controls.md States ("Disabled (no track): 40 percent opacity, no hover") |
| 10 | EQ HI/MID/LOW, channel strips | Tall empty wells with the label on top; no centre mark or fill at 0 dB, so the neutral state reads as a blank box | knobs.md (rotary, 270 degree sweep, neutral marked) |
| 11 | Stems page, `stems-page` | Each stem row has two unlabelled sliders (volume and filter?). All four stem pads share the accent colour, so stems are told apart by text only | deck-controls.md States ("never rely on colour alone" holds for text, but two controls without labels do not read) |
| 12 | Library sidebar, `*-daylight` | Selected row ("All tracks") is white, unselected rows are grey blocks, so the selected row looks like the blank one | labels-layout.md item 5 (state must be clear) |
| 13 | Header, deck A, `meters-lit-*` | "LOOP 4" chip sits directly under the artist line with no gap; remaining time is red for a 2 s demo track from the start (the end-of-track warning, expected for the demo) | labels-layout.md spacing note (low severity) |
| 14 | Sampler strip, `sampler-open-*` | The caption above the pads repeats the selected pad's label ("1 Kick" twice); pad labels such as "5 Tone A" fill the pad width with about 4 px to spare | labels-layout.md (low severity) |

Fine in the captures: sampler playing pad is lit and readable in both themes; hot cue colours and numbers read on dark and daylight; the clip lamp is red and visible in both; stems tab layout has no clipping or overlap; the header and Settings/PERFORM/Record chips do not collide.

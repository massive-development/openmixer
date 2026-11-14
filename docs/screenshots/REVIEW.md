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

Superseded by the redesign: the stem pads now live on the deck's STEMS tab (`stems-page.png`). `main-stems.png` is the main window with deck A's stems loaded (library column "Ready"), so it looks like `main-dark.png`. The notes below describe the pre-redesign card.

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

The sampler strip sits in the library band, right of the library, in every main-window scene. `--demo-library` loads eight generated samples so the pads are full. Two captures were checked by eye (`sampler-dark.png` and `sampler-daylight.png`, recaptured on 2026-10-27 as `sampler-open-dark.png` and `sampler-open-daylight.png`, beside this file): pad names, the caption, the mode and gain/pitch controls all fit and read at 1280x800, the selected pad carries the accent border, and daylight contrast is fine.

| Item | Status | Notes |
|------|--------|-------|
| Overlaps or clipping in the strip | none seen | Both captures show every pad, the caption and all six controls inside the strip. The layout test checks the same at four window sizes. |
| `main-dark` and `main-daylight` predate the strip | fixed | Recaptured 2026-10-27 on the redesigned UI; the sampler is the collapsed SAMPLER tab at the right of the library band, and `sampler-open-*` shows it open. |
| Lit (playing) pad and gate hold | not captured | A capture needs a pad pressed on screen; the press path is covered by the UI gate. |

## Automix bar (Wave 7, `main-automix`, 2026-10-20)

Scene: `tools/screenshots.sh main-automix` runs `--demo-automix`: three generated 24 s click tracks at 120, 122 and 124 BPM (broadband hits, so the BPM analysis finds a grid), all analysed before deck A starts playing on the mock device (silent). Automix is on in memory only (the demo's preferences file is temporary). The capture waits 19 s after the window opens (`delays` in the script). Two captures were checked by eye: 14 s (state Armed: "Next: Click 122 · 122.0 BPM · 1B", "18 beats left", Start now and Cancel lit) and 19 s (state Transitioning: progress bar about 40 %, "8 beats left", deck B playing, crossfader moving). `main-automix.png` is the 19 s capture.

| Found | Status | What changed / why |
|---|---|---|
| Bar stayed "Idle" with no grid although the analysis had landed | fixed | A deck loaded before its analysis finished kept a grid-less `DeckInfo` in the controller's cache, so it never armed. The cache now keeps only analysed decks (`controller/state.rs`, test `a_grid_that_lands_after_the_load_is_read_on_a_later_tick`). |
| "no track in the queue" error and the track dropped | fixed in the demo | The queue picked before the other tracks were analysed (`NotAnalysed`), so the run failed and that deck was suppressed. The demo now waits for every track to have a grid before deck A plays. The product gap is still there (see the AGENTS.md notes). |
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
| Narrow windows (900x600) | layout test only | The header strip is checked by the header layout tests (`om-ui` `header_layout.rs`, "parts do not overlap") and the button's rect test in `record_bar.rs`. (This line once named a test, `the_cells_clear_the_header_strip_and_each_other_at_the_test_sizes`, that is not in the tree.) Not looked at in a picture. |

## Mapping (Todo 36, `settings-mapping`, 2026-10-20)

Scene: `tools/screenshots.sh settings-mapping` runs `--demo-map --settings=mapping` on the mock device. The three fixture controller maps are merged into a temporary map file (`<scratch>/map/demo-map.json`), never the user's map. One capture, checked by eye.

| Item | Status | Notes |
|---|---|---|
| Mapping tab (`settings-mapping.png`) | fits, no overlap | Tab chips, the toolbar (Add to Reset), the map path line, the filter chips, the binding table with the selected-row highlight and the `!` conflict mark, the "Select a binding" hint, and the conflicts row with Keep first / Keep second. Nothing overlaps at 1280x800. |
| Below the fold | scrolls, not visible in the capture | The edit form, the Apply and Learn buttons, the monitor panel and the status line sit below the table inside the scrolling page. The capture stops at the conflicts row. |
| Narrow window (900x600) | not captured | The card is 720 px wide and shrinks to the window; the page scrolls. Not looked at in a picture. |
| Confirm steps (Reset, close with unsaved edits) | not captured | Covered by state tests in `settings_view/mapping.rs`. |

## Staged-state scenes (states never seen on screen, recaptured on the merged redesign, 2026-10-25)

These scenes use `--demo-scene=NAME` (DEMO / TEST ONLY: `flags::DemoScene`, `demo_scene.rs`, `SceneOptions` in `app.rs`). They stage state the other demo flags cannot: a drawer, a tab, the platter, a pad press, cues, a loop, playback, a hot trim. All run on `--mock-device`, the scratch library and temporary preferences. The mock device still renders the engine into memory, so a playing deck lights the meters though nothing is audible. The script writes to `$OUT_DIR` (default `/tmp/om-scenes`, never `docs/screenshots` unless `OUT_DIR=docs/screenshots` is set) and captures the newest window that appeared after it started its own process (checked with a second 1280x800 window open). PNGs are not committed. Captures reviewed from the merged build (PR #115 base), folder `/tmp/claude-1000/-home-brandon-projects-openmixer/f729f710-31a7-4d1e-a553-72f1f7b40305/scratchpad/scene-new/`.

| Scene | Run | Shows |
|---|---|---|
| `sampler-open-dark`, `-daylight` | `tools/screenshots.sh sampler-open-dark` | Drawer open, 8 pads loaded, pad 8 Bass (a latching loop pad) playing, pad 1 selected |
| `cues-loop-dark`, `-daylight` | `tools/screenshots.sh cues-loop-dark` | Deck A: cues 1 to 5 set, a short loop starting on cue 3 (shorter than the 0.25 s active window) so pad 3 is steadily ACTIVE, loop hatch on the waveforms |
| `meters-lit-dark`, `-daylight` | `tools/screenshots.sh meters-lit-dark` | Deck A looped over the whole track at trim 4.5: segmented channel and master meters lit, clip lamps latched |
| `platter-dark`, `-daylight` | `tools/screenshots.sh platter-dark` | Deck A playing from 8 s with the platter on: disc, deck letter, position ring, marker |
| `stems-page` (dark) | `tools/screenshots.sh stems-page` | Deck A on its STEMS tab, mock separator's stems loaded |
| `track-preview` (dark) | `OPENMIXER_DEMO_TRACK=/path/to/song.flac tools/screenshots.sh track-preview` | Deck A plays your file from 25 percent; waits 15 s (`SHOT_DELAY=N`) for analysis and the waveform. Symlinked into the scratch folder, never copied; shows as `0-demo-track`, untagged files show no artist |

### Fixed while capturing
- Lit transport labels (Pause, Cue, Sync) used near-black ink on every fill. On the daylight theme Play's green is dark, so "Pause" measured 2.5:1 (`cues-loop-daylight`, first capture). The ink is now the better of the candidates, as `text_on` already does for pads (`deck_view.rs::transport_style`, test `lit_transport_labels_read_at_4_5_to_1_in_both_themes`). Cites labels-layout.md item 5 (4.5:1). After the fix the label is white and readable.

### Seen in the earlier (pre-merge) captures and gone now
Loop band under the waveform (now a visible green hatch with edge lines, `cues-loop-*`), no active pad state, no "Set" labels, unsegmented meters, no fader ticks, unlabelled GAIN knob contrast, daylight waveform and bar-number contrast (the lanes are dark wells in both themes), empty deck B lane overlay. None reproduces; the earlier table is dropped.

### Defects in the recaptured scenes
Not fixed: each is a design call or sits in code outside this change.

| # | Where (image in the folder above) | Defect | Research |
|---|---|---|---|
| 1 | Stems page, `stems-page.png` | Each stem row has two sliders with no label (volume and filter by position only). All four stem pads use the one accent colour | deck-controls.md Layout ("stems pads as 4 labelled faders") |


| 2 | Deck B with no track, all scenes | Play, Cue, Sync and the loop buttons are dimmed, but the eight "Set" pads and the loop pill stay at full contrast, so B reads half enabled | deck-controls.md States ("Disabled (no track): 40 percent opacity, no hover") |
| 3 | Master meters, `meters-lit-dark.png` | The dB scale (0, -12, -24, -48) sits about 3 px from the bars and the knob; no numeric peak readout under the master | faders-meters.md Meters ("numeric peak readout under master") |
| 4 | Library sidebar, `*-daylight.png` | The selected row ("All tracks") is white and the other rows are grey blocks, so selection reads inverted | labels-layout.md item 5 (state clear without relying on tone alone) |
| 5 | Active cue pad, `cues-loop-dark.png`, `-daylight.png` | Pad 3 (ACTIVE) differs from set pads only by full opacity and a faint glow; at a glance it is hard to tell from pads 1, 2, 4 | deck-controls.md States ("full fill plus glow"); the window length is marked unsourced in `deck_view.rs` (`ACTIVE_WINDOW_S`) |
| 6 | Play button, all scenes | The word "Pause" fills the 44 px circle edge to edge (about 2 px spare) | deck-controls.md Buttons (56 px circle; the build uses 44) |
| 7 | Loop pill, `cues-loop-*.png` | An odd length shows "0.4" in the pill and header chip (a demo value, not a UI bug; real loops snap to beats) | none |
| 8 | Sampler strip, `sampler-open-*.png` | Caption above the pads repeats the selected pad's label ("1 Kick" twice); "5 Tone A" fills its pad with about 4 px to spare | labels-layout.md (low severity) |

Status of the defects above (polish pass, branch `feat/ui-redesign-polish`): 1 FIXED for the labels ("Volume" and "Filter" inside each track, the filter's detent starts under the caption); the one shared accent colour stays, because no research source gives per-stem colours. 2 FIXED (pads, length chip and Loop switch dim to 40 % with no track, no hover). 3 FIXED for the scale (dB marks end 6 px from the bars, was 3; the master fader is 8 px from its meter, was 6; `meters-lit-dark` checked, and the channel meters get the same gap). The held-peak readout under the master (`0.0`, `-10.1`) was already drawn and shows in the capture. 4 FIXED (tinted fill plus a 3 px focus-colour bar; indicator 3:1, text 4.5:1, tested in both themes; bar width and 18 % tint are unsourced and small). 5 FIXED (play glyph in the label and an ink outline on the active pad; the 0.25 s window stays unsourced). 6 FIXED (a squeezed round button uses the 12 px floor; the word is unchanged; research asks for icon + word at 56 px, which does not fit 44 px). 7 NOT FIXED (demo value). 8 caption FIXED (shows mode, pitch and gain); a loaded pad now shows its key over its name (`5` / `Tone A`), so "Tone A" has about 8 px to spare each side at 1280x800 (was about 4 px on one line); `sampler-open-dark` checked. 6 is re-checked in `meters-lit-dark` and `cues-loop-dark`: "Pause" sits inside the 44 px circle with a clear rim (the 12 px floor, already on this branch).

Checked and fine: platter on in both themes (disc, A label, position ring, marker, Platter switch lit, tabs unlit while it is open); playing sampler pad lit and readable in both themes; segmented meters, clip lamps and fader ticks in both themes; hot cue fills and the dashed "Set" pads in both themes; the stems tab layout (no clipping or overlap); the loop hatch over the waveform; the header (OPENMIXER, Automix, Notice chip, BPM, Record, Library, Settings) with no collisions; `track-preview` with a 24 s generated file (grid 124 BPM, waveform, meters).

## Recapture of the tracked set (2026-10-27)

All 21 scenes of `tools/screenshots.sh` were recaptured from HEAD with `OUT_DIR=docs/screenshots`. Why: the tracked `main-dark.png` (commit 38e00df, "refresh dark screenshot") still showed the pre-redesign window (boxed GAIN/EQ/FILTER sliders, old header), and `main-daylight.png` dated from 2026-10-23. The staged scenes listed above had never been committed. The knobs had not regressed: the rotary knobs for GAIN, HIGH/MID/LOW, FILTER and headphones MIX/VOL are in every main-window capture.

Looked at by eye: `main-dark`, `main-daylight`, `main-stems`, `meters-lit-dark`, `sampler-open-dark`. The other captures were taken but not inspected.

- `sampler-dark.png` and `sampler-daylight.png` are removed; `sampler-open-*` replace them.
- `main-dark`, `main-daylight` and `main-stems` show empty waveforms: the capture is taken before the analysis finishes. `tools/screenshots.sh main-waveform` waits for it.
- `main-stems` and `main-dark` look alike; see `stems-page` for the stems UI.

## Faceplate redesign (Phase 6, 2026-11-09)

How: no display was available, so the real main window (`Shell` over the real engine, the mock device, the generated demo library, deck A on the Cues scene, deck B loaded) was rendered offscreen with gpui's headless wgpu renderer on lavapipe, at 2x scale, by a scratch harness that is not in the repo. Matrix: dark, daylight and harbour x Roomy and Compact x 1280x720, 1440x900 and 1920x1080 (18 captures), plus the Loop and FX tabs (dark Roomy 1440x900, daylight Compact 1280x720), the Stems tab with the platter (dark Compact 1280x720), the library hidden (harbour Compact 1280x720), the sampler open (dark, Automatic, 1920x1080), the Appearance settings and the debug panel, an empty library with no deck loaded (dark Roomy 1440x900, daylight Compact 1280x720) and a library row whose file went missing (dark Compact and daylight Roomy, 1440x900). Each was looked at against `openmixer-workbench/docs/design-system/Design System.dc.html` sections 4a, 4b, Light mode and States (read as text; no browser was available to render the canvas).

**The PNGs in this folder were not refreshed in Phase 6** (they were on 2026-11-12, see the last section). They come from `tools/screenshots.sh`, which captures the binary's window on X11 with its own demo flags (`--demo-library`, `--demo-scene=…`, `--demo-automix`, `--demo-record`, `--settings=…`); the headless harness sets up a similar but not identical state and cannot run those paths, so its images would not be what the script produces. The tracked captures therefore still show the E1 window until `tools/screenshots.sh` is run with `OUT_DIR=docs/screenshots` on a display. The README screenshot is unchanged for the same reason.

| # | Found | Status | What changed / why not |
|---|---|---|---|
| 1 | The automix Off/On switch, the settings chips (theme, density, MIDI and others), the Appearance "current" marker and the sampler's selected pad lit in deck A's amber, though none of them names a deck | fixed | `LookAndFeel::accent` and the settings chip accent are the text colour; deck controls pass their own hue. Tests: `chrome_that_names_no_deck_lights_in_the_text_colour`, `a_lit_chip_is_neutral_never_a_deck_hue`. |
| 2 | Roomy forced at 1280x720: the library band was about 92 px with no rows, the sidebar cut through "Prep queue" and the SAMPLER handle letters spilled above and below the band | fixed | An explicit Roomy gives way to Compact where the library would not keep `LIBRARY_MIN_ROWS` rows; the Appearance note says so. Test: `every_density_choice_keeps_the_library_rows_from_the_minimum_window_up` (every choice, 1280/1440/1920 wide, 720 to 1200 tall). The handle's letters are also clipped to the handle now. |
| 3 | Library numerals (BPM, key, length) sat about 3 px above the text cells | fixed | Mono line height 1 in a top-aligned cell; numeral cells take the body line box. |
| 4 | The sidebar selection bar touched the first letter ("\|All tracks") | fixed | The bar was placed from the label's own box; the label is inset 11 px and the bar drawn at the row edge. |
| 5 | FX tab, Compact: "Feedback" ran into the Tone knob | fixed | FX knob cells widen for long parameter names (`fx_knob_text_width`, tested). |
| 6 | Empty deck: "-0:00.0 Remaining" in the deck hue | fixed | Reads "--", as BPM and key do (`header_shows_track_info_and_effective_bpm`). |
| 7 | Empty library says "No tracks match" with no search typed | not fixed | Copy for a first run ("add a watch folder in Settings") is a wording decision; the dashed disabled deck controls and empty pads read correctly. |
| 8 | Error row | none | The missing-file row is dimmed with a dashed edge and says "file is missing"; the status line says "Couldn't load smoke_2.wav: file is missing". No red. Matches the States "Row, error". |
| 9 | Loop tab: all eight loop sizes draw as "set" pads (tinted), so the active length does not stand out from its neighbours as in States "Loop active" | not fixed | The demo loop is 0.4 beats, which is none of the sizes, so no pad should be on; whether the sizes should rest instead of reading as set is a design call. |
| 10 | Compact deck controls leave an empty strip under the options row (the mixer column, 225 px, sets the row height) | not fixed | Known from Phase 5; filling it is a layout decision. |
| 11 | A library row whose key is compatible with the reference gets the `master` tint, which in daylight is a mid grey that can read as "selected" | not fixed | The compatibility tint takes `master`, which daylight sets to a neutral (unsourced). The selected row itself is slate as specified. |
| 12 | Sampler pads in the harness are empty (no demo samples), so lit and playing pads were not seen | not checked | The harness does not attach the sample bank; `tools/screenshots.sh sampler-open-*` does. |

Looked at, no defect beyond the above: the header (wordmark, latency, LUFS, Record, LIBRARY, Settings), both deck headers with their letters, the master tempo, the two waveforms (Low-Band style, signal-red playhead, cue flags with numbers, loop region, bar numbers), the mixer column, the crossfader with its A and B tags, the hot cue pads, the debug panel, and the Appearance card in all three themes where captured. Daylight and harbour hold the same structure as dark. Deck letters sit beside every deck hue (tags, crossfader, library rows), LOCK and CLIP are words (deck view and meter), and the error row states its reason in words.

Not verified: a live window on a real display (hover, press, keyboard focus rings as they appear, drag), real music in the waveforms and the library, frame times, and the `tools/screenshots.sh` scene set.

## Recapture on the faceplate (2026-11-12)

All 21 scenes of `tools/screenshots.sh` were recaptured from the real window (debug build, X11 on WSLg, 1280x800, `--mock-device`) and copied here, so the tracked captures and the README now show the Machined Faceplate.

- A demo run on `--mock-device` no longer shows the "Demo mode: no audio output" notice; it covered deck B's header in every capture. `--mock-device` without a demo flag still shows it.
- The `--demo-library` scenes (main, debug panel, settings tabs) now wait 8 s, so deck A's waveform, BPM and key are drawn. `main-waveform` is now the same picture as `main-dark`.
- The MIDI, Library and Mapping tabs print temp paths (`/tmp/om-shots.*`, `/tmp/openmixer-smoke-*`). They follow `TMPDIR`: capture with a neutral one, or a home folder name ends up in the picture.

Looked at by eye, all 21: no notice, no personal path, no overlap or clipping seen. Dark and daylight hold the same structure; cue flags, the loop region, lit meters with CLIP, the platter, the stems tab, the open sampler, the automix bar mid-transition and the recording timer all show.

Still wrong or open:
- Deck B is empty in every scene but `main-automix`, so half the window shows dashed, disabled controls.
- `main-dark` sits at the track start: the left half of the zoomed waveform is empty.
- `meters-lit-*`: the two CLIP words above the channel meters overlap each other ("CLIPLIP").
- `main-automix`: the "beats left" text is cut ("8 beats le") beside Skip and Cancel.
- `debug-panel`: deck A's title and artist are squeezed to "s…" and "Si…".
- `settings-library`: the stem model sentence runs past the card's right edge ("... were added. Load").
- No harbour scene exists in the script. Demo tracks are still synthetic.

## Fix pass on the recapture's defects (2026-11-12)

Five scenes recaptured and looked at after the fixes: `main-dark`, `meters-lit-dark`, `main-automix`, `debug-panel`, `settings-library`. The first four are replaced here. `main-automix.png` and the other 16 tracked captures predate this pass: they still show "8 beats le" and an empty deck B.

| Defect | Status | Cause and change |
|---|---|---|
| Two CLIP words overlapped ("CLIPLIP") | fixed | Each 7 px meter drew its own word (about 24 px) and the channel A and master meters sit 14 px apart. The mixer now draws one word in the gap between channel A's fader and the master fader (`ColumnLayout::clip_word`), lit while any of the three meters holds a clip; each meter's red top segment says which. A lone meter (the gallery) keeps its own word. |
| "8 beats le" in the automix bar | fixed | `REMAINING_WIDTH` was 72 px; the 13 px mono numeral needs 6.5 px a letter, so "128 beats left" needs 91. Now 92, with a test. |
| Debug panel squeezed deck A's title to "s…" | fixed | A deck head whose title would get under 96 px gives up the (often empty) cover slot and closes the readout gaps. With the debug column open the titles read "smoke_0" / "Sine Collective". |
| Stem model sentence ran past the Library card | fixed | A settings row's content sat in a flex row with no width cap, so text took its full width instead of wrapping. The content is capped at the row's width. |
| Deck B empty; `main-dark` at the track start | fixed | A demo run loads the second generated track (now a 126 BPM loop, not a 2 s sweep) into deck B and parks decks A and B at 16 s and 8 s. Scenes still place deck A themselves; the automix demo is unchanged. |

Still wrong or open:
- `main-automix`: during a transition at 1280 px the next-track name gets about 50 px ("Next: Clic", a hard cut: an ellipsis left only "Next…"); the header gives the bar a fixed strip.
- `settings-library`: the wrapped sentence pushes "Clear stem cache…" to the card's bottom edge; the page scrolls.
- `debug-panel`: deck B's artist still truncates ("Test Tone O…").
- The 17 tracked captures not retaken; no harbour scene; synthetic demo tracks.


## Design-system polish pass (2026-11-12)

A four-part review (captures by eye, colour and states, type and spacing, components against the canvas) listed the gaps; this pass fixes the ones that need no owner decision. Looked at after the fixes: `main-automix`, `main-record`, `sampler-open-dark`, `main-dark`, `cues-loop-dark`, `settings-library`. `main-dark`, `cues-loop-dark` and `settings-library` are replaced here; the other tracked captures predate the pass.

Fixed:
- Automix Start, Skip, Cancel, the Off/On switch and the transition chip, the Record button, the sampler pads and the EQ kill button are built from the shared faces: hover, pressed, the keyboard focus ring. The transition chip is one line; the recording readout fits its box; the progress bar keeps the 3 px radius.
- A selected library row keeps its loaded-on-deck edge or its unplayable edge; the empty table's line is in the body tier and muted.
- Bar numbers and the loop caption sit on a plate of the well, in separate lanes, bar numbers in the secondary text colour.
- The FX drawer lights in its scope's channel hue, and in the text colour on the master scope.
- Numbers inside phrases are mono: settings chips ("48000 Hz", "5 min"), the library count, the automix next-track line.
- Settings chips are the density's control height on scale spacing; a lone On chip is an Off | On switch.
- The demo no longer forces a 120 BPM grid on deck A, so the deck head and the library agree (124.0).
- Loop halve and double read ½ and 2× (canvas States, "Loop active").
- Dead pre-faceplate sizes and colours are deleted from `widgets/deck_header.rs` and `widgets/hotcues.rs`.

Open, each needs an owner decision:
- Red above -3 dB in the meters and on the LUFS readout (DESIGN.md prose says playhead, record and clip only; the canvas Meter row says red above -3 dB).
- The 16 px Roomy deck title in the canvas; the code draws 13 px in both densities.
- Selection uses deck B's slate, as the spec says.
- Daylight control edges at about 1.5:1 on the panel.
- The selected-deck outline in DESIGN.md, and the splash loader canvas: neither is built.

Open, not started:
- The library toolbar is cut, not reflowed, when the sampler or debug column is open.
- The FX panel, sampler strip and loudness panel lay out with fixed sizes in both densities.
- No bar or beat counter in the deck head; no "L R" caption under the master meter; no caption on the master fader.
- The settings card changes height between tabs.
- Legacy theme tokens (`library_*`, `disabledAlpha`, `translucency`) are still read from the JSON.

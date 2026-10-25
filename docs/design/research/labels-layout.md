# Labels, typography and page layout: research and spec

Status: research, no code changed (2026-10-24). Inputs: `docs/design/E1-brand.md`, `docs/PRODUCT.md`, `docs/screenshots/main-dark.png`, `crates/om-ui` and `crates/om-ui-gpui` sources.

Caveat 1: `main-dark.png` (committed 2026-10-23) is older than the source. It shows "Trim/Hi/Mid/Lo/Kill/Mstr/Qnt/Exit/Reloop"; the code now says GAIN/HI/MID/LOW, a MORE drawer and "Master/Quantize". Regenerate the screenshot before the next critique round. Items below are marked **[shot]** (only in the old screenshot) or **[code]** (in current source).

Caveat 2: web search returned mostly controller PDFs and table-of-contents pages. Layout conventions marked *(convention)* come from working knowledge of the four products and are not verified by a fetched page; cited facts are listed in Sources.

## 1. Label inventory

Rule: spell a word out when there is room; abbreviate only with the established DJ abbreviation (BPM, EQ, PFL is the exception, see below). Never show router ids.

| Where | Now | Problem | Replace with |
|---|---|---|---|
| Channel header [shot] | `Ch 1` | abbreviation, dim, tiny | `CH 1` caption, or the deck name `DECK 1` in the deck accent |
| Channel gain [shot/code] | `Trim` / `GAIN` (`TRIM_LABEL`) | two names for one knob | `GAIN` (matches Serato, Traktor, Pioneer) |
| EQ [shot/code] | `Hi Mid Lo` / `HI MID LOW` | inconsistent case and `Lo`/`LOW` | `HIGH MID LOW`; keep label-as-kill but add a tooltip "Kill band" |
| Kill row [shot] | `Kill` x3 | repeated word | drop the row; label press = kill, lit when killed |
| Filter [shot/code] | `Filter / Off` | "Off" reads as a button | `FILTER` with centre detent mark; value `Off` becomes `0` or blank |
| PFL [code] | `PFL` | jargon | `CUE` (headphone cue), tooltip "Send to headphones". Also frees transport `CUE` from clashing |
| Channel FX button | `FX` | OK, but ambiguous with page FX toggle | `FX` on strip = "FX on channel"; page toggle `EFFECTS` |
| Crossfader assign [shot/code] | `L Mid R` / `THRU A B` | `Mid`, `THRU` unclear | `A` `THRU` `B` order matches crossfader left to right; or `L` `-` `R` |
| Crossfader curve [code] | `LIN` / `PWR` / `Linear` / `Power` | abbreviation, mixed | `Smooth` / `Sharp` or `Blend` / `Cut`; if kept, `Linear` / `Power` in full |
| Master / Phones [code] | `Master`, `MASTER`, `PHONES`, `MIX`, `VOL` | `MIX` and `VOL` mean nothing alone | `MASTER`, `HEADPHONES`, with `CUE/MASTER MIX` and `HEADPHONE VOLUME` |
| Transport [shot] | `Mstr` | abbreviation | `Master` (code already does) |
| Transport [shot] | `Qnt` | abbreviation | `Quantize` (code already does) |
| Transport [code] | `Play`, `CUE`, `SYNC` | mixed case | one case: `PLAY CUE SYNC` as button caps, or Title case everywhere |
| Transport [code] | tooltip "Follow the sync leader", "sync master" | two words for one concept | `Sync master` only |
| Transport [shot] | `Key` | ambiguous | `Key lock` |
| Loop sizes [shot] | `1/4 1/2 1 2 4 ... 32` | unit missing | prefix caption `LOOP (beats)` |
| Loop controls [shot] | `1/2x`, `2x`, `Exit`, `Reloop` | `Exit` could mean the app; `Reloop` is also a hardware brand (Reloop) | `Halve`, `Double`, `Loop off`, `Re-loop` (tooltip "Return to last loop") |
| Rate [shot] | `0.0%` | no label, no sign | `TEMPO +0.0%` with a `RANGE +-8%` selector |
| Stems [shot/code] | `Load stems`, `Stems on/off`, `no stems -- Prepare stems` | three verbs for two states | `Split stems` / `Stems off`; empty well: `No stems yet. Split stems to separate vocals, drums, bass.` |
| Pad mode tabs [code] | `CUES STEMS MORE` | `MORE` hides Master/Key lock/Quantize/Slip | move toggles under the transport; tab `PADS` is not needed |
| Header strip [shot] | `Automix Off Auto 16 beats Off Next: none Start now Cancel` | two "Off", unexplained `Auto`, bar is a loose row | `AUTOMIX` segmented `Off / On`, `Transition 16 beats`, `Next: <title or "Nothing queued">`; state word only while running |
| Header [code] | `PERFORM` | internal term | `LIBRARY` toggle ("Hide library") or `FOCUS`; tooltip "Tab" |
| Header [shot] | `Record` | fine; ring is faint | `REC` with time counter once running |
| Banner | `Mock device: silent, no audio output (--mock-device).` | CLI flag shown to users | `Demo mode: no audio output. Choose an output in Settings.` ; `No audio device: running silent.` |
| Deck header [shot] | `-- BPM`, `G#m 1A`, `0:00.0 -0:02.0` | `1A` Camelot unlabelled; two clocks unlabelled | `KEY 1A (G#m)`, `ELAPSED`/`REMAINING` captions; empty `--` |
| Deck header [shot] | `No track loaded` | fine | `No track loaded. Drag one here.` |
| Library sidebar [shot] | `All tracks, Playlists, Watch folders, music, Prep queue` | `music` is a raw folder name | keep section headers, indent children, show folder basename in muted case |
| Library toolbar [shot] | `Load to A / Load to B` | decks are 1/2 elsewhere, A/B is crossfader | `Load to Deck 1`, `Load to Deck 2` |
| Library toolbar | `Analyse`, `Prepare stems`, `Cancel stems` | UK/US mix (settings use `Analyse`), `Prepare` vs `Split` | pick one spelling for the project and use `Split stems` |
| Library status [shot] | `3 tracks   3 in view` | duplicates | `3 tracks` (`3 of 120` only when filtered) |
| Library columns [code] | `Title Artist BPM Key Time Rating Genre Added Stems`; sort glyph `^`/`v` | ASCII arrows, `Time` | proper triangle glyph, `Length`; rating as stars |
| Library empty | `Library not available (see the terminal for the reason)` | tells a DJ to open a terminal | `Library could not open: <reason>. Open Settings > Library.` |
| Library actions [shot] | `Remove tracks` next to `Delete` | which is destructive? | `Delete playlist` and `Remove from playlist` |
| Sampler [shot/code] | `Pad 1 (1): loaded - one-shot - +0.0 st - 90% - plays to the end`, `Gain -`, `Gain +`, `Pitch -`, `Pitch +`, `HIDE`, `STOP` | status sentence is raw state; `Gain -` buttons crowd | pad name row: `Kick`, mode chip `One-shot`, knobs `GAIN` `PITCH` (+/- steppers), `Stop all` |
| Sampler empty [code] | `Path: ...|`, `<key> empty` | file path typed in | `Empty: drop a sample or press LOAD` |
| Settings audio [code] | `Out 1+2`, `Out`, `Fake` | `Out 1+2` reads as a sum; `Fake` is a test driver | `Outputs 1-2`, hide `Fake` outside developer mode (`Demo (silent)`) |
| Settings audio [code] | `Output 4.2 ms, round trip ..., xruns 0` | `xruns` | `Dropouts 0` and `Latency 4.2 ms` |
| Settings MIDI mapping [code] | `Target id`, `Absolute14` -> `14-bit`, `Ch1`, `Any` | internal id exposed | show `Deck 1 Play` (already via `target_label`) and hide the id behind Advanced |
| Settings MIDI [code] | `deck.0.play` in learn targets, mapping `Mixer Ch 1 Fader` | zero/one-based mix: id says `deck.0`, label says 1 | generated labels `Deck 1 Play`, `Channel 1 Fader` (change `target_label` "Ch" to "Channel") |
| Settings MIDI [code] | `Min`, `Max`, `Invert: on` | ok | `Range min`, `Range max`, `Invert` toggle |
| Debug panel [code] | `deck.0.rate`, `mixer.ch0.trim`, `sync.enabled.0`, `router.unknown`, `xruns`, `callbacks` | dotted ids, but a developer panel | keep, but hide behind `Ctrl+Shift+D`, titled `Developer`; never on by default |
| Settings pages [code] | `Audio MIDI Library Automix Appearance Plugins Recording Mapping` | `MIDI` vs `Mapping` overlap | `Audio, Controllers (MIDI + Mapping), Library, Automix, Appearance, Plugins, Recording` |

Where the dotted strings come from: the router `ParamId` Display impl (`om-core/src/param_id/mod.rs`, e.g. `loop.reloop`); `debug_panel.rs::parameter_rows` (`deck.{n}`, `mixer.ch{n}`); `settings_view.rs`/`midi.rs` learn targets (`deck.0.play`); `mapping_editor.rs::target_label` (the only place that humanises them). Audio pairs: `om-ui/src/settings/audio.rs`. The mock banner: `crates/openmixer/src/gui.rs:128`.

## 2. Findings (conventions with sources)

1. **Channel strip order, top to bottom** *(convention in Serato/Traktor/rekordbox/djay hardware and software)*: gain/trim, 3-band EQ (high, mid, low), filter, FX send, headphone cue (PFL), crossfader assign, channel fader with level meter beside it. Evidence: controller guides list the knob order Gain, EQ, Filter, with a PFL "cue" button and channel fader below ([Numark Mixtrack Pro 3 guide](https://www.numark.com/images/product_downloads/MixtrackPro3-UserGuide-v1.1.pdf), [Serato quick-start](https://serato.com:443/downloads/files/153138/Numark+Mixstream+Pro+Serato+DJ+Lite+Quick-Start+Guide.pdf)); the Traktor Z1 puts GAIN at the top of the filter/EQ section, -inf to +12 dB, and pressing an EQ knob kills the band ([Traktor Z1 manual](https://native-instruments.com/fileadmin/ni_media/downloads/manuals/traktor/Traktor_Z1_Manual_EN_30102024.pdf)). OpenMixer's order matches; its Trim/Filter are laid out side by side at the top, which differs (two cells in a row, [shot]).
2. **Proportions** *(convention, unverified by a fetched page)*: waveforms 30-45% of height, decks plus mixer 25-30%, library 25-35%; mixer 15-20% of width between two decks in a 2-deck layout, or centred between 4. Rekordbox 7 ships switchable layouts (2 PLAYER, Dual Player) ([rekordbox 7.1.4 manual](https://cdn.rekordbox.com/files/20250718115648/rekordbox7.1.4_manual_EN.pdf), [overview](https://rekordbox.com/en/feature/overview/)); OpenMixer's `STAGE_SHARE` 0.53 of the deck row and `DECK_ROWS_SHARE` 0.71 are in range.
3. **Library table**: row height 24-28 px, columns in this order: art, title, artist, BPM, key, length, rating, genre, added, with sortable headers, right-aligned numerics, the playing track highlighted, and BPM/key colour-coded against the loaded deck. Playlists/crates tree on the left. PRODUCT.md point 3 already asks for artwork and compatibility cues; the current table has neither.
4. **Typography**: use tabular numerals for every counter. CSS `tabular-nums` maps to OpenType `tnum`, intended for numbers that update, with timers as the stated example ([Dialpad docs](https://dialtone.dialpad.com/utilities/typography/font-variant-numeric.html)); Tailwind warns many fonts lack it ([Tailwind docs](https://Tailwindcss.com/docs/font-variant-numeric)). Inter lists `tnum`, `zero`, `case`, `ss01` ([Inter feature lab](https://rsms.me/inter/lab/?feat-tnum=1)). OpenMixer already bundles Inter and has a `font.tabularNumerals` theme flag (`font.rs`); verify it is on and that the **Regular-only** bundle (one weight) is replaced by Medium+Semibold, since hierarchy currently relies on size alone.
5. **Contrast/accessibility**: text 4.5:1 (3:1 large) ([WCAG 1.4.3](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html)); UI component boundaries and state indicators 3:1 against neighbours ([WCAG 1.4.11](https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html)). Practical: an unlit button needs a visible edge at 3:1 or a label at 4.5:1, a lit state must differ by more than hue alone (add a filled shape, not just colour) for colour-blind users.
6. **Dark-room restraint**: use dark grey rather than pure black, and let accents carry state ([IxDF on dark mode](https://ixdf.org/literature/topics/dark-mode), [UX Planet](https://uxplanet.org/dark-mode-ui-design-best-practices-8d3a00a83924)). Consistent with E1 rules 1-4.
7. **Daylight theme practice**: Serato DJ Pro 2.2 added a one-click Day Mode that inverts colours for sunlit use ([Digital DJ Tips](https://www.digitaldjtips.com/?p=2754383)); OpenMixer's Daylight set exists, so expose it as a header toggle, not only in Settings.
8. **Header/status bar contents** *(convention)*: left, brand and menu; centre, master clock/BPM and automix state; right, record, CPU/latency meter, master meter, settings. Audio dropouts and latency belong here, not a debug panel.

## 3. Critique of the screenshot

- **Hierarchy is flat.** Every control has the same grey rounded rectangle at the same weight; the deck-header numbers (the E1 "loudest text") are about 13 px and no bigger than `Search`. Only the title `smoke_0` stands out.
- **Alignment.** The channel strips are 144 px, with the strip's fader hanging 300 px lower than the deck controls; Trim/Filter and Hi/Mid/Lo form a 2+3 grid whose cells are different widths. The label `Ch 1` is dim, left-aligned and far from the deck it serves. Loop-size row (`1/4 1/2 1 2 ...`) and pad rows have different column pitches, so nothing lines up vertically.
- **Spacing.** Two button sizes plus 4-5 px gaps give no rhythm; the top bar controls (32 px) and the controls below (30-36 px) differ by a few pixels, which reads as sloppy. Large dead space inside the stems well and mixer card; the crossfader sits in the mixer card's bottom with Cue mix/Phones squashed beneath it.
- **Unfinished.** Waveform strips are placeholder rainbows with a hard red left edge; empty second deck shows an empty box; no artwork, no meters labelled, no scale on faders, `0 dB` floating beside the fader, the volume faders are blank bars with no ticks. Banner text is a CLI message. Library header has no sort glyph, no row hover, rating `0` is a number not stars. The sampler sentence is a debug string.
- **Colour.** Deck colour is only a 1 px border and a few lit buttons; EQ knobs are a flat blue wash that carries no information (no centre detent). Rainbow waveform fights the glacier/orchid accents; the ember brand colour appears nowhere.
- **Good**: layout skeleton (two decks, central mixer, bottom library, sampler right) is the familiar one; the E1 tokens are sound; deck-accent borders help.

## 4. Wireframes

### 1920x1080 (header 40, stage 480, controls 300, library 260)

```
+--------------------------------------------------------------------------------------------------+
| OPENMIXER  Automix[Off|On] Transition 16 beats  Next: Night Drive     124.0 BPM   REC  [ ] CPU 3% 4ms  Day  Settings |  40
+--------------------------------------------------------------------------------------------------+
| DECK 1  Title / Artist        KEY 8A  BPM 124.0  0:42 -3:12  |  DECK 2 ...                          |
| [ overview strip  ]                                          |  [ overview strip ]                   | 480
| [ zoomed waveform, bar numbers, cues ]                       |  [ zoomed waveform ]                  |
+----------+--------------------------------+-----------------+--------------------------------+-----+
| CH1 strip| DECK 1 transport / pads / loops| MIXER           | DECK 2 transport / pads / loops|CH2  |
| GAIN     | PLAY CUE SYNC  LOOP beats      | Master  Phones  | PLAY CUE SYNC ...              |GAIN | 300
| HIGH MID | pads 1-8 (hot cues | stems)    | Crossfader      | pads                           |HIGH |
| LOW      | TEMPO +0.0%                    | Curve           |                                |...  |
| FILTER   |                                |                 |                                |FADER|
| CUE FX   |                                |                 |                                |     |
| fader+VU |                                |                 |                                |     |
+----------+----------+---------------------+-----------------+--------------------------------+-----+
| Playlists | search        [Load to Deck 1] [Load to Deck 2] [Analyse] [Split stems]            | PADS  |
| tree      | art Title Artist BPM Key Length Rating Genre Added                              | 8 pad | 260
|           | rows, 28 px                                                                     | grid  |
+-----------+---------------------------------------------------------------------------------+-------+
```

### 1280x800 (header 36, stage 300, controls 262, library 190; sampler a 56 px drawer)

```
+------------------------------------------------------------------------------+
| OPENMIXER  Automix[Off|On]  Next: ...        124.0 BPM  REC  Day  Settings      | 36
+------------------------------------------------------------------------------+
| DECK 1 title  KEY BPM time        | DECK 2 title  KEY BPM time                | 300
| overview  / zoomed waveform       | overview / zoomed waveform                |
+------+------------------+---------+------------------+---------+--------------+
| CH 1 | transport, pads  | MIXER   | transport, pads  | CH 2                   | 262
| strip| (loops, stems in | master  | (loops, stems in | strip                  |
|      | a drawer)        | xfader  | a drawer)        |                        |
+------+------------------+---------+------------------+--------+---------------+
| tree (160) | search  buttons ...                                       | PADS > | 190
|            | table 24 px rows: Title Artist BPM Key Length            | (56 px)|
+------------+----------------------------------------------------------+--------+
```

## 5. Spec

- **Type scale (px)**: caption 10 uppercase +0.06em tracking, small 11, body 13, label 13 Medium, title 16 Semibold, heading 18, readout 22 Semibold tabular, big clock 28 on the stage only. Numerals: always `tnum`; BPM one decimal; time `m:ss.d`; use `case` for all-caps labels. Bundle Inter Medium and Semibold.
- **Spacing**: 4 px grid; gaps 8 between cards, 6 inside, card padding 12; control heights 28 (small), 36 (button), 48 (pad), minimum hit 28 px. One button radius (6), one card radius (10).
- **Caps rule**: captions (GAIN, HIGH, FILTER) in uppercase 10 px; buttons in Title case words (Play, Cue, Sync, Quantize); no mix of both on one control.
- **Channel strip (top to bottom)**: caption CH n in deck accent; GAIN knob; HIGH, MID, LOW knobs with detent at centre and label-as-kill; FILTER knob with centre detent; CUE (headphone) and FX buttons; A / THRU / B assign; fader with tick marks and stereo meter beside. Width 15% of window, min 96 px, never a 2+3 grid.
- **Header**: 40 px at 1080, 36 at 800. Left brand wordmark plus automix segment; centre master BPM; right REC, dropout/latency indicator, Day toggle, Settings. Banner becomes a dismissible chip in the header, not a separate row.
- **Library**: row 28 px at 1080, 24 at 800; columns art 36, Title flex, Artist flex, BPM 64 right, Key 56 with Camelot colour, Length 64 right, Rating 80, Genre 120, Added 96; hide Genre/Added below 1400 px. Playing track highlighted in its deck accent; BPM/key of compatible tracks (within 6% and +-1 Camelot) shown in readout colour, others dim. Sort glyph triangles; status `N tracks`.
- **Colour budget**: ember only on brand wordmark, playhead and master; deck accents on border, header name, lit buttons, waveform tint option; neutral waveform default (single hue per deck, spectrum optional). EQ knobs neutral until moved off centre. Every unlit button edge or fill at 3:1 versus its card, every label at 4.5:1; lit state adds a filled shape.
- **Order of work**: (1) fix label table items [code] and the banner; (2) regenerate screenshots; (3) strip layout and fader scale; (4) bold weights and `tnum` check; (5) library density, artwork and key/BPM compatibility; (6) header with latency and Day toggle.

## Sources
- https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html (fetched)
- https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html (4.5:1 / 3:1, search summary)
- https://rsms.me/inter/lab/?feat-tnum=1 (fetched)
- https://dialtone.dialpad.com/utilities/typography/font-variant-numeric.html and https://Tailwindcss.com/docs/font-variant-numeric (search summary)
- https://native-instruments.com/fileadmin/ni_media/downloads/manuals/traktor/Traktor_Z1_Manual_EN_30102024.pdf and https://www.numark.com/images/product_downloads/MixtrackPro3-UserGuide-v1.1.pdf (search summary)
- https://cdn.rekordbox.com/files/20250718115648/rekordbox7.1.4_manual_EN.pdf, https://rekordbox.com/en/feature/overview/ (search summary)
- https://www.digitaldjtips.com/?p=2754383 (Serato Day Mode); https://ixdf.org/literature/topics/dark-mode; https://uxplanet.org/dark-mode-ui-design-best-practices-8d3a00a83924

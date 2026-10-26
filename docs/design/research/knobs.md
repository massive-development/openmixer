# Rotary knobs: research and spec

Status: research, no code changed. Clean-room (ADR 0001): conventions only; every glyph below is drawn by us.

## 1. Findings

**Sourcing caveat.** Interaction behaviour is sourced below. Pixel sizes, sweep angles and stroke widths are not published by any vendor; they are convention from common practice and should be treated as our design choices to tune by eye.

### Interaction (sourced)
- FabFilter, the plugin reference: vertical drag is primary and slow drags are finer; Shift+drag or Shift+wheel is fine mode; Ctrl/Cmd-click resets to default; double-click opens typed entry; the wheel works on hover. An optional "rotate" mode grabs the arrow and drags around it, and moving the cursor farther from the knob gives finer control. Knobs drawn as a coloured arc do not support rotate mode. [Pro-Q knobs](https://www.fabfilter.com/help/pro-q/using/knobs), [Pro-L knobs](https://www.fabfilter.com/help/pro-l/using/knobs), [Twin 3 knobs](https://www.fabfilter.com/help/twin/using/knobs)
- Serato on-screen mixer: drag changes a knob, Shift is finer, Ctrl/Cmd-click resets, and a dot above the EQ knob turns blue when the band is neutral or reset. [Serato Play](https://support.serato.com/hc/en-us/articles/360001274856-Serato-Play)
- Traktor controller mapping: Shift+push resets an EQ or gain encoder; push alone kills the band. Users report filter reset regressing in 3.7.1, which shows reset must be uniform across all knobs. [NI forum](https://community.native-instruments.com/discussion/12307/problem-with-filter-reset-on-traktor-s2-mk2-with-traktor-pro-3-software)
- Ableton: double-click returns a parameter to default. [Ableton forum](https://forum.ableton.com/viewtopic.php?p=1819410)
- Soft takeover: VirtualDJ applies it to EQ knobs so a hardware/software mismatch cannot jump the value. I found no vendor that documents an on-screen mismatch indicator. [VirtualDJ wiki](https://virtualdj.com/wiki/Soft+Take-over.html)
- Pioneer DJM-900 channel EQ: forum quote of the manual gives -26 dB to +6 dB per band, with centre meaning no change. Secondhand, so unconfirmed. [DJTT forum](https://forum.djtechtools.com/t/where-to-find-the-eq-settings-for-djm-900/37157)

### Anatomy (convention)
- Hardware mixers: a round cap with a single line pointer, a printed scale ring, a centre click on EQ and filter, and a dead-ahead pointer at 12 o'clock for neutral. Trim and master are unipolar from about 7 to 5 o'clock.
- Software (Serato, FabFilter, Ableton): a thin value arc around a dark cap, a pointer line, and a bipolar arc drawn from the centre to the value. Accent colour carries the value; the cap stays neutral.
- Standard sweep is 270 degrees: -135 to +135 from 12 o'clock, leaving a 90 degree gap at the bottom.
- Value shows in a popup while dragging and on hover, never at rest; this matches what we already do. FabFilter additionally shows it in a hint bar. Hover popup plus drag popup is sufficient.

### Which parameters are knobs
Knobs: trim/gain, 3-band EQ, filter (colour), cue mix, phones level, FX parameters and FX wet/dry, sampler gain/pitch, and master level on small layouts. Faders: channel volume, crossfader, tempo. Master is a fader or knob depending on the hardware, so knob is fine for us. This matches what `mixer_view.rs` already does for trim, EQ, filter and FX; the cue and phones controls are not yet knobs.

## 2. GPUI feasibility
- gpui 0.2.2 `PathBuilder` has `stroke(width)`, `move_to`, `line_to`, `arc_to(radii, x_rot, large_arc, sweep, to)`, `rotate`, `dash_array` and `build()`; paint with `window.paint_path` inside `canvas(...)`. `waveform_paint.rs` already builds and paints fill paths this way, and `deck_view.rs` already uses `canvas`. See [Zed PathBuilder commit](https://git.secluded.site/zed/commit/c73af0a52f61106c36b64982ee016ffc1a73d7e6) for the arc addition, and `~/.cargo/registry/src/*/gpui-0.2.2/src/path_builder.rs`.
- Stroke round caps are not confirmed. If missing, cap arcs with a small filled circle (`window.paint_quad` with full corner radius).
- The pointer line is a two-point stroked path, so no rotated element is needed. Ticks are short strokes at fixed angles.
- Performance: each arc is tessellated per paint; a strip has about 5 knobs and two decks of FX, so under 40 small paths per frame. Cache the static track and tick paths per (size, theme) and rebuild only the value arc and pointer when the value changes. Waveforms are heavier and already paint fine.
- `arc_to` is SVG-style (endpoint, not sweep angle), so write a helper `arc(cx, cy, r, a0, a1)` that computes endpoints and flags, and splits at 180 degrees to avoid the large-arc ambiguity.

## 3. Spec

### Geometry
| Role | Cap diameter | Notes |
|---|---|---|
| Trim, EQ, filter | 32 px (28 min) | label below, caption size |
| Cue mix, phones, master | 36 px | |
| FX param / mix | 30 px | |
| Sampler gain, pitch | 28 px | |
| Stem filter (deck card) | 24 px | no label, value on hover only |

- Sweep: 270 degrees, start 135 degrees clockwise from 12 o'clock (7:30) to 405 (4:30).
- Track arc radius = cap radius + 3 px; stroke 2.5 px (2 px at 28 px or below), round caps.
- Value arc: same radius and stroke, accent colour. Unipolar runs from min; bipolar runs from the detent (centre) to the value.
- Cap: filled circle at radius - 1, `surface_raised`, 1 px `border_strong`. Pointer: line from 35% to 85% of cap radius, 2 px, `text_primary`.
- Detent mark: 2 px tick outside the track at the detent angle; at rest on a bipolar knob, a 2 px centre dot replaces the arc.
- Scale ticks: optional, 3 ticks (min, detent, max), 2 px long, only at 32 px and above.
- Angle mapping must be piecewise for knobs with an off-centre detent. EQ is -48 to +6 dB (kill at -48) and trim is 0 to 4 linear with unity at 1. A linear map puts 0 dB at 89% of the sweep and unity at 25%, so the pointer would not sit at 12 o'clock. Map each side of the detent to half the sweep (or map in dB for trim). Kill zone: the bottom 5% of the EQ sweep renders in the danger colour.

### Colour tokens
| Part | Dark | Daylight |
|---|---|---|
| Track arc | `border_strong` at 60% | `border_strong` |
| Value arc | channel accent | channel accent, 15% darker |
| Cap | `surface_raised` | `surface_raised` |
| Pointer | `text_primary` | `text_primary` |
| Detent tick / centre dot | `text_secondary` | `text_secondary` |
| Hover | cap `surface_hover`, arc +10% lightness | same |
| Active (drag) | arc stroke +0.5 px, popup shown | same |
| Disabled | all at 40% alpha, no popup | same |
| Kill / cut | arc and pointer `danger`, no glow | same |

Use no glow in daylight; in dark, an optional 1 px soft halo on the value arc only when dragged. These reuse `KnobStyle` fields (`body`, `fill`, `label`, `readout`); add `track`, `pointer` and `kill` colours there and in the JSON theme keys.

### Interaction
| Input | Behaviour | Today |
|---|---|---|
| Vertical drag | up is more, 200 px per full sweep | keep (`KNOB_TRAVEL_PX`) |
| Shift+drag | 10x finer | add |
| Wheel (hover) | 1/100 of range per notch; Shift 1/1000 | add |
| Double-click | reset to default/detent | exists (`click_count >= 2`) |
| Ctrl/Cmd-click | reset (alias) | add |
| Detent pull | 3% of range snaps; Shift bypasses | exists; add the bypass |
| Arrow keys on focus | one step; Shift fine; Home resets | add |
| Right-click | typed entry (later) | later |
| Hover / drag | value popup | exists |
| MIDI mismatch | ring marker at hardware position, soft takeover until crossed | add; open design, no vendor precedent |
| Circular drag | not provided; vertical only | skip |

## 4. Keep and change
Keep: the `Knob` model (range, detent, `snap_to_detent`, `at_rest`, readouts); drag travel logic and detent pull in `drag.rs`; double-click reset; popup on hover and drag; `Target` hit-testing. Replace: the rectangle body and fill in `knob_face` and `knob`, which is the part that reads as a slider. Add a `KnobPaint` canvas in `om-ui-gpui` and one shared angle helper in `om-ui` (testable, no GPUI). Add a `mixer_knob_height` to diameter mapping, since the layout is rect-based and knobs need square hit areas. Labels and popup placement stay as they are.

## 5. Risks
- Rounded stroke caps and arc quality in gpui 0.2.2 are unverified; build a prototype first.
- Small knobs (24 px) lose scale ticks and legibility; the stem filter may need to stay a mini-slider.
- The 40 px layout rects for EQ are not square; the layout needs changes in `mixer_view.rs` and the layout module.
- Piecewise angle mapping changes the pointer feel versus the linear drag; drag should operate on the angle fraction, not raw value.
- Soft takeover needs engine and MIDI state work, so it is out of scope for the visual pass.
- Hover popup can clip at the window edge; the existing popup placement should be reused.
- Theme JSON gains new keys, so unknown-key warnings apply to old themes.

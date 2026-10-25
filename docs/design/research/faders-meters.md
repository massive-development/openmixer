# Faders, crossfader and meters: conventions research

Scope: conventions only (ADR 0001, Lotus v. Borland). No vendor artwork, layouts or text are reproduced. Where a number below is not backed by a source it is marked **(proposal)**.

## 1. What exists today

- `om-ui-gpui/src/controls.rs::slider_with_popup` draws a track `div` plus a 12 px thumb (`thumb_size`), positioned by a linear fraction. Drag and double-click reset already work (`draggable`, `reset_target`). There is no scroll wheel on mixer faders (only in `deck_view.rs`), no shift-fine, and no ticks.
- `stereo_meter` is less thin than the brief assumed. It is a continuous bar per side (not segmented) with dB labels, zone colours, a hold tick and a latching clip lamp that clears on click. Meter width is 12 px (`mixer_meter_width`), segment tokens (`meter_segment` 4, `meter_segment_gap` 1) exist but the painter does not use them.
- Theme tokens: floor -48 dB, mid -18 dB, high -6 dB, clip 0.99 linear, release 0.3 s, hold 1.0 s, clip 2.0 s. Colours `#2EE59D`, `#FFC83D`, `#FF3B4E`.
- Engine (`om-engine/src/mixer/`): per-channel `PeakMeter` L/R plus master L/R. Each is a block peak with a per-block multiplicative decay (`next = max(block_peak, prev*decay)`), read lock-free through `MixerControl::channel_peak_left` etc. No RMS, no true-peak, no PFL meter. `om-dsp::MeterBallistics` adds display-side release, hold and clip latch.
- Gain law: channel fader is a **linear 0..1 gain** (`set_fader`), master and cue are linear 0..2 (`MAX_GAIN = 2.0`, i.e. +6 dB), trim 0..4. There is no dB taper, so the physical midpoint is -6 dB, not unity.
- Crossfader curves: `Linear` and `ConstantPower` only; `crossfader.curve` is a 0/1 param. No sharp-cut.
- Pitch: `om-ui/widgets/pitch.rs` has `PitchFader` with range indices.

## 2. Findings (with sources)

**Crossfader.** Mixers expose a steep curve for cuts and scratching and a shallow one for long blends; this is the most common crossfader adjustment ([Thomann DJ mixer features](https://www.thomannmusic.com/onlineexpert_page_dj_mixers_common_features.html)). RaneNote 146 names four shapes: constant-power, "no dip" (additive), medium (cut-in) and sharp (cutting and scratching), requires reverse (hamster) operation, and sets the sharp end at under 2.5 mm travel between full off and full level ([Rane, RaneNote 146](https://www.ranecommercial.com/legacy/note146.html)). Software offers Linear, Constant Power, Dipped and Cut ([Algoriddim sound settings](https://help.algoriddim.com/user-manual/djay-ios/settings/sound), conventions only). The Xone:43 has three switchable curves ([DJ Hook Up listing](https://thedjhookup.com/collections/dj-mixers/products/allen-heath-xone-43)). Takeaway: three curves (smooth/power, additive, sharp cut) plus a reverse switch.

**Meters.** Rane describes meters as a traffic light: green fine, yellow caution, red stop, with gain set so peaks sit high in yellow, and quasi-peak metering that holds the top LED for at least 0.5 s ([Rane, setting mixer level controls](https://www.rane.com/how-to-properly-set-rane-mixer-level-controls/); the search snippet was usable, the page itself returned 404 on direct fetch). Logic Pro treats red as "hot", not necessarily clipping, and says a clip indicator that lights regularly is the real warning ([Apple Logic Pro docs](https://help.apple.com/logicpro/mac/10.1/en.lproj/lgcp8e96d97b.html)). A forum comment praises 1 dB spacing near 0 dB for resolution ([Engine DJ community](https://community.enginedj.com/t/laidback-luke-talks-about-the-new-prime-series/1760)); this is anecdotal.

**Ballistics.** IEC 60268-18 is a technical report for digital peak meters (digital peak meter, [IEC TR 60268-18](https://ccn-scc.ca/standardsdb/standards/2001981)). Secondary sources give PPM numbers: 5 ms integration, 1.7 ms attack constant, 1.5 s fall to -20 dB ([Elliott Sound Products P55](https://sound-au.com/project55.htm), [itwissen PPM](https://itwissen.info/en/peak-program-meter-PPM-120117.html)). Not verified against the standard text.

**Pitch fader.** Turntable-style ranges are +-8 / 16 / 50 % with a reset button (Numark NTX1000, Reloop RP-5000 MK4; [Reloop listing](https://avlgear.com/collections/dj-equipment/products/reloop-rp-5000-mk4-professional-direct-drive-dj-turntable-precision-power-and-performance)); a newer Technics adds +-4 % and +-50 % ([Darko.Audio](https://darko.audio/2026/10/technics-new-sl-1210mkx-runs-pitch-from-%C2%B14-to-%C2%B150/)). Rane documents recalibrating when the "0" neutral is off centre ([Rane support](https://support.rane.com/en/support/solutions/articles/69000814143-rane-dj-twelve-and-twelve-mkii-calibrating-the-pitch-fader)).

**Soft takeover.** VirtualDJ applies it by default to pitch, volume, crossfader, gain and EQ; the physical control takes over only once it reaches the software value, with a "gradual" alternative ([VirtualDJ wiki](https://virtualdj.com/wiki/Soft+Take-over.html)). Known failure: it cannot reach values the hardware cannot express ([DJ TechTools forum](https://forum.djtechtools.com/t/new-to-midi-control-midi-cc-feedback/6892)).

**Gaps.** Searches found no authoritative source for DJ-channel-fader dB tick positions, total fader travel in mm, or a master headroom figure. Those are marked proposal. Audio-engineering convention (DAW faders) puts unity at roughly 3/4 up the travel with +6 to +10 dB above it and a log taper below; this is general knowledge, not cited.

## 3. Spec (proposal)

### Gain law
Fader position p (0..1, bottom to top) to gain: dB-law piecewise. p=1.0 gives +6 dB, p=0.75 gives 0 dB (unity), then linear in dB down to -30 dB at p=0.25, then a ramp to -inf at p=0. Master uses the same law. Channel fader is capped at 0 dB if +6 is unwanted; keep +6 on master to match `MAX_GAIN = 2.0`. Resolution: `gain = 10^(dB/20)`; engine already smooths the fader per block.

### Channel fader (vertical)
- Track slot: 4 px wide, 100..160 px long (fills strip height, min 96), inset 1 px dark well, radius 2.
- Cap: 22 w x 36 h (gripped, with a 2 px centre line across it), radius 3. Travel = slot length minus cap height. Ticks drawn beside the slot.
- Ticks (left side, 6 px long minor, 10 px major, 1 px): +6, 0 (unity, major, accent colour, label "0"), -5, -10, -20, -30, -inf. Labels 12 px minimum (existing `MIN_TEXT_PX`), only major ticks labelled.
- Master: same anatomy, slot 6 px wide, cap 26 x 40, ticks +6/0/-6/-12/-24/-48.

### Crossfader (horizontal)
- Slot 4 px x full width; cap 28 w x 20 h. A / B letters at the ends (12 px). Centre tick 10 px.
- Curve switch segmented control, three states: Smooth (constant power), Add (no dip, linear), Cut (sharp: a channel is silent past 52 % of travel toward the other side and full level by 48 %, so the fade zone is about 4 % of travel). Plus a Reverse toggle. Engine needs a third and fourth `CrossfaderCurve` variant and a param value range 0..3.
- Double-click: centre.

### Meters (segmented LED)
Width 12 px per side, 2 px between sides; segments 4 px tall + 1 px gap = 5 px pitch. A 160 px meter holds 30 segments; use 28 on channel, 36 on master (taller).

| Zone | Range (dBFS, top of segment) | Dark | Daylight | Segments (channel) |
|---|---|---|---|---|
| Off | below -48 | `#1B2024` | `#D5DADF` | - |
| Green | -48 to -13 (2.5 dB step to -18, 3 dB below) | `#2EE59D` | `#16A36B` | 14 |
| Amber | -12 to -4 (2 dB step) | `#FFC83D` | `#D99A00` | 5 |
| Red | -3 to 0 (1 dB step, matching the 1 dB near-top spacing reported above) | `#FF3B4E` | `#D62839` | 4 |
| Clip lamp | any sample at or above 0.99, latch 2 s or until click | `#FF3B4E` lit / `#4A2026` off | `#D62839` / `#F0C4CA` | 1 (above the bar, 6 px tall) |

Unlit segments show at 100 % of the Off colour; lit segments are solid. Changing `meter_mid_db` to -12 and `meter_high_db` to -3 matches the proposal.
- Peak hold: one segment, same colour as its zone, held 1.0 s (Rane: at least 0.5 s) then falls with the main release.
- Ballistics (display side, 60 fps): attack instant (block peak is already ~5 ms at 256 samples); release 20 dB per 1.5 s (about 13 dB/s) = existing decay at per-frame factor `10^(-13/20/60)` = 0.975, replacing the 0.3 s constant, which is too fast to read; hold 1.0 s; clip latch 2.0 s.
- Stereo: L and R columns side by side, label "L" "R" only on master. Channel meters pre-fader or post-fader: post (matches current engine tap), PFL lit state on the PFL button and a 2 px accent frame on the strip, not a separate meter.
- Master headroom: scale tops at 0 dBFS; red segments mean limiter-adjacent. Show a numeric peak readout (e.g. `-2.3`) under master, 12 px.
- Master cue: a mono-sum cue meter is optional (not in engine yet).

### Colours (faders)
Dark: slot `#0E1114`, ticks `#6B7680`, unity tick and label `#E8EDF2`, cap `#3A424A` with highlight line `#AEB8C2`. Daylight: slot `#C4CBD2`, ticks `#55616C`, unity `#1A2026`, cap `#F4F6F8` with edge `#8B96A1`. Cap highlight line turns the accent colour while dragging.

### Interaction
- Drag: relative to the grab point (do not jump the cap to the pointer).
- Shift+drag: 0.1x sensitivity (fine).
- Double-click: reset (channel to 0 dB, master to 0 dB, crossfader to centre, pitch to 0 %) (already wired for mixer; extend to the new law).
- Scroll wheel over a fader: 1 dB per notch (0.1 dB with shift).
- Popup: dB readout while dragging (exists); show `-inf` at the bottom.
- MIDI soft takeover (pickup): ignore hardware CC until it crosses the software value, then lock; show a ghost tick on the scale at the hardware position while unlocked. Applies to channel, master, crossfader, pitch.

### Pitch fader
Vertical, slot 4 x 140 px, cap 22 x 36, centre notch tick 12 px wide in accent colour with a snap zone of +-0.15 % of range (visual detent; software only). Range selector cycles +-8, +-16, +-50 (and +-6 optional); ticks: 0 major, +-(range/2) minor. Readout `+1.2 %` at 12 px, one decimal; reset on double-click or a RESET button. Changing range rescales cap position so the percent reading stays continuous (clamping if needed).

## 4. GPUI feasibility
Everything above is rectangles, text, radius and one-pixel lines, which `place()` / `skin()` already compose, so segmented meters (a loop of `div`s, about 60 per meter, 8 meters) and ticks are feasible. Gradients and shadows on caps exist in GPUI (`linear_gradient`, `shadow`) but current code does not use them; flat caps with a highlight line are the safe route. Drag and wheel use `on_mouse_down`, `on_mouse_move` and `on_scroll_wheel` as in `deck_view.rs`. Modifiers are available on the event (`ev.modifiers.shift`).

## 5. Risks
1. Gain law change: existing saved mixes, MIDI maps and `fader: 0..1` params change meaning (0.5 was -6 dB, now about -15 dB). Needs a migration or a versioned param, and tests in `drag.rs` and `mixer` that assume linear.
2. Element count and repaint: ~300 divs per frame at 60 Hz for meters; coalesce lit runs into one div per zone (as `scale.runs` does now) and draw unlit track once, keeping segment look via a repeating gap mask rather than per-segment divs if profiling shows cost.
3. Peak-only engine meter: sample peaks at block rate understate transient overs; true-peak or per-sample clip detection is an engine change. Block size sets attack accuracy.
4. Sharp-cut and reverse need `om-dsp::CrossfaderCurve` and param ranges extended, plus the MIDI/mapping editor entries.
5. Unsourced numbers (tick dB positions, travel, 22x36 cap, takeover ghost) are design choices, not standards; validate with a mockup at 1x and 2x DPI.
6. Soft takeover needs per-control state in the MIDI layer; hardware/software mismatch leaves controls unreachable after a deck swap, so provide a "jump" option.
7. Daylight colours are untested for contrast; check amber on light backgrounds (needs >= 3:1 for non-text UI).

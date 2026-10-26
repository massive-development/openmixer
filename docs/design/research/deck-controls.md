# Deck transport and performance controls: research and spec

Scope: conventions only (ADR 0001, Lotus v. Borland). No names, art, layouts or text copied from any product. Web search returned mostly manuals and hardware guides; where a point is unconfirmed it says so.

## 1. Findings

**What the references agree on**
- Pad modes group hot cues, loops, beat jump, sampler and stems into one pad area. Rekordbox hardware diagrams list Hot Cue, Beat Jump, Beat Loop, Sampler and Keyboard modes; Engine DJ lists Hot Cue and Loop modes with up to 8 saved points per track, and renaming, clearing and recolouring from a right-click on a filled pad. Sources: [rekordbox XDJ-AN diagram](https://cdn.rekordbox.com/files/20260709202905/XDJ-AN_HardwareDiagram_rekordbox_for_MAC_WIN_E1.pdf), [Engine DJ guide v5.1](https://cdn.inmusicbrands.com/engine/51/Engine%20DJ%20-%20User%20Guide%20-%20v5.1.0.pdf).
- Eight hot cues per track is the norm (Engine DJ guide above; Serato guidance via [third-party summary](https://music.loop.fans/blog/serato-guide-dj-suite-interface), unverified against the official manual). OpenMixer already has 8.
- Loop controls: in, out, exit/reloop, a beat-length selector with halve and double, and a roll or slip variant. Rekordbox lists loop in/out, reloop/exit, beat loop sizes, slip loop and beat jump up to 64 beats ([CDJ-1500X diagram](https://cdn.rekordbox.com/files/20260709202753/CDJ-1500X_HardwareDiagram_rekordboxdj_E1.pdf)). Serato saves loops per track and exposes them in a Loops tab of the virtual deck ([Serato overview](https://support.serato.com/hc/en-us/articles/14173738028175-Software-Overview-Serato-DJ-Pro-4-0)).
- Slip: the track keeps running silently under loops, scratches and reverses and rejoins the true position on release ([rekordbox DDJ-RX page](https://www.pioneerdj.com/en/product/dj-controllers/ddj-rx/)). Quantize snaps cues and loops to the beat grid (same page). Traktor calls slip-like behaviour Flux (community mappings: [DJTT DDJ-SR](https://maps.djtechtools.com/mappings/9337)).
- Sync, plus a designated tempo master that others follow; Traktor's Master Clock can auto-pick the master deck ([Traktor overview](https://docs.native-instruments.com/online-guides/traktor-pro-manual/en/traktor-overview)).
- Virtual deck header content (Serato): title, artist, BPM, key, progress, pitch percent, sync and key-lock status ([summary](https://music.loop.fans/blog/serato-guide-dj-suite-interface)).
- On-screen platter: djay documents a virtual turntable with needle drop (click on the record to seek) and scratch control, with start/stop on the turntable ([djay iOS manual](https://download.algoriddim.com/manual/djay%20iOS%20iPhone%20Manual%20v1%20%28low-quality%29.pdf)). Engine DJ lets you drag the waveform to scratch and click the overview to jump (guide above). No source found for Serato, Traktor or rekordbox on-screen platter specifics; the platter spec below is our own synthesis of the djay and Engine behaviour.
- Layout order (synthesis across Traktor deck layouts, Serato virtual decks, Engine guide): header (art, title, artist, key, BPM, time) on top; waveform; then platter plus pitch fader; then transport row; then pad area with mode tabs; loop controls adjoin the pads. Traktor offers switchable layouts from Micro to Full ([manual 3.2](https://www.native-instruments.com/fileadmin/ni_media/downloads/manuals/traktor/TRAKTOR_PRO_3.2_Manual_English_0719.pdf)), supporting a compact variant.
- Colour: hot cue pads are individually colour coded; empty pads are neutral and dim; lit means set. Key colour-coding is a common request (see [djay forum](https://community.algoriddim.com/t/key-traffic-light-system-like-rekordbox-and-traktor/34946)).
- Pad shortcut conventions from hardware: bottom four pads cue, top four loop on small devices ([djay forum](https://community.algoriddim.com/t/numark-mixtrack-platinum-fx-performance-pads-modes/13530)). Not needed on screen.

**Plain-language labelling.** The vendors rely on hardware silk-screen abbreviations. On screen we have room: use full words, with a one-line tooltip (already present in `transport.rs`).

## 2. Current inventory (deck_view.rs, om-ui widgets)

Today: `tile()` draws every button as the same rounded rectangle; Lit uses the deck accent with glow. Hot cues are 2x4 numbered squares (colour only when lit). The loop row is three tiles. Stems and More are text tiles. There is no platter, no beat jump, no loop in/out, and no cue names. `Master` and `Slip` are in the hidden More drawer; header chips read `SYNC`/`MASTER`.

| Current label | Proposed label | Control type |
|---|---|---|
| Play / Pause (double width) | Play / Pause (icon + word) | Primary round button, 56 px |
| CUE | Cue | Primary round button, 56 px, momentary |
| SYNC | Sync | Secondary pill, toggle |
| Master (More drawer) | Tempo leader (tooltip: "other decks follow this deck's tempo") | Toggle pill, promoted out of drawer |
| Key lock | Key lock | Toggle pill with lock glyph |
| Quantize | Snap to beat | Toggle pill |
| Slip | Slip | Toggle pill (tooltip as now) |
| Hot cue pads 1-8 (numbers) | Hot cue pads: number + name; empty reads "Set" | 8 pads, 2x4, colour coded |
| LOOP 1/4 (size tile, toggles) | Loop length chip with on/off | Segmented: [-] [length] [+] and separate Loop button |
| halve, double | Halve loop, Double loop | Icon+text stepper buttons |
| (missing) | Loop in / Loop out | Two small buttons with a Loop toggle |
| loop exit / reloop (code names) | Exit loop / Resume loop (single button: Loop, lit when active) | Toggle |
| (missing) | Beat jump back / forward with size | Stepper pair |
| Load stems / Stems on / Stems off | Separate stems / Stems on / Stems off | Toggle in pad area; stems pads as 4 labelled faders |
| Header chips SYNC, MASTER, LOOP | Same words, title case, as status badges | Read-only badge |
| (missing) | Platter | Circular canvas (below) |

## 3. Spec

**Layout (top to bottom in the deck card).** Header, overview, zoomed waveform, then a control band: platter (left, 160 px diameter, scales down to 112 in compact) beside the pitch fader (right); transport row; pad area with a mode strip (Hot cues, Loops, Beat jump, Stems) so one grid serves all four; toggle row (Sync, Tempo leader, Key lock, Snap to beat, Slip).

**Header.** Art 48 px square (rounded 6), title 15 px bold, artist 12 px muted, then key badge (colour from key wheel, text always shown), BPM 20 px tabular, remaining time `-m:ss` 20 px tabular, and status badges.

**Buttons.** Minimum hit target 44 px (matches the existing hotcue test). Primary: Play 56 px circle, Cue 56 px circle, 24 px gap between them, 12 px to Sync. Secondary pills: 36 px tall, 12 px radius, 8 px gaps; related toggles share one 4 px gap group. Pads: 52x44, 6 px gap, 6 px radius.

**States.**
- Idle: `well` fill, 1 px border, muted text.
- Hover: +6% lightness. Pressed: 2 px inward shift, -8% lightness.
- Lit (toggle on): deck accent fill, on-accent text, glow (existing `look.glow`). Play when playing: green fill; Cue held: amber.
- Disabled (no track): 40% opacity, no hover, no pointer.
- Empty pad: dashed 1 px border, "Set" label, no colour. Set pad: cue colour fill at 85%, name in dark/white by contrast, a 3 px colour tab on top edge. Active (playing from that cue): full fill plus glow. Loop pads show a loop glyph.
- Never rely on colour alone: label text and glyph change with state (Play to Pause).

**Colours.** Cue palette stays the existing 8 (`CUE_PALETTE`). Dark theme: card `#16181D`, well `#22252C`, border `#343842`, text `#E8EAF0`, muted `#9AA0AE`. Daylight: card `#F4F5F7`, well `#E3E6EB`, border `#B9BECA`, text `#1B1E24`, muted `#555B69`; lit pads use the same palette but darkened 12% so text stays at or above 4.5:1; on cue pads use black text where luminance above 0.4. Verify with the existing contrast tests. These are proposals; tokens should be added to `theme/tokens.rs`, not hard-coded.

**Platter.** Disc with outer ring (4 px) that is the position indicator: a 6 px marker advancing 1 turn per 1.8 s at 100% rate (33 1/3 rpm), spinning only while playing, scaled by tempo. Inner label 56% of the diameter: artwork clipped to a circle, else the deck colour with deck letter. Zones by radius: outer 15% band is pitch bend (drag clockwise nudges tempo up, counter-clockwise down, springs back); inner 85% is touch-scratch (pointer down pauses the motor, drag angle maps to position, release resumes; with Slip on, rejoin the true position). Mouse wheel over the platter nudges: one notch is a 20 ms shift. Touch uses the same events. Platter play position ring: the played arc is drawn in the deck colour around the rim (track progress, 0 to 360 degrees over track length), a conventional use of the ring.

## 4. GPUI feasibility (gpui 0.2.2, checked in the vendored source)

- The app already renders every frame (`window.request_animation_frame()` in `app.rs`), so a continuously rotating platter costs only paint work.
- Paths: `PathBuilder` provides `move_to`, `line_to`, `curve_to`, `arc_to`; `Window::paint_path` already draws waveform shapes. A ring, tick marks and the marker can be built as filled paths. Static geometry (disc, grooves) can be cached once; only the marker and progress arc need rebuilding per frame (a few hundred vertices).
- Rotated images: `paint_image` has no transform parameter and the waveform module notes `RenderImage` creation needs the `image` crate (a Cargo.lock change). `paint_svg` accepts a `TransformationMatrix`, so a rotating SVG label is possible. `gpui::svg().with_transformation(Transformation::rotate(..))` exists. Artwork rotation is therefore not free. Recommendation: do not rotate artwork; keep artwork static in the centre and rotate only the ring and marker (this is also easier to read).
- Pointer input: standard mouse down/move/up and scroll-wheel events suffice. Capture the drag by tracking the angle from the platter centre.

## 5. Risks

1. Artwork in a circle needs image decode and clip; GPUI 0.2.2 has no rounded image mask in the path we found. Fallback: square-cropped art with a ring overlay, or label-only.
2. Per-frame path rebuild at 60 fps across four decks; cache static geometry, budget under 0.3 ms per deck, test with the existing frame timer in `render`.
3. Touch-scratch needs low-latency position seeks from the UI thread to the audio engine; the current `DeckParam` route may add latency. Needs an engine API for scrub mode (not in `DeckSnapshot`).
4. Beat jump, loop in/out and Tempo leader exposure need model additions (`DeckParam` entries); check `om-core` before committing to the scope.
5. Accessibility: colour-only pad state, 12 px minimum text, glow on daylight theme. Contrast values above are untested.
6. Sources are thin for Serato, Traktor and rekordbox software UIs (hardware diagrams dominate). Treat layout order as convention, not verified fact; verify with the official manuals before citing in public docs.
7. Renaming Master to "Tempo leader" departs from industry vocabulary; users migrating may search for "Master". Keep "Master" in the tooltip.

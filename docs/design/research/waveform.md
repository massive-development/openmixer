# Waveform research: fixed band hues instead of a hue wheel

Scope: conventions only (ADR 0001). No layouts, artwork or names copied. Items marked (unverified) came from forum or secondary sources, or could not be confirmed.

## 1. What exists in the repo

- Analysis (`crates/om-analysis/src/waveform.rs`, `spectrum.rs`, `waveform_data.rs`): format 2 points hold `min`, `max`, loudness, spectral centroid (8 bit, log axis 25 Hz to 16 kHz) and spread. Finest level is 128 samples per point (about 2.9 ms), FFT 2048, hop 512, Hann. Peak range is kept as the silhouette. A pyramid of coarser levels exists.
- UI model (`crates/om-ui/src/waveform/`): `bands.rs` already contains a three-band path: edges 200 Hz and 3 kHz, colours blue `#2563FF`, amber `#FFA726`, near-white `#F5F7FA`, amplitude exponent 0.6, played brightness 0.45, 95th-percentile reference peak. `hue.rs` and the theme `wave_stops` implement the full-wheel centroid mapping from the E10 spec (stops from red through green, blue, magenta to white). `markers.rs`: playhead 2 px white with 1 px dark halo, beat line 1 px at alpha 0.25, bar line 2 px at 0.70, bar numbers 12 px with 48 px minimum spacing, cue flags 18x16 with an 8-colour palette, loop colour `#2EE59D` with fill alpha 0.18.
- Painting (`crates/om-ui-gpui/src/waveform_paint.rs`): each strip is three mirrored filled `Path`s (low at the back), not per-column quads. The overview paths are cached and cloned per frame; the zoomed strip rebuilds the paths from cached heights (`STRIP_STEP_PX` 2, cache three views wide). A `spectral` flag selects the spectrum or the three-band colours. The comment notes `paint_image` is avoided because `RenderImage` needs the `image` crate directly.
- Stage (`stage_view.rs`, `om-ui/src/widgets/stage.rs`): per deck a header plus a 48 px overview; zoomed strips are full width and stacked beneath; a 3 px deck-accent bar sits at the left of each zoomed strip; zoom strips hide below 32 px.

Consequence: the three-band, fixed-hue rendering is mostly built. A path with one colour per band cannot show a per-column hue, so it is already the right shape for the additive approach, whereas the hue-wheel mapping needs per-column colour (more shapes or a texture).

## 2. Findings

**Band splits and filters.**
- Mixxx splits at 600 Hz and 4 kHz using 4th-order Bessel low-pass, band-pass and high-pass filters (an 8th-order Butterworth is commented out) and stores the peak (max of absolute value) per stride, not RMS; the main waveform stores 441 points per second. Source: [analyzerwaveform.cpp](https://raw.githubusercontent.com/mixxxdj/mixxx/main/src/analyzer/analyzerwaveform.cpp).
- Serato documents the colour roles: red low, green mid, blue high. Ctrl-clicking a deck splits the view into three separate band waves (forum tip, unverified). Sources: [Serato overview article](https://support.serato.com/hc/en-us/articles/224969227-Track-Overview-Display), [Digital DJ Tips on reading colours](https://www.digitaldjtips.com/?p=2475091).
- rekordbox has Blue, RGB and 3-Band modes. RGB reads red low, yellow/green mid, blue high; 3-Band is blue low, amber mid, white high (secondary source). Source: [Reallychrism write-up](https://reallychrism.substack.com/p/the-secret-language-of-waveforms), [Elephorm course page](https://www.elephorm.com/formation/maitriser-rekordbox-6/comprendre-les-differents-modes-daffichage-de-la-waveform-dans-rekordbox-6).
- Traktor has four colour modes (X-Ray, Spectrum, Infrared, Ultraviolet) and no official band table; users guess low red, mid green, high blue-violet (unverified). Sources: [RA review](https://ra.co/reviews/8962), [NI community thread](https://community.native-instruments.com/discussion/comment/77507).
- I found no published crossover frequencies for Serato, Traktor, djay, rekordbox or Engine. Treat 200 Hz and 3 kHz as our own choice. The generic recipe is three band-pass signals driving R, G and B ([JUCE forum](https://forum.juce.com/t/multicoloured-waveform/21085)).

**Colour blending.** Mixxx's RGB shader (`res/shaders/rgbsignal.frag`) stores low, mid and high per column, applies per-band gains, sums `lowColor*low + midColor*mid + highColor*high`, then divides by the largest component so the dominant band reaches full brightness. Outside the envelope it draws the unscaled envelope as a 40 percent alpha shadow. It has no anti-aliasing: hard `>=` comparisons, and a fixed 4 px centre band. Source: [rgbsignal.frag](https://raw.githubusercontent.com/mixxxdj/mixxx/main/res/shaders/rgbsignal.frag). Brightness therefore comes from the silhouette height and the max-normalised blend, not a separate loudness channel.

**Amplitude shaping.** Peak, not RMS, gives the sharp transients DJs expect (Mixxx, above). Everything else here (gamma, symmetric silhouette) is common practice with no published source I could find. Our `AMPLITUDE_EXPONENT` 0.6 is in that range.

**Overview vs zoomed.** Serato's overview is a click-to-seek whole-track strip; zoom is by +/- buttons, shortcuts or wheel over the main waveform ([Serato manual search results](https://support.serato.com/hc/en-us/articles/115000487673-Overview)). Mixxx 2.4 renders waveform and overlay layers (beat grid, loops, markers, end-of-track) with GLSL at 60 fps, stacks markers that share a position at vertical offsets, and draws semi-transparent triangles before and after the track ([Mixxx 2.4 post](https://mixxx.org/news/2024-02-23-improved-waveforms)). I could not find sourced numbers for strip heights, playhead style, played-portion dimming or accessibility options in any program; the Serato and rekordbox manuals did not surface in search. Open question for a manual read.

**Mixxx shader files** (`computemaxsignal.frag`, `filteredsignal.frag`, `rgbsignal.frag`, `stackedsignal.frag`) confirm the approach: per-column texture of band levels, GPU compositing ([directory](https://github.com/mixxxdj/mixxx/tree/main/res/shaders)). I could not fetch libdjwaveform (403), so nothing is cited from it.

**GPUI.** `Window::paint_path`, `paint_quad` and `paint_image(bounds, radii, Arc<RenderImage>, frame_index, grayscale)` exist ([docs.rs](https://docs.rs/gpui/latest/gpui/struct.Window.html)). No custom-shader API is documented, so a GLSL port is not available. Paths are anti-aliased by the GPU path renderer (assumption, to verify by screenshot).

## 3. Recommended spec

**Bands.** Keep 200 Hz and 3 kHz edges. Compute from the existing FFT bins with raised-cosine crossovers, plus or minus 0.25 octave in log frequency, so energy is partitioned smoothly. Store per point: peak low, mid and high (u8, peak of band-weighted magnitude, as Mixxx) plus the existing `min`/`max`. The centroid and spread fields can stay for later or be dropped in format 3.

**Colour stops (additive on dark, normalised by max component).**

| Band | Dark theme | Daylight theme |
|---|---|---|
| Low | `#FF3B3B` | `#C62828` |
| Mid | `#35E06A` | `#1B8F45` |
| High | `#3D8BFF` | `#1D5FD1` |
| Silhouette (no data) | `#8A94A6` | `#5B6475` |

Column colour: `c = sum(band_i * colour_i)` using linear-light weights `w_i = (peak_i)^0.7`, then `c /= max(r,g,b)` so the dominant band reaches full strength, then lerp toward white by 0.15 when all three bands exceed 0.6 (loud full-range sections read as bright, not muddy). On the light theme blend in multiplicative form (`c = 1 - sum(w_i*(1-colour_i))` clamped, then darken to keep at least 4.5:1 against the strip background; bar and playhead need 3:1).
Alternative one-colour-per-band path (already built): paint three nested silhouettes, low at the back, with the three colours above at alpha 1.0. Gives the 3-band look for free and is the fallback.

**Shaping.** Height = `(peak / reference)^0.6`, reference the 95th percentile with a floor of 0.1 (existing). Mirror about the centre; `FILL` 0.94; minimum half height 0.75 px. Brightness floor: no column darker than 35 percent of its colour, so quiet passages stay visible.

**Sizes.** Overview 48 px (existing; 40 px minimum), whole track across the width, 1 px playhead, 2 px cue ticks, played portion at 0.45 brightness (existing), loop as a 0.18-alpha band. Zoomed strips 96 to 140 px each when stacked, never below 32 px. Zoom steps 1, 2, 4, 8, 16, 32 beats across the width (default 8); playhead at 50 percent; a 2 px white line with a 1 px dark halo (existing).

**Markers.** Beat lines 1 px at 0.25 alpha; bar lines 2 px at 0.70; bar numbers 12 px every 4 bars or at least 48 px apart (existing). Cues are 18x16 flags in the 8-colour palette with a 2 px line. Loops: 0.18 fill and 2 px edges. Stack coincident markers vertically as Mixxx does. Markers use hues absent from the waveform palette: orange `#FF6B35`, teal-green loops `#2EE59D`; mid-band green is close to the loop colour, so loops should use a distinct hatch or a top bar.

**Accessibility.** Offer three waveform styles: Colour (above), Three-band (nested, existing), Mono (single hue, luminance by band). Offer a colour-blind-safe palette (low `#E69F00`, mid `#F0E442`, high `#56B4E9`, Okabe-Ito based, [Okabe-Ito](https://jfly.uni-koeln.de/color/)). Reduced-motion should drop the scroll easing, not the scroll.

**Dual deck.** Existing stack is right: overview beside the header, two zoomed strips full width, deck accent bar 3 px. Align the two beat grids horizontally by beat phase if the app adds a sync view.

## 4. Risks and open questions

- Per-column colour with paths needs one shape per colour. Options: quantise column colour into 16 to 32 buckets and draw one path per bucket (about 30 draw calls per strip); or build an RGBA image and call `paint_image`, which needs `image::Frame` and so a direct dependency and a lockfile change. Measure at 60 fps with two zoomed strips and two overviews before choosing.
- Does the path renderer anti-alias edges, and does overdraw of three nested paths cost enough to matter on the intended GPU? Needs a profile.
- Peak versus RMS: peak looks spiky on dense material; consider drawing RMS as an inner, brighter core. Needs visual review on real tracks.
- Crossovers at 200 Hz and 3 kHz are unvalidated against competitors; check on bass-heavy, vocal-led and sparse tracks.
- Exact competitor heights, dimming and accessibility settings are unsourced; read the vendor manuals. Do not compare pixel for pixel (clean-room).
- Light-theme additive blending is untested; the multiplicative formula may look muddy.
- Decision needed: replace the full-wheel centroid mapping (`hue.rs`, `wave_stops`, format 2 centroid and spread bytes) or keep it behind a "Spectrum" style. The data already stored permits both.

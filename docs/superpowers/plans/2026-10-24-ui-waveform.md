# UI waveform redesign: fixed band hues, peak silhouette, readable markers

Status: merged in PR #115 (polish in PR #116).

> For the engineer: work test-first (superpowers:test-driven-development), one task per commit, `cargo fmt` + `cargo clippy -p om-analysis -p om-ui -p om-ui-gpui --all-targets` clean before each commit. Do NOT touch `Cargo.lock` (it has an unrelated local diff) and add NO new crate dependencies. Research basis: `docs/design/research/waveform.md` (read it first, sections 2 and 3).

## Goal

The fresh `docs/screenshots/main-dark.png` shows the waveform as three flat blocks (blue, amber, white) with no shape. Cause: each of the three band shapes is sized from a Gaussian guess (`bands.rs::band_fractions`, from centroid and spread), the shapes are nested opaque silhouettes, and the demo tracks are sine sweeps. Target: a Serato/rekordbox-style waveform.

- Fixed band hues blended additively per column: low red, mid green, high blue (dark and daylight tables below), max-normalised, so the dominant band reads at full strength and mixed sections go yellow/cyan/white-ish.
- Real per-band peaks from the FFT bin energies (not a Gaussian model), full-band peak as the visible silhouette (outline plus translucent body), peak-shaped heights.
- Palettes: Colour (dark, daylight), Three-band (the old nested look, kept), Mono, Colour-blind (Okabe-Ito). Marker polish: beat grid with bar numbers, cue and loop markers, played portion dimmed.
- Verifiable on a non-sine demo (synthetic kick/bass/hat loop) so screenshots show real shape.

### Decision: the hue-wheel centroid mapping is dropped

Delete `om-ui/src/waveform/hue.rs`, `colour.rs` (`wave_colour`, `wave_hsl`, `hsl_to_colour`, `is_audible`, `loudness_level`), `om-ui/tests/wave_colour.rs`, and the centroid and spread fields of `WaveformPoint`. Nothing in the GPUI layer paints with them already (`mod colour` comment in `waveform.rs` says so); users are the tuning example and its tests, which this plan rewrites. Risk is low except for one thing: the theme tokens `wave_stops`, `wave_saturation_max`, `wave_lightness_*`, `wave_floor_db`, `wave_spread_*` and the JSON `"waveform"` section are SHARED (`om-ui/src/theme/tokens.rs`, `theme/json.rs`, `tests/theme_json.rs`, both theme JSON files). Do NOT remove them in this plan: leave them parsed but unused (add a `// unused since the band palette; remove in a theme cleanup` comment in `tokens.rs`, the one-line shared edit). Removing them is a follow-up owned by the theme plan.

## Ownership and shared edits

Owned (free to change): `crates/om-analysis/src/waveform*` (incl. `waveform/tests*`), `crates/om-ui/src/waveform.rs` and `crates/om-ui/src/waveform/*`, `crates/om-ui/examples/wave_strips/*`, `crates/om-ui/tests/wave_colour.rs` (deleted) plus new `crates/om-ui/tests/wave_palette.rs`, `crates/om-ui-gpui/src/waveform_paint.rs`, `crates/om-ui-gpui/src/stage_view.rs`.

Shared files, minimal edit only (keep each diff tiny so merges are trivial):

| File | Edit |
|---|---|
| `crates/om-analysis/src/spectrum.rs` | Additive: a `pub(crate) fn frame_bands(&mut self, samples, index) -> [f32; 3]` on `FrameAnalyzer` (band energy shares from the same FFT magnitudes `frame` computes; share the windowing/FFT code through a private helper, do not duplicate). No change to existing fns. |
| `crates/om-analysis/src/lib.rs` | Re-exports: drop `CENTROID_*`/`SPREAD_*` re-exports if unused, add `WAVEFORM_FORMAT_VERSION` already exported; add `pub use waveform_demo::demo_loop;`. |
| `crates/om-ui/tests/library_common/mod.rs` and `crates/openmixer/tests/automix_rig/mod.rs` | 2 lines each: the `WaveformPoint { centroid: 0.3, spread: 0.2, .. }` literals become `low: 0.5, mid: 0.2, high: 0.1`. |
| `crates/om-ui/src/theme/tokens.rs` | Comment only (see above). |
| `crates/openmixer/src/library_smoke.rs` | In `ingest_generated`, write `smoke_0.wav` from `om_analysis::demo_loop` instead of a sweep (new 15-line `write_loop_track` next to `write_sweep_track`; one call-site change). Sweeps stay for the others. |
| `tools/screenshots.sh` | One new scene `main-waveform` (and its line in the header comment): reuse `capture` with `--demo-library`; no change to existing scenes. |
| `crates/om-ui-gpui/src/deck_view.rs` | NOT owned. Bar numbers and cue flags are elements there (`label(...)`, around lines 485-590). Only change them if Task 8 needs it, as a one-line colour/size swap to the new constants; otherwise leave. |

Persisting the style choice (a Settings > Appearance control and a prefs key) is out of scope: Task 7 exposes `waveform_paint::set_style(WaveStyle)` and a `WaveStyle::from_name` parser; wiring to settings is a follow-up in the settings plan. Default is `Colour`.

## Data model (format 3)

- `WaveformPoint { min, max, loudness, low, mid, high }`, all f32. `low/mid/high` are the band peaks: `peak * sqrt(share_band)` where `peak = max(|min|,|max|)` and `share` sums to 1 from true FFT bin energy with raised-cosine crossovers at 200 Hz and 3 kHz, +/- 0.25 octave in log frequency (research section 3). Each band peak is therefore at most the full-band peak.
- Byte layout, still 6 bytes per point: `min, max, low, mid, high, loudness` (levels use the existing sqrt quantiser; min/max the signed one). `WAVEFORM_FORMAT_VERSION = 3`; `WaveformAnalyzer::version()` = 3 so cached format 2 results are recomputed.
- Read format 2 (centroid and spread bytes) and format 1 (silhouette) still: format 2 maps to band peaks through the old Gaussian model (move `band_fractions` and `normal_cdf` from `om-ui/.../bands.rs` into `waveform_codec.rs` as `legacy_v2_points`, with their tests), format 1 stays a silhouette (`has_spectrum() == false`). This keeps old caches drawable until re-analysed, and all existing codec tests keep a legacy home.
- `merge` (pyramid, strip columns): widest min/max, band peaks = max of the band peaks (peak-hold, matching Mixxx), loudness RMS.

## Rendering model

`Column { peak, loudness, low, mid, high }` replaces centroid/spread. For each column:

1. Heights: `h = (peak / reference)^0.6` (existing `compress`, `reference_peak`); mirrored, `FILL` 0.94, min half height 0.75 px (existing).
2. Colour: `w_i = (band_i / peak)^0.7` (relative band weight, so a column's hue is about spectrum balance, its height about loudness), `c = sum(w_i * colour_i)`, `c /= max(c.r, c.g, c.b)`; lerp 0.15 toward white when all `w_i > 0.6`; brightness floor 35 percent. Daylight: multiplicative form `1 - sum(w_i * (1 - colour_i))`, then darken until it meets the contrast floor against the strip well (4.5:1 body, 3:1 grid/playhead).
3. Draw (bucketed paths, no image, no dependency): quantise each column's normalised weights to 4 levels per band (`round(w_i * 3)`), key = `(l, m, h)`, cap at 24 buckets by merging the rarest into the nearest key. Layer A: the full-band peak silhouette as one path in the Silhouette colour at alpha 0.35 (this is the "visible silhouette"; it shows where the loud peaks are even where the body is shorter). Layer B: per bucket one path made of the contiguous runs of columns in that bucket (each run a closed mirrored polygon, one extra shared edge column at each run boundary so runs do not gap). Total at most 25 `paint_path` calls per strip; the three-band style keeps its 3 nested paths.
4. Played portion: keep the existing dark overlay quad (alpha `1 - PLAYED_BRIGHTNESS`). Add a 1 px brighter top/bottom edge only if the screenshot reads flat.

## Palettes (constants in new `om-ui/src/waveform/palette.rs`)

| Band | Dark | Daylight | Colour-blind (both) | Mono (luminance only) |
|---|---|---|---|---|
| Low | `#FF3B3B` | `#C62828` | `#E69F00` | 55% grey |
| Mid | `#35E06A` | `#1B8F45` | `#F0E442` | 75% grey |
| High | `#3D8BFF` | `#1D5FD1` | `#56B4E9` | 95% grey |
| Silhouette | `#8A94A6` | `#5B6475` | same | same |

Colour-blind on daylight darkens by the same contrast rule. Palette choice: `WaveStyle { Colour, ThreeBand, Mono, ColourBlind }` plus dark/light derived from the strip well luminance (no new theme token, so no shared edit). The existing `LOW_COLOUR`/`MID_COLOUR`/`HIGH_COLOUR` (blue/amber/near-white) become the ThreeBand palette.

Markers: loop colour `#2EE59D` is close to mid green. In Colour style draw loops with a 3 px top bar and a 2 px diagonal hatch (alpha 0.12 lines at 6 px spacing) beside the 0.18 fill; hot cue and main cue colours stay (`#FF6B35` etc). Bar lines 2 px at 0.70, beat 1 px at 0.25, bar number 12 px with 48 px minimum spacing (all existing constants). Coincident cue flags stack vertically (extend `cue_flags` in `markers.rs`).

## Tasks (each: write the failing test, see it fail, implement, see it pass, commit)

**Task 1. Band energy shares from the FFT (om-analysis).** Tests first in `om-analysis/src/waveform/tests.rs` using `test_signals`: (a) a 60 Hz tone gives `low > 0.9 * peak`, `mid, high < 0.1 * peak`; 900 Hz mid; 9 kHz high. (b) Bimodal: 60 Hz plus 9 kHz at equal gain gives `low` and `high` both within 3 dB of each other and `mid < 0.2 * low` (the case the Gaussian model got wrong; this is the regression the redesign exists for). (c) every band peak `<=` full-band peak and shares sum to 1 (+/- 1e-3 before sqrt). (d) smoothness: a 150 -> 300 Hz sweep moves share across the 200 Hz edge with no step larger than 0.15 per 2.9 ms point. Implement `FrameAnalyzer::frame_bands` (spectrum.rs, additive) and use it in `WaveformBuilder::build`; keep the loudness path.

**Task 2. Format 3 codec and data (om-analysis).** Tests first: round trip within quantiser error for `low/mid/high`; `WAVEFORM_FORMAT_VERSION == 3`; a hand-built format 2 payload still decodes (via `legacy_v2_points`) with band peaks `<=` peak and a bass centroid giving `low` largest; format 1 stays silhouette; `merge` peak-holds bands. Update the cost tests (`waveform/tests/cost.rs`): time ratio vs legacy pass must stay under 3.0 (print the number; record it in the commit message) and memory under 1.5 (unchanged: still 6 bytes). Bump `WaveformAnalyzer::version` to 3 and its doc comment. Update the shared test literals (table above).

**Task 3. Demo signal (om-analysis, new `waveform_demo.rs`, public `demo_loop`).** `pub fn demo_loop(seconds: f64, bpm: f64, rate: f64) -> Vec<f32>`: four-on-the-floor kick (reuse the kick shape of `test_signals::add_kick`, copy the 10 lines; `test_signals` is cfg(test)), an offbeat 55 Hz saw bass with an 8 bar filter-open amplitude ramp, closed hats on 8ths (high-passed noise, 25 ms decay, open hat every 4th), a mid stab (a few sines 400-1200 Hz) on bar 3 and 4 of every 4, a 4 bar breakdown (kick and bass muted) at the middle so the waveform shows quiet and loud sections. Peak limited to 0.9, deterministic (seeded). Tests: length and rate; peak <= 0.9 and > 0.5; deterministic across calls; analysing it gives all three band peaks above 0.2 somewhere, low dominant on kick beats (first 30 ms after a kick) and high dominant in the hat gaps (between kicks, in the breakdown); the breakdown region's mean peak is under half the average of the rest. Export from `lib.rs`.

**Task 4. Palette and colour math (om-ui, pure).** New `om-ui/src/waveform/palette.rs` and `om-ui/tests/wave_palette.rs`; delete `hue.rs`, `colour.rs`, `tests/wave_colour.rs` and their re-exports; replace `Column` fields; in `bands.rs` remove the Gaussian code (moved in Task 2) and make `band_heights` read the stored peaks. Tests first:
- Hue ordering: with the dark table, a pure-low column's hue is within 20 degrees of 0, pure-mid within 20 of 130, pure-high within 20 of 215; low < mid < high on the hue circle in that order.
- Dominance: a column with `low 1.0, mid 0.1, high 0.1` has red channel >= 0.9 and a larger red than any other channel by 0.3; max-normalised: `max(r,g,b) == 1.0` (+/- 1/255) for every non-zero input; zero input gives the Silhouette colour at the brightness floor.
- Mixed: low+mid equal gives yellow-ish (r and g within 0.1 of each other and both > b + 0.3); all three at 1.0 gives near-white (every channel > 0.85, the 0.15 white lerp applied).
- Brightness floor: no non-zero column has luminance below 35 percent of its own hue's full-strength luminance.
- Contrast: for every style and both backgrounds (dark well `#0B0D12`-ish from the theme, daylight well from `openmixer-daylight.json`, read via `parse_theme` as the example does), every band's pure colour has WCAG contrast >= 4.5:1 against the well; playhead and bar-line colours >= 3:1. A test failing here means adjust the palette constants, not the threshold.
- Colour-blind: simulate deuteranopia and protanopia (published Machado 2009 severity 1.0 matrices; write them as consts in the test) and require the three band colours to differ pairwise by at least 0.12 in relative luminance OR 0.15 in the simulated Lab-ish distance (use plain Euclidean on linear RGB, `>= 0.2`), which Okabe-Ito satisfies and the red/green Colour palette does not (assert that too, to document why the style exists).
- Mono: luminance strictly increases low < mid < high, saturation is 0.
- Daylight: the multiplicative blend never yields a colour lighter than the well (no washed-out bars).
- Bucketing: for 10,000 random columns, `bucket_key` yields at most 24 distinct keys; `bucket_colour(key)` is within 0.12 (linear RGB distance) of the exact colour for 95 percent of columns; a constant column maps to the same key every time.

**Task 5. Shape model (om-ui, pure).** Tests first in `bands.rs`/`strip.rs` test modules:
- A column's body height is `compress(peak/reference)` and independent of band balance (changing balance never changes height, only colour).
- Silhouette: outline height equals the full-band peak height, `>=` body height, and for any audible column both are `>= MIN_HALF_PX` once converted to px (add `shape_px(height_frac, strip_h) -> f32` and test at strip heights 32, 48, 96, 140).
- Peak shaping on the demo loop (`demo_loop` analysed in the test, `om-analysis` is already a dependency): the overview body has a spike-to-trough ratio (95th percentile height / 20th percentile height of kick-aligned columns vs hat-gap columns) of at least 1.8, i.e. real shape, and fewer than 5 percent of columns are exactly full height (the previous flat blocks failed both).
- Runs: `runs(keys: &[Key]) -> Vec<(Key, Range<usize>)>` partitions the columns exactly (no gaps, no overlap, union is all columns), adjacent runs differ in key, and `bucket_runs` returns at most 24 distinct keys. A strip of 1,000 columns of alternating keys still partitions correctly and the run count equals the number of key changes plus one.
- `BandStrip` now caches `(height, key)` per column; `visible()` still returns only the on-screen window plus one column each side (keep the existing cache plan tests green).

**Task 6. Painting (om-ui-gpui `waveform_paint.rs`).** Replace `band_paths` with `column_paths`: Layer A silhouette path, Layer B per-bucket run paths (Colour, Mono, ColourBlind), and the unchanged 3-nested-paths route for ThreeBand and for silhouette-only data (`!has_spectrum()`). Keep the overview cache keyed on `(track, bounds, style, dark)`; the zoomed strip rebuilds only the visible window from cached `(height, key)`. Thin code, so test only the pure parts: a unit test that the number of paths returned for the demo-loop overview is `<= 25`, and that style or theme changes invalidate the overview key (`overview_key(...) != overview_key(...)`). `set_style`/`style()` use a thread_local `Cell<WaveStyle>`; dark/light derives from `scene.well` luminance. Remove the `spectral` bool in favour of `has_spectrum()` fallback to the grey silhouette (existing behaviour).

**Task 7. Style plumbing (owned files only).** `WaveStyle` in `om-ui/src/waveform/palette.rs`: `from_name("colour" | "three-band" | "mono" | "colour-blind")`, `name()`, `ALL`. Test round trip and unknown name returns `None`. Env override `OPENMIXER_WAVE_STYLE` read once in `waveform_paint` (for screenshots and CVD review; documented in the example help). No settings UI here.

**Task 8. Markers and grid polish (markers.rs, paint, stage_view).** Tests first in `markers.rs`: (a) `bar_label_step` yields labels at least `BAR_NUMBER_MIN_SPACING_PX` apart at every zoom step 1..32 beats and width 1280 and 1920; bar numbers start at 1 and are `bar_number(beat, 4)`; (b) loop hatch: `loop_hatch_lines(x0, x1, spacing)` returns lines inside `[x0, x1]` only, none for spans under 8 px; (c) coincident cues: `cue_flags` of two cues within 18 px get different `row` values (add a `row: u8` field, stack at 16 px offsets) and each flag stays inside the strip; (d) overview cue tick plus flag square uses the cue colour and never the band hues (palette-disjointness test: every `CUE_PALETTE` entry and `MAIN_CUE_COLOUR` is at least 60 degrees of hue away from the three band hues or differs in luminance by 0.25). Paint the hatch and top bar for loops in Colour style. `stage_view.rs`: keep the layout, make the accent bar 3 px full height as now, and assert (layout test in `om-ui/src/widgets/stage.rs` is NOT owned, so only add a `stage_view` unit test if one already exists; otherwise verify visually) that two zoom strips at 96-140 px stay visible at 1280x800 (they hide below 32 px). If the screenshot shows bar numbers unreadable on the new colours, change their colour in `deck_view.rs` to the grid white with a 1 px dark shadow (that is the one allowed shared edit there).

**Task 9. Tuning example and screenshots.** Rewrite `om-ui/examples/wave_strips` to render the new model (it already writes PNG without dependencies): remove hue stats and the hue-span guard; add `--synthetic` (uses `demo_loop`, no input tracks needed), `--style colour|three-band|mono|colour-blind`, both themes in one run, a key row drawing the three band swatches, and a bucket histogram printout. It must call the same pure functions as the painter (palette, bucket keys, shape_px), not copy the maths. Then add the `main-waveform` scene to `tools/screenshots.sh` (and regenerate `docs/screenshots/main-dark.png`, `main-daylight.png` since the waveform shows there; the earlier local diff of `main-dark.png` is a fresh capture, so keep or recapture, do not mix).

**Task 10. Perf and budget gate.** Add `om-ui-gpui` or `om-ui` test `waveform_budget` (pure part only, in `om-ui/tests/wave_budget.rs`): on a 6 minute demo-loop analysis, building the overview columns for 2 overviews at 600 px and 2 zoomed strips at 1280 px (cached 3-view window) takes under 50 ms total in release and the per-frame scroll path (`visible()` + bucket runs) under 0.3 ms per strip; print the numbers (`-- --nocapture`), assert with generous 5x headroom so CI noise does not flake (`#[ignore]` the strict version, run by hand with `--release -- --ignored`). Counts to assert exactly: paths per strip `<= 25`, vertices per strip `<= 2 * (columns + 2 * buckets)`. Then run the app (`cargo run -p openmixer --release -- --demo-library --mock-device`) with both decks playing and read the debug panel frame time; budget is no regression above 10 percent versus `main` for the frame time with 2 strips and 2 overviews, and analysis time within the existing 3x cost test. Record all numbers in the final commit message.

## Verification

1. `cargo test -p om-analysis -p om-ui` and `cargo test -p om-ui-gpui` (only the pure parts), then `cargo test -p openmixer --test automix_rig` and `--test smoke` because shared test literals changed.
2. Strips offline: `cargo run --release -p om-ui --example wave_strips -- --synthetic --theme dark --out /tmp/ws` then `--theme daylight`, and each `--style`. Open the PNGs. Pass criteria, by eye: kick beats read as tall red-dominant bursts, hat gaps as thin blue-dominant lines, bass sustain as a red body, the breakdown visibly quieter, and a visible silhouette outline above the body on mixed columns. Compare to `docs/screenshots/main-dark.png` (flat blocks) as the before.
3. Real window: `tools/screenshots.sh main-dark main-daylight main-waveform` (needs DISPLAY and ImageMagick; `SKIP_BUILD=1` to reuse a build). Check deck A loaded with the demo loop shows shape, beat grid with bar numbers on the zoomed strip, the played portion dimmed, and the playhead readable. Record a before/after pair in the PR description.
4. Colour-blind and Mono: `OPENMIXER_WAVE_STYLE=colour-blind tools/screenshots.sh main-waveform` (and `mono`) and eyeball the result; the CVD simulation tests are the gate, the eyeball is the sanity check.
5. Tempo caveat: the demo tracks show BPM `--` until the background analyser finishes, so the beat grid only appears in the `main-waveform` capture if the screenshot delay is long enough; copy the delay `main-automix` uses. If the analyser cannot find the beat of `demo_loop` (a test in Task 3: `BeatGridAnalyzer` on the loop returns 124 +/- 0.5 BPM, add it if the analyser is reachable from `om-analysis` tests), fall back to the click-track demo for the grid screenshot.

## Cost and performance budget

- Analysis: format 3 does the same one FFT per hop plus a three-bin-range sum per frame (O(bins)); gate with the existing cost test, time at most 3x the format 1 pass, memory at most 1.5x (unchanged at 6 B/point).
- Draw calls: at most 25 paths per strip plus 4-8 quads (grid, loop, playhead), at most 4 strips visible: about 120 path draws per frame worst case. The three-band style stays at 3 per strip.
- CPU per frame: overview paths are cached (cloned per frame as today); the zoomed strip rebuilds on scroll from cached `(height, key)` only, target under 0.3 ms per strip in release.
- Memory: one `(f32 height, u8 key)` per column instead of three f32 (saves about 60 percent in the strip cache).
- Hard rule: if frame time regresses over 10 percent in Task 10, lower buckets to 16 (4 levels collapse to 3 per band: 3x3x3 = 27, cap 16) before considering any other change.

## Risks

- True per-band time-domain peaks need a filter bank; this plan uses `peak * sqrt(share)` from FFT frames (46 ms window, so band peaks smear over about 3 frames of 2.9 ms points). A kick and a hat inside one window share the same shares. If the demo screenshots look smeared, upgrade the builder to run two biquad crossovers (use `om-dsp` if it is already a dependency of `om-analysis`; check `Cargo.toml`, do not add one) inside `waveform.rs` only, with the same format 3 bytes.
- Path anti-aliasing and thin runs: bucketed runs abut; check for hairline seams between neighbouring buckets at fractional device scale (the shared edge column in step 3 is meant to prevent it). If seams appear, overlap runs by 1 px.
- Additive hues are only a reference to what Serato and rekordbox look like; do not copy any pixel layout (clean-room, ADR 0001). Crossovers 200 Hz and 3 kHz are our own choice: check on a bass-heavy, a vocal-led and a sparse track (use `wave_strips` with real files).
- Daylight multiplicative blend may look muddy (research section 4). The contrast tests keep it legible, but review the daylight PNG by eye and tune constants before merging.
- Removing the `centroid`/`spread` fields breaks any branch that constructs `WaveformPoint` by literal: the two shared test helpers are the known ones (grep `centroid:` across the workspace and parallel worktrees under `.claude/worktrees` before merging).
- Format change invalidates every cached waveform: first launch re-analyses the library (the analyser version bump handles this); mention it in the PR.
- Mono and colour-blind styles have no settings UI until the settings plan lands; until then the env var is the only switch.

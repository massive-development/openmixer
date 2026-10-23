# Spectrum waveform (E10): design

Status: proposed. Tracks TODO.md item E10.

## Summary

The waveform is the centre of the screen (`docs/PRODUCT.md`). A DJ should be able to read the music from colour alone: where the kick is, where the vocal enters, where the hats open up, where the break and the drop are. The current waveform shows two hues in large flat blocks. Comparing the same track in OpenMixer and in the leading DJ programs shows those programs using the whole hue range, with neighbouring columns changing colour as the sound changes. E10 closes that gap.

Observed in a same-track comparison: the reference strips (zoomed and overview) span red, orange, green, cyan, blue and magenta, with near-white where high frequencies dominate, and show bar numbers on the beat grid. OpenMixer uses two hues in both strips and shows no bar numbers.

## Current behaviour

- `om-analysis/src/waveform.rs` splits the signal into three bands (below 250 Hz, 250 Hz to 4 kHz, above 4 kHz) and stores one RMS per band per point: `WaveformPoint { min, max, low, mid, high }`, 5 bytes per point, `WAVEFORM_FORMAT_VERSION` 1.
- `om-ui/src/waveform.rs` colours a column by blending three theme tokens (`wave_low`, `wave_mid`, `wave_high`), weighted by each band's energy relative to the track average. A blend of three fixed colours cannot leave the triangle between them.
- Three numbers cannot distinguish a kick from a bass line, or a vocal from a snare.

## Goals

1. The analysis keeps enough spectral detail per point to separate kick, bass, vocal, snare, hats and air.
2. Colour is a function of the sound: spectral centroid (log frequency) sets hue across the full wheel, loudness sets brightness, spectral spread sets saturation, and high-frequency-dominated material washes toward white.
3. The overview and the zoomed strip use the same mapping, so a section looks the same in both.
4. The hue curve is theme data, defined by both built-in themes, so the look is tunable without code changes.
5. The mapping is verified by tests: synthetic signals land on distinct, ordered hues, and real tracks use a wide hue range.
6. No regression to the real-time or frame budgets: analysis runs off the audio thread and painting stays within the budget in `docs/PRODUCT.md`.

## Non-goals

- Bar numbers on the beat grid, cue and loop regions, and structure hints (E2), and progressive fill-in while the file is read (E2a). The design must not block them.
- A live spectrum analyser or spectrogram view.
- Changes to the strip layout, zoom levels or seek behaviour.
- Reproducing another program's palette or look (see the clean-room rule in `agents.md`).

## Constraints

- Linux first; GPUI for drawing; no `unsafe` outside `om-stretch`.
- Analysis runs on the `WorkerPool` and is cancellable through `AnalysisContext`.
- Waveforms are cached in SQLite as JSON with base64 points. A new payload bumps `WAVEFORM_FORMAT_VERSION`; old entries must be rebuilt, never misread.
- The theme token set is shared with E1 (`crates/om-ui/src/theme`). Both built-in themes define every new token, and a theme file without the new keys inherits the built-in defaults.

## Design

### Analysis

Each point is computed from an FFT over 8 to 12 log-spaced bands and reduced to loudness, spectral centroid and spectral spread. The `min` and `max` sample range is kept as the silhouette; colour does not change the shape. FFT size, hop, window function, band edges and smoothing across neighbouring points are chosen during implementation and recorded in the PR.

### Point format

A point stores up to 6 bytes: `min`, `max`, loudness, centroid, spread, and one spare byte. The centroid is stored on a log-frequency axis, which gives steps of roughly 1/30 octave at 8 bits and avoids visible hue banding. `WAVEFORM_FORMAT_VERSION` becomes 2.

Pyramid levels merge points as follows: loudness as energy (RMS), the centroid energy-weighted in the log-frequency domain, and spread as pooled variance. A test checks that the overview and a zoomed strip produce the same hue for the same section.

### Colour mapping

A pure function in `om-ui`, next to the code it replaces, maps a column to a colour. The hue curve is a short list of stops (log-frequency position to hue, saturation scale and lightness scale) read from the theme, with limits on overall saturation and lightness so both themes stay legible. Brightness follows a perceptual loudness law with a floor, so quiet passages stay readable but dim.

`wave_low`, `wave_mid`, `wave_high` and `BandBalance` are removed once nothing reads them. The stage view and the mini-overview move to the new function in the same change.

### Cache migration

A cached waveform in format version 1 is treated as missing. The track loads immediately, the strip shows a neutral silhouette without hue until the new analysis finishes, and the rebuild queues on the `WorkerPool` behind loads the user requested. Progressive fill-in remains E2a.

### Cost limits

Waveform analysis for a track takes at most 3 times the current time on the same fixture, and per-track memory stays within 1.5 times the current pyramid. Benchmark tests record both and fail above the limit.

## Verification

- Synthetic signals: 60 Hz, 1 kHz and 10 kHz tones map to three clearly different hues in order of frequency; white noise maps to low saturation.
- Hue-range test: the 5th to 95th percentile circular span of per-column hue, over columns above the loudness floor, on the synthetic-mix fixtures and the reference tracks. The threshold is set from a first measurement, recorded with its numbers in the PR, and then locked. The test guards against collapse to a few hues; it does not assess taste.
- Overview and zoomed strips give the same hue for the same section.
- Old-format cache entries rebuild without error.
- Demo screenshots from the mock device show a wide hue range in both themes.
- A tuning tool renders a track's strips to PNG so the result can be reviewed on real audio. Reference tracks are not committed. The first reference track is "Make some room" by Renaux, with two or three more full tracks in other styles (heavy bass, vocal-led, sparse) added before the threshold is locked.

## Out of scope for the sandbox

Judging the result on real music and a real display is done by the maintainer. Competitor screenshots are not committed to the repository.

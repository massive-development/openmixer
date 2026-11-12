# ADR 0008: The Machined Faceplate supersedes E1 on look

## Status

Accepted

Decided by the project owner on 2026-11-08 (Phase 0 of the faceplate redesign; the answers are recorded in [`openmixer-workbench/docs/design/faceplate-decisions.md`](https://github.com/massive-development/openmixer-workbench/blob/main/docs/design/faceplate-decisions.md)). Docs only: no code changes with this ADR. The app keeps its E1 tokens until the build phases migrate them.

## Context

[`openmixer-workbench/docs/design/E1-brand.md`](https://github.com/massive-development/openmixer-workbench/blob/main/docs/design/E1-brand.md) set the app's first look: a "lit console in a dark room" with an ember brand colour for the playhead, master and wordmark, cool deck accents (glacier, orchid, mint, brass), a near-black ground with a violet cast, translucent glass cards, and the rule "active means glow" (a lit control carries a soft halo in its accent). The bundled interface font is Inter.

The project's design system, the Machined Faceplate (`openmixer-workbench/docs/design-system/DESIGN.md`, with the canvas `Design System.dc.html`), replaces that look. The website already uses it; the app is the last surface still on E1. `docs/PRODUCT.md` item 4 asked for "glow and contrast on active states", which contradicts the new system's "no glow" rule, so the bar and the look have to be brought back into line before code is written.

## Decision

The Machined Faceplate is the app's look. Where it and E1 disagree, the faceplate wins. In particular:

1. **Ember is retired.** No brand colour marks the playhead, master or wordmark. The `brand` ember token goes away during the migration.
2. **No glow.** Active states are never shown with halos, blur or bloom. An "on" control is a deck-hue tint (22% over the control surface) with a deck-hue border (70%), raised to 32% and 85% on hover. Playing transport is a solid deck fill with ground-coloured text. Depth comes from 1px etched lines and tonal steps; raised controls get one small drop shadow and nothing else casts one.
3. **Warm matte ground.** Matte warm neutrals replace the blue-black gradient backdrop and glass cards: ground `#121211`, panel `#181816`, control `#22211e`, well `#0a0a09`, etched line `#302e2a` (dark theme; daylight values are in `DESIGN.md`).
4. **Deck hues.** Amber (A, `#e3a24f`), Slate (B, `#86a9cb`), Sage (C, `#9cb585`), Clay (D, `#c9705c`). A deck hue never appears without its letter.
5. **Signal red for playhead and record only.** Signal Red (`#ef5a3f`) marks the playhead and recording and nothing else. It is never a deck colour and is not used for errors.
6. **New type.** Sansation 700 for the wordmark and headings (21px and up only), Source Sans 3 for all interface text, Ubuntu Mono for every number. Inter is replaced. Licences are checked before the fonts are bundled (Phase 2).
7. **Fibonacci spacing.** Spacing steps 3, 5, 8, 13, 21, 34; corners 2px on controls and 3px on large panels.
8. **Two densities.** Roomy and Compact share one structure; Compact is Roomy shifted one spacing step down (control height 26 to 21, knob 34 to 26, pad 55 to 34, row 34 to 26).

E1's other rules survive where the faceplate has no conflicting rule: restraint (colour only where it carries meaning), numbers as the clearest text, and the contrast floors (numerals 7:1, lit controls 3:1). E1's motion tokens are not carried over as design rules; motion is revisited in the verification phase.

`E1-brand.md` stays in the repo as a record, marked superseded.

## Consequences

- `docs/PRODUCT.md` item 4 no longer asks for glow; it asks for a deck-hue tint and border on active states and keeps the contrast requirement.
- The theme tokens, the three built-in theme files, the fonts, the control primitives, the waveform colour and the main layout all change, in the phases of the redesign plan. Glow, gradient caps, blur and the ember colour are removed from the UI crates by the end.
- Saved theme ids stay the same, so user preferences keep working; the theme JSON loader must keep accepting retired keys (with a warning) so user theme files do not break.
- Bundling three font families changes the binary size and `NOTICE`; each licence has to be compatible with AGPLv3 distribution.
- The waveform's default colour changes (the Low-Band rule: lows in the deck colour, highs in neutral), which `docs/PRODUCT.md` item 1 ("colour carries the spectrum") will need to reflect once Phase 4 is judged on real music.
- Contrast and colour-blind separation become tested properties of the new tokens (Phase 1).
- Risk: some of the look (inset lines, top highlights, font weights, letter-spacing) depends on what GPUI can draw. Spikes in Phases 2 and 3 confirm it; a failed spike changes the approach, not the design.

## References

- `openmixer-workbench/docs/design-system/DESIGN.md` and `openmixer-workbench/docs/design-system/Design System.dc.html` (the system)
- [`openmixer-workbench/docs/design/faceplate-decisions.md`](https://github.com/massive-development/openmixer-workbench/blob/main/docs/design/faceplate-decisions.md) (Phase 0 answers)
- [`openmixer-workbench/docs/design/E1-brand.md`](https://github.com/massive-development/openmixer-workbench/blob/main/docs/design/E1-brand.md) (superseded look)
- [`docs/PRODUCT.md`](../PRODUCT.md) (the bar), [`openmixer-workbench/docs/design/review-gates.md`](https://github.com/massive-development/openmixer-workbench/blob/main/docs/design/review-gates.md)
- ADR 0001 (clean room), ADR 0005 (experience-first product bar)

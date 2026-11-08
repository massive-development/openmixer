# Faceplate redesign: Phase 0 decisions

Date 2026-11-08. The owner's answers to the eight Phase 0 questions of the Machined Faceplate redesign (`docs/superpowers/plans/2026-11-05-ui-faceplate-redesign.md`; the system itself is `docs/design-system/DESIGN.md`). The owner accepted every recommendation in the plan. The decision to replace E1 is [ADR 0008](../adr/0008-machined-faceplate-supersedes-e1.md). Per [`review-gates.md`](review-gates.md), anything below marked **unsourced** is a placeholder the owner reviews, not a design value.

## D1. Replace E1 formally

**Yes.** Recorded in [ADR 0008](../adr/0008-machined-faceplate-supersedes-e1.md). [`E1-brand.md`](E1-brand.md) is marked superseded (content kept as history), and [`docs/PRODUCT.md`](../PRODUCT.md) item 4 now asks for a deck-hue tint and border on active states, no glow, with the contrast floors kept.

## D2. Fonts

**Sansation 700, Source Sans 3 400 and 500, Ubuntu Mono 500** replace the bundled Inter.

- Licences are verified in Phase 2, before any font file is committed: each must allow AGPLv3 distribution, and each licence text is bundled next to its fonts (as `Inter-OFL.txt` is today) and named in `NOTICE`. Source Sans 3 is expected to be OFL; the Sansation and Ubuntu Font Licence texts have to be read, not assumed.
- Weight selection from static files in GPUI is settled by the Phase 2 spike. Fallback: one file per weight registered under its own family name.
- The binary-size change is noted in the Phase 2 PR.

## D3. Four decks

**The main window stays two decks in this pass** (the app composes `DECKS = 2`). Deck C (Sage) and D (Clay) appear only in the theme tokens, the deck-tag chips, and the stems and sampler surfaces that already address more than two decks.

## D4. Waveform colour

**The Low-Band style is the default** (lows in the deck colour, highs in neutral on top). The current colour style stays selectable as a second style behind `OPENMIXER_WAVE_STYLE` (`crates/om-ui-gpui/src/waveform_paint.rs`). Both are judged on real music before anything is deleted, and `docs/PRODUCT.md` item 1 ("colour carries the spectrum") is updated when Phase 4 closes.

Note from the audit: the plan describes the current waveform as a spectral hue curve by centroid. `crates/om-ui/src/waveform/palette.rs` today has four styles, `colour` (the default: red, green and blue band hues blended per column), `three-band`, `mono` and `colour-blind`, and no style named spectral. "The spectral style kept as second" therefore means today's default `colour` style; the other three stay as they are unless Phase 4 decides otherwise.

## D5. Harbour Night

**Derived from dark and marked unsourced.** The design system gives dark and daylight values only. Phase 1 writes `openmixer-harbour.json` as a cool-cast variant of the dark roles (same structure, same contrast tests), labels its values unsourced in the theme file comment and the PR description, and the owner reviews it on a screenshot. It is not designed further in this pass.

## D6. Theme names

**Ids stay**: `openmixer.dark`, `openmixer.daylight`, `openmixer.harbour`, so saved preferences keep working. The display names are decided in Phase 1.

- Today: "Midnight Glass", "Daylight Frost", "Harbour Night". The glass and frost wording is retired with the look it described.
- Proposed: "Midnight", "Daylight", "Harbour".
- Chosen: **not yet.** Until Phase 1 the current names are kept (the default). Phase 1 records the final names here, replacing this line.

## D7. Controls in the comp the app may not have

Audit of the code on `main` at 850bfe7 (`crates/om-ui`, `crates/om-ui-gpui`). EXISTS means the app has it and only the look changes; RELABEL means the function exists under another name or form; NEW means it does not exist and is tracked as its own item, not built as part of this rebuild.

| Comp element | Verdict | What the app has today |
|---|---|---|
| Full / Controller toggle (header) | **NEW** | No mode switches the on-screen deck controls. The header has a LIBRARY toggle (was PERFORM; also the Tab key) that hides the library band so the stage grows (`header_view.rs`, `header_layout.rs` `library_toggle`, `app.rs` `performance`). That toggle is not this control. The comp does not say what Controller mode does; the owner defines it in its own item. The comp's library bar has a "Collapse" control, which is the likely new home of today's LIBRARY toggle (a relabel, settled in Phase 5). |
| "Compatible with A" library filter | **NEW** (the control); the logic EXISTS | Typing `key:~8A` in the search box already filters to keys compatible with 8A (`om-library` `search_parser.rs`, `search_compiler.rs`), and rows are tinted by compatibility (`library/mix_match.rs`, `track_table.rs`, `library_view.rs`). There is no one-click chip. The reference is "the playing deck, else the first loaded deck with a key" (`track_loader.rs` `reference_track`), not fixed to deck A, so the chip's label must name the reference deck. |
| "vs A" key column | **NEW** (the column); the data mostly EXISTS | `mix_match::key_relation` returns Same, Adjacent, Relative or Clash against the reference, shown only as a row tint. There is no column. The comp's values ("same", "rel", "+1", "-1", an em dash) also need the direction of an adjacent step, which `KeyRelation::Adjacent` does not carry. |
| Master tempo readout (header) | **EXISTS** (relabel of the caption only) | `header_view.rs` `master_bpm` shows the sync leader's tempo, else the first playing deck's, else deck 1's, centred in the header (`MASTER_BPM_WIDTH` 120), captioned "BPM" with one decimal (`format_bpm`). The comp captions it "Master tempo" and shows two decimals (`120.00`). |
| Delta BPM column (library) | **NEW** | No column. `mix_match::bpm_distance_percent` gives an unsigned percent distance (with half and double tempo counted as matches), used only for the tint strength. The comp shows a signed difference in BPM (`-1.0`, `+2.0`) against the reference. |

Conflict to resolve: the plan's Phase 5 step 4 lists the Delta BPM and "vs A" key cue as part of the library rebuild, while this decision says new items are tracked separately. The owner decides whether those two columns are new items or Phase 5 scope.

## D8. Density selection

**A preference with an automatic default.** Roomy and Compact are whole-window metric sets chosen on the Appearance settings page (`crates/om-ui/src/settings/appearance.rs`), with an Automatic choice as the default that picks from the window height. `MIN_WINDOW` is 1280x720 (`crates/om-ui/src/main_layout.rs`), where Roomy may not fit, so Automatic picks Compact at small heights. The height threshold is set in Phase 5 from the layout tests and marked unsourced until checked on a screenshot. Today's width-only sidebar switch (`COMPACT_WINDOW_WIDTH` 1400) stays a separate rule until Phase 5 replaces it.

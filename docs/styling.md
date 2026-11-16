# Styling a control (LookAndFeel)

Colours, radii, border widths, insets and font sizes come from the theme, never from literals in view code.

1. Read the theme's tokens (`om_ui::theme::ThemeTokens`, built from `resources/themes/*.json`).
2. Get the control's recipe from `om_ui::style::LookAndFeel::from_tokens(t)`: `face(Face, hue)` for any pressable control (rest, hover, pressed, on, on + hover, playing, disabled; `focus` is the 2px ring), `button` (idle, hover, active, disabled), `lit_button(accent)`, `knob`, `fader`, `tabs`, `tooltip`, `text_field` (idle, focus), `meter`, `panel`, `panel_caption`, `separator`, `drop_target`. Nothing glows: depth is the 1px etched edge, the 1px top highlight and, under knob and fader caps only, the one small drop.
3. In `om-ui-gpui`, draw a control face with `paint::face(el, &face_style)` (or a part from `parts`, or `controls::face_button` for an interactive button), a box with `paint::skin(el, &style)`, a bare border with `paint::border(el, width)`, and a colour with `paint(colour)`. Radii and insets come from the style fields and sizes from the density's `DensityMetrics`, not literals. `examples/controls_gallery.rs` shows every part in every state, theme and density.
4. A value no token covers: add a token field in `theme/tokens.rs` (default in `builtin_dark`), a reader in `theme/json.rs` (range-checked, with a key in the matching section), and a value in both built-in theme JSON files. Old theme files stay valid because a missing key inherits the default.
5. Add the new style to `LookAndFeel::entries()` so the contrast and live-switch tests cover it. Text on a control needs contrast 3.0 (control) or 4.5 (body) against its fill over both backdrop ends; disabled controls are exempt.
6. `crates/om-ui-gpui/tests/style_lint.rs` fails on raw colour calls (`rgb(`, `rgba(`, `hsla(`, `from_argb(`, bare 6 or 8 digit hex) in `src/`. Only `paint.rs` may name them.
7. Type: set text with a tier, `el.type_role(t, TypeRole::Label)` (`om-ui-gpui::font::TypeExt`; tiers in `om_ui::theme::TypeRole`), and draw every number with `font::numeral(t, Numeral::Value | Readout, text)`, which is always the mono family. `crates/om-ui-gpui/tests/type_lint.rs` fails on a family named outside `font.rs`, a number source drawn without `numeral`, the 10 px `Numeral::Scale` outside the fader dB scale, and a literal text size under 11 px.

8. Hue: a deck hue (`t.accent_for(deck)`) only on something that names that deck, beside its letter. Chrome that belongs to no deck (settings chips, header switches, the sampler) lights in the text colour; `LookAndFeel::accent` is that neutral, so pass the deck's hue explicitly where a control is a deck's.

Themes and type (Machined Faceplate, ADR 0008): the built-in themes are `resources/themes/openmixer-{dark,daylight,harbour}.json` (ids `openmixer.dark`, `openmixer.daylight`, `openmixer.harbour`; harbour is derived from dark and unsourced). A theme holds both metric sets, Roomy and Compact (`ThemeTokens::metrics(Density)`, JSON section `density.roomy` / `density.compact`, defaults from DESIGN.md); the window picks one with `main_layout::resolve_density` from the Appearance preference (Automatic picks by height; an explicit Roomy gives way to Compact where the library would lose its rows). Old E1 theme files still load: retired glass and glow keys warn, `recordRed` and `backdropTop` are read as `playhead` and `ground`. Fonts are bundled in `resources/fonts/` with their licences: Sansation Bold (display, 21 px and up only), Source Sans 3 Regular and Medium (all interface text) and Ubuntu Mono Regular (every number; the family has no 500, so DESIGN.md's 500 numerals draw in Regular) and Bold (deck letters and state words such as LOCK and CLIP). GPUI has no letter-spacing, so no tier carries any. Inter is no longer bundled.

gpui only has fixed border widths (0 to 12 px); a token width is rounded to a whole pixel.


## The web front end (Tauri window)

The Tauri window's front end ([docs/features/tauri-trial.md](features/tauri-trial.md)) styles from the same tokens, through CSS custom properties that `om_ui_bridge::theme_css(tokens, density)` generates. The shell sends them once at start (`theme_css` command) and the front end puts them in `<style id="om-theme">` before its first paint.

- **Naming:** every colour and numeric token field is `--om-<field>` with `_` turned into `-` (`text_primary` is `--om-text-primary`); the deck accents are `--om-accent-<i>` from 0. The metric set of the active density is `--om-density-<field>` (`control-height`, `knob-size`, `pad-size`, `row-height`, `mixer-width`, `panel-radius`, `control-radius`) and `--om-density-space-1` to `--om-density-space-6`. The window resolves the density once, for the 1600 × 780 opening size.
- **Units:** colours are `#rrggbb`, or `rgb(r g b / a)` when translucent. Fields ending `_ms` are `ms`; fields ending `_alpha`, `_db`, `_seconds`, `_level` or `_opacity`, and every `wave_*` field, are plain numbers; everything else is `px`. Non-finite values are left out.
- **Fonts:** `--om-font-display`, `--om-font-body` and `--om-font-mono` are the theme's family names as quoted CSS strings. `web/src/app.css` registers the bundled files under those names with `@font-face`; `npm run fonts` (run by `npm run build`) copies them from `resources/fonts/`. Numbers use the `.num` class (mono family, tabular figures), the web form of `font::numeral`.
- **A new token** must be added to `colour_fields` or `number_fields` in `crates/om-ui-bridge/src/theme_css.rs`; a test there fails when a `ThemeTokens` field is not emitted.
- **Waveform lanes** take their colours from `strip_ink`, which resolves the theme through the GPUI strip's `StripInk`; the shader has no colour of its own.

The no-literals rule applies to the web front end too: colours, sizes and fonts come from `--om-*` variables, never literals in component CSS or TypeScript. Debt, literals the front end uses today with no token (in `crates/om-ui-tauri/web/src/`):

- `App.svelte`: the page grid (rows 44, 104, at least 180 and 372 px; mixer columns 246 and 245 px) and the wordmark's `letter-spacing: 0.14em`.
- Font weights 400, 500 and 700 in `App.svelte`, `DeckHeader`, `DeckPanel`, `DeckTag`, `HotcuePads`, `LoopRow`, `Transport`, `Button`, `Knob`, `Mixer`, `ChannelStrip` (there is no weight token).
- Sizes derived with a literal factor: the deck tag and header letter (`calc(<type token> * 1.618)` in `DeckTag`, `DeckHeader`, `DeckPanel`) and the icon size, `1.25em` of the label (`Icon.svelte`).
- Hairlines: 1 px and 2 px pointer, centre and tick lines in `Fader.svelte` and `Knob.svelte`, the 1 px lane separator in `Waveforms.svelte`; the fader scale's tick spacing (12.5%) and the knob pointer's length (30%).
- `LoopRow.svelte`: the container query that folds the loop row below 360 px.
- `controls/drag.ts`: 160 px of drag travel for a knob's full range.

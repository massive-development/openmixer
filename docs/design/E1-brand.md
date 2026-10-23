# E1: OpenMixer brand and visual tokens

Status: tokens and the first controls done (2026-10-23). The rest of the window moves onto the tokens control by control. Bar: `docs/PRODUCT.md` point 4, ADR 0005. Nothing here copies another DJ program: the palette, the reserved brand colour and the rules below are ours; only common layout conventions are shared.

## Idea

A lit console in a dark room. The room is near-black ink with a faint violet cast; almost everything on screen is quiet. Colour is spent on only three things: the music (waveform), the state a hand needs to know (what is lit, what is clipping) and the numbers a DJ reads while mixing. One warm signature colour, **ember**, marks the brand and the "now" cues. Decks get cooler, calmer hues so ember never competes with them.

## Rules

1. **Restraint.** Surfaces are translucent white on ink, at low alpha. A control is only coloured when it is on.
2. **Ember is reserved.** `brand` is used for brand marks and "now" cues (playhead, master). It is never a deck accent (a test enforces it).
3. **Deck accents are cool and distinct.** Dark: glacier `#4CC9F0`, orchid `#B794F6`, mint `#5FD6A4`, brass `#E3D26F`. Daylight: the same hues darkened until white text on a lit button holds 3:1 (`#0B7FA8`, `#7C4DCC`, `#0B8A63`, `#8A6D00`).
4. **Active means glow.** A lit control carries its accent as fill and as a soft halo (`glowAlpha`, `glowRadius`). The halo is capped at alpha 0.6 and 0.2 minimum by test, so it reads in a dark room without bloom.
5. **Numbers are the loudest text.** BPM, key and time use `readout` (warm near-white in the dark theme, ink in daylight), at least 7:1 on the card, in the `readout` size, with the unit in `readoutDim` at 4.5:1.
6. **Depth by light, not borders.** Cards lift with one soft shadow (`shadowBlur`, `shadowOffsetY`); wells sink with a darker fill. Borders stay 1 px.
7. **Motion is brief.** Two durations only: fast (90 ms) for press and hover, base (180 ms) for state changes. Nothing over 300 ms (test).

## Token set (new in E1)

| Token | Section | Dark | Daylight | Use |
|---|---|---|---|---|
| `brand` | colours | `#FF8F5A` ember | `#C2531C` | brand marks, "now" cues |
| `readout` | colours | `#F7F3EC` | `#0F1626` | BPM, key, time numerals |
| `readoutDim` | colours | `#9AA0B4` | `#4A5670` | units and labels beside numerals |
| `accents` (changed) | root | glacier, orchid, mint, brass | darker set | deck 1 to 4 |
| `backdropTop/Bottom`, `glowCool/Warm`, `onAccent` (changed) | colours | ink with a violet cast, ice and ember glows | ice and ember glows | window ground |
| `type.readout` | type | 22 | 22 | primary numeral |
| `type.readoutSmall` | type | 14 | 14 | key, remaining time |
| `style.glowAlpha` | style | 0.45 | 0.30 | strength of a lit control's halo |
| `style.glowRadius` | style | 10 | 8 | reach of the halo (px) |
| `style.shadowBlur`, `shadowOffsetY` | style | 18, 6 | 14, 4 | card and raised-control lift |
| `style.motionFastMs`, `motionBaseMs` | style | 90, 180 | 90, 180 | transition lengths |

Type scale after E1 (px): small 11, caption 12, body 13, label 14, title 16, heading 17, mono 15, readoutSmall 14, readout 22. Labels stay quiet; only numerals get the large size.

All tokens have defaults in `ThemeTokens::builtin_dark`, so older theme files stay valid. `om_ui::style::LookAndFeel` exposes them as `readout`, `lift`, `glow(accent)`, `motion` and `brand`.

## Applied so far

- Lit buttons (`om-ui-gpui::controls::button`) draw `LookAndFeel::glow(accent)` around the lit fill.
- Deck header (`deck_view`): elapsed time, key and BPM use the readout style (numerals in `readout`, the unit in `readoutDim`). The readout divs set `line_height(relative(1.1))`: gpui's default line height is about 1.6x, which pushed the artist line out of the header.

## Not done yet

- Tabular (fixed-width) numerals so a ticking clock does not shimmer: needs a font choice and a check that gpui exposes font features.
- A bundled typeface (today the system default), and letter-spacing for captions.
- Card lift on the cards themselves, glow on knobs, faders, pads and tabs, the brand mark in the header, ember on the playhead.
- Animating the two motion durations (they are tokens only).
- Everything has been checked by contrast tests only; nobody has looked at it on a real display in a dark room.

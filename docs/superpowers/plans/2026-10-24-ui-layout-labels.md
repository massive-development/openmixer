# UI layout, labels and type: implementation plan

Date 2026-10-24, branch `feat/ui-redesign`. Inputs: `docs/design/research/labels-layout.md` (its screenshot critique is stale; section 1 below re-judges from the fresh `docs/screenshots/main-dark.png`), `docs/design/E1-brand.md`.

**Ownership.** This plan owns `crates/om-ui-gpui/src/{app,library_view,sampler_view,automix_bar,debug_panel,font}.rs`, `settings_view/*`, the label and format code (`om-ui/src/{format,mapping_editor,automix_bar,record_bar,main_layout}.rs`, `om-ui/src/settings/{audio,midi}.rs`, new `om-ui/src/labels.rs`, `om-ui/src/header_layout.rs`), theme tokens (`om-ui/src/theme/*`) and the layout rect code. **It must land first for layout.** Other plans (deck cards, mixer strips, stage) consume the rects defined in "Rect contract" and must not edit `main_layout.rs`.

**Constraints.** No new crate dependencies; do not touch `Cargo.lock` (run `git diff --stat Cargo.lock` before each commit; the working tree already has an unrelated change, never `git add` it). Commits end with the Co-Authored-By line from the session attribution. Run `cargo fmt`, `cargo clippy -p om-ui -p om-ui-gpui`, `cargo test -p om-ui -p om-ui-gpui -p openmixer`.

## 0. What the fresh screenshot shows (1280x800)

Still wrong (fix in this plan):
1. Banner row (28 px, CLI text `(--mock-device)`) stacks above a 32 px header; the three header controls have different heights and baselines (Record pill 32 high at y44, PERFORM at y44, Settings at y41, AUTO at y44). The Record pill is a wide empty box.
2. No brand mark, no master BPM, no latency/dropout indicator; ember (`theme.brand`) appears only as the negative time (red-ish) which is a wrong "now" cue. Ember must mark: wordmark, playhead, the running/elapsed clock of a playing deck, master. Remaining time stays `readout`.
3. Channel strips (x0..144 and 1136..1280) span y62..573, and deck cards and mixer span y313..573: tops and bottoms line up, good. But strips are 15% of 1280 = 144 while the mixer card is only 214 wide and the deck cards 355: the mixer is the narrowest column and is crammed (FX button orphaned top-left, `MIX`/`VOL` cryptic, `PWR`). Fix proportions: strips 128, mixer 25% of the centre column.
4. Strip EQ cells are empty wells with `HI MID LOW` kill labels above; `PFL`; `A THRU B`. Rename per research table (GAIN, HIGH/MID/LOW, FILTER, CUE, A/THRU/B ordering is the mixer plan's, labels here).
5. Library: 20 px rows, 18 px header with `Time`/`Rating 0`/`Stems None`, status line `3 tracks 3 in view`, sidebar buttons wrap (`Remove tracks`), library card touches the window bottom (no margin), sampler handle `PADS +` is a stub (and its card also touches the bottom edge).
6. Type: everything Regular weight, sizes 12/14/16 on the labels; deck readouts are the only hierarchy.

Already fine, keep: skeleton, deck accent borders, stage position, tabular numerals (font.rs sets `tnum`; verified by test, gpui `FontFeatures` is used).

## Rect contract (other plans code against this)

All in `om-ui` (pure, no gpui). Units px, `Rect {x,y,w,h}` as in `om_ui::geometry`.

```rust
// om-ui/src/main_layout.rs
pub const HEADER_HEIGHT_TALL: i32 = 40;   // window height >= 900
pub const HEADER_HEIGHT_SHORT: i32 = 36;  // below 900
pub fn header_height(window_h: i32) -> i32;
pub const CHANNEL_MIN_WIDTH: i32 = 112;   // NEW: strips never narrower
pub const CHANNEL_WEIGHT: f32 = 0.13;     // was 0.15; strip = max(13%, 112)
pub const MIXER_WEIGHT: f32 = 0.72;       // was 0.6 (share of a deck card's weight 1.0)
pub const STAGE_SHARE: f32 = 0.50;        // was 0.53
// CellSpec gains `min_width: i32` (0 = none). split() honours it like RowSpec.min_height.

// NEW: one call for the whole page.
pub struct Page {
    pub header: HeaderRects,
    pub cells: Vec<PlacedCell>,   // unchanged type: {kind: CellKind, bounds: Rect}
    pub area: Rect,               // window minus header (and debug column)
}
pub fn page_for(engine_decks: usize, window: (i32, i32), debug_w: i32, gap: i32,
                state: LayoutState, header: HeaderNeeds) -> Page;
// cells_for(...) keeps its signature and delegates, for the deck/mixer plans' existing tests.

// om-ui/src/header_layout.rs (NEW; record_bar::record_rect / automix_width become thin wrappers)
pub struct HeaderNeeds { pub automix: bool, pub record: bool, pub settings: bool, pub notice: bool }
pub struct HeaderRects {
    pub bar: Rect,                 // x0,y0,w=window,h=header_height
    pub brand: Rect,               // OPENMIXER wordmark, ember
    pub automix: Option<Rect>,     // segmented Off|On + "Transition N beats" + "Next:" text
    pub master_bpm: Option<Rect>,  // centred, 120 px
    pub notice: Option<Rect>,      // dismissible chip (replaces the 28 px banner row)
    pub latency: Rect,             // "4 ms  0 dropouts" 120 px
    pub record: Option<Rect>,
    pub library_toggle: Rect,      // was PERFORM
    pub settings: Option<Rect>,
}
```
Invariants (tested): every header rect has `y == 3` and `h == header_h - 6` (one control height, one baseline); none overlap each other or any cell; cells lie in `area`; **channel strip `bounds.y == stage.y`, `strip.bottom == deck card bottom == mixer card bottom`** (strips span stage plus controls); strip `w >= 112`; deck card `w == deck card of the other deck` (+/-1 px rounding); library band bottom `== window_h - gap` (margin, not flush). Removed: `BANNER_HEIGHT`, `TOP_BAR_HEIGHT` (replaced by `header_height`), `PERFORM_RESERVE`/`SETTINGS_BUTTON_RESERVE` in app.rs.

Library metrics (om-ui `library/browser` or `main_layout`): `pub fn library_metrics(window_h) -> LibraryMetrics { row_h: 28 (24 if window_h<900), header_h: 28, toolbar_h: 36, sidebar_w: 170 (160 if window_w<1400) }`; `ROW_HEIGHT` const in `library_view.rs` is deleted. Columns: Title flex, Artist flex, BPM 64 right, Key 56, Length 64 right, Rating 80, Genre 120, Added 96; Genre and Added hidden under 1400 px (`Column.min_window_w`).

Type scale tokens (`ThemeTokens` builtin dark/daylight and `theme/json.rs`; E1 doc table currently disagrees with code, 12/12/14/16/20/20/16/40/28 vs 11/12/13/14/16/17/15/22/14: **code is truth until Task 8, then align both to**): caption 10 (uppercase labels), small 11, body 13, label 13, title 16, heading 18, readoutSmall 14, readout 22, clock 28. New fields `font_weight_label: u16 = 500`, `font_weight_title: u16 = 600`, read through `font::weight(theme, Role)`; ignored (falls back to 400) when Medium/Semibold are not bundled.

## Tasks (strict order, tests first; commit after each)

### Task 1. Label module and its tests (red then green)
Create `om-ui/src/labels.rs` (declare in lib.rs). Tests first in the same file:
- `param_label("deck.0.play") == "Deck 1 Play"`, `("mixer.ch0.trim") == "Channel 1 Gain"`, `("mixer.ch1.eq.2") == "Channel 2 Low"`, `("mixer.ch0.fader") == "Channel 1 Fader"`, `("sync.enabled.0") == "Deck 1 Sync"`, `("deck.0.rate") == "Deck 1 Tempo"`, `("router.unknown") == "Unrouted messages"`; unknown ids pass through unchanged (existing behaviour).
- `output_pair_label(0) == "Outputs 1-2"`, `(1) == "Outputs 3-4"`.
- `notice_text(Notice::Demo) == "Demo mode: no audio output. Choose an output in Settings."`, `notice_text(Notice::NoDevice) == "No audio device: running silent."`; a property test that no label output contains `--`, `deck.`, `mixer.`, `.ch`.
- `audio_stats_text` -> `"Latency 4.2 ms, round trip 9.1 ms at 48000 Hz / 256 samples\nCPU 25% (peak 1.50 ms of 5.33 ms), dropouts 2"`.
- `channel_labels()`: `GAIN HIGH MID LOW FILTER CUE`, `loop_label`, `"Delete playlist"`, `"Remove from playlist"`, `"Load to Deck 1"`, `"Split stems"`; column title `"Length"`; `status_count(total, shown)` = `"3 tracks"` or `"3 of 120"`.
Implement by moving `mapping_editor::target_label` here (keep a `pub use`/wrapper; update its test at mapping_editor.rs:471 to "Channel 1 Fader"). Update `settings/audio.rs` (`pair_labels`, line 81, and `stats` line 55 plus tests lines 234, 312, 386) and `om-ui/src/settings/midi.rs` so learn target labels come from `param_label`; the stored id stays the dotted string (never changes on disk).
Gate: `cargo test -p om-ui labels audio midi mapping_editor`.

### Task 2. Apply labels in the owned views
Tests first (om-ui-gpui `#[cfg(test)]` on view-model functions, no window): midi learn notice `"Learning Deck 1 Play"` and `"Learned a control for Deck 1 Play and saved to ..."` (replace asserts at settings_view/midi.rs:385,395,459; the router id is passed to `midi_learn` unchanged); mapping editor row shows label, field caption `"Target"` and hint `"pick a control or type its id under Advanced"` (mapping.rs:36,46); debug panel: rows keep ids but panel title becomes `Developer`, shown only with `--debug-panel`, and the first column shows `param_label(id)` with the id in a dim second column (`parameter_rows` returns `(label, id, value)`; fix tests at debug_panel.rs:394). Sampler: `HIDE`->`Close`, `STOP`->`Stop all`, `Gain -`->steppers under a `GAIN` caption, status sentence becomes `"Kick  One-shot  +0.0 st  90%"` (sampler_binding.rs:364), empty pad `"Empty: drop a sample or press Load"`, handle `PADS +` -> vertical `SAMPLER` with chevron glyph. Library: `Library could not open: <reason>. Open Settings > Library.` (library_view.rs:103), `Remove tracks`->`Remove from playlist`, `Delete`->`Delete playlist`, `Stems`->`Split stems`, `Cancel stems`, status `N tracks`. Automix bar: `AUTO` -> segmented `AUTOMIX Off|On`, `Next: Nothing queued` (om-ui/automix_bar.rs, update its tests), state word only while running. `crates/openmixer/src/gui.rs:128`: banner string becomes `labels::notice_text(Notice::Demo)`; the no-device branch uses `Notice::NoDevice` (one-line edit, tell the CLI owner).
Gate: tests above; then a grep gate test `no_dotted_ids_in_user_strings` in om-ui-gpui that scans `settings_view/*.rs` and `automix_bar.rs` literals for `deck.0`/`mixer.ch` outside `#[cfg(test)]` and `debug_panel.rs`.

### Task 3. Header layout tests then code
Write `header_layout` tests first, at the four sizes **1280x720, 1280x800, 1920x1080, 2560x1440** (and 1100x700 degraded): all rects inside `bar`, pairwise non-overlapping, uniform `y`/`h`, ordered left to right brand < automix < master_bpm < latency < record < library_toggle < settings, `master_bpm` centred within 1 px of `w/2` when it fits, and at 1280 wide automix >= 300 px. Rewrite `record_bar::record_rect/automix_width` as wrappers so existing tests (main_layout.rs test around line 600) keep passing, then migrate that test to `page_for`. Implement `HeaderRects` and wire `app.rs::render` (delete banner row, `PERFORM_RESERVE`, hand-computed rects at app.rs:451-545). Notice chip dismiss state lives in `Shell` (`notice_dismissed: bool`). Ember: wordmark and master BPM numeral use `theme.brand`; remove the red negative time (deck plan consumes `theme.brand` for the elapsed clock of a playing deck).

### Task 4. Page layout tests then constants
Tests first in `main_layout.rs`, parameterised over **(1280,720), (1280,800), (1920,1080), (2560,1440)** with 2 decks and with 4 decks, state default and performance, sampler open and closed: (a) no two cells overlap, all inside `area`; (b) invariants listed in Rect contract (strip/stage/deck/mixer alignment, strip min width, symmetric strips equal width, library bottom margin); (c) controls row >= `CONTROLS_MIN_HEIGHT` (262) at 720; library >= `LIBRARY_MIN_HEIGHT` 150 at 720 (else sampler/stage give way: assert stage shrinks first); (d) proportions: stage 0.45-0.55 of deck rows, mixer width 0.18-0.26 of window at 1920, strips 0.07-0.13. Then implement: constants above, `CellSpec.min_width`, `page_for`, `header_height`; the layout area subtracts a bottom margin equal to `gap`. Keep JSON layout files working (`min_width` optional, default 0).

### Task 5. Library table polish
Tests first on `track_table` (columns, hidden-below-width, `Length` title, rating as five-star string `"***--"` mapped to glyph in the view, sort glyph model `SortGlyph::{None,Up,Down}`), `library_metrics` sizes at 720/800/1080, playing row highlight index, BPM/key compatibility dim flag exists (`mix_match` already). Then `library_view.rs`: row height from metrics, right-aligned numerics, triangle glyphs (Inter has U+25B2/25BC? verify with the font check in Task 7, else draw with a small div), hover row, playing track tinted with its deck accent at alpha 0.18, toolbar buttons same height as header controls, sidebar buttons in a 2-row grid without wrapping (`New`, `Rename`, `Delete playlist`, `Remove from playlist` shortened with tooltip), folder basename muted.

### Task 6. Strip/mixer label constants only
This plan only supplies strings (`labels::channel_labels`) and the `CHANNEL_*` rects; the mixer plan redraws strip internals. Verify with a test that `mixer_view.rs` uses no literal from the old set (`"Trim"`, `"PFL"`, `"Lo"`).

### Task 7. Fonts: Inter Medium and Semibold (conditional)
Allowed only if all hold: (1) `resources/fonts/Inter-Regular.ttf` came from the Inter release whose licence is `resources/fonts/Inter-OFL.txt` (SIL OFL 1.1: bundling and embedding permitted, must not sell the font alone and must keep the licence text; Reserved Font Name rule applies only to modified fonts, so do not subset or rename); (2) the static `Inter-Medium.ttf` and `Inter-SemiBold.ttf` of the **same release** are available (download the official release zip from https://github.com/rsms/inter/releases, verify the sha256 against the release page, put the two files in `resources/fonts/`; ~300 KB each, ~0.6 MB binary growth, acceptable); (3) no new dependency: `include_bytes!` and `add_fonts` already exist in `font.rs`. **Stop and skip the task, keeping size-only hierarchy, if the files cannot be fetched and verified.** Add rows to `docs/legal/asset-provenance.csv` for all three files (currently the Regular file has no row): `Inter Regular/Medium/SemiBold (static TTF),"Rasmus Andersson (The Inter Project Authors)",https://github.com/rsms/inter,SIL OFL 1.1 (resources/fonts/Inter-OFL.txt)`. Tests first: extend `font.rs` tests: each bundled byte slice starts with TrueType magic and contains `tnum`; `font::weight(theme, Role::Label) == FontWeight::MEDIUM`. Register all three in `register`. Use `.font_weight()` for caption/label/title/readout roles. Run `tools/check_docs_rust.py` if it lints docs. Tabular numerals: already on (`tnum`); add a test that the readout styles go through `interface_font` (no element overrides `.font(...)`).

### Task 8. Type scale tokens and docs
Tests first in `theme/tokens.rs`: scale monotonic (caption < small < body <= label < title < heading < readout), caption >= 10, dark/daylight equal, JSON round trip for new fields, old theme files still load (defaults). Apply scale; update `docs/design/E1-brand.md` table ("Type scale after E1" and "Not done yet": tabular numerals done) and note ember rule (wordmark, playhead, playing clock, master BPM).

### Task 9. Visual verification
`tools/screenshots.sh main-dark main-daylight settings-audio settings-midi settings-mapping debug-panel main-stems` (needs DISPLAY :0 WSLg, ImageMagick, xwininfo; `SKIP_BUILD=1` after a build). Also capture the four sizes with `OPENMIXER_WINDOW=1280x720`, `1280x800`, `1920x1080`, `2560x1440` and view each PNG: check no clipped text (`Remove from playlist`, `Next: Nothing queued`), header controls share one baseline, strips line up with deck cards, library bottom has a margin, no dotted id visible in the main window/settings/MIDI/mapping screens (debug-panel scene only inside `Developer`). Commit the regenerated `docs/screenshots/*.png` (main-dark.png currently has an unrelated local modification: regenerate rather than keep it). Replace the "stale screenshot" caveat in `docs/design/research/labels-layout.md`.

## Risks
- Layout constant changes ripple into deck/mixer plans: they must rebase on Task 4 and use `page_for`/the invariants, not literals. Land Tasks 3-4 as a single early PR.
- `min_width` on cells interacts with `split` rounding: strips can steal width from the stage at 1280; guard with test (c) and allow strips down to 112.
- `Notice` strings live in `crates/openmixer` (outside the owned set): one-line edit; coordinate.
- Font download is unverifiable offline; Task 7 is skippable and the `font_weight_*` tokens must degrade gracefully (gpui synthesises no bold for missing faces: it may fall back to Regular or a faux weight; check in the screenshot).
- Glyphs (triangles, chevrons) may be missing from Inter Regular and render as tofu; check in Task 9 or draw shapes.
- The working tree already modifies `Cargo.lock` and `docs/screenshots/main-dark.png`; do not stage `Cargo.lock`, and stage only intended files (`git add <paths>`, never `-A`).
- Research doc claims (convention-only proportions) are unverified; treat ratios as starting points and judge on the screenshots.
- Hard-coded `learn_targets: vec!["deck.0.play"]` in settings_view.rs:341 is test scaffolding; keep it but ensure the visible label is humanised.

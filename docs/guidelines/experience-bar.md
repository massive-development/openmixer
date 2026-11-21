# Experience bar

`docs/PRODUCT.md` is the bar and ADR 0005 the decision. Interface work is not a wave and is not finished when the logic tests pass.

- **Look at it.** A PR that changes what the window shows includes a screenshot of the Tauri window (`import -silent` into a temp folder), and you have looked at it (not just that it rendered). Say what you saw and what is still wrong.
- **Use real music** for anything involving waveforms, the library, analysis or colour. The demo tracks are synthetic sweeps and hide most defects; `cargo run -p om-ui --example wave_strips -- TRACK...` renders a real track's waveform strips to PNG.
- **Judge it as a DJ would.** At the default window size and the minimum (`MIN_WINDOW`), can a DJ read the playing deck's BPM, key, time and phrase position at a glance, and reach EQ, transport and pads without looking? If not, say so in the PR.
- **Layout and look may change.** Replace a layout or control when a better one is found. Keep the tests that guard function (no overlap, no clipping, the minimum window); update the ones that only recorded the old arrangement.
- **Tokens, not literals** still applies (see "Styling a control"); if the look you need has no token, add one.
- **Clean-room still applies.** Borrow the reason a layout works, not another product's names, icons, artwork or pixel layout.

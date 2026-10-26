# UI work: review gates

Why: AI-built UI drifts toward what looks plausible to the model that built it. The fix is a cheap habit of asking the same few questions at each decision, not heavy review passes. Spend tokens where a decision is uncertain, not everywhere.

## The questions (asked by whoever makes a design decision)

1. Could this be done better?
2. Is there an example in this repo or in `docs/design/research/` I can reference?
3. If not, is there a well-defined public source or established convention (vendor docs, standards, manuals)? Search once, cite the URL.
4. If still nothing, mark it **unsourced** and keep it small and reversible. Do not pass it off as researched.

Clean-room rule (ADR 0001) applies: conventions only, no copied layouts or artwork.

## Spending rule

- Research and planning are done once per item, in docs, and reused.
- Implementers follow the docs. They do not re-research settled values and do not invent unsettled ones silently: they apply the questions above, then report what is unsourced.
- Review is targeted, one pass per branch: a cheaper-model agent compares screenshots and the diff against the research docs and lists only deviating or unsourced items. No blanket multi-agent code review. Use `/code-review` only when a change touches engine, audio or persistence code.
- Machine checks do the routine work: tests, `cargo fmt`, clippy.
- Screenshots come from the current branch with realistic content, never from an old capture.

## Who decides

- Unsourced items and the final look are the user's call.
- Shared files (`mixer_view.rs`, `drag.rs`, `deck_view.rs`, layout rects) are edited one branch at a time.

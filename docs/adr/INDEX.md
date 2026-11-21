# Architecture Decision Records

| ADR | Decision | Status |
|---|---|---|
| [0001](0001-clean-room-doctrine.md) | Clean-room doctrine: features are free to implement, expression (names, logos, artwork) is not copied, no Mixxx code | Accepted |
| [0002](0002-stack-juce-agpl.md) | Technology stack: JUCE with AGPLv3 | Superseded by 0004 |
| [0003](0003-pure-agpl-no-cla.md) | Pure AGPLv3, no contributor licence agreement, DCO sign-off | Accepted |
| [0004](0004-stack-rust-gpui.md) | Technology stack: Rust with GPUI, cpal, symphonia, Rubber Band, midir, rusqlite | Accepted; UI choice superseded in direction by 0009 |
| [0005](0005-experience-first-product-bar.md) | Experience-first product bar: `docs/PRODUCT.md` is the bar, and interface work is judged by looking at it | Accepted |
| [0006](0006-controller-map-provenance.md) | Controller maps are written clean-room from the MIDI spec and the maintainer's own hardware traces | Accepted |
| [0007](0007-clap-hosting-with-clack.md) | CLAP plugin hosting with clack, behind the `clap` feature | Accepted |
| [0008](0008-machined-faceplate-supersedes-e1.md) | The Machined Faceplate look supersedes E1 | Accepted |
| [0009](0009-web-front-end-tauri-trial.md) | Move the interface to a web front end (Tauri v2 and Svelte); GPUI stays the default until the cut-over | Accepted |

## Writing an ADR

Add `docs/adr/NNNN-title.md`, add a row above, and use these sections:

```markdown
# ADR NNNN: Title

## Status
Proposed | Accepted | Rejected | Superseded by NNNN

## Context
The problem that prompted the decision.

## Decision
What was decided.

## Consequences
What follows from it, good and bad.

## References
Related documents, issues and discussions.
```

# ADR 0005: Experience-first product bar

## Status

Accepted

## Context

The roadmap grew out of the C++ port: nine waves ordered engine first, with the interface as one wave (Wave 5) and "depth-first, core mixing production-ready before extras" as a standing decision. That ordering delivered a sound, headless-tested engine, but it also made the interface a single section that could be treated as done when its widgets existed and its logic tests passed. The first look at the real window (2026-10-21) showed an interface far from what a DJ would replace their current software for: flat controls, thin waveforms, a plain library table.

## Decision

1. `docs/PRODUCT.md` defines the bar (what a working DJ must see, reach and trust) and applies to every area.
2. The roadmap runs in parallel tracks (Experience, Performance, Hardware, Intelligence, Pro surface, Release). Experience work is not gated on any wave.
3. A change to what a DJ sees is verified by looking at it (screenshot, real music where relevant), not by logic tests alone.
4. Layout, theme and control design may change freely. The tests that stay fixed are the ones that guard function: no overlap, no clipping, the minimum window.
5. ADR 0001 is unchanged. Common layout conventions are functional and free to use; names, artwork, icons, pixel layouts and manual text are not.

## Consequences

- Visual and workflow quality becomes a first-class deliverable with its own track and checklist, instead of a by-product of the interface wave.
- Engine work no longer implicitly outranks interface work in planning.
- Reviews gain a check (see CONTRIBUTING.md, "Experience bar") and a cost: screenshots and a real-music pass on interface PRs.
- The wave numbers in `TODO.md` and `docs/PROGRESS.md` remain as stable identifiers; they no longer imply an order.

## References

- `docs/PRODUCT.md`, `ROADMAP.md`, ADR 0001, ADR 0004.

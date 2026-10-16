# ADR 0001: Clean-Room Doctrine

## Status

Accepted

## Context

OpenMixer aims to provide a free, open-source alternative to commercial DJ software while avoiding copyright infringement. We must distinguish between copyrightable expression and uncopyrightable methods of operation.

## Decision

We adopt the **Lotus v. Borland** doctrine (49 F.3d 807, 1st Cir. 1995):

1. **Functional features are free** - User interface elements that are methods of operation (two-deck layout, BPM detection, sync mechanisms, mixer controls) are not copyrightable
2. **Expression is not** - Names, logos, artwork, icons, specific screen layouts, manual text, sound packs, and FX presets are protected expression
3. **No verbatim copying** - We do not copy any text, code, or creative content from existing DJ applications

### Rules

1. Features/workflows are free to implement; expression is not
2. No djay/Algoriddim names, logos, artwork, icons, screenshots, or manual text
3. No verbatim screen layouts - functional conventions only
4. No Mixxx application code (GPL libraries with attribution are fine)
5. All assets must be original with provenance recorded
6. No bundled ML model weights
7. No Spotify integration
8. Streaming adapters only under partner terms
9. ASIO SDK never committed (optional local build flag only)
10. IP counsel review before public release

## Consequences

- All assets must be tracked in `docs/legal/asset-provenance.csv`
- Legal reviews required before release
- Automated license checking in CI
- Trademark screening for all UI elements

## References

- Lotus Development Corp. v. Borland International, Inc., 49 F.3d 807 (1st Cir. 1995)
- 17 U.S.C. §102(b) - Not copyrightable subject matter
- https://www.gnu.org/licenses/agpl-3.0.txt

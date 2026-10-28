# ADR 0006: Controller Map Provenance

## Status

Accepted

## Context

ADR 0001 forbids copying other DJ applications' code and expression and, in rule 4, Mixxx application code. Controller support is where that rule is easiest to break by accident: other projects publish large collections of device mappings and scripts (XML, JavaScript, JSON) that describe the same hardware OpenMixer wants to support, and copying or porting one of them is copying their expression, with their licence attached.

The facts a map encodes (which byte of an input report is a fader, which note a pad sends, which output report lights an LED) are functional and belong to the device. How another project chose to name, group, script and document them is theirs.

OpenMixer has two kinds of maps: MIDI maps (`om-midi`, JSON, `resources/controllers/`) and, once HID lands, HID report maps (`om-hid`, JSON). Both are data files, so a copied map would be a copied file.

## Decision

A map for a specific device is derived only from:

1. **The standards**: the MIDI specification and the USB HID specification and usage tables.
2. **Vendor-published documentation**: a MIDI implementation chart, programmer's guide or HID descriptor the vendor publishes for that device.
3. **The maintainer's own captures** from hardware they own or have been lent: MIDI event traces, HID report dumps and descriptors, recorded while operating each control in turn.

A map is never derived from:

- another application's mapping files or scripts (Mixxx, Traktor, Rekordbox, Serato, Virtual DJ and the like), including as a "reference" to check against;
- vendor driver or application code, decompiled or otherwise;
- forum posts, gists or wikis that reproduce either of the above.

Rules that follow:

1. **Provenance note.** Every device map ships with `docs/controllers/<vendor>-<model>.md` stating the sources used (standard, vendor document with URL and date, capture session with date and firmware version), who recorded them, and the capture files kept in `fixtures/controllers/` where they are small enough. `docs/controllers/README.md` has the template. A map without a note is not merged.
2. **Our own names.** Control names in a map are ours, chosen from what the control does, not copied from vendor labels or other projects' maps beyond what is needed to identify the physical control.
3. **Contributor attestation.** A pull request that adds or changes a device map says in its description that it follows this ADR, and names the source of each non-obvious value. A reviewer who recognises a value from another project's map asks where it came from.
4. **No authentication workarounds.** If a device needs a vendor handshake, licence check or encrypted channel before it reports, we do not implement a workaround. Record the finding in the provenance note and take it to IP counsel under ADR 0001 rule 10 before any code is written.
5. **Generic maps stay generic.** The existing generic maps in `resources/controllers/` come from the MIDI specification and follow rule 2 and 3 of ADR 0001 already.
6. **Named devices.** Device names appear in maps and notes only to say which hardware a map is for. No vendor logos, artwork or trade dress.

## Consequences

- Device support grows at the speed of capture sessions on owned hardware. That is slow and is the price of not depending on anyone else's expression.
- The maintainer's hardware (a Traktor Kontrol S3 and Numark devices; a Pioneer DDJ-S3 when repaired) is the first set. A contributor who owns other hardware can add a map by following the same capture workflow.
- HID work (Todo 35) must make capture easy: a dump tool that writes a report log in a form the map author can read, so the workflow does not need another project's tooling.
- Reviewers need a checklist, which `docs/controllers/README.md` provides.
- We lose the shortcut of importing existing maps. We keep our licence position and our clean-room statement.

## References

- ADR 0001 (Clean-Room Doctrine), rules 4 and 10
- `docs/controllers/README.md`
- `fixtures/controllers/`, `resources/controllers/`

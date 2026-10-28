# Controller provenance notes

ADR 0006 requires one note per device map, named `<vendor>-<model>.md`, in this directory. A map without a note is not merged.

## Template

```markdown
# <Vendor> <Model>

- Map file: `resources/controllers/<file>.json`
- Protocol: MIDI | HID | both
- Firmware / driver mode the captures were taken in:

## Sources

| Source | Kind | Detail |
|--------|------|--------|
| MIDI specification / USB HID usage tables | standard | which parts |
| <vendor document> | vendor-published | URL, retrieved <date> |
| Capture session <date> | own capture | who, device serial or unit, how recorded |

## Captures

Files in `fixtures/controllers/` (or a note on why they are not kept), and how each control was exercised.

## Not covered

Controls or behaviours not mapped, and anything that needs verification on other units or firmware.

## Attestation

No value in this map was taken from another application's mapping files or scripts, vendor driver or application code, or reproductions of either.
```

## Review checklist

- Is there a note, and does it name a source for every non-obvious value?
- Do control names describe the function rather than copy another project's names?
- Do the capture files in `fixtures/controllers/` replay through the trace harness and give the map's events?
- Does the device need a handshake? If so, stop and see ADR 0006 rule 4.

## Notes

None yet. The first devices (Traktor Kontrol S3, Numark units) are tracked in the beta milestone.

# Architecture Decision Records

This index contains all Architecture Decision Records (ADRs) for the OpenMixer project.

## ADR Index

| ID | Title | Status |
|----|-------|--------|
| 0001 | Clean-Room Doctrine | Accepted |
| 0002 | Technology Stack - JUCE with AGPLv3 | Superseded by ADR 0004 |
| 0003 | Pure AGPLv3 with No Contributor CLA | Accepted |
| 0004 | Technology Stack - Rust with GPUI | Accepted (superseded in part, see the note in the ADR) |
| 0005 | Experience-first product bar | Accepted |
| 0006 | Controller map provenance | Accepted |
| 0007 | CLAP plugin hosting with clack | Accepted |
| 0008 | The Machined Faceplate supersedes E1 on look | Accepted |

## ADR Templates

When creating new ADRs, use this template:

```markdown
# ADR NNNNN: Title

## Status

Proposed | Accepted | Rejected | Superseded

## Context

Describe the context and problem that prompted this decision.

## Decision

Describe the decision that was made.

## Consequences

Describe the consequences of this decision, both positive and negative.

## References

Links to relevant documents, discussions, or resources.
```

## Decision Categories

- **Legal** - Licensing, clean-room doctrine, compliance
- **Technology** - Frameworks, libraries, tooling
- **Architecture** - System design, patterns
- **Process** - Development workflows, testing

## History

- 2026-10-13 - ADRs 0001-0003 created
- 2026-10-18 - ADR 0004 created; ADR 0002 superseded
- 2026-10-21 - ADR 0005 created
- 2026-10-27 - ADR 0004 marked superseded in part (plugins, stems, MIDI backend, CI scope)
- 2026-10-28 - ADR 0006 created
- 2026-11-06 - ADR 0007 proposed
- 2026-11-07 - ADR 0007 accepted
- 2026-11-08 - ADR 0008 accepted (Machined Faceplate supersedes E1)

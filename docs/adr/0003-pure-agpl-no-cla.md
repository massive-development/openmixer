# ADR 0003: Pure AGPLv3 with No Contributor CLA

## Status

Accepted

## Context

Open-source projects often require contributors to sign a Contributor License Agreement (CLA) to allow dual-licensing or relicensing. We must decide our stance.

## Decision

**Pure AGPLv3, no Contributor License Agreement (CLA)**

### Rationale

1. **Simplicity** - Contributors can submit code without legal overhead
2. **Community trust** - Aligns with pure free software philosophy
3. **Adequate protection** - AGPLv3's copyleft ensures all modifications remain open
4. **Speed** - No legal review needed for each contributor

### Implications

- All contributions become part of the AGPLv3 codebase
- No dual-licensing options
- All contributors must agree to the DCO (Developer Certificate of Origin)

## Consequences

- We track contributions via git history and DCO sign-off
- Users receive all code under AGPLv3 terms
- No proprietary forks or closed-source derivatives possible

## References

- AGPLv3: https://www.gnu.org/licenses/agpl-3.0.txt
- DCO: https://developercertificate.org/

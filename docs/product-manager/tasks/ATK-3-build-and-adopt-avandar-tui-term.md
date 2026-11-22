---
id: ATK-3
title: Build and adopt avandar-tui-term
status: todo
priority: medium
assignee: jpsyx
labels: [feature, terminal, release, adoption]
estimate: 13
project: PROJ-1
milestone: MS-3
cycle:
parent:
github:
blocked_by: [ATK-2]
created: 2026-11-21
updated: 2026-11-21
---

# ATK-3: Build and adopt avandar-tui-term

## Description

Specify and build `avandar-tui-term`, move the capability probe's reusable
terminal behavior into it, and adopt the crate in dif and huckleberry-cli.

## Acceptance criteria

- [ ] Every M3 exit criterion in `docs/milestones.md` is satisfied.
- [ ] The spec and plan lifecycle, releases, adoptions, verification, code
      review, and walkthrough are complete.

## Notes

### Pointers (as of 2026-11-21)

- `docs/milestones.md` - Read Milestone 3 for the crate boundary, adoption
  sequence, and terminal-layer exit criteria.
- `rig/probe/` - Identify the proven terminal lifecycle and capability code
  whose permanent library home is this crate.
- `docs/inventory/dif.md` and `docs/inventory/huckleberry-cli.md` - Locate the
  terminal setup, teardown, clipboard, and capability behavior to replace.
- `docs/specs/` - Write and settle the crate spec before creating crate code.

### Log

- 2026-11-21 created from roadmap Milestone 3.

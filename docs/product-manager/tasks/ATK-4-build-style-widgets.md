---
id: ATK-4
title: Build and adopt style widgets
status: todo
priority: medium
assignee: jpsyx
labels: [feature, release, adoption]
estimate: 8
project: PROJ-1
milestone: MS-4
cycle:
parent:
github:
blocked_by: [ATK-3]
created: 2026-11-21
updated: 2026-11-21
---

# ATK-4: Build and adopt style widgets

## Description

Specify and create `avandar-tui-widgets` with its style and portable text-width
modules, then adopt them in huckleberry-cli and dif.

## Acceptance criteria

- [ ] Every M4 exit criterion in `docs/milestones.md` is satisfied.
- [ ] The spec and plan lifecycle, releases, adoptions, verification, code
      review, and walkthrough are complete.

## Notes

### Pointers (as of 2026-11-21)

- `docs/milestones.md` - Read Milestone 4 for the first widgets scope and its
  text-width policy.
- `docs/specs/architecture.md` and `docs/rules/tui.md` - Preserve feature
  boundaries, semantic color, and pure rendering decisions.
- `docs/inventory/huckleberry-cli.md` and `docs/inventory/dif.md` - Locate the
  duplicated style helpers and text-width behavior to replace.
- `docs/specs/` - Write the multi-milestone widgets spec before implementation.

### Log

- 2026-11-21 created from roadmap Milestone 4.

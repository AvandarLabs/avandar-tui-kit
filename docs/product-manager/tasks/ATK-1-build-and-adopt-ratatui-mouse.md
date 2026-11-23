---
id: ATK-1
title: Build and adopt ratatui-mouse
status: in-progress
priority: high
assignee: jpsyx
labels: [feature, terminal, release, adoption]
estimate: 13
project: PROJ-1
milestone: MS-1
cycle:
parent:
github:
blocked_by: []
created: 2026-11-21
updated: 2026-11-22
---

# ATK-1: Build and adopt ratatui-mouse

## Description

Write the M1 implementation plan, build `ratatui-mouse` to its settled spec,
release `v0.1.0`, and complete the three-stage adoption in dif.

## Acceptance criteria

- [ ] Every M1 exit criterion in `docs/milestones.md` is satisfied.
- [ ] The completed plan is deleted, the implemented spec is archived, and
      the required code review and walkthrough are complete.
- [ ] `v0.1.0` is tagged and dif's adoption is merged with its tests passing.

## Notes

### Pointers (as of 2026-11-22)

- `docs/milestones.md` - Read Milestone 1 first for scope, dependencies,
  cross-repository adoption steps, and exit criteria.
- `docs/specs/ratatui-mouse.md` and `docs/mouse.md` - The settled design and
  implemented integration/surface contracts. Keep the unfinished spec and plan
  until the required verification and closing workflow finish.
- `docs/testing.md` and `rig/` - The implemented X1-X14 harnesses and release
  gates. Finish Windows CI runtime verification and record the named person's
  subjective judgments, then complete closing reviews before release.
- `docs/inventory/dif.md` - Locate the mouse, selection, and pane behaviors
  that the new crate replaces during adoption.

### Log

- 2026-11-21 created from roadmap Milestone 1.
- 2026-11-22 implementation is in progress: crate and local terminal matrices
  are present, with Task 17 wiring CI and permanent docs. Windows runtime
  execution and the named human release judgments remain pending, followed by
  required closing reviews/walkthrough, release, and adoption. No acceptance
  criterion is closed by local implementation alone.

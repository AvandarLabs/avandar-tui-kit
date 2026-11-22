---
id: ATK-1
title: Build and adopt ratatui-mouse
status: todo
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
updated: 2026-11-21
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

### Pointers (as of 2026-11-21)

- `docs/milestones.md` - Read Milestone 1 first for scope, dependencies,
  cross-repository adoption steps, and exit criteria.
- `docs/specs/ratatui-mouse.md` - The settled crate design and public API. Use
  it to write the implementation plan before creating crate code.
- `docs/testing.md` and `rig/` - Extend the M0 harness with mouse scenarios
  X1 through X14 across every required terminal layer.
- `docs/inventory/dif.md` - Locate the mouse, selection, and pane behaviors
  that the new crate replaces during adoption.

### Log

- 2026-11-21 created from roadmap Milestone 1.

---
id: ATK-7
title: Build and adopt avandar-tui-pty
status: todo
priority: medium
assignee: jpsyx
labels: [feature, terminal, release, adoption]
estimate: 13
project: PROJ-1
milestone: MS-7
cycle:
parent:
github:
blocked_by: [ATK-3, ATK-4]
created: 2026-11-21
updated: 2026-11-21
---

# ATK-7: Build and adopt avandar-tui-pty

## Description

Specify and build portable pseudo-terminal sessions and rendering integration,
then move dif's embedded terminal panes onto the crate.

## Acceptance criteria

- [ ] Every M7 exit criterion in `docs/milestones.md` is satisfied.
- [ ] The spec and plan lifecycle, release, adoption, all terminal test layers,
      code review, and walkthrough are complete.

## Notes

### Pointers (as of 2026-11-21)

- `docs/milestones.md` - Read Milestone 7 for the PTY boundary, platform
  behavior, and dif migration.
- `docs/inventory/dif.md` - Locate embedded pane process management, emulator
  state, rendering, resize, and output notifications.
- `docs/terminals.md` and `rig/` - Use measured cross-platform behavior and the
  full real-terminal harness for PTY verification.
- `docs/specs/` - Write and settle the crate spec before implementation.

### Log

- 2026-11-21 created from roadmap Milestone 7.

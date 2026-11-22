---
id: ATK-11
title: Build layout, command, and line-input widgets
status: todo
priority: medium
assignee: jpsyx
labels: [feature, release, adoption]
estimate: 13
project: PROJ-1
milestone: MS-11
cycle:
parent:
github:
blocked_by: [ATK-4]
created: 2026-11-21
updated: 2026-11-21
---

# ATK-11: Build layout, command, and line-input widgets

## Description

Add the layout and command families plus line input to
`avandar-tui-widgets`, then complete the roadmap's dif and huckleberry-cli
adoptions.

## Acceptance criteria

- [ ] Every M10 exit criterion in `docs/milestones.md` is satisfied.
- [ ] The plan lifecycle, release, adoptions, verification, code review, and
      walkthrough are complete.

## Notes

### Pointers (as of 2026-11-21)

- `docs/milestones.md` - Read Milestone 10 for layout, command, input modules,
  feature boundaries, and adoption order.
- `docs/inventory/dif.md` and `docs/inventory/huckleberry-cli.md` - Locate panel,
  tabs, command palette, hotkey table, menu, and line-input implementations.
- `docs/specs/` - Continue from the widgets spec established by ATK-4 and
  settle this milestone's public APIs before implementation.
- `docs/rules/tui.md` - Preserve keyboard-first access and pure drawing across
  every new widget family.

### Log

- 2026-11-21 created from roadmap Milestone 10.

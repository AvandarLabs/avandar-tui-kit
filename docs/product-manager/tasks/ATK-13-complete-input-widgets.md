---
id: ATK-13
title: Complete the input widgets
status: todo
priority: medium
assignee: jpsyx
labels: [feature, release, adoption]
estimate: 13
project: PROJ-1
milestone: MS-13
cycle:
parent:
github:
blocked_by: [ATK-12]
created: 2026-11-21
updated: 2026-11-21
---

# ATK-13: Complete the input widgets

## Description

Add the remaining input family, including the form engine, to
`avandar-tui-widgets` and complete its source-application adoptions.

## Acceptance criteria

- [ ] Every M12 exit criterion in `docs/milestones.md` is satisfied.
- [ ] The plan lifecycle, release, adoptions, verification, code review,
      walkthrough, and final widgets-spec archival are complete.

## Notes

### Pointers (as of 2026-11-21)

- `docs/milestones.md` - Read Milestone 12 for the remaining input modules,
  form behavior, and two adoptions.
- `docs/inventory/huckleberry-cli.md` and `docs/inventory/dif.md` - Locate
  prompts, text areas, forms, validation, and submission behavior to replace.
- `docs/specs/` - Finish the widgets spec begun by ATK-4 and keep it current
  until this final widgets milestone is implemented.
- `docs/rules/tui.md` - Preserve focus management, keyboard equivalence, and
  pure drawing throughout the form engine.

### Log

- 2026-11-21 created from roadmap Milestone 12.

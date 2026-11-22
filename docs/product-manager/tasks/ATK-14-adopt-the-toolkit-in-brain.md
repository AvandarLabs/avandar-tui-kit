---
id: ATK-14
title: Adopt the toolkit in brain
status: todo
priority: medium
assignee: jpsyx
labels: [adoption, enhancement]
estimate: 13
project: PROJ-1
milestone: MS-14
cycle:
parent:
github:
blocked_by: [ATK-3, ATK-7, ATK-8, ATK-9, ATK-11, ATK-12, ATK-13]
created: 2026-11-21
updated: 2026-11-21
---

# ATK-14: Adopt the toolkit in brain

## Description

Move brain onto every applicable toolkit crate, beginning with terminal and
PTY support when available and finishing after the agent, document, and widget
milestones are complete.

## Acceptance criteria

- [ ] Every M13 exit criterion in `docs/milestones.md` is satisfied.
- [ ] brain's own prerequisite upgrade and staged adoptions are merged with all
      tests passing.
- [ ] Any toolkit gaps discovered by adoption are documented and resolved
      within the milestone's scope, followed by review and walkthrough.

## Notes

### Pointers (as of 2026-11-21)

- `docs/milestones.md` - Read Milestone 13 for the staged adoption order,
  prerequisite upgrade, and rule against speculative additions.
- `docs/inventory/brain.md` - Locate brain's terminal, PTY, agent, document,
  layout, command, picker, tree, and input implementations.
- `docs/architecture.md` - Adopt toolkit parts without transferring ownership
  of brain's event loop or application state.
- `docs/testing.md` - Run every toolkit and application test layer touched by
  the migration before considering adoption complete.

### Log

- 2026-11-21 created from roadmap Milestone 13.

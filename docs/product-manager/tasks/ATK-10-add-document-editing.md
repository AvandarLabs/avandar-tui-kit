---
id: ATK-10
title: Add document editing
status: todo
priority: medium
assignee: jpsyx
labels: [feature, release]
estimate: 13
project: PROJ-1
milestone: MS-10
cycle:
parent:
github:
blocked_by: [ATK-9, ATK-11]
created: 2026-11-21
updated: 2026-11-21
---

# ATK-10: Add document editing

## Description

Extend `avandar-tui-document` with opt-in nano-style and modal vim-style
editing, including undo, paste, cursor movement, and save behavior.

## Acceptance criteria

- [ ] Every M9.b exit criterion in `docs/milestones.md` is satisfied.
- [ ] The plan lifecycle, release, verification, code review, walkthrough, and
      final document-spec archival are complete.

## Notes

### Pointers (as of 2026-11-21)

- `docs/milestones.md` - Read Milestone 9.b for editing modes, feature
  boundaries, testing, and explicit non-goals.
- `docs/specs/` - Continue from the settled document spec and update it when
  research or implementation changes the public behavior.
- `docs/rules/tui.md` and `docs/rules/rust.md` - Keep editing state explicit,
  rendering pure, errors typed, and platform behavior portable.
- `crates/avandar-tui-document/` - Extend the viewing crate created by ATK-9
  without making editing a mandatory dependency.

### Log

- 2026-11-21 created from roadmap Milestone 9.b.

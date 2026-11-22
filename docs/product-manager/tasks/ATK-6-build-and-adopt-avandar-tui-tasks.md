---
id: ATK-6
title: Build and adopt avandar-tui-tasks
status: todo
priority: medium
assignee: jpsyx
labels: [feature, release, adoption]
estimate: 8
project: PROJ-1
milestone: MS-6
cycle:
parent:
github:
blocked_by: [ATK-2]
created: 2026-11-21
updated: 2026-11-21
---

# ATK-6: Build and adopt avandar-tui-tasks

## Description

Specify and build reusable task, thread, and child-process primitives, then
adopt them in huckleberry-cli and dif.

## Acceptance criteria

- [ ] Every M6 exit criterion in `docs/milestones.md` is satisfied.
- [ ] The spec and plan lifecycle, release, adoptions, cross-platform
      verification, code review, and walkthrough are complete.

## Notes

### Pointers (as of 2026-11-21)

- `docs/milestones.md` - Read Milestone 6 for task abstractions, platform
  requirements, and both adoptions.
- `docs/inventory/huckleberry-cli.md` and `docs/inventory/dif.md` - Locate the
  existing thread, child, and task-state patterns to consolidate.
- `docs/specs/architecture.md` - Keep async runtime and process dependencies
  behind the intended crate boundary.
- `docs/specs/` - Write and settle the crate spec before implementation.

### Log

- 2026-11-21 created from roadmap Milestone 6.

---
id: ATK-9
title: Build document viewing and opener support
status: todo
priority: medium
assignee: jpsyx
labels: [feature, release, adoption]
estimate: 8
project: PROJ-1
milestone: MS-9
cycle:
parent:
github:
blocked_by: [ATK-4]
created: 2026-11-21
updated: 2026-11-21
---

# ATK-9: Build document viewing and opener support

## Description

Research current ratatui document crates, specify and build read-only document
viewing and opener support, then adopt it in dif and huckleberry-cli.

## Acceptance criteria

- [ ] Every M9.a exit criterion in `docs/milestones.md` is satisfied.
- [ ] Research, the spec and plan lifecycle, release, adoptions, verification,
      code review, and walkthrough are complete.

## Notes

### Pointers (as of 2026-11-21)

- `docs/milestones.md` - Read Milestone 9.a for the mandatory ecosystem
  research, read-only scope, and adoption sequence.
- `docs/inventory/dif.md` and `docs/inventory/huckleberry-cli.md` - Locate
  document rendering, scrolling, opening, and external-editor behavior.
- `docs/specs/architecture.md` - Keep parser and opener dependencies behind the
  document crate's features.
- `docs/specs/` - Record the research and settle the multi-milestone document
  spec before implementation.

### Log

- 2026-11-21 created from roadmap Milestone 9.a.

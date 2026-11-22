---
id: ATK-5
title: Build and adopt avandar-tui-charts
status: todo
priority: medium
assignee: jpsyx
labels: [feature, release, adoption]
estimate: 8
project: PROJ-1
milestone: MS-5
cycle:
parent:
github:
blocked_by: [ATK-4]
created: 2026-11-21
updated: 2026-11-21
---

# ATK-5: Build and adopt avandar-tui-charts

## Description

Specify and build reusable chart rendering in `avandar-tui-charts`, then have
huckleberry-cli render its charts through the published crate.

## Acceptance criteria

- [ ] Every M5 exit criterion in `docs/milestones.md` is satisfied.
- [ ] The spec and plan lifecycle, release, adoption, verification, code
      review, and walkthrough are complete.

## Notes

### Pointers (as of 2026-11-21)

- `docs/milestones.md` - Read Milestone 5 for chart modules, release scope, and
  adoption criteria.
- `docs/inventory/huckleberry-cli.md` - Locate the chart models and rendering
  behavior that become the crate's first adoption.
- `docs/specs/architecture.md` and `docs/rules/tui.md` - Keep chart decisions
  pure and the umbrella feature boundary lightweight.
- `docs/specs/` - Write and settle the crate spec before implementation.

### Log

- 2026-11-21 created from roadmap Milestone 5.

---
id: ATK-8
title: Build and adopt avandar-tui-agent
status: todo
priority: medium
assignee: jpsyx
labels: [feature, terminal, release, adoption]
estimate: 13
project: PROJ-1
milestone: MS-8
cycle:
parent:
github:
blocked_by: [ATK-7]
created: 2026-11-21
updated: 2026-11-21
---

# ATK-8: Build and adopt avandar-tui-agent

## Description

Specify and build coding-agent process frontends and presentation support,
including the Windows launch path, then migrate dif's agent integration.

## Acceptance criteria

- [ ] Every M8 exit criterion in `docs/milestones.md` is satisfied.
- [ ] The spec and plan lifecycle, release, adoption, cross-platform
      verification, code review, and walkthrough are complete.

## Notes

### Pointers (as of 2026-11-21)

- `docs/milestones.md` - Read Milestone 8 for agent modules, features, launch
  requirements, and adoption criteria.
- `docs/inventory/dif.md` - Locate agent discovery, launch, stream handling,
  status presentation, and command construction to replace.
- `rig/probe/` - Reuse the M0 evidence for Windows `.cmd` shim argument handling
  rather than inventing a separate quoting policy.
- `docs/specs/` - Write and settle the crate spec before implementation.

### Log

- 2026-11-21 created from roadmap Milestone 8.

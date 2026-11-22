---
id: ATK-2
title: Publish the workspace to crates.io
status: todo
priority: medium
assignee: jpsyx
labels: [release, chore, adoption]
estimate: 3
project: PROJ-1
milestone: MS-2
cycle:
parent:
github:
blocked_by: [ATK-1]
created: 2026-11-21
updated: 2026-11-21
---

# ATK-2: Publish the workspace to crates.io

## Description

Configure release-plz and crates.io publication, switch the workspace to
`ratatui-core`, and move dif from the tagged mouse dependency to the published
crate.

## Acceptance criteria

- [ ] Every M2 exit criterion in `docs/milestones.md` is satisfied.
- [ ] Publication, dependency migration, docs, verification, review, and
      walkthrough work are complete.

## Notes

### Pointers (as of 2026-11-21)

- `docs/milestones.md` - Read Milestone 2 for publication scope and the exact
  release and adoption checks.
- `Cargo.toml` and `crates/` - Centralize publish metadata and dependency
  changes in the workspace before changing individual crate manifests.
- `docs/specs/architecture.md` - Follow the shared release policy and umbrella
  dependency boundaries.

### Log

- 2026-11-21 created from roadmap Milestone 2.

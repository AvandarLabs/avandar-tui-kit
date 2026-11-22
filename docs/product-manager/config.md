---
prefix: ATK
cadence_weeks: 2
current_cycle:
counters:
  task: 14
  project: 1
  initiative: 0
  milestone: 14
---

# Workspace config

Machine-readable settings live in the frontmatter above. Increment a counter
after allocating an ID of that type.

## Priorities

`none` < `low` < `medium` < `high` < `urgent`.

## Labels

- `bug` - broken behavior.
- `feature` - new user-facing capability.
- `enhancement` - improvement to existing behavior.
- `tech-debt` - internal cleanup or refactor.
- `chore` - maintenance, tooling, or operations.
- `release` - packaging and publication work.
- `adoption` - migration of a source application onto the toolkit.
- `terminal` - behavior that requires real-terminal verification.

## Estimates

Fibonacci points: `1, 2, 3, 5, 8, 13`.

Roadmap sizing maps to estimates as follows: small is 3, medium is 8, and
large is 13.

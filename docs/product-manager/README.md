# Product board

This directory is the project's planning system, managed by the
`repo-product-manager` skill. It recreates Linear's object model
(initiatives -> projects -> milestones -> cycles -> tasks) in plain
markdown, committed to the repo. There is no Linear connection.

## Layout

- `config.md` - cadence, current cycle, labels, priorities, and ID counters.
- `team.md` - assignable people.
- `initiatives/` - strategic goals spanning projects.
- `projects/` - bodies of work; milestones live inside each project file.
- `cycles/` - 2-week iterations, with plan and retro.
- `tasks/` - atomic work items, one file each.
- `archive/` - done or cancelled tasks.

## How it works

- Each entity is one Markdown file with YAML frontmatter and a descriptive body.
- Entity files are the single source of truth. Board, roadmap, and velocity
  views are computed on demand rather than stored.
- New tasks default to the person who added them and begin in `backlog` until
  triaged.

Ask the `repo-product-manager` skill in natural language to add, edit, triage,
plan cycles, generate status updates, or answer "what should we work on?".

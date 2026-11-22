---
id: PROJ-1
name: Complete the avandar-tui-kit roadmap
status: planned
health: on-track
lead: jpsyx
members: [jpsyx]
initiative:
target_date:
github:
created: 2026-11-21
updated: 2026-11-21
---

# PROJ-1: Complete the avandar-tui-kit roadmap

## Goal

Deliver, publish, and adopt every building-block crate in the ordered roadmap,
ending with all four source applications using the toolkit where intended.

## Scope

This project tracks Milestones 1 through 13 from `docs/milestones.md`. The
completed Milestone 0 test rig is historical context and is not an active task.
Each milestone task includes planning, implementation, verification, release,
adoption, documentation, review, and walkthrough work required by the roadmap.

## Milestones

- **MS-1 - M1: `ratatui-mouse`** (status: planned)
  - Build and release mouse support, then adopt it in dif.
  - Tasks: ATK-1
- **MS-2 - M2: Publishing to crates.io** (status: planned)
  - Establish automated publication and move dif to the published mouse crate.
  - Tasks: ATK-2
- **MS-3 - M3: `avandar-tui-term`** (status: planned)
  - Build shared terminal lifecycle and capability support.
  - Tasks: ATK-3
- **MS-4 - M4: Widgets: style** (status: planned)
  - Create the first `avandar-tui-widgets` modules for style and text width.
  - Tasks: ATK-4
- **MS-5 - M5: `avandar-tui-charts`** (status: planned)
  - Build reusable chart rendering and adopt it in huckleberry-cli.
  - Tasks: ATK-5
- **MS-6 - M6: `avandar-tui-tasks`** (status: planned)
  - Build task and child-process primitives and adopt them in two applications.
  - Tasks: ATK-6
- **MS-7 - M7: `avandar-tui-pty`** (status: planned)
  - Build portable pseudo-terminal support and migrate dif's embedded panes.
  - Tasks: ATK-7
- **MS-8 - M8: `avandar-tui-agent`** (status: planned)
  - Build coding-agent frontends and migrate dif's agent integration.
  - Tasks: ATK-8
- **MS-9 - M9.a: `avandar-tui-document` viewing and opener** (status: planned)
  - Research the ecosystem, build read-only document support, and adopt it.
  - Tasks: ATK-9
- **MS-10 - M9.b: `avandar-tui-document` editing** (status: planned)
  - Add opt-in nano-style and vim-style document editing.
  - Tasks: ATK-10
- **MS-11 - M10: Widgets: layout and command** (status: planned)
  - Add layout, command, and line-input widget families.
  - Tasks: ATK-11
- **MS-12 - M11: Widgets: pick and tree** (status: planned)
  - Add picker and tree widget families and their adoption.
  - Tasks: ATK-12
- **MS-13 - M12: Widgets: input** (status: planned)
  - Complete the input family and adopt it in source applications.
  - Tasks: ATK-13
- **MS-14 - M13: brain adopts the toolkit** (status: planned)
  - Move brain onto every applicable completed toolkit crate.
  - Tasks: ATK-14

## Status updates

- **2026-11-21 - on-track**: Milestone 0 is complete. The remaining roadmap is
  captured as dependency-ordered tasks, with ATK-1 as the next unblocked item.

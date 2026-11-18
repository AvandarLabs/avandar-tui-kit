# Inventory of extractable UI elements

Every UI element, and every utility a UI leans on, that could be extracted
from the four apps this toolkit comes from. An element is listed whether one
app uses it or all of them: an element only one app has is often one the other
apps did without because rebuilding it was too much trouble.

The inventory was taken in November 2026 by reading each app's docs and
source in depth. It records what exists, not what the toolkit will ship: the
specs decide that.

| App | What it is | Page |
| --- | --- | --- |
| `dif` | Two-pane review shell with a coding agent in an embedded terminal | [dif.md](dif.md) |
| `brain` | Persistent shell: tasks, fuzzy search, file tree, agent sessions in tabs | [brain.md](brain.md) |
| `huckleberry-cli` | Menu-driven shell with dashboards, forms and dial pickers | [huckleberry-cli.md](huckleberry-cli.md) |
| `quickstarter` | Project scaffolder whose `rust-tui` template states the rules | [quickstarter.md](quickstarter.md) |

Each page records, per element: what it does, where it lives (repo-relative),
its rough size, what is generic and what is app-specific, its dependencies,
the extraction effort, its test coverage, and the lessons its code and docs
record. Each element carries a category tag, defined below.

## Categories

| Tag | Covers |
| --- | --- |
| `term` | Borrowing the terminal and giving it back: raw mode, screens, panic hooks, keyboard flags, clipboard |
| `mouse` | Mouse modes, hit-testing, clicks, selection, wheel routing, pass-through |
| `pty` | Embedded terminal panes: the child process, the emulator, key encoding, supervision |
| `agent` | Coding-agent frontends: launch, resume, typing and submitting prompts, message queues |
| `cmd` | Commands and keys: palettes, shortcut tables, help, key classifiers, vim motions |
| `layout` | Frames, splits, tabs, focus, modal hosting, status lines, centered rects |
| `pick` | Menus, lists, fuzzy pickers, quick picks, checklists |
| `tree` | File trees |
| `input` | Text fields, confirms, dials, forms, wizards, question models |
| `view` | Read-only viewers: markdown, documents with a cursor, logs, links and opening targets |
| `chart` | Text charts and tables |
| `style` | Themes, color math, wrapping, truncation |
| `task` | Background work a UI needs: cancellable tasks, keep-last-good data, workers, debouncers |

## Across the apps

Where more than one app has a version, the "best start" column names the one
to start from and why. Single-app elements are listed too.

| Element | dif | brain | huckleberry-cli | quickstarter | Best start |
| --- | --- | --- | --- | --- | --- |
| Terminal guard | Partial, no panic hook | Guard with recorded steps, no panic hook | Guard and panic hook, on stderr | Guard and panic hook | brain's ordered guard plus quickstarter's panic hook |
| Mouse modes | Correct (1000, 1002, 1006) | Buggy (resets 1002 and 1003) | None | None | dif |
| Click, multi-click, selection, copy | Full | None | None | None | dif, generalized beyond two fixed panes |
| Wheel routing by pane | Yes | Yes | None | None | Either |
| Clipboard | pbcopy, OSC 52, tmux | pbcopy only | None | None | dif, plus Linux and Windows tools |
| Embedded terminal pane | Yes | Yes | None | None | brain's env allowlist and paced writer, dif's respawn into the same screen |
| Key to bytes | Yes, gaps | Yes, fewer gaps | None | None | brain, then fill the gaps |
| Typing a prompt into an agent | Alt+Enter newlines, 200 ms | Bracketed paste, 400 ms, control characters stripped | None | None | brain |
| Agent adapters, controller, resume | Claude, Codex | Claude, Codex, OpenCode, pi | None | None | brain |
| Message queue into the agent | Yes | None | None | None | dif |
| Command palette | Substring filter | Generic `CommandPalette<A>`, word filter | None | Word filter | brain |
| Shortcuts table, parity tests | Table, no parity test | Table and parity tests | Hints only | Table and parity tests | brain or quickstarter |
| Help modal | Yes | Yes, scrolls | None | Yes | brain |
| Key map as semantic motions | None | Classifiers | One-handed motions | Classifier by overlay | huckleberry-cli plus quickstarter |
| Vim motions and counts | Grammar | Count prefix | None | None | Merge both |
| Modal host | `Option` fields | One overlay enum | Precedence list | Overlay enum | brain |
| Confirm dialog | Yes, arrows reversed | Two versions | Via menus | None | Merge brain's two |
| Text input | None | Single line, copied three times; compose | Text, secret, paste-aware | None | huckleberry-cli plus brain's scroll and error row |
| Menus with drill-down | None | None | Menu stack and breadcrumb | None | huckleberry-cli |
| Selection prompt | None | Quick pick | Numbered scrolling select | dialoguer | huckleberry-cli |
| Fuzzy picker | None | nucleo, grouped, separator-insensitive | Word filter in listing | None | brain |
| List view | None | Variable-height list | Grouped, column-aligned listing | None | Both, different jobs |
| File tree | None | Full | None | None | brain |
| Dials | None | None | Full | None | huckleberry-cli |
| Forms and wizards | None | Captive wizard | Retained-draft form engine | None | huckleberry-cli |
| Markdown | Full, with tables | Small, line-based | None | None | dif |
| Document view with cursor | Yes | None | None | None | dif |
| Log viewer | Activity log in a pane | Viewer and live tail | Pager | None | brain |
| Open link or path | Yes | Yes, plus a picker for several links | None | None | Merge |
| Tabs | View strip | Numbered tabs with stable ids | Dashboard tabs | None | brain for ids, huckleberry-cli for drawing into any area |
| Status line and notices | Notice with expiry | Three tiers plus error banner | Status note in footer | Status | brain |
| Charts and tables | Markdown tables | None | Bars, sparkline, stripes, line plot, grouped table | None | huckleberry-cli |
| Theme by role | None | Duplicated palette | 22 roles | 9 roles | huckleberry-cli's mechanism, quickstarter's core roles |
| Color math | None | Oklab | None | None | brain |
| Background work | Watcher threads | Job coordinator, workers, debouncer | Cancellable task, keep-last-good | None | Several; runtime-neutral versions needed |

## Problems that repeat across apps

These are the reasons several elements need a redesign rather than a copy.

- **Character counts instead of terminal cells.** dif's markdown, tables and
  cursor, brain's palette and elision, and huckleberry-cli's column allocator
  count characters, so wide characters and emoji misalign. Every toolkit
  width goes through cell width.
- **No panic hook** in dif or brain.
- **Paste is not bracketed** in dif or brain, so a multi-line paste into an
  agent submits at each newline.
- **macOS only:** clipboard, openers and editor launchers in dif and brain.
- **Hard-coded colors** in dif and brain.
- **Docs drift from code** in all three apps, usually in hints and decision
  records.
- **Two ways to render prompts.** huckleberry-cli renders prompts as ANSI
  lines so one implementation runs inline, full screen or inside a panel; the
  others render ratatui widgets. The input spec has to choose.

## Crate map

The inventory adds whole families (pickers, trees, forms, dials, viewers,
charts) that a single `widgets` crate would make heavy, so the crate map was
split along the heavy dependencies. The decided map is in
[`../architecture.md`](../architecture.md).

# M1: dif Mouse Adoption Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> `superpowers:subagent-driven-development` (recommended) or
> `superpowers:executing-plans` to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking. Load `super-wrapper` and the target
> repository's instructions before execution.

**Goal:** Adopt tagged `ratatui-mouse` v0.1.0 in dif in three independently
verified stages, then close milestone 1.

**Architecture:** dif owns its controller, targets, hit areas, and surface
adapters. The crate decides gestures and emits outcomes; dif retains focus,
its cursor-driven markdown scrolling, clipboard, notices, event loop, and pane
input writers. Pass-through stays disabled until stages 1 and 2 pass.

**Tech Stack:** Existing dif Rust edition 2024/Rust 1.89, ratatui 0.30,
crossterm 0.29, vt100 0.16; git-tagged ratatui-mouse with vt100 enabled.

**Spec:** [Reconciled mouse spec](../specs/ratatui-mouse.md), especially
How dif adopts it, and [toolkit plan](2026-11-22_m1-ratatui-mouse.md).
After toolkit completion, read the spec at `docs/archive/ratatui-mouse.md`
and the permanent toolkit walkthrough instead of the deleted toolkit plan.

**State:** Queued and blocked on the reviewed, verified, accessible v0.1.0
release. Planning authorizes no changes in AvandarLabs/avandar and no Git
publication or integration actions.

## Global Constraints

- Work in `AvandarLabs/avandar`, under
  `agent-skills/public-skills/skills/diff-review/scripts/dif/`; every path in
  these tasks is relative to that crate unless qualified. Set `DIF_ROOT`
  to this repo-relative path when running commands from that repository root.
- Read that repository's current instructions and actual code before edits;
  use `wt` isolation. The inventory is context, not permission to fix other bugs.
- Use `ratatui-mouse = { git = "https://github.com/AvandarLabs/avandar-tui-kit", tag = "v0.1.0", features = ["vt100"] }`.
  Do not finish with a local path, branch dependency, or untagged override.
- Preserve modal routing, cursor-based markdown scroll, view-specific selection
  visibility, focus behavior, copy notices, and existing clipboard behavior.
  Do not migrate clipboard to M3 or refactor PTY ownership in M1.
- Every stage runs dif's tests before the next stage starts. Use red/green
  TDD for integration behavior; preserve the existing tests in stage 1 and
  remove only tests whose ownership moves to the crate in stage 2.
- Use injected Instant values for app decisions. Poll at the smaller of
  50 ms and the controller deadline; tick on input and timeout iterations.
- Keyboard selection and the Select text palette command remain reachable
  where mouse modifiers or motion are intercepted. Shift and Alt mean the
  crossterm bits, not a cross-terminal promise about physical Option keys.
- Run every affected terminal layer using that repository's test setup;
  perform desktop interaction only in a VM. Report an actual unavailable
  layer instead of claiming complete verification.
- Push, PR, merge, and tag actions require their own explicit authorization.
  Approval of this plan does not authorize them. Follow that repository's
  code-review, walkthrough, docs-lifecycle, and cleanup requirements.

## Review Focus

| Failure mode | Expected behavior | Test owner |
| --- | --- | --- |
| Mutable app borrows split among controller and surfaces | No workaround duplicating controller state or losing pane updates | Stage 1 |
| Background tick without a new input event | Edge selection progresses while input is idle | Stage 1 |
| Switching main views or opening a modal during selection | No stale highlight or unintended pane input | Stages 1 and 2 |
| Cleanup on panic or I/O failure | Full mouse cleanup and raw/alternate restoration on the drawing stream | Stage 2 |
| Consumed keyboard selection event in a mouse-requesting pane | No key leak or duplicate copy; focus still follows pressed_target | Stage 3 |

## File map and starting state

As inspected on 2026-11-22, dif already depends on ratatui 0.30, crossterm
0.29 and vt100 0.16, with Rust 1.89. Its loop draws then polls at 50 ms;
`src/tui/mouse.rs` owns fixed `PaneAreas`, `src/tui/app/select.rs` maps pane
selection, `src/tui/multi_click.rs` counts clicks, and `src/tui/selection.rs`
projects and extracts ranges. `src/tui/mod.rs` owns terminal setup/restore;
`src/tui/draw.rs` registers rectangles and paints selection. App state is in
`src/tui/app/mod.rs`; palette commands and shortcut descriptions are in
`src/tui/palette.rs` and `src/tui/shortcuts.rs`.

The stage-local interfaces below are the handoffs. Use inline tests in the
changed app modules where practical; no new app testing framework is needed.
Record actual docs files from the target repository before changing behavior.
Refresh this map at execution time.

## Task 1: dif adoption stage 1, with pass-through disabled

**Repository/files:** `AvandarLabs/avandar`, paths under `DIF_ROOT`:
`Cargo.toml`, `Cargo.lock`, `src/tui/{mouse,event_loop,draw}.rs`,
`src/tui/app/{mod,select}.rs`, initialization files found from `App` creation,
and the repository's relevant docs. Read its AGENTS.md before changes.

**Interfaces:** Add the tagged git dependency with `features = ["vt100"]`.
dif owns `Target` (main pane carrying current view identity, agent pane, tabs),
`HitAreas<Target>`, `MouseController<Target>`, and surface adapters. Use the
crate API, not a second gesture implementation.

- [ ] Establish dif's existing test baseline. Add app-integration assertions
  for focus, per-view wheel cursor behavior, modal blocking, view-dependent
  highlight visibility, copied-text notice, and its existing selection cases.
  Preserve existing tests in this first stage and identify expected new
  tolerance/word-drag behavior explicitly; do not silently change assertions.
  ```rust
  // In dif app fixtures, with pass-through disabled and a modal open:
  assert_eq!(pane_input_after, pane_input_before);
  assert_eq!(markdown_cursor_after_wheel, markdown_cursor_before_wheel + 3);
  assert_eq!(copy_notice, "Copied 10 characters"); // file.rs:12
  ```

- [ ] Run `cargo test --manifest-path "$DIF_ROOT/Cargo.toml"` red for new
  wiring tests. Register draw-time areas and route through the controller with
  pass-through disabled. Implement the `SurfaceSet<Target>` seam now so the
  controller can select; retain old modules until the next deletion stage.
- [ ] Split borrowed app fields explicitly into controller, areas, and
  surfaces, avoiding overlapping `&mut App` borrows. Fold `next_deadline` into
  `min(50 ms, deadline.saturating_duration_since(now))`; call tick each pass,
  including timeout passes, not only after input.
- [ ] Run dif tests and its required lint/docs/terminal layers; preserve all
  existing behavior before enabling new forwarding. If a pre-existing test
  conflicts with a deliberate spec change, document the exact conflict before
  advancing. Commit as `feat(dif): integrate tagged mouse controller`.

## Task 2: dif adoption stage 2, remove duplicated selection code

**Repository/files:** Under `DIF_ROOT`, delete `src/tui/{multi_click,selection}.rs`;
modify `src/tui/{mod,draw,mouse}.rs`, `src/tui/app/{mod,select}.rs`, and
affected selection integration tests/docs. `src/clipboard.rs` stays.

**Interfaces:** `SurfaceSet<Target>` supplies Vt100Surface for agent/log panes
and dif's own cursor-based markdown surface using crate word functions.
Draw uses `HighlightRects::from_range` and `render`; Copy uses existing app
clipboard behavior. Terminal cleanup calls the crate's reporting functions.

- [ ] Write integration tests for markdown cursor-derived scrolling,
  agent/log history selection, current-view-only highlighting, and app copy
  notifications; run red before deleting local implementations.
  ```rust
  assert_eq!(copied_text, "file.rs:12");
  assert_eq!(screen.scrollback(), scrollback_before_copy);
  assert!(highlighted_targets.iter().all(|target| target == &visible_target));
  ```

- [ ] Remove duplicated click/text/range logic and only the tests the crate
  now owns. Retain app-specific integration tests and no fallback local copy.
- [ ] Add full M5 normal cleanup and a panic hook, using the drawing stream;
  compose with raw-mode/alternate-screen cleanup without installing a
  competing terminal framework. Verify clean exit, error, and panic paths.
- [ ] Run dif's full tests/lints and every touched terminal layer; commit as
  `refactor(dif): replace local selection with ratatui-mouse`.

## Task 3: dif adoption stage 3 and milestone completion

**Repository/files:** Under `DIF_ROOT`, modify `src/tui/{mouse,event_loop,palette,shortcuts}.rs`,
`src/tui/app/{mod,select}.rs`, pane input call sites identified at execution,
and relevant docs/tests. Update ATK-1 in the toolkit only after verified adoption.

**Interfaces:** Enable controller pass-through, route Forward bytes to the
correct pane's existing input writer, and expose the keyboard-reachable
"Select text" palette command for the focused pane.

- [ ] Write app tests for a mouse-requesting pane receiving exact bytes,
  alternate-screen wheel becoming arrows, Shift/Alt forcing selection when
  delivered, modal blocking, and the palette command reaching select mode.
  The normal app key handler must not forward consumed select-mode keys.
  ```rust
  assert_eq!(forwarded_bytes, b"\x1b[<0;1;1M");
  assert_eq!(bytes_forwarded_during_select_mode.len(), 0);
  assert_eq!(keyboard_copied_text, "file.rs:12");
  ```

- [ ] Run tests red; enable forwarding and wire the command. Test that
  focus follows `pressed_target` even when the outcome is Forward or Copy,
  and text copies correctly using keys when modifier motion is unavailable.
- [ ] Run dif's complete checks after this third stage and exercise mouse,
  stock/full tmux, keyboard select mode, clean exit, and panic behavior in
  every affected supported terminal layer. Confirm the final dependency uses
  the accessible v0.1.0 tag, with no local path or untagged override.
- [ ] Follow that repository's review/walkthrough workflow, then wait for
  explicit merge/push authorization. Merge only after approval and passing
  checks; perform its required verified branch/worktree cleanup.
- [ ] Record the adoption merge and test evidence in ATK-1; close/archive
  ATK-1 and mark MS-1 complete only when all toolkit release and dif adoption
  exit criteria are true. M2 publication remains a separate task.

## Completion and lifecycle

- [ ] Preserve toolkit release and adoption merge references in ATK-1.
- [ ] Delete this queued adoption plan only after the adoption is implemented;
  retain permanent behavior docs and its required walkthrough in the owning
  repository. Do not mark the entire milestone done at toolkit tag creation.
- [ ] Confirm plan/spec documentation links remain valid after archival and
  deletion. Any new write to the toolkit uses its own worktree and rules.

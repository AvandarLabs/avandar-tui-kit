# M1: ratatui-mouse Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> `superpowers:subagent-driven-development` (recommended) or
> `superpowers:executing-plans` to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking. Load `super-wrapper` before execution
> and apply the current frontend's routing and review rules.

**Goal:** Build and verify `ratatui-mouse`, release the workspace's `v0.1.0`
git tag, and hand off dif's three-stage adoption to the linked queued plan.

**Architecture:** Pure geometry, text, routing, and selection primitives feed
one app-owned `MouseController<T>`. Borrowed `SelectableSurface` adapters serve
app-held rows and optional vt100 screens. The crate reports outcomes and
deadlines; the app retains its loop, focus, clipboard, rendering, and child I/O.

**Tech Stack:** Rust edition 2024, Rust 1.88 floor, ratatui 0.30, crossterm
0.29, unicode-segmentation 1, optional vt100 0.16. Existing Node/Playwright
xterm.js, private tmux, Orca, Tart/VNC, and Windows ConPTY rigs.

**Spec:** [Reconciled mouse spec](../specs/ratatui-mouse.md), including its
Public API and Test plan sections. Read it with
[architecture](../architecture.md), [architecture design](../specs/architecture.md),
[terminal measurements](../terminals.md), [testing](../testing.md),
[Rust rules](../rules/rust.md), and [TUI rules](../rules/tui.md).

**State:** Queued for review. Writing this plan does not authorize execution,
pushes, merges, PRs, tags, or publication. No implementation has started.

## Global Constraints

- Workspace version `0.1.0`, edition `2024`, `rust-version = "1.88"`, MIT;
  every crate inherits package settings, dependency versions, and lints.
- Runtime dependencies: ratatui, crossterm, unicode-segmentation, and vt100
  only behind `vt100`. No default feature. Do not add a runtime serialization,
  clipboard, clock, terminal parser, async runtime, or error-derive dependency.
- `unsafe_code = "forbid"`; clippy pedantic and nursery warnings are errors.
  Public fallible APIs use `MouseError` with handwritten `Display` and `Error`.
- Use the exact public names, signatures, enum variants, bounds, and visibility
  in the spec. Re-export public items from the crate root. Private helpers may
  split along responsibilities; functions stay at most 45 lines and production
  files roughly below 400 lines. Do not add a framework or own app state.
- All timing enters as `now: Instant`. The library creates no thread, reads no
  clock, sleeps nowhere, and performs no clipboard or child-process I/O.
- Measure grapheme clusters with ratatui's `CellWidth`, never unicode-width,
  `Span::width`, `Line::width`, byte lengths, or character counts for columns.
  Copy original text, not a portable-text rewrite or normalized replacement.
- macOS/Linux enable exactly `\x1b[?1000h\x1b[?1002h\x1b[?1006h`; teardown
  uses the full M5 sequence. No reset between setup and teardown. Windows uses
  crossterm's console path, writes no crate mouse sequence, and ignores the
  bare `Moved` events ConPTY's outer 1003 reporting supplies.
- Defaults: wheel 3 rows; multi-click 500 ms and 1 cell; word punctuation
  `_-./~:@+%#?&=`; Shift OR Alt selects; copy on release, keep after copy,
  clear on key press, clear on resize, pass-through, wheel-to-arrows, and
  built-in select-mode keys all enabled. Edge interval 50 ms, maximum 10 rows,
  margin 0 rows; redraw interval 16 ms. Later human tuning must be recorded.
- Keyboard equivalents remain required. Stock tmux forwards pane events after
  setup. tmux 3.8 in xterm.js drops Alt-drag motion; 3.3a can absorb rapid
  right/Control presses. Missing DECRQM replies in 3.3a and ConPTY are not resets.
- No feature changes behavior of an already exposed item. M1 retains ratatui;
  ratatui-core migration is M2, output-arrival selection policy is M7,
  clipboard and full terminal lifecycle integration are M3.
- Red/green TDD for behavior. A failing import may establish an initial API,
  but each behavior must subsequently fail on its assertion against a minimal
  implementation. Do not accept a zero-test filter or unrelated build failure
  as evidence. Configuration and documentation need checks, not tautological tests.
- Real terminal correctness is checked by logs and terminal state, never
  pixels. Wait for conditions, not fixed sleeps. iTerm2 runs only in the VM.
- Published entrypoints contain no personal paths, source-app names, private
  notes, or extraction credits. Internal adoption docs name repositories and
  repo-relative paths. Generated skill directories are out of scope.

## Review Focus

| Input or failure mode | Required behavior | Owning task |
| --- | --- | --- |
| Zero-sized or disappearing areas, extreme coordinates, history eviction | No arithmetic panic, stale highlight, click, or copy; cancel invalid ownership | 2, 4, 7, 8 |
| Partial writes, flush failure, panic, explicit disable | Typed failure and best-effort cleanup; no double successful teardown or abandoned mode | 1, 11, 12 |
| Grapheme clusters, wide-cell halves, wrapped history beyond the viewport | Whole original characters and logical lines; restore scroll offset | 3, 4, 5 |
| Lost release, second press, focus loss, resize, late tick | No phantom copy or pass-through switch; deadlines stop when cancelled | 7, 8, 9 |
| Intercepted modifiers, Windows hover, missing query replies | Assert M0-specific degradation and preserve keyboard selection without blocking | 6, 10, 12, 14, 15, 16 |

## Starting state and authority

As inspected on 2026-11-22, the workspace has the empty umbrella crate and
`rig/probe`; no mouse crate or mouse scenarios exist. The existing
`crates/*` workspace glob already discovers a new crate. The feature-matrix
script already reads cargo metadata, so do not replace it with a hardcoded
crate list. All three runner build paths currently omit the `vt100` feature
required by `mouse_lab`. xterm's npm test script lists test files explicitly.
Orca's runner currently waits for a program to exit without driving keys.

The spec reconciliation is an existing uncommitted change in the planning
worktree. Preserve it. Before execution, recheck Git state and commit the
approved spec on the work branch before creating crate code, as the roadmap
requires. Reuse an isolated worktree through `wt`; do not edit main or assume
the planning branch has been merged. Refresh paths and source observations
when each task starts.

Some older prose in `docs/milestones.md`, `docs/decisions.md`, and
`docs/rules/tui.md` still assumes no stock-tmux forwarding or unmeasured
modifiers. For M0 capability facts the reconciled spec and `docs/terminals.md`
are authoritative. Task 17 aligns the stale prose rather than restoring
obsolete behavior or requesting a second Alt decision.

The locally available vt100 0.16.2 source exposes `set_scrollback`,
`scrollback`, `cell`, `row_wrapped`, mouse protocol/encoding, alternate screen,
and application cursor queries. `set_scrollback(usize::MAX)` clamps to actual
history length; save and restore the offset to discover it. This is a source
inspection, not a completed behavioral proof: Task 4 must prove wrapped-history
access and offset restoration against a real parser before controller work.

## File ownership

Paths below are repository-relative. All are planned changes, not claims that
files already exist. Add module declarations only as their implementation lands.

| Files | Responsibility |
| --- | --- |
| `Cargo.toml`, `Cargo.lock`, `clippy.toml` | Shared dependencies, lock resolution, disallowed text-width calls |
| `crates/ratatui-mouse/Cargo.toml`, `src/lib.rs`, `README.md` | Feature contract, exports, public entrypoints |
| `src/modes.rs`, `src/modes/platform.rs`, `src/error.rs` in the mouse crate | Reporting lifecycle, sole platform split, typed errors |
| `src/settings.rs`, `src/content.rs`, `src/hit.rs`, `src/clicks.rs` | Settings, inclusive coordinates, hit registry, click counter |
| `src/words.rs`, `src/surface.rs`, `src/surface/line.rs`, `src/vt100.rs` | Cell-aware text, surface traits, adapters |
| `src/selection.rs`, `src/highlight.rs`, `src/outcome.rs` | Selection data, highlight rectangles, reports |
| `src/passthrough.rs`, `src/passthrough/encoding.rs`, `src/select_mode.rs` | Pure routing, byte encoding, key commands |
| `src/controller/mod.rs`, `plain.rs`, `select.rs`, `forward.rs`, `timing.rs`, `keyboard.rs` | Controller API and distinct gesture responsibilities |
| `crates/ratatui-mouse/tests/*.rs`, `tests/common/mod.rs`, `tests/common/surface_contract.rs` | Public behavior tests and composable shared fixtures |
| `crates/avandar-tui-kit/Cargo.toml`, `src/lib.rs`, `tests/mouse.rs` | Optional mouse re-export, with no default dependency activation |
| `crates/ratatui-mouse/examples/mouse_lab.rs`, `examples/mouse_lab/{args,app,draw,log,session}.rs` | Example entrypoint, fixture app, structured logs, terminal shell |
| `rig/xterm/src/mouse_{scenarios,assertions}.mjs`, `test/mouse*.test.mjs` | Browser-driven mouse suite and semantic assertions |
| `rig/xterm/src/{args,harness,scenarios}.mjs`, `rig/xterm/package.json` | Scenario dispatch, feature-aware builds, test discovery |
| `rig/orca/run.sh`, `rig/orca/mouse_lab.mjs`, `rig/orca/test/mouse_lab.test.mjs` | Live keyboard-driven lab verification |
| `rig/iterm2/driver/{cli_args,main,scenarios}.py`, `driver/mouse_scenarios.py`, `tests/test_mouse_scenarios_unit.py` | VM scenario dispatch, build support, VNC mouse checks |
| `.github/workflows/ci.yml`, `docs/testing.md`, `docs/terminals.md` | Platform matrix, commands, measured results |
| `docs/mouse.md`, `docs/architecture.md`, `docs/decisions.md`, `docs/milestones.md`, `docs/rules/tui.md`, `CHANGELOG.md`, root `README.md` | Permanent overview, release judgments, aligned capability documentation |

If any listed module exceeds its budget, split only that module along a real
responsibility and update the plan before proceeding. Do not add unrelated
widgets or alter the M0 probe to make mouse assertions pass.

## Execution conventions

Tasks 1 to 17 build and verify the toolkit. Task 18 closes its implementation
branch and reaches the release approval boundary. The
[companion dif adoption plan](2026-11-22_m1-dif-mouse-adoption.md) owns the
three later stages in AvandarLabs/avandar. Splitting by repository lets the
completed toolkit plan be deleted at branch closure while adoption remains
explicitly queued. Both plans together cover all of M1.

For Rust tasks, place integration tests in the named file, import the public
API, and use exact names from the spec. `tests/common/mod.rs` provides small
fixtures as needed: `create_mouse_event(kind, position, modifiers) -> Event`,
`create_areas() -> HitAreas<TestTarget>`, and a `TestSurfaces` implementation
of `SurfaceSet<TestTarget>`. Use enum targets `Left`, `Right`, `Tab`, and
`Divider`. Construct `Instant` once in each test and advance it by values;
never sleep. Each test named below must contain the listed concrete assertions.
Assertion snippets below omit imports and fixture setup; they pin the expected
values rather than providing whole test bodies.
Add focused fixtures in their owning task, not a universal test framework.

For each task: complete its red step, confirm the expected failure, implement,
run its green checks, update `docs/mouse.md` for the capability just added,
and commit the task's explicit files on the work branch. Run `just check`
once at the end of each task that changes Rust. Use targeted tests during
red/green loops. Commands below run from the toolkit root unless labelled dif.

## Task 1: Reporting lifecycle and first usable crate

**Files:** Create mouse `Cargo.toml`, `src/{lib,modes,error}.rs`,
`src/modes/platform.rs`, `tests/modes.rs`, and `docs/mouse.md`; modify root
`Cargo.toml`, `Cargo.lock`, and `docs/architecture.md`.

**Interfaces:** Produce `ENABLE_MOUSE_REPORTING`, `DISABLE_MOUSE_REPORTING`,
`enable_mouse_reporting(&mut impl Write) -> Result<(), MouseError>`,
`disable_mouse_reporting(&mut impl Write) -> Result<(), MouseError>`,
`MouseReporting<W: Write>::enable(W) -> Result<Self, MouseError>` and
`disable(self) -> Result<(), MouseError>`. Use the spec's two error variants.

- [x] Write `modes_bytes_and_guard_restore_once`: on macOS/Linux a recording
  writer receives exactly M1 then M5 on drop; explicit disable does not append
  another M5. Check the complete byte strings, including M5's eight resets.
- [x] Write `write_and_flush_failures_preserve_io_source`: inject short writes,
  write failure, and flush failure; `write_all` completes short writes, errors
  match Enable/DisableReporting and `source()` retains the I/O error. A failed
  explicit cleanup must not prevent a best-effort drop cleanup. A partial
  enable followed by an error attempts best-effort teardown before returning
  the original enable error; check this with the injected writer too.
  ```rust
  assert_eq!(ENABLE_MOUSE_REPORTING, b"\x1b[?1000h\x1b[?1002h\x1b[?1006h");
  assert_eq!(DISABLE_MOUSE_REPORTING,
      b"\x1b[?9l\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1005l\x1b[?1006l\x1b[?1015l\x1b[?1016l");
  // In the recording-writer guard case, after explicit disable and drop:
  assert_eq!(recorded_bytes, [ENABLE_MOUSE_REPORTING, DISABLE_MOUSE_REPORTING].concat());
  ```

- [x] Run `cargo test -p ratatui-mouse --test modes`. Initially it must fail
  for the missing package/API, then for incorrect bytes or lifecycle behavior.
- [x] Add the manifest with inherited metadata/lints and no default feature;
  add shared unicode-segmentation 1 and vt100 0.16 versions with reasons.
  The member uses `vt100 = { workspace = true, optional = true }` and
  `vt100 = ["dep:vt100"]` in its feature table. Add no placeholder modules
  or unfinished public methods.
- [x] Implement Unix writes/flushes and Windows crossterm console operations
  only in `modes/platform.rs`. Verify crossterm's restoration semantics; do not
  emulate them with raw Windows calls. Document the supplied-writer boundary.
- [x] Repeat the modes test, `just lint-targets`, and `just check`; expect
  pass. Windows no-byte and console-state behavior is completed on the real
  runner in Task 17, not simulated by a redirected writer on macOS.
- [x] Document modes, panic-hook caller responsibility, and the new crate's
  architecture/dependency rows; commit as `feat(mouse): add reporting lifecycle`.

## Task 2: Geometry, settings, and click counting

**Files:** Create `src/{settings,content,hit,clicks}.rs`,
`tests/{geometry,clicks}.rs`; modify exports and `docs/mouse.md`.

**Interfaces:** Produce the spec's `MouseSettings`, `WordCharacters`,
`ContentPoint`, inclusive ordered `ContentRange`, `HitAreas<T>`,
`ClampedPosition`, `clamp_position_into(Rect, Position, u16) -> Option<ClampedPosition>`,
and `ClickCounter<T>::register_press(&T, Position, Instant, &MouseSettings) -> u8`.

- [x] Write `topmost_hit_clamp_and_content_roundtrip`: later pushed modal
  areas win; empty rectangles hit nothing; `area_of` returns the latest area;
  `Rect(10,5,20,8)` clamps `(3,2)` to `(10,5)` with overshoot `-3`, and
  `(60,20)` to `(29,12)` with overshoot `8`. Screen/content conversion
  round-trips with nonzero horizontal and vertical origin. Reversed ranges
  normalize, inclusive containment and union preserve both ends.
- [x] Write `click_run_obeys_time_target_distance_and_reset`: counts are
  `[1,2,3,1]`; a 500 ms gap and one cell in both axes still continue; a
  larger gap, two cells, a different target, or reset restarts at 1.
- [x] Add `empty_and_extreme_geometry_does_not_overflow`: zero width/height,
  near-u16 coordinate limits, negative content origins, and margins at least
  the area's height do not panic or produce out-of-area cells.
  ```rust
  let clamped = clamp_position_into(Rect::new(10, 5, 20, 8), Position::new(3, 2), 0).unwrap();
  assert_eq!(clamped.position, Position::new(10, 5));
  assert_eq!(clamped.overshoot_rows, -3);
  assert_eq!(counts, [1, 2, 3, 1]);
  ```

- [x] Run `cargo test -p ratatui-mouse --test geometry --test clicks` and
  confirm the expected red assertion for each behavior.
- [x] Implement the interfaces with safe widened/saturating arithmetic and
  spec defaults. Keep click storage target-generic with `Clone + PartialEq`,
  without imposing `Copy`, `Eq`, `Hash`, or `Ord` on app targets.
- [x] Run the same tests and `just check`; expect pass. Commit as
  `feat(mouse): add hit areas content coordinates and click counting`.

## Task 3: Cell-aware text and LineSurface

**Files:** Create `src/{words,surface}.rs`, `src/surface/line.rs`,
`tests/{words,line_surface}.rs`, `tests/common/{mod,surface_contract}.rs`;
create the exact pass-through request value types in `src/passthrough.rs`
required by `SelectableSurface::pane_request` (routing/encoding stays Task 6);
create `clippy.toml`; modify exports, root workspace lint configuration if
needed, and `docs/mouse.md`.

**Interfaces:** Produce `word_columns_in_line(&str,u32,&WordCharacters) -> Option<(u32,u32)>`,
`character_columns_in_line(&str,u32) -> Option<(u32,u32)>`,
`text_in_columns(&str,u32,u32) -> &str`, `SelectableSurface`, `SurfaceSet<T>`,
`NoSurfaces`, `ScrollPosition`, and `LineSurface::new(&[L], &mut ScrollPosition,u16)`
with `with_wrapped_rows(&[bool])`. The surface trait signatures are exactly
the spec's Surfaces block, including `&mut self` text lookups and `i32` scrolls.

- [x] Write `words_use_cells_and_keep_original_graphemes`: for `"a界b"`,
  character columns 1 and 2 both yield `(1,2)` and text selecting either cell
  is `"界"`; `"file.rs:12"` is one default word. Quotes/brackets are excluded,
  blanks/past-end return None, and combining text remains byte-for-byte intact.
- [x] Write shared `assert_surface_contract(&mut dyn SelectableSurface)`
  over common visual rows `["alpha", "beta  ", "a界b", "wrappe", "d  "]`
  with wrapped flags `[false,false,false,true,false]`, width six, two visible rows,
  and the viewport initially scrolled away from the start. The full inclusive
  range copies `"alpha\nbeta\na界b\nwrapped"`. Assert joined soft wraps, real newlines,
  no trailing newline, whole wide cells, unchanged origin after `text`, and
  the actual signed count returned at both scroll limits.
  ```rust
  assert_eq!(character_columns_in_line("a界b", 2), Some((1, 2)));
  assert_eq!(text_in_columns("a界b", 2, 2), "界");
  assert_eq!(word_columns_in_line("file.rs:12", 4, &WordCharacters::default()), Some((0, 9)));
  ```

- [x] Run `cargo test -p ratatui-mouse --test words --test line_surface`;
  prove the behavioral red failures, then implement grapheme/cell traversal
  and the borrowed row adapter. Handle empty rows and absent wrap entries
  safely; invalid optional wrap metadata must not cause indexing panics.
- [x] Disallow `unicode_width::UnicodeWidthStr::width`,
  `unicode_width::UnicodeWidthChar::width`, `ratatui::text::Span::width`, and
  `ratatui::text::Line::width`, using resolved defining paths if clippy requires
  them, and enable workspace `clippy::disallowed_methods` checking. Verify
  lint configuration is accepted rather than relying on an untested spelling.
- [x] Run both suites and `just check`; expect pass. Document the surface
  contract as a reusable test recipe for app-defined surfaces, without adding
  an unapproved public test-support feature. Commit as
  `feat(mouse): add cell-aware text and selectable line surfaces`.

## Task 4: Prove and implement the vt100 surface

**Files:** Create `src/vt100.rs`, `tests/vt100_surface.rs`; reuse
`tests/common/surface_contract.rs`; modify exports and `docs/mouse.md`.

**Interfaces:** Produce feature-gated `Vt100Surface::new(&mut vt100::Screen)`
and `SelectableSurface` implementation. Task 6 adds
`PaneMouseRequest::from_screen(&vt100::Screen) -> Self` once its types exist.

- [ ] Write `history_wraps_and_offset_restoration` using a real
  `vt100::Parser::new(3,8,20)` fed enough output to create more than three
  history rows. Set a nonzero offset; select a soft-wrapped logical line
  across history/live-screen boundaries; assert exact text and original offset.
- [ ] Write `line_numbers_follow_oldest_history_until_capacity`: recreate
  the adapter between output bursts; selected text stays stable until history
  reaches capacity. Also exercise alternate-screen zero history, both halves
  of a wide character, and the shared surface contract.
  ```rust
  let saved_origin = surface.origin();
  let copied = surface.text(history_range);
  assert_eq!(copied, expected_history_text);
  assert_eq!(surface.origin(), saved_origin);
  drop(surface);
  assert_eq!(parser.screen().scrollback(), saved_offset);
  ```

- [ ] Run `cargo test -p ratatui-mouse --features vt100 --test vt100_surface`;
  prove red failures before implementation. If public vt100 APIs cannot
  support a required behavior, stop and report the concrete spec conflict;
  do not fork, patch, or silently drop a requirement. Gate the integration
  test file with `#![cfg(feature = "vt100")]` so empty-feature checks compile.
- [ ] Implement offset discovery by saving the offset, setting usize::MAX,
  reading the clamped value, and restoring. Map content lines into temporary
  viewports using public `cell` and `row_wrapped`; cache only within the
  borrowed adapter so later reconstructions see new output. Use a private
  restoration guard where offset-changing reads could otherwise leave state.
- [ ] Run the suite, `cargo test -p ratatui-mouse --no-default-features`, and
  `just check`; expect pass. Document the history-cap eviction limit and
  constructor borrowing. Commit as `feat(mouse): add vt100 history selection`.

## Task 5: Selection data and highlight rectangles

**Files:** Create `src/{selection,highlight,outcome}.rs`,
`tests/highlight.rs`; modify exports, controller-private construction helpers
when needed, and `docs/mouse.md`.

**Interfaces:** Produce spec `SelectionUnit`, `SelectionStage`, read-only
`Selection<T>` accessors, `MouseReport<T>`, `PointerPress`, `MouseOutcome<T>`,
`DEFAULT_HIGHLIGHT_STYLE`, and `HighlightRects::{from_range,as_slice,render}`.
Selection mutation stays crate-private; do not expose new public constructors.

- [ ] Write `highlight_clips_to_at_most_three_rectangles`: one-row selection
  gives one rectangle; a multirow selection gives partial first/last plus the
  full middle; offscreen ranges give no rectangles. Check nonzero horizontal
  origins, empty areas, and clipping on all four edges.
- [ ] Write `highlight_patches_style_without_replacing_text`: render to a
  real `Buffer`, assert unchanged cell symbols and unrelated style attributes,
  plus reversed-video default. Character snapping belongs before `from_range`,
  which receives no surface; verify a snapped two-cell range highlights both.
  ```rust
  let rectangles = HighlightRects::from_range(
      ContentRange::new(ContentPoint { line: 0, column: 1 }, ContentPoint { line: 0, column: 2 }),
      ContentPoint { line: 0, column: 0 }, Rect::new(10, 5, 8, 3));
  assert_eq!(rectangles.as_slice(), &[Rect::new(11, 5, 2, 1)]);
  ```

- [ ] Run `cargo test -p ratatui-mouse --test highlight` to establish red;
  implement inclusive-to-rectangle conversion and style patching, then rerun
  it and `just check`. Commit as `feat(mouse): add selection reports and highlight`.

## Task 6: Routing and pass-through encoders

**Files:** Create `src/passthrough.rs`, `src/passthrough/encoding.rs`,
`tests/passthrough.rs`; modify `src/vt100.rs`, exports, and `docs/mouse.md`.

**Interfaces:** Produce `PaneMouseRequest`, `MouseTracking`, `MouseEncoding`,
`PressRoute`, `WheelRoute`, `press_route`, `wheel_route`, `build_mouse_report`,
`build_wheel_arrow_keys`, and feature-gated `PaneMouseRequest::from_screen`,
all with signatures from the spec's Pass-through and vt100 blocks.

- [ ] Write `routing_priority_selects_before_forwarding`: cover every row of
  both routing tables with selection mode, either default modifier separately,
  pass-through disabled, tracking off/on, alternate screen, and wheel arrows.
- [ ] Write `encoders_use_pane_coordinates_and_protocol_limits`: for area
  `(10,5,20,8)`, left Down at `(10,5)` in SGR is `b"\x1b[<0;1;1M"` and Up
  is `b"\x1b[<0;1;1m"`. Assert Shift/Alt/Control codes 4/8/16, drag +32,
  wheel 64..67, default coordinate limit 223, UTF-8 limit 2015, and SGR's
  larger coordinates. Rejected coordinates/events return None without overflow.
- [ ] Write `tracking_filters_and_cursor_arrows`: Press tracking excludes
  releases/drags/wheel; AnyMotion never encodes bare Moved; rows -3/3 emit
  three `CSI A`/`CSI B`, with application mode using `SS3 A`/`SS3 B`.
  Feed each supported mode/encoding into a real vt100 parser and assert mapping.
  ```rust
  assert_eq!(build_mouse_report(&left_down, Rect::new(10, 5, 20, 8), &sgr_request),
      Some(b"\x1b[<0;1;1M".to_vec()));
  assert_eq!(build_wheel_arrow_keys(-3, false), b"\x1b[A\x1b[A\x1b[A");
  assert_eq!(press_route(&sgr_request, KeyModifiers::ALT, false, &MouseSettings::default()), PressRoute::Select);
  ```

- [ ] Run `cargo test -p ratatui-mouse --all-features --test passthrough`,
  confirm red, implement pure routing/encoding, then rerun and `just check`.
  Commit as `feat(mouse): add pane mouse routing and encoding`.

## Task 7: Controller ownership, plain gestures, and forwarding

**Files:** Create `src/controller/{mod,plain,forward}.rs`,
`tests/{controller_plain,controller_forward}.rs`;
modify `tests/common/mod.rs`, exports, and `docs/mouse.md`.

**Interfaces:** Produce `MouseController::new(MouseSettings)`, settings
accessors, `handle_event<S: SurfaceSet<T>>(&mut self,&Event,Instant,&HitAreas<T>,&mut S) -> MouseReport<T>`,
`gesture_owner`, `cancel_gesture`, `clear_selection`, `forget_target`, and
the spec's selection/text accessors as their backing state is introduced.
Do not publish placeholder select-mode methods; Task 10 adds them.

- [ ] Write `plain_gesture_keeps_press_owner`: press Left, drag over Right,
  release there; all outcomes name Left, `Dragged`/`DragEnded` carry the
  original start and clamped end. A press/release without drag emits Clicked.
- [ ] Write `lost_release_and_missing_owner_cancel_without_copy`: second
  press replaces the gesture, Up without Down produces no click/copy, removed
  area, FocusLost, Resize, and `forget_target` cancel with no final outcome.
  Use non-Copy enum targets to prove generic bounds stay minimal.
- [ ] Write `forward_route_is_fixed_at_press`: later modifiers or mouse
  request changes do not switch the gesture to selection. All Forward bytes
  stay pane-relative and `pressed_target` accompanies the press. Bare Moved
  yields no actionable outcome or forwarded bytes, even with AnyMotion.
  ```rust
  assert_eq!(controller.gesture_owner(), Some(&TestTarget::Left));
  assert!(matches!(release_report.outcome, MouseOutcome::DragEnded { target: TestTarget::Left, .. }));
  assert!(!matches!(orphan_release.outcome, MouseOutcome::Clicked { .. } | MouseOutcome::Copy { .. }));
  ```

- [ ] Run `cargo test -p ratatui-mouse --test controller_plain --test controller_forward`
  and prove red; implement the private gesture enum and owner lookup per
  event, keeping plain and forwarding branches in their own modules.
- [ ] Rerun both suites and `just check`; expect pass. Commit as
  `feat(mouse): add controller gesture ownership and forwarding`.

## Task 8: Controller selection, extension, and copy

**Files:** Create `src/controller/select.rs`, `tests/controller_selection.rs`;
modify controller state, `src/selection.rs`, and `docs/mouse.md`.

**Interfaces:** Complete `selection() -> Option<&Selection<T>>`,
`selected_text<S: SurfaceSet<T>>(&self,&mut S) -> Option<String>` and selection
paths of `handle_event`. Consume surface expansion methods from Task 3.

- [ ] Write `selection_stages_distinguish_click_from_returning_drag`: a
  single press has Pressed state and no selected text; release emits Clicked;
  a drag away then back stays Dragging and release copies one whole character.
- [ ] Write `multi_click_drag_preserves_anchor_unit`: double-click a path,
  drag in either direction to extend by words; triple-click wrapped text,
  extend by logical lines; original unit remains included. A drag resets the
  click run; a one-cell click drift still continues it.
- [ ] Write `modifier_extension_and_copy_settings`: Shift and Alt each
  extend a Done selection on its own target, blank-only selection drops,
  copy-off emits no Copy, keep-off clears after copy, and key/resize clearing
  obey their flags. Bare modifier keys and key releases do not clear.
- [ ] Write `selection_cancellation_never_copies`: disappearance, changed
  areas, focus loss, replacement press, and cancelled gesture produce no
  phantom copy or highlight outside the owner. Snap wide-character endpoints
  before storing ranges and before highlight projection.
  ```rust
  assert_eq!(controller.selection().unwrap().stage(), SelectionStage::Dragging);
  assert!(matches!(release_report.outcome, MouseOutcome::Copy { ref text, .. } if text == "界"));
  ```

- [ ] Run `cargo test -p ratatui-mouse --test controller_selection` red;
  implement the spec's stage/unit transitions and finish/drop rules. Rerun
  it and `just check`; commit as `feat(mouse): add selection gesture controller`.

## Task 9: Wheel, edge-scroll deadlines, and redraw budget

**Files:** Create `src/controller/timing.rs`, `tests/controller_timing.rs`;
modify controller event dispatch and `docs/mouse.md`.

**Interfaces:** Produce spec `tick<S: SurfaceSet<T>>(&mut self,Instant,&HitAreas<T>,&mut S) -> MouseReport<T>`
and `next_deadline() -> Option<Instant>`. Complete wheel dispatch, including
`Wheel { rows, columns, modifiers, target }` for horizontal and vertical input.

- [ ] Write `edge_ticks_scroll_without_new_events`: dragging below the area
  schedules now and never scrolls itself; tick scrolls overshoot rows capped
  at 10, then schedules +50 ms. Repeated drag events alone never accelerate it.
  At a boundary, release, owner removal, resize or cancellation clears work.
- [ ] Write `wheel_during_selection_updates_content_end`: normal wheel
  returns signed 3-row Wheel for the hovered target regardless of focus;
  active selection scrolls its owner, updates the endpoint, and emits no Wheel.
  Exercise pane request routing and horizontal columns without flipping direction.
- [ ] Write `redraw_budget_delivers_pending_change_at_deadline`: changes
  within 16 ms suppress extra redraws, tick at the deadline marks redraw due,
  and next_deadline is the minimum of redraw and edge work. A late tick does
  not replay a burst of missed scrolling intervals. Zero intervals/maxima and
  tiny panes do not spin an unbounded loop in one call or panic.
  ```rust
  assert_eq!(controller.next_deadline(), Some(now));
  // After one due edge tick with overshoot 20 and default maximum 10:
  assert_eq!(surfaces.surface_mut(&TestTarget::Right).unwrap().origin().line, previous_origin_line + 10);
  // Inspect minimum with a pending redraw too; edge-only next tick is +50 ms.
  ```

- [ ] Run `cargo test -p ratatui-mouse --test controller_timing` red;
  implement timing from injected Instant values, one due scroll step per tick,
  and bounded arithmetic. Rerun and `just check`; commit as
  `feat(mouse): add timed edge scrolling wheel routing and redraw budget`.

## Task 10: Keyboard select mode

**Files:** Create `src/select_mode.rs`, `src/controller/keyboard.rs`,
`tests/select_mode.rs`; modify controller API, exports, and `docs/mouse.md`.

**Interfaces:** Produce spec `SelectionMotion`, `SelectionCommand`,
`selection_command_from_key(&KeyEvent) -> Option<SelectionCommand>` and
`MouseController::{enter_select_mode,exit_select_mode,select_mode_target,select_mode_cursor,apply_command}`
with the exact signatures in the controller API block.

- [ ] Write `keymap_covers_the_spec_and_ignores_releases`: table-test h/j/k/l,
  arrows, w/b, 0/Home, $/End, Ctrl+U/D, PageUp/Down, g/G, v/Space, V, W,
  y/Enter, Esc/q. Releases and unbound keys map to None.
- [ ] Write `keyboard_only_selection_scrolls_and_copies`: enter on the last
  visible row, v then motions selects cells, V selects lines, W selects a
  word, y/Enter copies and leaves, Cancel clears then leaves on the second
  press. Traverse wrapped/wide text and both viewport limits.
- [ ] Write `select_mode_intercepts_before_pane_forwarding`: unbound key
  presses are consumed, target's mouse always selects, another-target press
  exits, missing target/area refuses entry, and disabling built-in key mapping
  still permits `apply_command` while preventing key leakage to the program.
  ```rust
  assert_eq!(selection_command_from_key(&KeyEvent::new(KeyCode::Char('W'), KeyModifiers::SHIFT)),
      Some(SelectionCommand::SelectWord));
  assert!(controller.select_mode_target().is_none()); // after Copy or second Cancel
  ```

- [ ] Run `cargo test -p ratatui-mouse --test select_mode` red; implement
  pure motion/command decisions and controller integration. Rerun and
  `just check`; commit as `feat(mouse): add keyboard select mode`.

## Task 11: Umbrella integration and observable mouse_lab

**Files:** Modify umbrella manifest/lib; create `tests/mouse.rs` there,
mouse `examples/mouse_lab.rs`, `examples/mouse_lab/{args,app,draw,log,session}.rs`,
`tests/mouse_lab.rs`, mouse README; modify shared dev-dependencies if needed.

**Interfaces:** `avandar_tui_kit::mouse` re-exports ratatui-mouse behind `mouse`.
`mouse_lab` requires feature `vt100`; flags are exactly `--log <path>`,
`--query-modes`, and `--settings <json>` plus standard help. No child is spawned.

- [ ] Write `umbrella_mouse_reexport_is_usable`: import
  `avandar_tui_kit::mouse::{HitAreas,MouseController,MouseSettings}` with feature
  mouse, create a controller and hit list, and call a no-surface event path.
- [ ] Write `lab_fixture_and_log_records_are_deterministic`: left text includes
  `file.rs:12`, CJK, a three-row soft wrap, and offscreen history; right parser
  has fixed history and wide cells. Log lines distinguish event, report,
  selection, deadline, tick, sequence, layout, and readiness records. Tests
  parse escaped quotes/newlines correctly and preserve exact copied text.
  Left fixture begins `alpha`, `beta`, `gamma`, `file.rs:12`; X5 selects the
  first three rows and X13 uses keyboard motions plus W on the path row.
  Log `kind: "report"` contains `outcome: { kind, target, text?, bytes? }`,
  `pressed_target`, and `is_redraw_due`; bytes are numeric arrays.
  A `kind: "layout"` record contains `areas: [{target,x,y,width,height}]`.
  Target strings are `left`, `right`, `tab`, `divider`. Each record has a
  monotonic sequence number so assertions can fence one interaction's output.
  Settings JSON accepts snake_case setting names and integer milliseconds for
  duration keys ending in `_ms` (for example `auto_scroll_interval_ms`), rejects
  unknown fields/invalid values with a recovery hint, and preserves unspecified
  defaults. This encoding belongs only to the example, not the public library.
  Extract testable example support functions into the named example modules;
  `tests/mouse_lab.rs` may include those modules with explicit relative paths.
  Do not make private lab helpers part of the crate's public API.
  ```rust
  let controller = avandar_tui_kit::mouse::MouseController::<u8>::new(
      avandar_tui_kit::mouse::MouseSettings::default());
  assert!(controller.gesture_owner().is_none());
  ```

- [ ] Run `cargo test -p avandar-tui-kit --features mouse --test mouse` and
  `cargo test -p ratatui-mouse --features vt100 --test mouse_lab` red. Implement
  the umbrella feature with optional path-and-version dependency and docs.
- [ ] Implement the lab's own app loop: Tab focuses; s starts select mode;
  m cycles requests; a toggles alternate screen; PageUp/Down scroll; q quits;
  ! panics outside select mode. Inside select mode its keys win. Draw records
  areas and layout; copy and Forward are logged, never sent to clipboard/child.
- [ ] Use a flushed JSONL `kind: ready` record plus `<log>.ready` only after
  mouse modes and initial layout are ready. Log mode-query replies or bounded
  absence; missing replies never gate input forever. A panic hook restores
  modes before panic output, raw mode and alternate screen are also restored.
- [ ] Add serde 1 with derive and serde_json 1 for lab JSON settings/logs as
  dev-dependencies, declared once in workspace dependencies and kept off
  normal dependency edges. No
  library serde derives or parser dependency is permitted. Add a dev-only
  ratatui dependency with its `crossterm` feature for the example backend;
  normal library dependency edges keep ratatui defaults off.
- [ ] Run both suites, `cargo build -p ratatui-mouse --example mouse_lab --features vt100`,
  `./scripts/ci/check-features.sh`, and `just check`; expect pass. Commit as
  `feat(mouse): expose umbrella feature and add mouse lab`.

## Task 12: xterm runner support and lifecycle scenarios

**Files:** Create xterm `src/mouse_scenarios.mjs`, `src/mouse_assertions.mjs`,
`test/mouse.test.mjs`; modify `src/{args,harness,scenarios}.mjs`,
`package.json`; extend mouse lab log synchronization only if necessary.

**Interfaces:** Add scenario `mouse` for all lab scenarios and `x1` through
`x14` for individual ones, valid only with `mouse_lab`. Default lab scenario
is `mouse`; existing probe/default passthrough behavior stays compatible.
`runMouseScenario(runtime, scenario) -> Promise<object>` returns per-scenario
results and uses existing session lifecycle, bridge, readiness, and log archive.

- [ ] Write Node tests that parser/build dispatch selects
  `cargo build -p ratatui-mouse --example mouse_lab --features vt100`, preserves
  extra arguments, rejects lab scenarios for other programs, and keeps probe
  builds unchanged. Include new test files in the explicit npm test script.
  ```javascript
  assert.equal(parseHarnessArgs(['mouse_lab']).scenario, 'mouse');
  assert.equal(parseHarnessArgs(['capability_probe']).scenario, 'all');
  assert.throws(() => parseHarnessArgs(['capability_probe', 'x1']), CliUsageError);
  ```

- [ ] Run `node --test rig/xterm/test/mouse.test.mjs` red, then implement
  dispatch and feature-aware builds. Do not turn on every workspace feature
  for every existing example or normalize mouse reports through the probe schema.
  Update help for all new scenarios; help must exit 0 without terminal/build
  side effects. Invalid invocations print usage and recovery guidance to
  stderr, leave machine stdout empty, and fail nonzero. Preserve NO_COLOR
  and redirected-output behavior of the changed entrypoints.
- [ ] Add X1, X2, X3: clean exit/panic restores direct outer tracking; Unix
  setup is drag+SGR; Windows setup is any+SGR with missing DECRQM replies.
  Bare motion produces no actionable controller result on Windows, no raw
  motion on Unix. Include hover while pane pass-through is enabled.
- [ ] Run `just xterm mouse_lab x1 -- --query-modes` before/after lifecycle
  implementation, expecting semantic assertion failure then pass; run X2/X3
  with their individual names. Synchronize pointer-negative checks with an
  input/log fence, not an absence check after a fixed delay.
- [ ] Run `npm --prefix rig/xterm test` and `just check`; preserve probe
  baselines. Commit as `test(mouse): drive mouse lab lifecycle in xterm`.

## Task 13: xterm gesture and keyboard scenarios X4 to X14

**Files:** Extend `src/mouse_scenarios.mjs`, `src/mouse_assertions.mjs`,
`test/mouse.test.mjs`; split gesture fixtures to `src/mouse_fixtures.mjs` if
needed; modify `docs/testing.md`.

**Interfaces:** Scenarios consume logged lab layout to convert cells to DOM
coordinates. The report includes scenario IDs, success/assertion details, and
platform conditions; failures retain raw JSONL through RIG_XTERM_LOG_DIR.

- [ ] Write assertion tests against deliberately wrong reports: X4 wrong
  pane/target, X5 wrong copied characters, X6 split wrapped line, X7 copy
  missing history, X8 wrong signed step, X9 leaked Wheel, X10 wrong pane bytes,
  X11 modifier leak, X12 wrong arrow encoding, X13 missing keyboard Copy,
  X14 wrong button/modifier must each fail with the scenario ID.
  ```javascript
  // In independent sessions: left pane rows "alpha", "beta", "gamma".
  assert.equal(copyReport.outcome.text, 'alpha\nbeta\ngamma'); // X5
  assert.equal(forwardReports.length, 0); // X11 selection path
  assert.equal(keyboardCopy.outcome.text, 'file.rs:12'); // X13 selects path with W
  ```

- [ ] Run Node tests red, then implement each browser interaction and its
  exact spec assertion. X7 waits for selection reaching a known history row
  before release. Double/triple clicks wait on previous press processing;
  timing intervals only express the gesture, never readiness sleeps.
- [ ] X11 runs independent fresh sessions for Shift and Alt. On Linux and
  Windows Shift causes native selection and no app selection/Forward, while
  raw Windows Moved may occur. Alt selects/copies on all direct platforms.
  X13 injects only keyboard events and checks Copy without any mouse input.
- [ ] Run `just xterm mouse_lab mouse`, then
  `just linux ./rig/xterm/run.sh mouse_lab mouse`; all X1..X14 must pass.
  Run Node tests and `just check`; document commands and commit as
  `test(mouse): cover mouse gestures selection and pass-through in xterm`.

## Task 14: Stock and full tmux verification

**Files:** Extend xterm mouse scenarios/assertions and `test/mouse.test.mjs`;
modify `rig/xterm/src/tmux_session.mjs` only if lifecycle observation needs it,
plus `docs/testing.md` and `docs/mouse.md`.

**Interfaces:** Existing `just xterm-tmux <stock|full> mouse_lab <scenario>`
dispatches the same suite through a private server, with version/config
expectations rather than a single `mouse on` boolean assumption.

- [ ] Write `tmux_mouse_expectations_follow_measured_version` asserting
  stock and full both forward pane events, 3.8 omits Alt-drag motion, 3.3a
  rapid right/Control clicks may arrive Up-only, and missing queries are not
  reported as reset states. Incorrect no-stock-mouse expectations must fail.
  ```javascript
  assert.equal(stockPaneClick.hasDown, true);
  assert.equal(tmux38AltDrag.hasDrag, false);
  assert.equal(tmux33AltDrag.hasDrag, true);
  assert.equal(tmux33RapidRightClick.hasDown, false);
  assert.equal(tmux33RapidRightClick.hasUp, true);
  ```

- [ ] Run Node tests red; implement version-aware assertions. For X1/X2
  distinguish app-requested modes from tmux's own outer tracking after app
  exit. Keep keyboard Copy mandatory, not an excuse to skip positive events.
- [ ] Run `just xterm-tmux stock mouse_lab mouse` and
  `just xterm-tmux full mouse_lab mouse` on macOS; run both through
  `just linux ./rig/xterm/run-tmux.sh <config> mouse_lab mouse` for 3.3a.
  All fourteen scenarios execute in each configuration with the documented
  X11/X14 degradation assertions, not silent skips.
- [ ] Rerun Node tests and `just check`; commit as
  `test(mouse): assert stock and full tmux behavior`.

## Task 15: Live Orca keyboard verification

**Files:** Create `rig/orca/mouse_lab.mjs`, `rig/orca/test/mouse_lab.test.mjs`;
modify `rig/orca/run.sh`, `rig/probe/tests/orca_runner_cli.rs`,
`docs/testing.md`.

**Interfaces:** `just orca mouse_lab -- --query-modes` uses feature-aware
builds, creates an unfocused terminal, waits for lab readiness, drives X1 and
X13 with Orca terminal APIs, waits for successful exit, and returns JSONL.
Reuse `--interactive` for externally controlled sessions; do not change the
probe's existing batch-by-default behavior.

- [ ] Write mocked-driver tests proving a lab launch does not append
  `--batch`, has the vt100 feature at build, waits on ready/log conditions,
  sends only keyboard input, and closes its terminal on failure or success.
  ```javascript
  assert.equal(programArgs.includes('--batch'), false);
  assert.equal(inputCalls.some(call => call.type === 'pointer'), false);
  assert.equal(copyReport.outcome.text, 'file.rs:12');
  ```

- [ ] Run `node --test rig/orca/test/mouse_lab.test.mjs` and
  `cargo test -p rig-probe --test orca_runner_cli` red. Implement the lab
  path through the `orca-cli` skill's documented commands, without focus or
  desktop pointer control. Inspect current CLI capabilities before wiring it.
- [ ] Re-run the capability probe's mouse section in live Orca and compare
  its report to existing baselines. Run the lab X1 with mode queries and X13;
  assert setup/teardown and exact keyboard Copy from logs. No pointer tests
  or Orca scroll-preference claim is made at this layer.
- [ ] Run both focused suites and `just check`; document evidence and
  commit as `test(mouse): verify mouse lab in live Orca`.

## Task 16: iTerm2 VM scenarios

**Files:** Create `rig/iterm2/driver/mouse_scenarios.py`,
`tests/test_mouse_scenarios_unit.py`; modify driver `cli_args.py`, `main.py`,
`scenarios.py`, and `docs/testing.md`.

**Interfaces:** Add `mouse`, `x1`..`x14` scenario dispatch and a feature-aware
lab build, without replacing existing probe or matrix behavior.
`run_mouse_scenario(context: ScenarioRuntimeContext, scenario: str) -> dict`
uses existing VM lifecycle, guest defaults backup, VNC input, and log waiters.

- [ ] Write Python tests for lab argument dispatch, expected build command,
  per-scenario assertions, and restoration after a failed selection/clipboard
  check. Feed wrong Copy and mode states and confirm the scenario fails.
  ```python
  self.assertIn("--features", cargo_args)
  self.assertIn("vt100", cargo_args)
  self.assertEqual(copy_report["outcome"]["text"], "file.rs:12")
  self.assertTrue(defaults_restored_after_failure)
  ```

- [ ] Run `python3 -m unittest discover -s rig/iterm2/tests -p 'test_*.py'`
  red; implement VNC cell mapping from the logged geometry, ! panic, q exit,
  and lab-specific log waits. Do not reuse the probe's F12 panic binding.
- [ ] Run `just iterm2 mouse_lab mouse -- --query-modes`, then the same
  command with `--tmux stock` and `--tmux full` before `--`. Execute X1..X10,
  X12..X14 plus iTerm2's X11 Shift/Cmd checks. X14 expects no app events with
  default iTerm2 context-menu behavior; it must not hang waiting for them.
- [ ] Verify Option-drag contiguous and Option+Cmd-drag rectangular native
  selection using the guest clipboard; Shift/Cmd reach the app, with Cmd
  represented as Alt. Verify wheel directions with natural scrolling off/on.
  Restore clipboard test settings and guest defaults on every exit path.
- [ ] Repeat Python unit tests and `just check`; record actual results in
  docs and commit as `test(mouse): cover iTerm2 mouse behavior in the VM`.

## Task 17: CI matrix, permanent docs, and complete verification

**Files:** Modify `.github/workflows/ci.yml`, mouse README/rustdoc,
root README, `docs/{mouse,testing,terminals,architecture,decisions,milestones}.md`,
`docs/rules/tui.md`; create `CHANGELOG.md`. Update the tracked task through
`repo-product-manager` only at meaningful execution/status transitions.

**Interfaces:** Existing three-platform CI adds `mouse_lab mouse` with vt100
and keeps the capability probe comparisons. Linux/macOS add both tmux modes;
Windows keeps ConPTY event artifacts on every run. Use Node entrypoints in
PowerShell rather than requiring Bash, with explicit nonzero exit propagation.

- [ ] Add the lab suite to all three jobs and preserve stable + Rust 1.88
  checks. Run feature rows for the mouse crate and umbrella: empty, each
  declared feature, and all. The existing metadata-driven script should need
  no change. Required-feature examples must not break empty-feature all-targets.
- [ ] Document keyboard equivalents, event-loop integration, all known terminal
  limits, surface rules, example commands, dependency/features, and the M0
  distinction between native selection and in-app selection. Correct obsolete
  stock-tmux/no-Alt statements in the listed docs with no unrelated rewrites.
- [ ] Put the release judgments in `docs/testing.md`: edge interval 50 ms,
  maximum 10 rows, wheel 3, highlight legibility on dark/light/NO_COLOR.
  Record reviewer/date/default changes when a person performs them. Agents
  run correctness tests; a person supplies only these subjective judgments.
- [ ] Run `just check`, `just lint-targets`,
  `./scripts/ci/check-features.sh`, `just linux cargo test --workspace --all-features`,
  `just linux cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `npm --prefix rig/xterm test`, and the Python VM-driver tests.
- [ ] Execute the complete matrix below. Reuse fresh evidence from Tasks
  12..16 only if the tested code and scenario logic have not changed since;
  rerun every affected layer after a fix. Do not rewrite expected capability
  baselines just to turn a failing mouse scenario green.
- [ ] Request explicit push authorization if absent before starting remote
  Windows CI. After an authorized push, run `just windows --wait`, inspect
  that run's three-platform checks and downloaded Windows logs, and resolve
  failures. A local cross-target lint is not a Windows execution substitute.
- [ ] Verify no personal paths/internal app names enter published entrypoints,
  and record remaining subjective judgments without claiming release readiness.
  Commit as `docs(mouse): document verified behavior and release checklist`.

### Verification matrix

| Layer | Required run | Passing evidence |
| --- | --- | --- |
| 1 | macOS, Linux floor, Windows; empty/vt100/all feature rows | Every layer-1 spec area, both surface contract runs, no feature leakage |
| 2 | Windows MSVC and Linux GNU cross-target clippy | No warnings/errors in platform split |
| 3 | xterm.js on macOS and Linux, X1..X14 | Correct outcome/text/bytes and platform modifier assertions |
| 4 | xterm.js in stock/full tmux, macOS 3.8 and Linux 3.3a, X1..X14 | Positive forwarding plus measured Alt-motion/rapid-click/query limits |
| 5 | Probe mouse modes in live Orca, then lab X1/query and X13 | M0 modes match, keyboard Copy exact, unfocused terminal cleaned up |
| 6 | iTerm2 alone, stock/full tmux; X1..X10 and X12..X14 plus modifier/wheel checks | Exact logs, native clipboard selection, modes restored, defaults restored |
| 7 | Windows ConPTY, X1..X14, stable and Rust 1.88 build/tests | any+SGR setup, ignored hover, clean/panic restoration, uploaded logs |
| Human | Release judgments in testing.md | Named reviewer/date and any default adjustments reverified |

## Task 18: Toolkit completion, review, and release boundary

**Files:** Move `docs/specs/ratatui-mouse.md` to
`docs/archive/ratatui-mouse.md`; delete this completed plan only after the
toolkit implementation is complete; update references, permanent docs,
changelog, and `docs/code-walkthroughs/<completion-date>_m1-ratatui-mouse.md`.

**Interfaces:** A verified toolkit branch ready for full-branch review, then
an explicitly authorized merge and `v0.1.0` tag. No crates.io publishing in M1.

- [ ] Resolve implementation loose ends and review questions. Before deleting
  this toolkit plan, retain outstanding release criteria and a link to the
  queued adoption plan in ATK-1's Notes, with repo-relative pointers, using
  `repo-product-manager`. Keep ATK-1 open until dif is merged.
- [ ] Archive the implemented spec, delete the completed toolkit plan,
  and fix inbound relative links, including the companion adoption plan's
  spec link. Replace its toolkit-plan link with the permanent walkthrough
  when that file is produced. If a required layer remains unverified,
  keep the plan in progress and state the actual blocker instead.
- [ ] Run `avandar-code-review`, fix every surviving finding, rerun
  `just check` and every affected terminal layer, and commit review fixes.
- [ ] Run `diff-review` against all branch changes; copy its generated
  `.difit/<stem>-walkthrough.md` to the dated permanent walkthrough path.
  Run `just check` and commit the walkthrough. Do not invent a walkthrough
  without the required workflow.
- [ ] Complete the final whole-branch review using the workflow's current
  frontend routing. Present the branch for Pablo's full review and approval;
  do not merge under planning or implementation authorization alone.
- [ ] Once explicitly authorized, reverify and merge to main, perform verified
  merged-worktree cleanup, and separately obtain any missing tag/push approval.
  Confirm human release judgments, clean main, and passing Windows CI before
  creating/pushing `v0.1.0`. Record the exact release commit in ATK-1.
- [ ] The milestone is still incomplete here: proceed to dif adoption after
  the tag is accessible, with that repository's own instructions and worktree.

## Coverage and handoff

| Spec area | Tasks |
| --- | --- |
| Modes, Windows split, guards, errors | 1, 11, 12, 17 |
| Hit areas, clicks, content, defaults | 2 |
| Words, surface contract, app rows, vt100 history | 3, 4 |
| Selection stages, highlighting, outcomes | 5, 7, 8 |
| Pane routing, every protocol encoding, arrows | 6, 7 |
| Wheel, edge scrolling, redraw deadlines | 9 |
| Select mode and all keyboard equivalents | 10, 11 |
| Example and feature wiring | 11 |
| X1..X14 and seven terminal layers | 12..17 |
| Docs, human judgments, release and reviews | 17, 18 |
| Three adoption stages | Companion adoption plan, tasks 1 to 3 |

Recommended execution: subagent-driven, sequentially, because independent
checks at the surface/controller/rig boundaries can catch interface drift
before the next layer depends on it. Do not dispatch parallel mutations into
the same worktree. Native execution is also possible after plan approval,
with the required final independent review. No execution method is selected
by this document.

Completion is not a promise that all listed commands already pass: they are
the future verification obligations. Before implementing, obtain review of
this plan and an execution choice. Preserve the release, Windows-CI push,
and cross-repository merge approval boundaries throughout execution.

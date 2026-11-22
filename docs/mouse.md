# Mouse reporting

`ratatui-mouse` provides reporting setup, frame hit areas, content coordinates,
click counting, settings, and borrowed text surfaces for ratatui apps using
crossterm. It is a library building block: the app owns its event loop, drawing stream, terminal lifecycle,
and panic hook. The current crate exposes
the reporting lifecycle and pure interaction primitives; selection and event
routing are planned in the [mouse specification](specs/ratatui-mouse.md).

## Using reporting

Pass the stream the app draws on to `enable_mouse_reporting` and
`disable_mouse_reporting`, or give it to `MouseReporting::enable`. The guard
also accepts a mutable reference to that stream. Dropping an active guard
attempts teardown once and ignores any I/O error. Calling its consuming
`disable` method returns cleanup errors; successful explicit cleanup prevents
another teardown on drop. Failed explicit cleanup leaves the guard active so
drop attempts cleanup again before returning the original failure.

A failed enable can leave reporting partially active. Both the free function
and guard constructor attempt best-effort teardown before returning the
original enable error, even if cleanup also fails. `MouseError` distinguishes
`EnableReporting` from `DisableReporting`, and its error source retains the
underlying I/O error for callers to inspect.

Use one reporting owner at a time. Guards are not a nested restoration stack.
Pausing for a child program means a full teardown followed by a fresh enable;
no tracking mode is reset between setup and teardown.

## Stream and platform boundary

On macOS and Linux, setup writes exactly modes 1000, 1002, and 1006 in that
order, then flushes. This requests press/release events, drag motion while a
button is held, and SGR coordinates. It never enables hover mode 1003.
Teardown writes all eight resets, in order: 9, 1000, 1002, 1003, 1005, 1006,
1015, and 1016, then flushes. The exported `ENABLE_MOUSE_REPORTING` and
`DISABLE_MOUSE_REPORTING` constants contain those complete byte sequences.
Short writes are completed with `write_all`. Teardown is safe before setup
and repeated teardown emits the full reset sequence again.

These writes use only the supplied drawing stream. The crate never chooses
stdout, stderr, or a controlling terminal on the app's behalf, since a
different stream could be redirected away from the UI.

On Windows, the platform module directly invokes crossterm 0.29's
`EnableMouseCapture` and `DisableMouseCapture` console operations. It neither
writes nor flushes the supplied writer and does not use the commands' ANSI
representations. Crossterm saves the first console input mode process-wide
and restores that value on disable, rather than capturing a fresh snapshot
for each guard. Disabling before crossterm has initialized that snapshot may
return a typed I/O error; it does not panic. The host pseudo-console can
request hover reports independently, as described in
[the terminal measurements](terminals.md).

All stream writes, flushes, and console operations live in
`crates/ratatui-mouse/src/modes/platform.rs`; lifecycle and error handling are
shared across platforms.

## Panic cleanup belongs to the app

The crate does not install or replace a panic hook. Apps using it directly
must install their own hook and perform best-effort teardown through
`disable_mouse_reporting` on the drawing stream. On macOS and Linux a hook
can also write and flush `DISABLE_MOUSE_REPORTING` itself. On Windows it must
use the function so crossterm restores the console input mode. Coordinate
writer ownership with the hook, since an unwinding panic runs the hook before
it drops local guards. A second teardown during unwinding is safe.

## Dependencies and verification

There are no default features. Shared ratatui and crossterm versions keep app
and toolkit types compatible. `unicode-segmentation` finds grapheme boundaries
for app-held-text selection. The optional `vt100` dependency is enabled only
by the `vt100` feature for planned emulator-backed surfaces; the current
crate exposes the pane-request value types but no emulator adapter yet.

The modes integration tests exercise exact bytes, short writes, write and
flush error sources, cleanup after partial enable, and the guard's drop and
explicit-cleanup paths on stream-based platforms. Cross-target lint checks
the Windows console implementation. Live terminal behavior and Windows
console-state restoration are verified by the M1 terminal integration tasks,
including Task 17, rather than by treating a redirected writer on macOS as a
Windows console.

## Frame geometry and content coordinates

Keep a `HitAreas<T>` in app state, clear it before drawing, then push each
interactive content rectangle as it is drawn. Later pushes win, including
modal overlays and backdrops. Empty rectangles are ignored. Lookup returns
borrowed targets without requiring them to be copied or cloned; `area_of`
requires only equality and returns the target's most recent area. Iteration
keeps drawing order from bottom to top.

`clamp_position_into` returns the nearest cell in the full area and signed
vertical overshoot: negative above, positive below. A nonzero edge margin
counts the outer rows as outside while keeping the returned cell inside the
original area. Margins are capped at the visible height. When margins
overlap, the nearer physical edge wins, with ties toward the top. Thus both
rows of a two-row pane with margin one remain active scrolling edges. Empty
areas, including rectangles with no representable cells at coordinate limits,
return `None`.

`ContentPoint` stores a signed content line and a cell column. Screen mapping
adds the screen offset within the content rectangle to both axes of the
surface's origin. Reverse mapping returns `None` outside the viewport; it
handles extreme origins without overflowing. Forward mapping requires a
screen cell inside the area and saturates at the content types' numeric limits
if the translated coordinate cannot be represented. `ContentRange::new`
orders either pair of endpoints in reading order. Containment includes both
ends, and union takes the smallest range containing both ranges.

## Click runs and settings

`ClickCounter<T>` accepts a target, screen position, caller-provided `Instant`,
and `MouseSettings` for each left press. Counts cycle through 1, 2, 3, 1.
The next press continues only when its target matches the previous target,
both cell distances are within the configured tolerance, and its time is no
more than the configured interval after the previous press. The default
boundaries of 500 ms and one cell are inclusive. `reset` starts a fresh run,
for example after a drag. Targets need only `Clone + PartialEq`; the library
never reads a clock.

`MouseSettings::default` supplies the specification's defaults: wheel steps
of three rows, the click limits above, Shift or Alt for selection, 50 ms edge
scrolling with at most ten rows per step and no edge margin, and a 16 ms
redraw interval. Copy on release, keeping copied selections, clearing on key
press or resize, pass-through, wheel arrows, and built-in select-mode keys
are enabled. Those selection and routing settings are consumed by later M1
parts. `WordCharacters` recognizes Unicode letters and digits plus configured
punctuation, defaulting to `_-./~:@+%#?&=` so paths and URLs form words.

The geometry and click integration tests exercise stacking, empty and extreme
rectangles, edge-margin overlap, both origin axes, inclusive ranges, click
thresholds and resets, and custom word punctuation. These primitives perform
no terminal I/O and require only the pure-logic test layers.

## Cell-aware text and borrowed surfaces

The `words` functions work in inclusive cell columns, using ratatui's
`CellWidth` per grapheme, matching its buffer rendering. Selecting either half
of a wide grapheme returns the original whole cluster. Combining sequences and
emoji are copied from the original bytes without normalization. Controls and
standalone zero-width graphemes occupy no selectable cells, matching ratatui's
rendered text traversal. Word classification follows the cluster's first
character, keeping attached combining marks with their base. Default
punctuation makes paths, URLs, and `file.rs:12` single words; quotes, brackets,
and blanks are boundaries.

`SelectableSurface` uses stable content coordinates and mutable text lookups
so adapters can temporarily inspect off-screen content while preserving their
origin. `SurfaceSet<T>` borrows the surface for an app-defined target;
`NoSurfaces` handles apps with no selectable text. The trait's default
`pane_request` returns `None`. `PaneMouseRequest`, `MouseTracking`, and
`MouseEncoding` describe terminal panes' requests; routing and encoders follow
in later M1 parts.

`LineSurface` borrows `AsRef<str>` rows and a `ScrollPosition`. Each row is a
visual content line. `with_wrapped_rows` marks rows joined to their successor;
missing flags mean unwrapped and excess flags are ignored. A final row cannot
wrap beyond the available content. Copy joins soft wraps, inserts real
newlines, trims trailing blanks at logical-line ends, and copies ranges taller
than the viewport without scrolling. `line_at` spans all visual rows of a
logical line, including stored trailing cells. Empty rows have a column-zero
endpoint. An empty surface has an empty `line_bounds`; lookups of nonexistent
characters return the requested singleton cell, while words return `None`.

Scrolling preserves the horizontal column and returns the signed rows actually
moved at either limit. Construction clamps a stale row offset to the last full
viewport. A zero-height viewport uses one row for limits. Borrowing the adapter
ends before the app accesses its scroll state again.

### Reusing the surface contract in app tests

The reusable fixture is
[`tests/common/surface_contract.rs`](../crates/ratatui-mouse/tests/common/surface_contract.rs),
with [`line_surface.rs`](../crates/ratatui-mouse/tests/line_surface.rs) showing
how to call `assert_surface_contract(&mut dyn SelectableSurface)`. Copy that
test-only recipe into an app's test helpers, construct its surface over the
five `ROWS` with `WRAPPED_ROWS`, six columns and two visible rows, and start
scrolled to a positive origin. The checks cover copying the full history,
preserved origin axes, wide cells, logical-line joining, and actual signed
scroll counts at both ends. Logical endpoints may include adapter-specific
padding; their selected text must agree. There is no public test-support
feature or runtime dependency on this fixture.

This slice performs no terminal I/O. Host and Linux pure-logic tests and
cross-target lint apply; real terminal scenarios are exercised by later M1
integration tasks.

# Mouse reporting

`ratatui-mouse` provides reporting setup and teardown for ratatui apps using
crossterm. It is a library building block: the app owns its event loop,
drawing stream, terminal lifecycle, and panic hook. The current crate exposes
the reporting lifecycle; the remaining mouse interaction APIs are planned in
the [mouse specification](specs/ratatui-mouse.md).

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
and toolkit types compatible. `unicode-segmentation` supports the planned
app-held-text selection APIs. The optional `vt100` dependency is enabled only
by the `vt100` feature for planned emulator-backed surfaces; this first
reporting slice exposes no emulator API.

The modes integration tests exercise exact bytes, short writes, write and
flush error sources, cleanup after partial enable, and the guard's drop and
explicit-cleanup paths on stream-based platforms. Cross-target lint checks
the Windows console implementation. Live terminal behavior and Windows
console-state restoration are verified by the M1 terminal integration tasks,
including Task 17, rather than by treating a redirected writer on macOS as a
Windows console.

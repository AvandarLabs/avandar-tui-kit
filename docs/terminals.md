# Target terminals

The toolkit has two first-class target terminals: **iTerm2** and **xterm.js**.
xterm.js is the emulator inside Orca and VS Code's integrated terminal, so
"works in xterm.js" means "works in Orca", which is the top priority. Both
terminals are equally first-class: neither is a fallback for the other.

The backend (crossterm) is a separate question and decides nothing here. Every
backend writes the same escape sequences; what an app can do depends on which
of those sequences the terminal implements.

## What first-class means

- **Every feature works in both, or degrades in a documented way.** When a
  terminal lacks a capability, the toolkit falls back on its own (for
  example, a local clipboard tool when OSC 52 is not available) and the crate's
  docs say what the user sees.
- **No crate is done until it is tested in both.** A crate that writes to or
  reads from the terminal carries tests for each terminal, at the layers below.
- **A regression in either one blocks a release.**
- **Defaults never depend on an optional capability.** Key bindings work
  without the kitty keyboard protocol, and copying works without OSC 52. Those
  capabilities add to the defaults; they are never required by them.

Other terminals (Terminal.app, kitty, WezTerm, Ghostty, Alacritty, and tmux in
front of any of them) are best effort: nothing is done to break them, and
nothing is tested in them.

## What each terminal is known to do

"Verify" means the toolkit must confirm the behavior with the capability
probe before relying on it.

| Capability | iTerm2 | xterm.js |
| --- | --- | --- |
| Mouse modes 1000, 1002, 1006 | Yes | Yes |
| Resetting a mouse mode mid-session | **Turns reporting off entirely**: 1000, 1002 and 1003 share one slot | Verify |
| Hover reporting (mode 1003) | Breaks Cmd-hover and Cmd-click on links, so it is never turned on | Never turned on, for parity |
| Native selection while the app has the mouse | Option-drag | Depends on the embedding app; verify in Orca |
| Option or Alt key | Sends glyphs such as `¡` or `£` unless set to send Escape, so chord decoders accept both | Depends on the embedding app's "Option is Meta" setting; verify in Orca |
| Kitty keyboard protocol | Verify | Recent, and off by default in VS Code; verify in Orca |
| Clipboard over OSC 52 | Only with "Applications in terminal may access clipboard" on | Only when the embedding app loads xterm.js's clipboard add-on; one add-on version garbled non-ASCII text. Verify in Orca. |
| Bracketed paste (mode 2004) | Yes | Yes |
| Synchronized output (mode 2026) | Verify | Listed in xterm.js 6.0; verify the version Orca ships |
| Links | Cmd-click on detected URLs | The embedding app's link handler |

## How both are tested

Testing runs at four layers. The first three are automated.

1. **Escape-sequence tests, in every crate.** What a part writes and how it
   reads input are pure functions, so tests assert the exact bytes: that the
   mouse setup never contains mode 1003, that no mouse mode is reset before
   teardown, that a paste is bracketed. These run with `cargo test` and cover
   both terminals' rules at once.
2. **xterm.js, headless.** `@xterm/headless` is xterm.js without a browser.
   A small Node harness runs the toolkit's example programs in a
   pseudo-terminal wired to a headless xterm.js, sends input the way xterm.js
   encodes it (keys, SGR mouse reports, bracketed paste), and asserts the
   screen contents and terminal modes that result. It is pinned to the xterm.js
   version Orca ships, and runs in CI.
3. **iTerm2, scripted.** iTerm2's Python API opens a session, runs the same
   example programs, sends input and reads the screen back. It runs on macOS
   with iTerm2 installed, as its own `just` recipe.
4. **A manual checklist per release** for what no script can see: Cmd-hover and
   Cmd-click on links, native selection with the app holding the mouse, the
   clipboard permission prompt, and how the wheel feels. Each item is checked in
   iTerm2 and in Orca.

## The capability probe

A small example program in this repository, run inside a terminal, reports
which of the "verify" capabilities above that terminal actually has: the
mouse modes it accepts, whether resetting one switches reporting off, the kitty
keyboard protocol, synchronized output, and how Option and Alt arrive. It is
the first thing built, because its results fill in the table above for Orca
and iTerm2, and it stays as a diagnostic users can run.

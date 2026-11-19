# Target terminals and platforms

The toolkit names what it is built and tested for. Everything on this page is
first-class: none of it is a fallback for anything else.

| | First-class |
| --- | --- |
| Terminals | **iTerm2** and **xterm.js** (the emulator inside Orca and VS Code's integrated terminal) |
| Multiplexer | **tmux**, inside either terminal |
| Platforms | **macOS**, **Linux** and **Windows** |

The backend (crossterm) is a separate question and decides nothing here. Every
backend writes the same escape sequences; what an app can do depends on which
of those sequences the terminal in front of it implements.

## The configurations

An app talks to whatever is directly in front of it. Without tmux, that is the
terminal. With tmux, it is tmux: tmux is a terminal emulator of its own, which
then redraws onto the real terminal. So tmux inside iTerm2 and tmux inside
xterm.js behave mostly alike for the app, and differ only in the features
tmux hands through to the outer terminal (the clipboard is the main one). Both
are tested anyway.

| Configuration | macOS | Linux | Windows |
| --- | --- | --- | --- |
| iTerm2 | Yes | iTerm2 is macOS only | iTerm2 is macOS only |
| xterm.js | Yes | Yes | Yes |
| tmux in iTerm2 | Yes | | |
| tmux in xterm.js | Yes | Yes | tmux does not run natively on Windows. Under WSL the app is a Linux build, which the Linux column covers. |

xterm.js tests are pinned to the version Orca ships. Orca 1.4.258 bundles
xterm.js 6.0.0.

## What first-class means

- **Every feature works in every configuration, or degrades in a documented
  way.** When a capability is missing, the toolkit falls back on its own (a
  local clipboard tool when OSC 52 is not available, keyboard scrolling when
  tmux is not forwarding the mouse) and the crate's docs say what the user
  sees.
- **No crate is done until it is tested in all of them.** A crate that writes
  to or reads from the terminal carries tests for each configuration, at the
  layers below, on all three platforms.
- **A regression in any of them blocks a release.**
- **Defaults never depend on an optional capability.** Key bindings work
  without the kitty keyboard protocol, copying works without OSC 52, and every
  action is reachable without the mouse. Those capabilities add to the
  defaults; they are never required by them.
- **No crate assumes Unix.** A part that needs something Unix-only (`/dev/tty`,
  process groups, a login shell) has a Windows path with the same behavior,
  behind the same API.

Other terminals (Terminal.app, Windows Terminal, kitty, WezTerm, Ghostty,
Alacritty) are best effort: nothing is done to break them, and nothing is
tested in them.

## What each one is known to do

"Verify" means the toolkit must confirm the behavior with the capability
probe before relying on it.

### Terminals

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
| Synchronized output (mode 2026) | Verify | Listed in xterm.js 6.0; verify in Orca |
| Links | Cmd-click on detected URLs | The embedding app's link handler |

### tmux

tmux is tested with two configurations, because its defaults turn most of
this off: the **stock** configuration, and a **full** one that sets `mouse on`,
`set-clipboard on`, `allow-passthrough on`, `extended-keys on` and
`focus-events on`. The toolkit works in both. In the stock configuration it
degrades as the table says.

| Capability | Stock tmux | Full configuration |
| --- | --- | --- |
| Mouse | The app receives no mouse events at all, so the keyboard must reach everything | Forwarded to the app when it asks for the mouse |
| Clipboard over OSC 52 | Verify | tmux takes the sequence and sets the outer terminal's clipboard. A second form wrapped in a passthrough sequence covers setups that allow passthrough instead. The toolkit sends both. |
| Kitty keyboard protocol | Not offered to the app; verify what `extended-keys` reports instead | Verify |
| Bracketed paste | Yes | Yes |
| Synchronized output | Verify | Verify |
| `TERM` | `tmux-256color` or `screen-256color`, never the outer terminal's | Same |
| iTerm2's own tmux integration (`tmux -CC`) | Verify | Verify |

The minimum supported tmux is 3.3, the first release with
`allow-passthrough`.

### Windows

On Windows, a console program does not write straight to the terminal: the
operating system's pseudo-console (ConPTY) sits in between, parses the
program's output and re-emits it. That makes Windows a platform to test in its
own right, not only a build target.

| Area | What differs on Windows |
| --- | --- |
| Escape sequences | ConPTY may rewrite or drop sequences it does not know. Verify each "verify" row above on Windows separately. |
| Key events | crossterm reports both press and release. Every key handler acts on presses only. |
| The controlling terminal | There is no `/dev/tty`. Parts that draw outside stdout use the console handles instead. |
| Embedded terminals | Children run under ConPTY, not a Unix pseudo-terminal. There is no login shell to resolve aliases through. |
| Stopping a child and its descendants | Unix process groups do not exist. A job object does the same work. |
| Clipboard | `clip.exe` instead of `pbcopy`, `wl-copy` or `xclip` |
| Opening files and links | `start` instead of `open` or `xdg-open` |

## How it is tested

Testing runs at five layers. The first three run in CI on every pull request.

1. **Escape-sequence tests, in every crate, on all three platforms.** What a
   part writes and how it reads input are pure functions, so tests assert the
   exact bytes: that the mouse setup never contains mode 1003, that no mouse
   mode is reset before teardown, that a paste is bracketed. These run with
   `cargo test` on macOS, Linux and Windows.
2. **xterm.js, headless, on all three platforms.** `@xterm/headless` is
   xterm.js without a browser. A small Node harness runs the toolkit's example
   programs in a pseudo-terminal wired to a headless xterm.js, sends input the
   way xterm.js encodes it (keys, SGR mouse reports, bracketed paste), and
   asserts the screen contents and terminal modes that result. On Windows the
   pseudo-terminal is ConPTY, so this layer is also the Windows terminal test.
3. **tmux in xterm.js, headless, on macOS and Linux.** The same harness runs
   the example programs inside a private tmux server, once with the stock
   configuration and once with the full one.
4. **iTerm2, scripted, on a Mac with iTerm2.** iTerm2's Python API opens a
   session, runs the same example programs (directly and inside tmux), sends
   input and reads the screen back. iTerm2 is a desktop app, so this layer runs
   locally through its own `just` recipe and not in CI. It must pass before a
   release.
5. **A manual checklist per release** for what no script can see: Cmd-hover and
   Cmd-click on links, native selection while the app holds the mouse, the
   clipboard permission prompt, and how the wheel feels. Each item is checked
   in iTerm2 and in Orca, with and without tmux, and in Orca on Windows.

The Node harness and the iTerm2 script are test tooling only. No published
crate depends on either.

## The capability probe

A small example program in this repository, run inside a terminal, reports
which of the "verify" capabilities above that configuration actually has: the
mouse modes it accepts, whether resetting one switches reporting off, the kitty
keyboard protocol, synchronized output, the clipboard routes, and how Option
and Alt arrive. It is the first thing built, because its results replace every
"verify" on this page with a fact, and it stays as a diagnostic users can run.

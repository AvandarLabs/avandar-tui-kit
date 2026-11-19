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
tmux hands through to the outer terminal (colors and the clipboard). Both are
tested anyway.

| Configuration | macOS | Linux | Windows |
| --- | --- | --- | --- |
| iTerm2 | Yes | iTerm2 is macOS only | iTerm2 is macOS only |
| xterm.js | Yes | Yes | Yes |
| tmux in iTerm2 | Yes | | |
| tmux in xterm.js | Yes | Yes | tmux does not run natively on Windows. Under WSL the app is a Linux build, which the Linux column covers. |

On Windows, Windows Terminal is best effort, like every terminal not named
here.

## What first-class means

- **Every feature works in every configuration, or degrades in a documented
  way.** When a capability is missing, the toolkit falls back on its own (a
  local clipboard tool when OSC 52 is not available, keyboard scrolling when
  tmux is not forwarding the mouse) and the crate's docs say what the user
  sees.
- **No crate is done until it is tested in all of them.** A crate that writes
  to or reads from the terminal carries tests for each configuration, on all
  three platforms. [`testing.md`](testing.md) says how.
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

## What each one does

Measured on 2026-11-18 by running a probe program and injecting real mouse
and key events, in these versions:

- **iTerm2 3.7.7** on macOS 15.7, with its default settings.
- **xterm.js 6.0.0**, the version Orca 1.4.258 ships: in a live Orca terminal,
  and on its own in a headless browser.
- **tmux 3.8**, inside each of the two, with tmux's stock configuration and
  with a full one (below).

"Verify" marks what has not been measured yet. Each entry becomes a test when
the part that depends on it is built, so this table cannot drift from the
code.

### Rules that hold everywhere

These held in iTerm2, xterm.js and tmux alike, so the toolkit treats them as
rules, not as quirks of one terminal.

- **Mouse reporting is one slot, not three switches.** Modes 1000, 1002 and
  1003 are one setting with three values: turning one on replaces the others,
  and turning any of them off, even one that was never on, **switches mouse
  reporting off entirely**. So the toolkit turns on exactly the modes it
  wants, in order, and never resets a mouse mode until teardown.
- **crossterm's `EnableMouseCapture` is the wrong sequence.** It ends by
  turning on mode 1003, so hover reporting becomes the active setting and
  every pointer move floods the app. The toolkit writes its own sequence
  (1000, 1002, 1006) and never uses that one. Windows is the exception in
  form only: there the same crossterm command writes no sequence at all and
  is how mouse input is turned on (see [Windows](#windows)).
- **Asking whether mode 1000 is on is not asking whether the mouse is on.**
  After the toolkit's sequence, the terminal reports 1002 as set and 1000 as
  reset. A check has to look at all of them.
- **The kitty keyboard protocol cannot be assumed.** Both terminals have it,
  tmux does not offer it. Shift+Enter and Ctrl+Enter arrive as plain Enter
  without it.
- **The background color can be asked for.** Both terminals answer the color
  query, and tmux passes it through, so a theme can tell light from dark. It
  has to: a fresh iTerm2 has a light background. The answer can be late or
  missing (see the tmux table), so the theme waits a short, bounded time and
  otherwise uses a palette that reads on both. The theme section of
  [`specs/architecture.md`](specs/architecture.md) has the rules.

### Terminals

| Capability | iTerm2 3.7.7 | xterm.js 6.0.0 |
| --- | --- | --- |
| Identifies itself | `iTerm2 3.7.7` | `xterm.js(6.0.0)`; Orca also sets `TERM_PROGRAM=Orca` |
| Mouse modes 1000, 1002, 1006 | Yes | Yes |
| Click, drag, wheel | Reported at the exact cell | Reported at the exact cell |
| Wheel direction | Verify: an injected wheel-down arrived as wheel-up, so the direction depends on a system setting or on how the event was injected | Wheel-down arrives as wheel-down. Verify against Orca's own setting. |
| Shift+click, Option+click | Reported to the app with the modifier | macOS: reported to the app with the modifier. Linux and Windows: xterm.js keeps Shift for its own selection, so the app never sees it. Verify there. |
| Right click, Ctrl+click | Not reported with the default settings | Reported |
| Hover without a button | Not reported, because mode 1003 stays off | Not reported, because mode 1003 stays off |
| Native selection while the app has the mouse | Verify which modifier | macOS: none by default: Shift+drag selected nothing. xterm.js has an Option setting the embedding app may turn on. Linux and Windows: Shift. |
| Option or Alt with a letter | Verify. The glyph decoders accept both forms. | Sends nothing as Escape+letter by default: the keyboard layout's glyph arrives instead, unless the embedding app sets "Option is Meta" |
| Alt+Enter | Verify | Escape then Enter |
| Shift+Enter, Ctrl+Enter | Verify | Same bytes as Enter |
| Kitty keyboard protocol | Yes: answers the query, flags push and pop | Yes in Orca: answers the query, flags push and pop |
| Bracketed paste | Yes | Yes. A paste arrives wrapped, with newlines as carriage returns. |
| Focus events | Yes | Yes |
| Synchronized output (mode 2026) | Yes | Yes |
| Background color query | Answered. Light on a fresh install. | Answered. Dark in Orca. |
| Clipboard over OSC 52 | Advertised. Writing needs "Applications in terminal may access clipboard". Verify. | Not advertised. Depends on the embedding app's clipboard add-on. Verify in Orca. |
| Cell size in pixels | Answered | Not answered |
| Links | Cmd-click on detected URLs | The embedding app's link handler |
| Its own alerts | A modal alert stalls the session until it is dismissed | Not applicable |

### tmux

tmux is tested with two configurations, because its defaults turn most of
this off: the **stock** configuration, and a **full** one that sets `mouse on`,
`set-clipboard on`, `allow-passthrough on`, `extended-keys on` and
`focus-events on`. The toolkit works in both. In the stock configuration it
degrades as the table says.

| Capability | Stock tmux 3.8 | Full configuration |
| --- | --- | --- |
| Identifies itself | `tmux 3.8`, `TERM=tmux-256color` | Same |
| Mouse | The app receives no mouse events, so the keyboard must reach everything. Verify with injected events. | Forwarded to the app when it asks for the mouse. Verify with injected events. |
| Mouse modes | Accepted, with the one-slot rule above | Same |
| Kitty keyboard protocol | Not offered: the query goes unanswered | Not offered, even with `extended-keys on` |
| Bracketed paste, focus events | Accepted | Accepted |
| Synchronized output | Accepted | Accepted |
| Background color query | Passed to the outer terminal and answered inside xterm.js. Inside iTerm2 no answer came before tmux's own replies: verify whether it is late or missing. Either way the app waits with a timeout and never assumes an order. | Same |
| Clipboard over OSC 52 | Verify | tmux takes the sequence and sets the outer terminal's clipboard. A second form wrapped in a passthrough sequence covers setups that allow passthrough instead. The toolkit sends both. Verify. |
| iTerm2's own tmux integration (`tmux -CC`) | Verify | Verify |

The minimum supported tmux is 3.3, the first release with
`allow-passthrough`.

### How wide a character is

The number of cells a string takes is decided by the terminal, and the three
do not agree. Measured by printing each string and asking where the cursor
ended up:

| Text | iTerm2 3.7.7 | xterm.js 6.0.0 in Orca | tmux 3.8 |
| --- | --- | --- | --- |
| ASCII, accented letters, combining accents | 1 | 1 | 1 |
| CJK, Hangul, fullwidth forms | 2 | 2 | 2 |
| Box drawing, block eighths, braille, private-use icons | 1 | 1 | 1 |
| A plain emoji, a family joined with ZWJ, a flag | 2 | 2 | 2 |
| An emoji with a variation selector (a red heart) | 1 | 1 | 2 |
| An emoji with a skin tone | 2 | 4 | 2 |
| A flag built with ZWJ (the rainbow flag) | 1 | 1 | 2 |
| A zero-width space | 1 | 1 | 0 |

Everything the toolkit draws itself (borders, bars, markers, sparklines) is in
the rows where all three agree. Emoji sequences are where they split, so the
toolkit never uses one in its own drawing, and measures text an app hands it
with one width function for the whole workspace. Before it draws such text it
rewrites the rows that split into forms all three agree on: the variation
selector and the skin tone are dropped, a ZWJ sequence that starts with a
one-cell character is cut to that character, and a zero-width cluster is
removed. The text width section of
[`specs/architecture.md`](specs/architecture.md) has the policy.

Orca's terminal counts a plain emoji as two cells, which xterm.js does only
with its Unicode 11 add-on loaded, so the test harness loads that add-on too.

### Windows

On Windows, a console program does not write straight to the terminal: the
operating system's pseudo-console (ConPTY) sits in between, parses the
program's output and re-emits it. That makes Windows a platform to test in its
own right, not only a build target.

| Area | What differs on Windows |
| --- | --- |
| Escape sequences | ConPTY may rewrite or drop sequences it does not know. Every row above is measured again on Windows. |
| Key events | crossterm reports both press and release. Every key handler acts on presses only. |
| Input | crossterm reads the console's input records, not bytes. Anything a terminal answers (the background color) has to arrive as key events, and the kitty keyboard protocol and paste events are not available. |
| Mouse reporting | Turned on in the console's input mode through crossterm, not with the mode sequence. The pseudo-console then asks the terminal for mouse reports itself. Verify which tracking mode it asks for. |
| The controlling terminal | There is no `/dev/tty`. Parts that draw outside stdout use the console handles instead. |
| Embedded terminals | Children run under ConPTY, not a Unix pseudo-terminal. There is no login shell to resolve aliases through. |
| Stopping a child and its descendants | Unix process groups do not exist. A job object does the same work. |
| Clipboard | `clip.exe` instead of `pbcopy`, `wl-copy` or `xclip` |
| Opening files and links | `start` instead of `open` or `xdg-open` |

## The capability probe

A small example program in this repository, run inside a terminal, reports
what that configuration does: the mouse modes it accepts and how they
interact, the kitty keyboard protocol, synchronized output, the colors, the
clipboard routes, how wide each kind of character is, and how Option and Alt
arrive. It is the first thing built. Its output is what the tables above are
checked against, and it stays as a diagnostic anyone can run when an app
misbehaves in their terminal.

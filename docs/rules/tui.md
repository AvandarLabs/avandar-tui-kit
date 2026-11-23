# Terminal UI rules

What every crate in this toolkit promises the apps that use it. The apps the
toolkit is extracted from run their event loops in different ways (threads
with a polling loop, or an async runtime drawing on stderr), so these rules
are about what a crate does when it is called, never about how an app is
shaped.

## Parts, not a framework

A crate hands the app types and functions to call. It never owns the event
loop, never spawns a thread the app did not ask for, never holds the app's
state, and never decides on the app's behalf what an event means for the
rest of the app. When a part needs time to pass (an auto-scroll timer, a
settle delay before submitting a prompt), it says so through a deadline or a
tick method the app calls, not a thread of its own.

The test of a good boundary: the part reports what happened (a click on a
target, a selection copied, bytes to forward), and the app decides what that
means. A crate that takes `&mut App` has the boundary in the wrong place.

## Decisions are pure functions

What a key means, which row a click landed on, which rows a filter keeps,
where a modal sits: each is a function that takes values and returns values,
with tests beside it. The code that touches the terminal, a child process, or
the clock stays thin enough to read in one screen. This is what lets an app
test its whole UI by calling functions, without opening a screen.

## Rendering is a pure function of the state

Widgets read their state and draw it. They never mutate anything, never ask
the terminal a question, and never decide what should happen next. Where a
widget needs to remember something between frames (a scroll offset, the
areas it drew for hit-testing), that lives in an explicit state type the app
owns and passes in, in the ratatui `StatefulWidget` style.

## Every action is reachable from the command palette

Apps built with the palette crate follow the rule quickstarter's template
established: the palette lists every command, and a keyboard shortcut is a
faster way to reach a command, never the only way. The shortcuts table and the
palette are one source of truth, and the toolkit provides the parity check an
app runs in its own tests to prove it.

## The terminal is borrowed, and it is always given back

Raw mode, the alternate screen, mouse reporting and keyboard enhancement
flags belong to the user's shell. Any part that turns one on also gives back
a way to turn it off that runs on every exit path: a guard that restores on
drop, which covers an early `?`, and a panic hook that restores before the
panic message prints, which covers a crash. A part never resets a terminal
mode in the middle of a session, because some terminals (iTerm2 among them)
keep several mouse modes in one slot and switch reporting off entirely.

## Color is semantic

Parts never name a color. They ask for a role (heading, accent, value,
selected, muted, faint, success, warning, error, info, prompt) and the app's
theme maps roles to colors. `NO_COLOR` turns color off, and color never
carries meaning alone.

A theme never assumes a dark terminal. The app asks the terminal for its
background color at startup, which every first-class terminal answers, and
the theme picks a dark or a light palette from the answer. When no answer
comes within a short, bounded wait, the theme uses a third palette that reads
on both. A fresh iTerm2 has a light background, so a guess of dark is wrong
on a default install. See the theme section of
[`../specs/architecture.md`](../specs/architecture.md).

## Every first-class target, every time

iTerm2 and xterm.js are first-class terminals, tmux is first-class inside
both, and macOS, Linux and Windows are first-class platforms. Every part works
in all of them, or degrades in a way its docs describe. A default (a key
binding, how copying works) never depends on a capability one of them lacks:
the kitty keyboard protocol, OSC 52 and the mouse add to the defaults, they
never replace them. Stock tmux forwards app-requested pane mouse input even
when its own `mouse` option is off, but modifier motion and rapid clicks have
version-specific limits. A part that touches
terminal I/O is not done until it has escape-sequence tests and has run in
each configuration. See `docs/terminals.md` for what each one is known to do
and how they are tested.

## Keyboard first, mouse as well

Everything a part does with the mouse is also reachable from the keyboard.
Mouse support adds speed, never a capability the keyboard lacks. On macOS and
Linux parts leave hover reporting (mode 1003) off, preserving the host's hover
features. Windows ConPTY enables outer any-motion itself; handlers ignore bare
moves, and docs disclose the host-hover limit. Shift and Alt select in-app only
where delivered. Native Shift selection in xterm.js on Linux/Windows and
Option selection in iTerm2 belong to the host. Every app with a selectable pane
offers keyboard select mode, including where tmux 3.8 filters Alt-drag motion.

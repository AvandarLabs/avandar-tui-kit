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

Parts never name a color. They ask for a role (heading, accent, value, muted,
success, warning, error, info, prompt) and the app's theme maps roles to
colors. `NO_COLOR` turns color off. The default palette assumes a dark
terminal, because terminals do not reliably say which they are.

## Keyboard first, mouse as well

Everything a part does with the mouse is also reachable from the keyboard.
Mouse support adds speed, never a capability the keyboard lacks. Parts must
not break the host terminal's own mouse features: hover reporting (mode 1003)
stays off so the terminal's Cmd-hover and Cmd-click keep working.

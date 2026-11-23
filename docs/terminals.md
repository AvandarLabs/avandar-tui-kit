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

Measured by running `capability_probe` and injecting real mouse and key events
across all thirteen configurations, in these versions:

- **iTerm2 3.7.7** on macOS 15 (`tuikit-macos` Apple Silicon Tart VM), driven
  over VNC with default settings and targeted preference toggles.
- **xterm.js 6.0.0** with `@xterm/addon-unicode11` `0.9.0`: in a live **Orca
  1.4.258** terminal on macOS, standalone in headless Chromium on macOS and
  Debian 12 Linux (`rustc 1.88.0`), and on Windows Server 2025 (`windows-2025`,
  build `10.0.26100`) through **ConPTY**.
- **tmux 3.8** on macOS (inside xterm.js, Orca, and iTerm2, plus `tmux -CC` in
  iTerm2) and **tmux 3.3a** in the Linux floor container, each with tmux's
  stock configuration (an empty file) and with a full configuration (below).

The capability measurements below are backed by `capability_probe` and checked
against `rig/expected/*.json`. M1's separate `mouse_lab` asserts the library's
outcomes against those facts: exact text/bytes, modifier routing, ignored hover,
and cleanup. Its local Unix and VM matrices pass; its new Windows runtime gate
remains pending. See [testing](testing.md) for the distinction and release gates.

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
- **The background color can be asked for.** Both Unix terminals answer the
  color query, and `tmux 3.8` passes it through, so a theme can tell light
  from dark. It has to: a fresh iTerm2 has a light background (`#fafafa`). The
  answer can be late (`tmux -CC` during client attach) or missing (`tmux 3.3a`
  and Windows ConPTY), so the theme waits a short, bounded time and otherwise
  uses a palette that reads on both. The theme section of
  [`specs/architecture.md`](specs/architecture.md) has the rules.

### Terminals

| Capability | iTerm2 3.7.7 | xterm.js 6.0.0 (macOS and Linux) | Windows (xterm.js 6.0.0 via ConPTY) |
| --- | --- | --- | --- |
| Identifies itself (`XTVERSION` and env) | `iTerm2 3.7.7`, `TERM_PROGRAM=iTerm.app`, `TERM=xterm-256color` | Orca answers `xterm.js(6.0.0)` and sets `TERM_PROGRAM=Orca`; standalone `xterm.js` leaves `XTVERSION` unanswered (`TERM=xterm-256color`) | `XTVERSION` unanswered, `TERM=xterm-256color` |
| Mouse modes 1000, 1002, 1006 and `DECRQM` | Yes (`1002` and `1006` `set`, `1000` and `1003` `reset`; teardown resets all four) | Yes (same `DECRQM` reports in Orca and standalone `xterm.js`; teardown resets all four) | Enabled via console input mode through crossterm; ConPTY drives outer `DECSET 1003` + `1006` (`DECRQM` does not cross ConPTY) and resets outer tracking to `"none"` on exit or panic |
| Click, drag, wheel | Reported at the exact cell | Reported at the exact cell | Reported at the exact cell |
| Wheel direction | macOS WindowServer applies `com.apple.swipescrolldirection` before iTerm2 encodes SGR `64`/`65`: with natural scrolling off, hardware wheel-up arrives as `scroll_up` and wheel-down as `scroll_down`; with macOS's default natural scrolling on, wheel-up arrives as `scroll_down` and wheel-down as `scroll_up` | DOM `deltaY > 0` arrives as `scroll_down` and `deltaY < 0` as `scroll_up`. Against Orca's own scroll setting, an agent cannot measure wheel direction because Orca's CLI cannot move a pointer; the toolkit routes `ScrollUp` and `ScrollDown` as received and exposes scroll configuration to the app | DOM `deltaY > 0` arrives as `scroll_down` and `deltaY < 0` as `scroll_up` |
| `Shift+click`, `Option/Alt+click`, `Option/Alt+drag` | `Shift+click` reported (`shift=true`); `Option+click` and `Option+drag` intercepted by iTerm2 (0 app events); direct `Cmd+click` and `Cmd+drag` reported (`alt=true`); tmux 3.8 forwards their Alt press/release but filters drag motion | macOS: `Shift+click`, `Alt+click`, and `Alt+drag` all reported with modifiers. Linux: `Shift+click` kept by `xterm.js` for selection (0 app events); `Alt+click` and `Alt+drag` reported with `alt=true` | `Shift+click` press/release kept by `xterm.js` for selection; `Alt+click` and `Alt+drag` reported across ConPTY with `alt=true` |
| Right click, `Ctrl+click` | Not reported with default settings (opens context menu) | Reported on macOS and Linux | Reported |
| Hover without a button | Not reported (mode `1003` stays off) | Not reported (mode `1003` stays off) | Reported (`Moved` events arrive), because ConPTY enables outer mode `1003` (`mouseTrackingMode: "any"`). The mouse controller ignores bare `Moved` events, including for pane any-motion requests |
| Native selection while the app has the mouse | `Option+drag` (contiguous) and `Option+Cmd+drag` (rectangular) bypass mouse capture (0 app events) and copy to `NSPasteboard`; `Shift+drag` and direct `Cmd+drag` select in the app; tmux 3.8 filters Cmd/Alt drag motion, so select mode supplies the complete keyboard equivalent | macOS: none by default (`Shift+drag` selects nothing unless the embedding app enables Option selection). Linux: `Shift+drag` | `Shift+drag` in `xterm.js` |
| Option or Alt with a letter | Default `Option Key Sends = Normal` sends the macOS layout character (`å` for `Option+a`, even with Kitty `>1u` pushed); `Esc+` sends `Esc` + letter (`alt=true`) | macOS sends nothing (`macOptionIsMeta` off by default); Linux sends `alt=true` | Sends `alt=true` |
| `Alt+Enter`, `Shift+Enter`, `Ctrl+Enter` | Without Kitty flags and `Option = Normal`, all three arrive as plain `Enter` (`Esc+` disambiguates `Option+Enter` as `alt=true`). With Kitty `>1u` pushed, all three disambiguate (`alt=true`, `shift=true`, `ctrl=true`), even with `Option = Normal` | Without Kitty flags, `Alt+Enter` arrives as `Enter` with `alt=true`, and `Shift+Enter`/`Ctrl+Enter` arrive as plain `Enter` (Orca supports Kitty `?u`/`>1u`) | `Alt+Enter` arrives as `Enter` with `alt=true`; `Shift+Enter` and `Ctrl+Enter` arrive as plain `Enter` (with or without `--push-kitty`) |
| Kitty keyboard protocol | Yes (`?u`, `>1u`, `<u`) | Yes in Orca (`?u`, `>1u`, `<u`); not answered in standalone browser `xterm.js` | Not answered across ConPTY |
| Bracketed paste | Yes (`Event::Paste` with `\n` normalized to `\r`) | Yes (`Event::Paste` with `\n` normalized to `\r`) | `bracketedPasteMode` stays off; paste arrives as `Press`+`Release` `Key` events in a single console read burst (`burst_press_count == 10` for a 10-char paste) |
| Focus events (`?1004`) | Yes (`FocusLost` and `FocusGained`) | Yes (`FocusLost` and `FocusGained`) | Yes (`HPCON` owns outer `?1004h` until `ClosePseudoConsole` and delivers `FocusLost` and `FocusGained`) |
| Synchronized output (`?2026`) | Yes (`DECRQM` reports `set` and `reset`) | Yes (`DECRQM` reports `set` and `reset`) | `DECRQM` unanswered across ConPTY (harmless no-op) |
| Background color query (`OSC 11 ; ?`) | Answered (`#fafafa`, light on a fresh install) | Answered (`rgb(18, 39, 56)`, dark in Orca and the harness) | Not answered across ConPTY (times out after 2 s, selecting the no-answer palette) |
| Clipboard over `OSC 52` | Blocked by default (`AllowClipboardAccess = false`); updates macOS clipboard when `"Applications in terminal may access clipboard"` (`AllowClipboardAccess = true`) is enabled | Reaches `xterm.js` on macOS and Linux. In live Orca `1.4.258`, `OSC 52` leaves the host clipboard unchanged because Orca does not enable a host-clipboard handler by default | Reaches `xterm.js` across ConPTY and updates the outer clipboard |
| Cell size in pixels (`CSI 16 t`) | Answered (`7x17` logical points) | Not answered | Not answered |
| Links | `Cmd+click` on detected URLs | The embedding app's link handler | The embedding app's link handler |
| Its own alerts | A modal alert stalls the session until it is dismissed | Not applicable | Not applicable |

### tmux

tmux is tested with two configurations: the **stock** configuration (an empty
file), and a **full** one that sets `mouse on`, `set-clipboard on`,
`allow-passthrough on`, `extended-keys on` and `focus-events on`. Both run
against **tmux 3.8** on macOS (inside xterm.js, Orca, and iTerm2, plus
`tmux -CC` in iTerm2) and **tmux 3.3** (`3.3a`) in the Linux floor container.
The toolkit works in every combination and keeps every action
keyboard-reachable regardless.

| Capability | Stock tmux (`tmux 3.8` and `tmux 3.3a`) | Full configuration (`tmux 3.8` and `tmux 3.3a`) |
| --- | --- | --- |
| Identifies itself | `tmux 3.8` on macOS, `tmux 3.3a` in the Linux floor container (`TERM=tmux-256color`, `TERM_PROGRAM=tmux`) | Same |
| Mouse | `tmux 3.3a` defaults `mouse` to `off` while `tmux 3.8` defaults `mouse` to `on`. In both `3.3a` and `3.8` (in both `xterm.js` and iTerm2), even with the empty `stock` file, once the pane app writes Rule M1 (`?1000h?1002h?1006h`), `tmux` enables outer mouse tracking and forwards pane clicks, drags, wheel, and modified clicks at the exact cells (`mouse off` disables only `tmux`'s own status-bar, pane-border, and copy-mode mouse handling). On `Alt+drag` inside `xterm.js`, `tmux 3.3a` forwards `Down`, `Drag`, and `Up` (`alt=true`), whereas `tmux 3.8` (`stock` and `full`) forwards `Down` and `Up` (`alt=true`) without `Drag` motion events. The same tmux 3.8 filter applies to iTerm2 Cmd-drag, represented as Alt: the app sees a zero-length press and no copied drag. In `tmux 3.3a`, rapid `right_click` and `ctrl_click` deliver `up` while `tmux 3.3a`'s click timer absorbs `down` (`3.8` delivers `down` and `up`). Every action stays keyboard-reachable regardless | Same pane forwarding plus `tmux`'s own status-bar and pane-border mouse handling (`mouse on`) |
| Mouse modes and `DECRQM` | One-slot rule holds in both; clean or panic teardown resets outer mouse tracking. `tmux 3.8` answers `DECRQM` (`?Pm$p`), with present state `0` for unsupported legacy formats 9, 1015, and 1016. State `0` is neither an absent reply nor a reset observation; `tmux 3.3a` leaves `DECRQM` unanswered | Same |
| Kitty keyboard protocol | Not offered (`?u` goes unanswered in `3.8` and `3.3a`). In iTerm2 (`Option = Normal`), `Option+Enter`, `Shift+Enter`, and `Ctrl+Enter` all arrive as plain `Enter` | Not offered (`?u` unanswered). With `extended-keys on`, `Option+Enter` in iTerm2 arrives with `alt=true`, while `Shift+Enter` and `Ctrl+Enter` remain plain `Enter` without `modifyOtherKeys` |
| Bracketed paste | Accepted and delivered as `Event::Paste` in `3.8` and `3.3a` | Same |
| Focus events (`?1004`) | `focus-events` defaults to `off` in `stock` (`3.8` and `3.3a`), so `FocusLost` and `FocusGained` are dropped in `stock` | Delivered (`FocusLost` and `FocusGained`) in both `3.8` and `3.3a` (`focus-events on`) |
| Synchronized output (`?2026`) | Accepted (`DECRQM` reports `set` and `reset` in `3.8`, unanswered in `3.3a`) | Same |
| Background color query (`OSC 11 ; ?`) | `tmux 3.8`: passed to the outer terminal and answered (~2-5 ms) inside both `xterm.js` (dark) and iTerm2 (`#fafafa`, light). `tmux 3.3a`: not forwarded (no answer within 2 s; the theme falls back to the no-answer palette) | Same (`tmux 3.8` answers promptly in `xterm.js` and iTerm2; `tmux 3.3a` does not forward) |
| Clipboard over `OSC 52` | Neither plain `OSC 52` nor `DCS tmux;` passthrough updates `tmux`'s buffer or the outer terminal's clipboard (`set-clipboard external`, `allow-passthrough off`), identically in `3.8` and `3.3a` | Identical in `3.8` and `3.3a` (`set-clipboard on`, `allow-passthrough on`): plain `OSC 52` sets `tmux`'s buffer and forwards to the outer terminal; `DCS tmux;` passthrough forwards to the outer terminal without setting `tmux`'s buffer; sending both sets `tmux`'s buffer and forwards to the outer terminal (updating the host clipboard when the outer terminal allows `OSC 52`, such as `xterm.js` or iTerm2 with `AllowClipboardAccess=true`) |
| Cell size in pixels (`CSI 16 t`) | Answered in `tmux 3.8`; unanswered in `tmux 3.3a` | Answered in `tmux 3.8`; unanswered in `tmux 3.3a` |
| iTerm2's `tmux -CC` integration (`tmux 3.8`) | Runs in a native iTerm2 integration window (`XTVERSION = "tmux 3.8"`, `TERM=tmux-256color`, Kitty `?u` unanswered, `CSI 16 t` returns synthetic `16x32` while rendering `7x17` pt cells). Mouse (`click`, `drag`, `wheel`, `Shift+click`, and `Option+drag` native selection), bracketed paste, and focus events (`FocusLost`/`FocusGained`) all work. Plain and `DCS`-wrapped `OSC 52` reach iTerm2 directly via `%output` notifications without setting `tmux`'s buffer. `OSC 11 ; ?` queried immediately at session creation (~3 ms) can answer `#000000` before the control client finishes attaching (~17 ms -> `#fafafa`), confirming why the theme bounds its wait and never assumes dark without a valid reply | Same window, mouse, paste, focus, and `OSC 11 ; ?` behavior, and plain or combined `OSC 52` also sets `tmux`'s buffer alongside forwarding to iTerm2 |

The minimum supported tmux is 3.3 (`3.3a` in the Linux floor container), the
first release with `allow-passthrough`.

### How wide a character is

The number of cells a string takes is decided by the terminal, and the
configurations do not agree. Measured by printing each sample between two
cursor-position queries (`DSR 6n`) across all thirteen configurations:

| Text | iTerm2 3.7.7 | xterm.js 6.0.0 in Orca | Standalone xterm.js 6.0.0 (+ Unicode 11) | Windows ConPTY | tmux 3.8 | tmux 3.3a |
| --- | --- | --- | --- | --- | --- | --- |
| ASCII (`A`), precomposed acute (`é`) | 1 | 1 | 1 | 1 | 1 | 1 |
| Decomposed combining acute (`e` + `U+0301`) | 1 | 1 | 1 | 2 | 1 | 1 |
| CJK, Hangul, fullwidth forms | 2 | 2 | 2 | 2 | 2 | 2 |
| All 17 toolkit glyphs (ellipsis, middle dot, chevron, pointer triangle, box drawing, block eighths, braille, private-use icon) | 1 | 1 | 1 | 1 | 1 | 1 |
| Plain emoji (`😀`) | 2 | 2 | 2 | 2 | 2 | 2 |
| Family joined with ZWJ (`👨‍👩‍👧`) | 2 | 2 | 6 | 8 | 2 | 2 |
| Regional flag (`🇺🇸`) | 2 | 2 | 2 | 4 | 2 | 2 |
| Emoji with VS16 (`❤️`) / plain heart (`❤`) | 2 / 1 | 1 / 1 | 1 / 1 | 2 / 1 | 2 / 1 | 1 / 1 |
| Emoji with skin tone (`👍🏽`) / plain (`👍`) | 2 / 2 | 4 / 2 | 4 / 2 | 4 / 2 | 2 / 2 | 4 / 2 |
| Rainbow flag ZWJ (`🏳️‍🌈`) / base flag (`🏳`) | 2 / 1 | 1 / 1 | 3 / 1 | 5 / 1 | 2 / 1 | 1 / 1 |
| Zero-width space (`U+200B`) | 1 | 1 | 1 | 1 | 0 | 0 |

Three findings from `rig/expected/*.json` anchor the toolkit's width policy:

1. **Every one of the 17 toolkit-drawn glyphs measured `1` cell across all 13
   configurations**: ellipsis (`…`), middle dot (`·`), chevron (`›`), pointer
   triangle (`▸`), box drawing (`─`, `│`, `┌`, `├`, `└`), block eighths (`▏`,
   `▍`, `▌`, `▊`, `█`), braille (`⠁`, `⣿`), and private-use icon (`\uE0B0`).
2. **Precomposed vs. decomposed accents split on Windows ConPTY.** Precomposed
   `é` (`U+00E9`) measures `1` cell everywhere, while decomposed `e` + `U+0301`
   measures `1` cell on Unix terminals and in `tmux` (`3.8` and `3.3a`) and `2`
   cells through Windows ConPTY (`conhost.exe`). M4's portable-text
   normalization accounts for combining marks alongside emoji sequences.
3. **Emoji and zero-width sequences split across terminals and `tmux`
   versions.** `iTerm2 3.7.7` measures `heart_vs16`, `thumbs_up_skin_tone`, and
   `rainbow_flag` as `2` cells and `zwsp` as `1` cell; `tmux 3.8` and `tmux
   3.3a` measure `zwsp` as `0`; `tmux 3.3a` and Orca measure `heart_vs16=1`,
   `thumbs_up_skin_tone=4`, and `rainbow_flag=1`; standalone `xterm.js`
   measures `family_zwj=6` and `rainbow_flag=3`; and Windows ConPTY measures
   `family_zwj=8`, `regional_flag_us=4`, `rainbow_flag=5`, and `zwsp=1`.

Because of that split, the toolkit never uses emoji sequences in its own
drawing, measures text with one width function for the whole workspace, and
rewrites app-supplied text into portable forms before drawing: variation
selectors and skin tones are dropped (`heart_plain=1`, `thumbs_up_plain=2`), a
ZWJ sequence starting with a one-cell character is cut to that character
(`white_flag_base=1`), and zero-width clusters are removed. The text width
section of [`specs/architecture.md`](specs/architecture.md) has the policy.

Orca's terminal counts a plain emoji as two cells, which xterm.js does only
with its Unicode 11 add-on loaded, so the test harness loads
`@xterm/addon-unicode11` too.

### Windows

On Windows, a console program does not write straight to the terminal: the
operating system's pseudo-console (ConPTY) sits in between, parses the
program's output and re-emits it. Every row below was measured on GitHub's
`windows-2025` runner (Windows Server 2025, OS build `10.0.26100`).

| Area | What differs on Windows |
| --- | --- |
| Escape sequences | ConPTY drops queries it does not forward: `XTVERSION`, `DECRQM`, Kitty `?u`, `OSC 11 ; ?`, and `CSI 16 t` go unanswered, while SGR attributes, cursor-position reports (`DSR 6n`), and `OSC 52` cross ConPTY intact. |
| Key events | crossterm reports both `Press` and `Release` (`Alt+a` and `Alt+Enter` arrive with `alt=true`; `Shift+Enter` and `Ctrl+Enter` arrive as plain `Enter`). Every key handler acts on presses only. |
| Input and paste | crossterm reads console input records, not raw VT bytes. `OSC 11 ; ?` goes unanswered across ConPTY (2 s timeout selects the no-answer palette), `bracketedPasteMode` stays off, and a paste arrives as `Press`+`Release` `Key` events in a single console read burst (`burst_press_count == 10` for a 10-character paste). |
| Mouse reporting | Turned on in the console's input mode through crossterm rather than Unix `DECSET` bytes. ConPTY then enables outer `DECSET 1003` + `1006` (`mouseTrackingMode: "any"`) on `xterm.js`, delivering bare hover `Moved` events alongside clicks, drags, wheel, right click, `Ctrl+click`, `Alt+click`, and `Alt+drag` (`Shift+click` stays with `xterm.js` selection), and resets outer tracking to `"none"` on exit or panic. |
| Focus events | `HPCON` holds outer `?1004h` until `ClosePseudoConsole` and delivers `FocusLost` and `FocusGained` console records. |
| `.cmd` shims (`cmd.exe /D /S /C`) | Agents installed as `.cmd` shims pass through two `cmd.exe` layers (`cmd.exe /D /S /C "<invocation>"` and `"<exe>" %*` inside the `.cmd` script) before `CommandLineToArgvW`. Measured on `windows-2025` (`rig_probe::cmd_shim`), `Command::raw_arg` with per-argument quoting (`""` for empty strings, doubled backslashes only before `"` and closing quotes, `"\^^^""` for embedded `"` (closes quote, emits `\^^^"`, and reopens quote), and `%%cd:~,%` for `%`) preserves empty arguments, spaces in both the `.cmd` path and arguments, embedded quotes with `&`, `""` vs `\"`, trailing backslashes, `%VAR%` and `%1` without expansion, and `cmd.exe` metacharacters (`^ & \| < > ( ) !VAR!`), while `\0`, `\r`, and `\n` cannot cross `cmd.exe` and are stripped. |
| The controlling terminal | There is no `/dev/tty`. Parts that draw outside stdout use `CONOUT$` and console handles instead. |
| Embedded terminals | Children run under ConPTY, not a Unix pseudo-terminal. There is no login shell to resolve aliases through. |
| Stopping a child and its descendants | Unix process groups do not exist. A job object does the same work. |
| Clipboard | `clip.exe` (and `OSC 52` across ConPTY when the outer terminal handles it) instead of `pbcopy`, `wl-copy` or `xclip`. |
| Opening files and links | `start` instead of `open` or `xdg-open`. |

## The capability probe

`capability_probe` (in `rig/probe` during Milestone 0, moving to
`avandar-tui-term` in Milestone 3), run inside a terminal, reports what that
configuration does: identity, the mouse modes it accepts and how setup and
teardown leave them, mouse buttons and modifiers, Option/Alt and modified
`Enter`, the kitty keyboard protocol, bracketed paste, focus events,
synchronized output, background color replies, `OSC 52` plain and `tmux`
forms, cell size, and all 33 character-width samples. Its normalized answers
live in `rig/expected/*.json`, and it stays as a diagnostic anyone can run
when an app misbehaves in their terminal.

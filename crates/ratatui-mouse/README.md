# ratatui-mouse

Mouse reporting, hit testing, text selection, keyboard select mode, and terminal
pane routing for ratatui apps using crossterm. The app owns its event loop,
clock, drawing stream, surfaces, and terminal cleanup. Copy and forwarded-input
reports contain data for the app to handle; this crate performs no clipboard or
child-process I/O.

There are no default features. Enable `vt100` for borrowed emulator-backed
surfaces. The `avandar-tui-kit` umbrella exposes this crate as
`avandar_tui_kit::mouse` when its `mouse` feature is enabled.
The umbrella feature does not enable `vt100`: enable `ratatui-mouse/vt100`
separately when using the adapter. Rust 1.88 or newer, ratatui 0.30, and
crossterm 0.29 are required. Ratatui, crossterm, and unicode-segmentation are
the base dependencies; vt100 0.16 is optional.

```rust
use ratatui_mouse::{HitAreas, MouseController, MouseSettings};

let controller = MouseController::<u8>::new(MouseSettings::default());
let areas = HitAreas::<u8>::new();
assert!(controller.gesture_owner().is_none());
assert!(areas.iter().next().is_none());
```

## Event-loop integration

Keep one controller and the last drawn `HitAreas` in app state. Each frame
clears and rebuilds areas in drawing order, registering selectable content
inside borders. Pass events, an explicit `Instant`, those areas, and borrowed
`SurfaceSet` adapters to `handle_event`. Wait no longer than the earlier of the
app's idle deadline and `next_deadline()`, then call `tick` on every loop pass,
including passes without input. Handle each report's `pressed_target` for
focus, `Copy` text for the clipboard, `Forward` bytes for the child, and
`is_redraw_due` for drawing. Bind a command that calls `enter_select_mode`.

Enable reporting on the drawing stream with `MouseReporting` or the free
functions. Install an app panic hook that disables reporting on that stream
before printing the panic; the guard also attempts cleanup on drop. On Windows
use `disable_mouse_reporting`, since console input modes need restoration.
Pausing for a child means full teardown followed by full setup.

## Keyboard equivalents

Select mode consumes keys before a pane program can receive them. Its built-in
map can be replaced with app commands through `apply_command`.

| Mouse action | Keyboard path | Owner |
| --- | --- | --- |
| Focus a target; activate a tab, button, or row | App focus keys and activation commands | App |
| Wheel outside selection | App scroll keys, including page and history limits | App |
| Drag to select or extend | `v`/Space, then h/j/k/l or arrows; w/b moves by word | Crate |
| Double click a word; triple click a line | `W`; `V` | Crate |
| Select through history or scroll while selecting | Ctrl+U/D, PageUp/Down, g/G; Home/End or 0/$ for line edges | Crate |
| Release to copy; click elsewhere to clear | `y`/Enter; Esc/q clears first, then leaves | Crate |
| Modifier selection in a pane with mouse input | Enter select mode, then the keys above | App entry command and crate |
| Forward mouse input or wheel arrows | The pane program's own keys or arrow keys | Pane program |
| Drag a divider | App resize commands, when supported | App |

## Surface contract

`LineSurface` borrows visual rows and scroll state; `Vt100Surface` borrows a
screen and its retained history. Custom surfaces implement `SelectableSurface`
and are looked up through `SurfaceSet<T>`. Content ranges are inclusive, with
columns measured in cells. Return whole wide characters from either cell,
logical lines spanning soft wraps, and signed rows actually scrolled. Copy
joins soft wraps without newlines, trims trailing logical-line blanks, adds
no final newline, copies offscreen ranges in full, and preserves the viewport.
Register the same content rectangle and wrapping that drawing used.

Drop a vt100 adapter before new output or resize, then borrow it again.
Selection coordinates remain stable while history grows, but retained-history
eviction shifts text beneath them. Clearing stale selections is app policy.
Render `HighlightRects` after the surface, snapping range ends to whole
characters. The default highlight uses reversed video, without fixed colors.

## Terminal limits

| Configuration | Behavior and keyboard fallback |
| --- | --- |
| macOS and Linux | Setup requests modes 1000, 1002, 1006 in order and never requests hover. Any-motion panes receive button motion only. |
| Windows ConPTY | Console setup causes outer any-motion plus SGR; bare moves are ignored. Queries do not cross ConPTY. Host hover can be affected until cleanup. |
| xterm.js on Linux/Windows | Shift-drag selects natively and never reaches the app. Alt-drag selects in the app; select mode always works. |
| xterm.js on macOS | Shift/Alt select in the app; there is no native selection modifier by default. |
| iTerm2 | Option-drag is native contiguous selection; Option+Command is native rectangular selection. Shift and direct Command-drag select in the app (Command arrives as Alt). Default right/Control clicks open native menus. |
| Stock/full tmux | Both forward app-requested mouse events even with `mouse off`. tmux 3.8 drops Alt/Command drag motion; tmux 3.3a forwards it but can absorb rapid right/Control presses. Orphan releases do nothing. Use select mode where motion is lost. |
| Query support | tmux 3.3a leaves queries unanswered; tmux 3.8 can report unsupported legacy formats. Missing and unsupported replies never prove reset. Only test programs query. |
| Wheel direction | Routed exactly as received. macOS natural scrolling inverts iTerm2 reports; the crate never flips them. Orca's own scroll setting remains unmeasured. |

Native selection copies visible terminal cells, including borders, through the
host clipboard. In-app selection understands pane text, soft wraps, and
history, returning `Copy` data to the app. The M0 capability probe measured
terminal behavior; the M1 laboratory verifies the crate's outcomes against it.

## Interactive laboratory

From the workspace, run:

```sh
cargo run -p ratatui-mouse --example mouse_lab --features vt100 -- --log mouse.jsonl --query-modes
cargo run -p ratatui-mouse --example mouse_lab --features vt100 -- --settings '{"auto_scroll_interval_ms":75}'
```

The left pane uses fixed app-held text, including paths, CJK, soft wraps, and
history. The right pane uses a fixed vt100 script with retained history and
wide cells. It starts at the oldest retained row. No program is spawned and
copies and forwarded bytes only enter the log.

Tab switches focus, `s` enters selection mode, `m` cycles the right pane's
mouse request, `a` toggles its alternate screen, PageUp/PageDown scroll the
focused pane, `q` quits, and `!` exercises panic cleanup. Selection keys take
priority while select mode is active. Run with `--help` for flags and examples.

The example flushes a sequenced JSONL log and creates `<log>.ready` only after
mouse reporting and initial layout are ready. `--query-modes` records replies
or bounded absence after setup and teardown. Serialization, nonblocking query
I/O, and the drawing backend are example-only dependencies.

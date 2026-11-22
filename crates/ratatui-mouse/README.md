# ratatui-mouse

Mouse reporting, hit testing, text selection, keyboard select mode, and terminal
pane routing for ratatui apps using crossterm. The app owns its event loop,
clock, drawing stream, surfaces, and terminal cleanup. Copy and forwarded-input
reports contain data for the app to handle; this crate performs no clipboard or
child-process I/O.

There are no default features. Enable `vt100` for borrowed emulator-backed
surfaces. The `avandar-tui-kit` umbrella exposes this crate as
`avandar_tui_kit::mouse` when its `mouse` feature is enabled.

```rust
use ratatui_mouse::{HitAreas, MouseController, MouseSettings};

let controller = MouseController::<u8>::new(MouseSettings::default());
let areas = HitAreas::<u8>::new();
assert!(controller.gesture_owner().is_none());
assert!(areas.iter().next().is_none());
```

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

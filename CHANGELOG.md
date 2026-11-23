# Changelog

## Unreleased

### Added

- `ratatui-mouse`: reporting lifecycle, hit areas, cell-aware text selection,
  timed edge scrolling, highlights, wheel routing, pane protocol encoding,
  and keyboard select mode. The app owns its loop, clock, clipboard, and I/O.
- Optional `vt100` surfaces over emulator text and retained history. No default
  feature is enabled; the umbrella's `mouse` feature re-exports the crate.
- `mouse_lab` and X1-X14 terminal scenarios across xterm.js, stock/full tmux,
  live Orca keyboard checks, and iTerm2 in a virtual machine. CI adds the mouse
  suite and feature rows on macOS, Linux, and Windows with retained ConPTY logs.

### Release gates

- Windows execution of the new laboratory and feature matrix is pending.
- A named person must judge edge scrolling (50 ms, maximum 10 rows), wheel
  distance (3 rows), and highlight legibility on dark/light/`NO_COLOR` terminals.
- No version tag or publication has occurred. The full checklist lives in
  [testing](docs/testing.md#human-release-judgments).

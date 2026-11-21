"""Layer 6 headless Tart + VNC driver package for iTerm2, tmux, and tmux -CC."""

# Default prepared Tart macOS VM name for Layer 6 probes.
DEFAULT_VM_NAME = "tuikit-macos"

# Default terminal column count configured for guest iTerm2 sessions.
DEFAULT_COLUMNS = 100

# Default terminal row count configured for guest iTerm2 sessions.
DEFAULT_ROWS = 30

# Default logical point width of a single iTerm2 character cell.
DEFAULT_CELL_WIDTH_PT = 7.0

# Default logical point height of a single iTerm2 character cell.
DEFAULT_CELL_HEIGHT_PT = 17.0

__all__ = [
    "DEFAULT_CELL_HEIGHT_PT",
    "DEFAULT_CELL_WIDTH_PT",
    "DEFAULT_COLUMNS",
    "DEFAULT_ROWS",
    "DEFAULT_VM_NAME",
]

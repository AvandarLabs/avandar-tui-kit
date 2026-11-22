//! Tunable mouse behavior and word classification.

use std::time::Duration;

use crossterm::event::KeyModifiers;

/// The characters, besides letters and digits, that are part of a word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordCharacters {
    punctuation: String,
}

impl WordCharacters {
    /// Letters, digits and each character of `punctuation`.
    #[must_use]
    pub fn new(punctuation: impl Into<String>) -> Self {
        Self {
            punctuation: punctuation.into(),
        }
    }

    /// Whether `character` is part of a word.
    #[must_use]
    pub fn contains(&self, character: char) -> bool {
        character.is_alphanumeric() || self.punctuation.contains(character)
    }
}

impl Default for WordCharacters {
    fn default() -> Self {
        Self::new("_-./~:@+%#?&=")
    }
}

/// Every tunable value for mouse interactions.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct MouseSettings {
    /// Rows per notch of the wheel. Defaults to 3.
    pub wheel_step_rows: u16,
    /// The longest gap between presses of one multi-click. Defaults to 500 ms.
    pub multi_click_interval: Duration,
    /// How many cells apart two presses of one multi-click may be. Defaults to 1.
    pub multi_click_distance: u16,
    /// Word characters. Defaults to letters, digits and `_-./~:@+%#?&=`.
    pub word_characters: WordCharacters,
    /// Return text for copying when a selection finishes. Defaults to true.
    pub should_copy_on_release: bool,
    /// Leave the copied selection highlighted. Defaults to true.
    pub should_keep_selection_after_copy: bool,
    /// A key press outside select mode clears selection; bare modifiers never do.
    /// Defaults to true.
    pub should_clear_on_key_press: bool,
    /// A resize clears the selection. Defaults to true.
    pub should_clear_on_resize: bool,
    /// Any held modifier selects instead of passing through on a pane press.
    /// Defaults to Shift or Alt; either alone is enough.
    pub selection_modifiers: KeyModifiers,
    /// Forward the mouse to panes whose programs requested it. Defaults to true.
    pub is_passthrough_enabled: bool,
    /// Send wheel arrow keys to full-screen programs without mouse tracking.
    /// Defaults to true.
    pub is_wheel_to_arrows_enabled: bool,
    /// Use the built-in select mode key map. Defaults to true.
    pub should_handle_select_mode_keys: bool,
    /// Time between edge-scroll steps. Defaults to 50 ms.
    pub auto_scroll_interval: Duration,
    /// The most rows one edge-scroll step moves. Defaults to 10.
    pub auto_scroll_max_rows: u16,
    /// Top and bottom rows that count as past the edge. Defaults to 0.
    pub edge_margin_rows: u16,
    /// The shortest interval between requested redraws. Defaults to 16 ms.
    pub redraw_interval: Duration,
}

impl Default for MouseSettings {
    fn default() -> Self {
        Self {
            wheel_step_rows: 3,
            multi_click_interval: Duration::from_millis(500),
            multi_click_distance: 1,
            word_characters: WordCharacters::default(),
            should_copy_on_release: true,
            should_keep_selection_after_copy: true,
            should_clear_on_key_press: true,
            should_clear_on_resize: true,
            selection_modifiers: KeyModifiers::SHIFT | KeyModifiers::ALT,
            is_passthrough_enabled: true,
            is_wheel_to_arrows_enabled: true,
            should_handle_select_mode_keys: true,
            auto_scroll_interval: Duration::from_millis(50),
            auto_scroll_max_rows: 10,
            edge_margin_rows: 0,
            redraw_interval: Duration::from_millis(16),
        }
    }
}

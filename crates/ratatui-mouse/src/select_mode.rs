//! Keyboard commands for selecting app-owned content.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use ratatui::layout::Rect;

use crate::{ContentPoint, SelectableSurface, WordCharacters};

/// A cursor movement in select mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SelectionMotion {
    /// One character left.
    Left,
    /// One character right.
    Right,
    /// One row up.
    Up,
    /// One row down.
    Down,
    /// Start of the next word.
    WordForward,
    /// Start of the previous word.
    WordBackward,
    /// Start of the logical line.
    LineStart,
    /// End of the logical line.
    LineEnd,
    /// Half a viewport up.
    HalfPageUp,
    /// Half a viewport down.
    HalfPageDown,
    /// One viewport up.
    PageUp,
    /// One viewport down.
    PageDown,
    /// First content row.
    FirstLine,
    /// Last content row.
    LastLine,
}

/// What select mode can be told to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SelectionCommand {
    /// Move the cursor, extending an active selection.
    Move(SelectionMotion),
    /// Start selecting cells at the cursor, or stop.
    ToggleCellSelection,
    /// Start selecting logical lines at the cursor, or stop.
    ToggleLineSelection,
    /// Select the word under the cursor.
    SelectWord,
    /// Return selected text and leave select mode.
    Copy,
    /// Clear a selection, or leave when none exists.
    Cancel,
}

/// The default key map, ignoring unbound keys and key releases.
#[must_use]
pub fn selection_command_from_key(key: &KeyEvent) -> Option<SelectionCommand> {
    if key.kind == KeyEventKind::Release {
        return None;
    }
    if key.modifiers == KeyModifiers::CONTROL {
        return match key.code {
            KeyCode::Char('u' | 'U') => Some(SelectionCommand::Move(SelectionMotion::HalfPageUp)),
            KeyCode::Char('d' | 'D') => Some(SelectionCommand::Move(SelectionMotion::HalfPageDown)),
            _ => None,
        };
    }
    if !(key.modifiers - KeyModifiers::SHIFT).is_empty() {
        return None;
    }
    command_from_code(key.code)
}

const fn command_from_code(code: KeyCode) -> Option<SelectionCommand> {
    use SelectionCommand::{
        Cancel, Copy, Move, SelectWord, ToggleCellSelection, ToggleLineSelection,
    };
    use SelectionMotion::{
        Down, FirstLine, LastLine, Left, LineEnd, LineStart, PageDown, PageUp, Right, Up,
        WordBackward, WordForward,
    };
    Some(match code {
        KeyCode::Char('h') | KeyCode::Left => Move(Left),
        KeyCode::Char('j') | KeyCode::Down => Move(Down),
        KeyCode::Char('k') | KeyCode::Up => Move(Up),
        KeyCode::Char('l') | KeyCode::Right => Move(Right),
        KeyCode::Char('w') => Move(WordForward),
        KeyCode::Char('b') => Move(WordBackward),
        KeyCode::Char('0') | KeyCode::Home => Move(LineStart),
        KeyCode::Char('$') | KeyCode::End => Move(LineEnd),
        KeyCode::PageUp => Move(PageUp),
        KeyCode::PageDown => Move(PageDown),
        KeyCode::Char('g') => Move(FirstLine),
        KeyCode::Char('G') => Move(LastLine),
        KeyCode::Char('v' | ' ') => ToggleCellSelection,
        KeyCode::Char('V') => ToggleLineSelection,
        KeyCode::Char('W') => SelectWord,
        KeyCode::Char('y') | KeyCode::Enter => Copy,
        KeyCode::Esc | KeyCode::Char('q') => Cancel,
        _ => return None,
    })
}

pub(crate) fn clamp_cursor(
    surface: &mut dyn SelectableSurface,
    point: ContentPoint,
    width: u16,
) -> Option<ContentPoint> {
    let bounds = surface.line_bounds();
    if bounds.is_empty() {
        return None;
    }
    let line = point.line.clamp(*bounds.start(), *bounds.end());
    let column = point.column.min(row_end(surface, line, width));
    Some(surface.character_at(ContentPoint { line, column }).start)
}

pub(crate) fn move_cursor(
    surface: &mut dyn SelectableSurface,
    cursor: ContentPoint,
    motion: SelectionMotion,
    area: Rect,
    words: &WordCharacters,
) -> ContentPoint {
    use SelectionMotion::{
        Down, FirstLine, HalfPageDown, HalfPageUp, LastLine, Left, LineEnd, LineStart, PageDown,
        PageUp, Right, Up, WordBackward, WordForward,
    };
    let page = i64::from(area.height.max(1));
    let half_page = (page / 2).max(1);
    let point = match motion {
        Left => step_cursor(surface, cursor, false, area.width),
        Right => step_cursor(surface, cursor, true, area.width),
        WordForward => word_cursor(surface, cursor, true, words, area.width),
        WordBackward => word_cursor(surface, cursor, false, words, area.width),
        LineStart => surface.line_at(cursor.line).start,
        LineEnd => surface.line_at(cursor.line).end,
        FirstLine => ContentPoint {
            line: *surface.line_bounds().start(),
            column: 0,
        },
        LastLine => ContentPoint {
            line: *surface.line_bounds().end(),
            column: 0,
        },
        Up => offset_rows(cursor, -1),
        Down => offset_rows(cursor, 1),
        HalfPageUp => offset_rows(cursor, -half_page),
        HalfPageDown => offset_rows(cursor, half_page),
        PageUp => offset_rows(cursor, -page),
        PageDown => offset_rows(cursor, page),
    };
    clamp_cursor(surface, point, area.width).unwrap_or(cursor)
}

const fn offset_rows(cursor: ContentPoint, rows: i64) -> ContentPoint {
    ContentPoint {
        line: cursor.line.saturating_add(rows),
        ..cursor
    }
}

fn row_end(surface: &mut dyn SelectableSurface, line: i64, width: u16) -> u32 {
    let logical_end = surface.line_at(line).end;
    if logical_end.line == line {
        logical_end.column
    } else {
        // Copied text trims trailing blanks, so wrapped-row geometry comes
        // from the viewport the app used to render those rows.
        surface
            .origin()
            .column
            .saturating_add(u32::from(width))
            .saturating_sub(1)
    }
}

fn step_cursor(
    surface: &mut dyn SelectableSurface,
    cursor: ContentPoint,
    is_forward: bool,
    width: u16,
) -> ContentPoint {
    let character = surface.character_at(cursor);
    let bounds = surface.line_bounds();
    if is_forward {
        let end = row_end(surface, cursor.line, width);
        if character.end.column < end {
            ContentPoint {
                column: character.end.column.saturating_add(1),
                ..cursor
            }
        } else if cursor.line < *bounds.end() {
            ContentPoint {
                line: cursor.line.saturating_add(1),
                column: 0,
            }
        } else {
            cursor
        }
    } else if character.start.column > 0 {
        surface
            .character_at(ContentPoint {
                column: character.start.column - 1,
                ..cursor
            })
            .start
    } else if cursor.line > *bounds.start() {
        let line = cursor.line.saturating_sub(1);
        let column = row_end(surface, line, width);
        surface.character_at(ContentPoint { line, column }).start
    } else {
        cursor
    }
}

fn word_cursor(
    surface: &mut dyn SelectableSurface,
    cursor: ContentPoint,
    is_forward: bool,
    words: &WordCharacters,
    width: u16,
) -> ContentPoint {
    let original_word = surface.word_at(cursor, words);
    let mut point = cursor;
    loop {
        let candidate = step_cursor(surface, point, is_forward, width);
        if candidate == point {
            return point;
        }
        point = candidate;
        if let Some(word) = surface.word_at(point, words)
            && (!is_forward || Some(word) != original_word)
        {
            return word.start;
        }
    }
}

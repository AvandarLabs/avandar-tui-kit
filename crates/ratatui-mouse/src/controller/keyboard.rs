//! Keyboard selection reuses content expansion and injected-time redraws.

use std::time::Instant;

use crossterm::event::{KeyEvent, KeyEventKind};
use ratatui::layout::Rect;

use super::{MouseController, create_mouse_report};
use crate::{
    ContentPoint, HitAreas, MouseOutcome, MouseReport, SelectableSurface, Selection,
    SelectionCommand, SelectionStage, SelectionUnit, SurfaceSet,
    select_mode::{clamp_cursor, move_cursor},
    selection_command_from_key,
};

pub(super) struct SelectMode<T> {
    target: T,
    cursor: ContentPoint,
    extending: Option<SelectionUnit>,
}

impl<T: Clone + PartialEq> MouseController<T> {
    /// Starts select mode at `start`, or the start of the last visible row.
    /// Targets without an area, surface, or content rows are ignored.
    pub fn enter_select_mode<S: SurfaceSet<T>>(
        &mut self,
        target: T,
        start: Option<ContentPoint>,
        areas: &HitAreas<T>,
        surfaces: &mut S,
    ) -> MouseReport<T> {
        let Some(area) = areas.area_of(&target) else {
            return create_mouse_report(MouseOutcome::Ignored, None);
        };
        let Some(surface) = surfaces.surface_mut(&target) else {
            return create_mouse_report(MouseOutcome::Ignored, None);
        };
        let origin = surface.origin();
        let Some(cursor) = clamp_cursor(
            surface,
            start.unwrap_or_else(|| ContentPoint {
                line: origin
                    .line
                    .saturating_add(i64::from(area.height.saturating_sub(1))),
                column: origin.column,
            }),
            area.width,
        ) else {
            return create_mouse_report(MouseOutcome::Ignored, None);
        };
        scroll_to_cursor(surface, cursor, area.height);
        self.cancel_gesture();
        self.clear_selection();
        self.select_mode = Some(SelectMode {
            target,
            cursor,
            extending: None,
        });
        self.timing.has_pending_redraw = false;
        MouseReport {
            is_redraw_due: true,
            ..create_mouse_report(MouseOutcome::Consumed, None)
        }
    }

    /// Ends select mode and drops its selection. Returns whether it was on.
    pub fn exit_select_mode(&mut self) -> bool {
        let was_on = self.select_mode.take().is_some();
        if was_on {
            self.cancel_gesture();
            self.clear_selection();
            self.timing.has_pending_redraw = true;
        }
        was_on
    }

    /// The target select mode is on for.
    #[must_use]
    pub fn select_mode_target(&self) -> Option<&T> {
        self.select_mode.as_ref().map(|mode| &mode.target)
    }

    /// The content cursor that the app draws while select mode is on.
    #[must_use]
    pub fn select_mode_cursor(&self) -> Option<ContentPoint> {
        self.select_mode.as_ref().map(|mode| mode.cursor)
    }

    /// Runs an app-mapped command, ignoring commands while select mode is off.
    pub fn apply_command<S: SurfaceSet<T>>(
        &mut self,
        command: SelectionCommand,
        now: Instant,
        areas: &HitAreas<T>,
        surfaces: &mut S,
    ) -> MouseReport<T> {
        let previous_selection = self.selection_visual();
        let previous_cursor = self.select_mode_cursor();
        self.validate_state(areas, surfaces);
        let outcome = self.run_command(command, areas, surfaces);
        let has_changed = previous_selection != self.selection_visual()
            || previous_cursor != self.select_mode_cursor();
        self.complete_report(create_mouse_report(outcome, None), has_changed, now)
    }

    pub(super) fn handle_select_key<S: SurfaceSet<T>>(
        &mut self,
        key: &KeyEvent,
        areas: &HitAreas<T>,
        surfaces: &mut S,
    ) -> MouseOutcome<T> {
        if key.kind == KeyEventKind::Release {
            return MouseOutcome::Ignored;
        }
        if self.settings.should_handle_select_mode_keys
            && let Some(command) = selection_command_from_key(key)
        {
            return self.run_command(command, areas, surfaces);
        }
        MouseOutcome::Consumed
    }

    pub(super) const fn stop_keyboard_selection(&mut self) {
        if let Some(mode) = &mut self.select_mode {
            mode.extending = None;
        }
    }

    fn run_command<S: SurfaceSet<T>>(
        &mut self,
        command: SelectionCommand,
        areas: &HitAreas<T>,
        surfaces: &mut S,
    ) -> MouseOutcome<T> {
        let Some(mut mode) = self.select_mode.take() else {
            return MouseOutcome::Ignored;
        };
        let Some(area) = areas.area_of(&mode.target) else {
            return MouseOutcome::Ignored;
        };
        let Some(surface) = surfaces.surface_mut(&mode.target) else {
            return MouseOutcome::Ignored;
        };
        let outcome = match command {
            SelectionCommand::Copy => return self.copy_keyboard_selection(mode.target, surfaces),
            SelectionCommand::Cancel if self.selection.is_none() => {
                self.cancel_gesture();
                return MouseOutcome::Consumed;
            }
            SelectionCommand::Cancel => {
                self.clear_selection();
                mode.extending = None;
                MouseOutcome::Consumed
            }
            _ => {
                self.update_keyboard_selection(&mut mode, command, surface, area);
                MouseOutcome::Consumed
            }
        };
        self.select_mode = Some(mode);
        outcome
    }

    fn copy_keyboard_selection<S: SurfaceSet<T>>(
        &mut self,
        target: T,
        surfaces: &mut S,
    ) -> MouseOutcome<T> {
        let text = self
            .selected_text(surfaces)
            .filter(|text| !text.trim().is_empty());
        self.cancel_gesture();
        self.clear_selection();
        text.map_or(MouseOutcome::Consumed, |text| MouseOutcome::Copy {
            target,
            text,
        })
    }

    fn update_keyboard_selection(
        &mut self,
        mode: &mut SelectMode<T>,
        command: SelectionCommand,
        surface: &mut dyn SelectableSurface,
        area: Rect,
    ) {
        match command {
            SelectionCommand::Move(motion) => {
                mode.cursor = move_cursor(
                    surface,
                    mode.cursor,
                    motion,
                    area,
                    &self.settings.word_characters,
                );
                scroll_to_cursor(surface, mode.cursor, area.height);
                if let Some(unit) = mode.extending {
                    let endpoint = self.expand_point(surface, mode.cursor, unit);
                    if let Some(selection) = &mut self.selection {
                        selection.range = selection.anchor.union(endpoint);
                    }
                }
            }
            SelectionCommand::ToggleCellSelection => {
                self.toggle_keyboard_selection(mode, surface, SelectionUnit::Cell);
            }
            SelectionCommand::ToggleLineSelection => {
                self.toggle_keyboard_selection(mode, surface, SelectionUnit::Line);
            }
            SelectionCommand::SelectWord => {
                self.start_keyboard_selection(mode, surface, SelectionUnit::Word);
            }
            SelectionCommand::Copy | SelectionCommand::Cancel => {}
        }
    }

    fn toggle_keyboard_selection(
        &mut self,
        mode: &mut SelectMode<T>,
        surface: &mut dyn SelectableSurface,
        unit: SelectionUnit,
    ) {
        if mode.extending == Some(unit) {
            mode.extending = None;
        } else {
            self.start_keyboard_selection(mode, surface, unit);
        }
    }

    fn start_keyboard_selection(
        &mut self,
        mode: &mut SelectMode<T>,
        surface: &mut dyn SelectableSurface,
        unit: SelectionUnit,
    ) {
        self.cancel_gesture();
        let anchor = self.expand_point(surface, mode.cursor, unit);
        self.selection = Some(Selection {
            target: mode.target.clone(),
            anchor,
            range: anchor,
            unit,
            stage: SelectionStage::Done,
        });
        mode.extending = Some(unit);
    }
}

fn scroll_to_cursor(surface: &mut dyn SelectableSurface, cursor: ContentPoint, height: u16) {
    let origin = surface.origin();
    let bottom = origin
        .line
        .saturating_add(i64::from(height.saturating_sub(1)));
    let rows = if cursor.line < origin.line {
        cursor.line.saturating_sub(origin.line)
    } else {
        cursor.line.saturating_sub(bottom).max(0)
    };
    let rows = i32::try_from(rows).unwrap_or(if rows < 0 { i32::MIN } else { i32::MAX });
    if rows != 0 {
        surface.scroll_by(rows);
    }
}

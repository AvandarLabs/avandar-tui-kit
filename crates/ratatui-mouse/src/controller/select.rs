//! Selection gestures keep complete characters and their original anchor.

use ratatui::layout::{Position, Rect};

use super::{Gesture, MouseController};
use crate::{
    ContentPoint, ContentRange, MouseOutcome, PointerPress, SelectableSurface, Selection,
    SelectionStage, SelectionUnit, SurfaceSet,
};

pub(super) struct SelectGesture<T> {
    pub target: T,
    pub press: PointerPress,
    pub is_extension: bool,
}

impl<T: Clone + PartialEq> MouseController<T> {
    pub(super) fn start_selection(
        &mut self,
        target: &T,
        press: PointerPress,
        surface: &mut dyn SelectableSurface,
    ) -> Gesture<T> {
        let point = press.content_point.expect("surface press has content");
        let previous = self.selection.take().filter(|selection| {
            selection.target == *target
                && selection.stage == SelectionStage::Done
                && press
                    .modifiers
                    .intersects(self.settings.selection_modifiers)
        });
        let is_extension = previous.is_some();
        self.selection = Some(if let Some(mut selection) = previous {
            selection.range =
                selection
                    .anchor
                    .union(self.expand_point(surface, point, selection.unit));
            selection.stage = SelectionStage::Pressed;
            selection
        } else {
            self.create_selection(target, press, surface, point)
        });
        Gesture::Select(SelectGesture {
            target: target.clone(),
            press,
            is_extension,
        })
    }

    pub(super) fn continue_selection<S: SurfaceSet<T>>(
        &mut self,
        gesture: &SelectGesture<T>,
        position: Position,
        area: Rect,
        is_release: bool,
        surfaces: &mut S,
    ) -> MouseOutcome<T> {
        let Some(mut selection) = self.selection.take() else {
            return MouseOutcome::Ignored;
        };
        let Some(surface) = surfaces.surface_mut(&gesture.target) else {
            return MouseOutcome::Ignored;
        };
        if is_release
            && selection.stage == SelectionStage::Pressed
            && selection.unit == SelectionUnit::Cell
            && !gesture.is_extension
        {
            return MouseOutcome::Clicked {
                target: gesture.target.clone(),
                press: gesture.press,
            };
        }
        let point = ContentPoint::from_screen_position(position, area, surface.origin());
        selection.range = selection
            .anchor
            .union(self.expand_point(surface, point, selection.unit));
        selection.stage = if is_release {
            SelectionStage::Done
        } else {
            SelectionStage::Dragging
        };
        if is_release {
            return self.finish_selection(selection, surface);
        }
        self.selection = Some(selection);
        MouseOutcome::Consumed
    }

    fn create_selection(
        &self,
        target: &T,
        press: PointerPress,
        surface: &mut dyn SelectableSurface,
        point: ContentPoint,
    ) -> Selection<T> {
        let unit = match press.clicks {
            2 => SelectionUnit::Word,
            3 => SelectionUnit::Line,
            _ => SelectionUnit::Cell,
        };
        let anchor = self.expand_point(surface, point, unit);
        Selection {
            target: target.clone(),
            anchor,
            range: anchor,
            unit,
            stage: SelectionStage::Pressed,
        }
    }

    fn finish_selection(
        &mut self,
        selection: Selection<T>,
        surface: &mut dyn SelectableSurface,
    ) -> MouseOutcome<T> {
        let text = surface.text(selection.range);
        if text.trim().is_empty() {
            return MouseOutcome::Consumed;
        }
        let outcome = if self.settings.should_copy_on_release {
            MouseOutcome::Copy {
                target: selection.target.clone(),
                text,
            }
        } else {
            MouseOutcome::Consumed
        };
        if !self.settings.should_copy_on_release || self.settings.should_keep_selection_after_copy {
            self.selection = Some(selection);
        }
        outcome
    }

    fn expand_point(
        &self,
        surface: &mut dyn SelectableSurface,
        point: ContentPoint,
        unit: SelectionUnit,
    ) -> ContentRange {
        let range = match unit {
            SelectionUnit::Cell => surface.character_at(point),
            SelectionUnit::Word => surface
                .word_at(point, &self.settings.word_characters)
                .unwrap_or_else(|| surface.character_at(point)),
            SelectionUnit::Line => surface.line_at(point.line),
        };
        surface
            .character_at(range.start)
            .union(surface.character_at(range.end))
    }
}

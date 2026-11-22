//! Click runs driven by caller-provided press times.

use std::time::Instant;

use ratatui::layout::Position;

use crate::MouseSettings;

/// Counts presses close enough to be a double or triple click.
pub struct ClickCounter<T> {
    last_press: Option<Press<T>>,
}

struct Press<T> {
    target: T,
    position: Position,
    time: Instant,
    count: u8,
}

impl<T: Clone + PartialEq> ClickCounter<T> {
    /// A counter that has seen no press.
    #[must_use]
    pub const fn new() -> Self {
        Self { last_press: None }
    }

    /// Records a left press and returns 1, 2 or 3. After 3, the run restarts.
    pub fn register_press(
        &mut self,
        target: &T,
        position: Position,
        now: Instant,
        settings: &MouseSettings,
    ) -> u8 {
        let count = self
            .last_press
            .as_ref()
            .filter(|press| {
                press.target == *target
                    && position.x.abs_diff(press.position.x) <= settings.multi_click_distance
                    && position.y.abs_diff(press.position.y) <= settings.multi_click_distance
                    && now
                        .checked_duration_since(press.time)
                        .is_some_and(|elapsed| elapsed <= settings.multi_click_interval)
            })
            .map_or(1, |press| press.count % 3 + 1);
        self.last_press = Some(Press {
            target: target.clone(),
            position,
            time: now,
            count,
        });
        count
    }

    /// Forgets the last press, so the next counts as the first.
    pub fn reset(&mut self) {
        self.last_press = None;
    }

    /// Removes click history only when it belongs to `target`.
    pub(crate) fn forget_target(&mut self, target: &T) -> bool {
        let has_target = self
            .last_press
            .as_ref()
            .is_some_and(|press| press.target == *target);
        if has_target {
            self.reset();
        }
        has_target
    }
}

impl<T: Clone + PartialEq> Default for ClickCounter<T> {
    fn default() -> Self {
        Self::new()
    }
}

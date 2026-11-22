//! The umbrella mouse feature exposes the standalone controller API.
#![cfg(feature = "mouse")]

use avandar_tui_kit::mouse::{HitAreas, MouseController, MouseSettings, NoSurfaces};

#[test]
fn umbrella_mouse_reexport_is_usable() {
    let mut controller = MouseController::<u8>::new(MouseSettings::default());
    let areas = HitAreas::new();
    let report = controller.handle_event(
        &crossterm::event::Event::FocusLost,
        std::time::Instant::now(),
        &areas,
        &mut NoSurfaces,
    );
    assert!(controller.gesture_owner().is_none());
    assert_eq!(
        report.outcome,
        avandar_tui_kit::mouse::MouseOutcome::Ignored
    );
}

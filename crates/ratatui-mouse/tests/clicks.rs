//! Click-run thresholds and configurable word classification.

use std::time::{Duration, Instant};

use ratatui::layout::Position;
use ratatui_mouse::{ClickCounter, MouseSettings, WordCharacters};

#[derive(Clone, PartialEq)]
struct Target(String);

#[test]
fn click_run_obeys_time_target_distance_and_reset() {
    let settings = MouseSettings::default();
    let target = Target("pane".into());
    let now = Instant::now();
    let mut counter = ClickCounter::new();
    let counts = [0, 100, 200, 300].map(|millis| {
        counter.register_press(
            &target,
            Position::new(10, 5),
            now + Duration::from_millis(millis),
            &settings,
        )
    });
    assert_eq!(counts, [1, 2, 3, 1]);
}

#[test]
fn click_boundaries_are_inclusive_and_compared_with_previous_press() {
    let settings = MouseSettings::default();
    let target = Target("pane".into());
    let now = Instant::now();
    let mut counter = ClickCounter::new();
    assert_eq!(
        counter.register_press(&target, Position::new(10, 5), now, &settings),
        1
    );
    assert_eq!(
        counter.register_press(
            &target,
            Position::new(11, 6),
            now + Duration::from_millis(500),
            &settings
        ),
        2
    );
    assert_eq!(
        counter.register_press(
            &target,
            Position::new(12, 7),
            now + Duration::from_secs(1),
            &settings
        ),
        3
    );
}

#[test]
fn slow_distant_other_target_and_reset_restart_runs() {
    let settings = MouseSettings::default();
    let target = Target("pane".into());
    let other = Target("modal".into());
    let now = Instant::now();
    for (second_target, position, gap) in [
        (&target, Position::new(10, 5), 501),
        (&target, Position::new(12, 5), 100),
        (&target, Position::new(10, 7), 100),
        (&other, Position::new(10, 5), 100),
    ] {
        let mut counter = ClickCounter::new();
        counter.register_press(&target, Position::new(10, 5), now, &settings);
        let first = counter.register_press(
            second_target,
            position,
            now + Duration::from_millis(gap),
            &settings,
        );
        let second = counter.register_press(
            second_target,
            position,
            now + Duration::from_millis(gap + 100),
            &settings,
        );
        counter.reset();
        let after_reset = counter.register_press(
            second_target,
            position,
            now + Duration::from_millis(gap + 200),
            &settings,
        );
        assert_eq!([first, second, after_reset], [1, 2, 1]);
    }
}

#[test]
fn custom_thresholds_and_extreme_positions_are_respected() {
    let mut settings = MouseSettings::default();
    settings.multi_click_distance = u16::MAX;
    settings.multi_click_interval = Duration::ZERO;
    let now = Instant::now();
    let mut counter = ClickCounter::new();
    assert_eq!(
        counter.register_press(&"pane", Position::new(0, 0), now, &settings),
        1
    );
    assert_eq!(
        counter.register_press(&"pane", Position::new(u16::MAX, u16::MAX), now, &settings),
        2
    );
    assert_eq!(
        counter.register_press(
            &"pane",
            Position::new(0, 0),
            now + Duration::from_nanos(1),
            &settings
        ),
        1
    );
}

#[test]
fn words_include_unicode_alphanumerics_and_configured_punctuation() {
    let defaults = MouseSettings::default();
    assert!(
        "café/文件.rs:12_~@+%#?&=-"
            .chars()
            .all(|character| defaults.word_characters.contains(character))
    );
    for character in [' ', '\n', '!', ',', '🦀'] {
        assert!(!defaults.word_characters.contains(character));
    }
    let custom = WordCharacters::new("!");
    assert!(custom.contains('!'));
    assert!(custom.contains('é'));
    assert!(!custom.contains('/'));
}

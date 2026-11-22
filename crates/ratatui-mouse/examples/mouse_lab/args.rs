//! Example-only argument and JSON settings encoding.

use std::path::PathBuf;
use std::time::Duration;

use ratatui_mouse::{MouseSettings, WordCharacters};
use serde::Deserialize;

pub const HELP: &str = "Explore mouse selection and pane routing without clipboard or child effects.\n\nUsage: mouse_lab [--log <path>] [--query-modes] [--settings <json>]\n\nOptions:\n  -h, --help          Show this help and exit\n  --log <path>       JSONL output (default: mouse_lab.jsonl)\n  --query-modes      Query reporting after setup and teardown (default: off)\n  --settings <json>  Override MouseSettings using snake_case names (default: {})\n                     Durations use integer *_ms keys, word_characters is a string,\n                     selection_modifiers is an array of shift|alt|control|super|hyper|meta.\n                     Unknown keys, nulls, negative numbers, and zero intervals are rejected.\n\nSettings defaults:\n  wheel_step_rows=3, multi_click_interval_ms=500, multi_click_distance=1\n  word_characters=\"_-./~:@+%#?&=\", selection_modifiers=[\"shift\",\"alt\"]\n  auto_scroll_interval_ms=50, auto_scroll_max_rows=10, edge_margin_rows=0\n  redraw_interval_ms=16\n  The following default to true: should_copy_on_release,\n  should_keep_selection_after_copy, should_clear_on_key_press,\n  should_clear_on_resize, is_passthrough_enabled,\n  is_wheel_to_arrows_enabled, should_handle_select_mode_keys.\n\nKeys: Tab focuses, s selects, m cycles pane mouse modes, a toggles the pane's\nalternate screen, PageUp/PageDown scroll, q quits, ! panics. In select mode\nthe built-in selection keys win; Escape cancels, y copies to the log only.\n\nExamples:\n  mouse_lab --log mouse.jsonl --query-modes\n  mouse_lab --settings '{\"auto_scroll_interval_ms\":75,\"wheel_step_rows\":5}'\n";

#[derive(Debug)]
pub struct Args {
    pub log_path: PathBuf,
    pub should_query_modes: bool,
    pub settings: MouseSettings,
}

pub fn parse_args<I, S>(arguments: I) -> Result<Option<Args>, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let arguments: Vec<String> = arguments
        .into_iter()
        .map(|value| value.as_ref().to_owned())
        .collect();
    if arguments
        .iter()
        .any(|value| value == "-h" || value == "--help")
    {
        return Ok(None);
    }
    let mut args = Args {
        log_path: "mouse_lab.jsonl".into(),
        should_query_modes: false,
        settings: MouseSettings::default(),
    };
    let mut values = arguments.iter();
    while let Some(flag) = values.next() {
        match flag.as_str() {
            "--log" => {
                args.log_path = values
                    .next()
                    .filter(|value| !value.starts_with('-'))
                    .ok_or_else(|| invalid("--log needs a path"))?
                    .into();
            }
            "--query-modes" => args.should_query_modes = true,
            "--settings" => {
                args.settings = parse_settings(
                    values
                        .next()
                        .ok_or_else(|| invalid("--settings needs JSON"))?,
                )?;
            }
            _ => return Err(invalid(&format!("unknown argument {flag:?}"))),
        }
    }
    Ok(Some(args))
}

fn invalid(message: &str) -> String {
    format!(
        "{HELP}\n❌ mouse_lab: {message}.\n  ⚠️ Run mouse_lab --help and correct the invocation."
    )
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Overrides {
    wheel_step_rows: Option<u16>,
    multi_click_interval_ms: Option<u64>,
    multi_click_distance: Option<u16>,
    word_characters: Option<String>,
    should_copy_on_release: Option<bool>,
    should_keep_selection_after_copy: Option<bool>,
    should_clear_on_key_press: Option<bool>,
    should_clear_on_resize: Option<bool>,
    selection_modifiers: Option<Vec<String>>,
    is_passthrough_enabled: Option<bool>,
    is_wheel_to_arrows_enabled: Option<bool>,
    should_handle_select_mode_keys: Option<bool>,
    auto_scroll_interval_ms: Option<u64>,
    auto_scroll_max_rows: Option<u16>,
    edge_margin_rows: Option<u16>,
    redraw_interval_ms: Option<u64>,
}

pub fn parse_settings(json: &str) -> Result<MouseSettings, String> {
    let overrides = parse_overrides(json)?;
    let mut settings = MouseSettings::default();
    apply_scalars(&mut settings, &overrides);
    if let Some(punctuation) = overrides.word_characters {
        settings.word_characters = WordCharacters::new(punctuation);
    }
    if let Some(modifiers) = overrides.selection_modifiers {
        settings.selection_modifiers = parse_modifiers(&modifiers)?;
    }
    for (name, value, destination) in [
        (
            "multi_click_interval_ms",
            overrides.multi_click_interval_ms,
            &mut settings.multi_click_interval,
        ),
        (
            "auto_scroll_interval_ms",
            overrides.auto_scroll_interval_ms,
            &mut settings.auto_scroll_interval,
        ),
        (
            "redraw_interval_ms",
            overrides.redraw_interval_ms,
            &mut settings.redraw_interval,
        ),
    ] {
        if let Some(milliseconds) = value {
            if milliseconds == 0 {
                return Err(invalid(&format!("{name} must be positive")));
            }
            *destination = Duration::from_millis(milliseconds);
        }
    }
    Ok(settings)
}

const fn apply_scalars(settings: &mut MouseSettings, overrides: &Overrides) {
    if let Some(value) = overrides.wheel_step_rows {
        settings.wheel_step_rows = value;
    }
    if let Some(value) = overrides.multi_click_distance {
        settings.multi_click_distance = value;
    }
    if let Some(value) = overrides.auto_scroll_max_rows {
        settings.auto_scroll_max_rows = value;
    }
    if let Some(value) = overrides.edge_margin_rows {
        settings.edge_margin_rows = value;
    }
    if let Some(value) = overrides.should_copy_on_release {
        settings.should_copy_on_release = value;
    }
    if let Some(value) = overrides.should_keep_selection_after_copy {
        settings.should_keep_selection_after_copy = value;
    }
    if let Some(value) = overrides.should_clear_on_key_press {
        settings.should_clear_on_key_press = value;
    }
    if let Some(value) = overrides.should_clear_on_resize {
        settings.should_clear_on_resize = value;
    }
    if let Some(value) = overrides.is_passthrough_enabled {
        settings.is_passthrough_enabled = value;
    }
    if let Some(value) = overrides.is_wheel_to_arrows_enabled {
        settings.is_wheel_to_arrows_enabled = value;
    }
    if let Some(value) = overrides.should_handle_select_mode_keys {
        settings.should_handle_select_mode_keys = value;
    }
}

fn parse_modifiers(names: &[String]) -> Result<crossterm::event::KeyModifiers, String> {
    use crossterm::event::KeyModifiers;
    names
        .iter()
        .try_fold(KeyModifiers::NONE, |modifiers, name| {
            let modifier = match name.as_str() {
                "shift" => KeyModifiers::SHIFT,
                "alt" => KeyModifiers::ALT,
                "control" => KeyModifiers::CONTROL,
                "super" => KeyModifiers::SUPER,
                "hyper" => KeyModifiers::HYPER,
                "meta" => KeyModifiers::META,
                _ => return Err(invalid(&format!("unknown selection modifier {name:?}"))),
            };
            Ok(modifiers | modifier)
        })
}

fn parse_overrides(json: &str) -> Result<Overrides, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|error| invalid(&error.to_string()))?;
    let object = value
        .as_object()
        .ok_or_else(|| invalid("settings must be a JSON object"))?;
    if object.values().any(serde_json::Value::is_null) {
        return Err(invalid(
            "settings values cannot be null; omit a key to keep its default",
        ));
    }
    serde_json::from_value(value).map_err(|error| invalid(&error.to_string()))
}

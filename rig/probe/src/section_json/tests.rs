use super::*;

#[test]
fn write_record_encodes_raw_bytes_as_lowercase_hex() {
    let write_line = format_write_record("identity:xtversion", b"\x1b[>0q\x1b[c");
    assert_eq!(
        write_line,
        r#"{"kind":"write","label":"identity:xtversion","hex":"1b5b3e30711b5b63"}"#
    );
}

#[test]
fn identity_section_serializes_environment_and_xtversion_fields() {
    let snapshot = EnvironmentSnapshot {
        os: "macos",
        arch: "aarch64",
        term: Some("xterm-256color".to_owned()),
        term_program: Some("Orca".to_owned()),
        term_program_version: Some("1.4.0".to_owned()),
        is_tmux: false,
        is_ssh: false,
        has_no_color: true,
        orca_version: Some("1.4.0".to_owned()),
        is_stdin_tty: true,
        is_stdout_tty: true,
    };
    let identity_line = format_identity_section(&snapshot, true, Some("xterm.js(6.0.0)"), false);
    assert!(identity_line.contains(r#""section":"identity""#));
    assert!(identity_line.contains(r#""xtversion":"xterm.js(6.0.0)""#));
}

#[test]
fn mouse_modes_section_marks_expected_m1_and_m5_states() {
    let after_m1 = [
        ModeReport {
            mode: 1000,
            state: ModeReportState::Reset,
        },
        ModeReport {
            mode: 1002,
            state: ModeReportState::Set,
        },
        ModeReport {
            mode: 1003,
            state: ModeReportState::Reset,
        },
        ModeReport {
            mode: 1006,
            state: ModeReportState::Set,
        },
    ];
    let after_m5 = [1000, 1002, 1003, 1006].map(|mode| ModeReport {
        mode,
        state: ModeReportState::Reset,
    });
    let mouse_line = format_mouse_modes_section(&[], &after_m1, &after_m5, false);
    assert!(mouse_line.contains(r#""is_m1_expected":true,"is_m5_reset":true"#));
}

#[test]
fn kitty_section_reports_answered_when_any_flag_query_succeeds() {
    let kitty_line = format_kitty_section(Some(0), Some(1), Some(0), false);
    assert!(kitty_line.contains(r#""answered":true,"initial_flags":0,"pushed_flags":1"#));
}

#[test]
fn modes_section_serializes_enable_and_disable_states() {
    let mode_line = format_modes_section(
        &[SingleModeProbe {
            mode: 2004,
            name: "bracketed_paste",
            after_enable: Some(ModeReportState::Set),
            after_disable: Some(ModeReportState::Reset),
        }],
        false,
    );
    assert!(mode_line.contains(r#""name":"bracketed_paste","after_enable":"set""#));
}

#[test]
fn cell_size_section_serializes_answered_dimensions_and_null_fallback() {
    let cell_line = format_cell_size_section(
        Some(CellPixelSize {
            height_px: 18,
            width_px: 9,
        }),
        false,
    );
    assert!(cell_line.contains(r#""answered":true,"height_px":18,"width_px":9"#));
    assert!(format_cell_size_section(None, false).contains(r#""answered":false"#));
}

#[test]
fn widths_section_serializes_sample_name_category_and_cells() {
    let widths_line = format_widths_section(
        &[MeasuredWidthSample {
            name: "ascii_a",
            category: "ascii_and_accents",
            cells: Some(1),
        }],
        false,
    );
    assert!(widths_line.contains(r#"{"name":"ascii_a","category":"ascii_and_accents","cells":1}"#));
}

#[test]
fn osc52_section_serializes_emission_and_form_flags() {
    let osc52_line = format_osc52_section(true, true, false);
    assert!(osc52_line.contains(r#""emitted":true,"plain_sent":true,"tmux_sent":false"#));
}

#[test]
fn background_color_section_serializes_answered_and_timeout_variants() {
    let color_reply = ColorReply {
        r: 26,
        g: 27,
        b: 38,
        channels: ["1a1a".to_owned(), "1b1b".to_owned(), "2626".to_owned()],
        luminance: 0.012_345,
        is_dark: true,
    };
    let color_line =
        format_background_color_section("background_color", "initial", 14, Some(&color_reply));
    assert!(color_line.contains(r#""answered":true,"phase":"initial","elapsed_ms":14"#));
    assert!(color_line.contains(r#""is_dark":true"#));

    let timeout_line = format_background_color_section("background_color", "timeout", 2_000, None);
    assert!(timeout_line.contains(r#""answered":false,"phase":"timeout","elapsed_ms":2000"#));
}

#[test]
fn summary_record_serializes_mode_and_control_descriptions() {
    let summary_line = format_summary_record("batch", "Ctrl+C", "F12");
    assert_eq!(
        summary_line,
        r#"{"kind":"summary","mode":"batch","exit_chord":"Ctrl+C","panic_key":"F12"}"#
    );
}

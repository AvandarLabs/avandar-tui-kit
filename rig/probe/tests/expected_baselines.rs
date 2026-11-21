//! Schema, completeness, and empirical invariant tests for all 13 `rig/expected/*.json` baselines.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use rig_probe::widths::WIDTH_SAMPLES;

const EXPECTED_BASELINE_FILES: &[&str] = &[
    "iterm2-tmux-cc.json",
    "iterm2-tmux-full.json",
    "iterm2-tmux-stock.json",
    "iterm2.json",
    "orca-tmux.json",
    "orca.json",
    "xterm-linux.json",
    "xterm-macos.json",
    "xterm-tmux-full-linux.json",
    "xterm-tmux-full-macos.json",
    "xterm-tmux-stock-linux.json",
    "xterm-tmux-stock-macos.json",
    "xterm-windows.json",
];

const REQUIRED_TOP_LEVEL_KEYS: &[&str] = &[
    "background_color",
    "cell_size",
    "identity",
    "interactive",
    "kitty_keyboard",
    "modes",
    "mouse_modes",
    "osc52",
    "widths",
];

#[derive(Debug, Clone, PartialEq)]
enum JsonValue {
    Null,
    Bool(bool),
    Number(i64),
    Str(String),
    Array(Vec<Self>),
    Object(BTreeMap<String, Self>),
}

impl JsonValue {
    const fn as_object(&self) -> Option<&BTreeMap<String, Self>> {
        if let Self::Object(map) = self {
            Some(map)
        } else {
            None
        }
    }

    const fn as_str(&self) -> Option<&str> {
        if let Self::Str(value) = self {
            Some(value.as_str())
        } else {
            None
        }
    }

    const fn as_bool(&self) -> Option<bool> {
        if let Self::Bool(value) = self {
            Some(*value)
        } else {
            None
        }
    }

    const fn as_i64(&self) -> Option<i64> {
        if let Self::Number(value) = self {
            Some(*value)
        } else {
            None
        }
    }
}

struct JsonParser<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> JsonParser<'a> {
    const fn new(source: &'a str) -> Self {
        Self {
            bytes: source.as_bytes(),
            cursor: 0,
        }
    }

    fn skip_whitespace(&mut self) {
        while self.cursor < self.bytes.len() && self.bytes[self.cursor].is_ascii_whitespace() {
            self.cursor += 1;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.skip_whitespace();
        self.bytes.get(self.cursor).copied()
    }

    fn expect_byte(&mut self, expected: u8) {
        assert_eq!(self.peek().expect("unexpected EOF"), expected);
        self.cursor += 1;
    }

    fn parse_string(&mut self) -> String {
        self.expect_byte(b'"');
        let mut decoded = String::new();
        while self.cursor < self.bytes.len() {
            let current = self.bytes[self.cursor];
            self.cursor += 1;
            if current == b'"' {
                return decoded;
            }
            if current == b'\\' {
                decoded.push(self.parse_escape());
            } else {
                let tail = std::str::from_utf8(&self.bytes[self.cursor - 1..]).expect("UTF-8");
                let first_char = tail.chars().next().expect("char");
                decoded.push(first_char);
                self.cursor += first_char.len_utf8() - 1;
            }
        }
        panic!("unterminated JSON string");
    }

    fn parse_escape(&mut self) -> char {
        let escaped = self.bytes[self.cursor];
        self.cursor += 1;
        match escaped {
            b'"' => '"',
            b'\\' => '\\',
            b'/' => '/',
            b'n' => '\n',
            b'r' => '\r',
            b't' => '\t',
            b'u' => {
                let hex = std::str::from_utf8(&self.bytes[self.cursor..self.cursor + 4]).unwrap();
                self.cursor += 4;
                char::from_u32(u32::from_str_radix(hex, 16).unwrap()).unwrap()
            }
            other => panic!("unsupported JSON escape '\\{}'", char::from(other)),
        }
    }

    fn parse_array(&mut self) -> JsonValue {
        self.expect_byte(b'[');
        let mut items = Vec::new();
        if self.peek() != Some(b']') {
            loop {
                items.push(self.parse_value());
                if self.peek() != Some(b',') {
                    break;
                }
                self.cursor += 1;
            }
        }
        self.expect_byte(b']');
        JsonValue::Array(items)
    }

    fn parse_object(&mut self) -> JsonValue {
        self.expect_byte(b'{');
        let mut entries = BTreeMap::new();
        if self.peek() != Some(b'}') {
            loop {
                let key = self.parse_string();
                self.expect_byte(b':');
                assert!(entries.insert(key, self.parse_value()).is_none());
                if self.peek() != Some(b',') {
                    break;
                }
                self.cursor += 1;
            }
        }
        self.expect_byte(b'}');
        JsonValue::Object(entries)
    }

    fn parse_value(&mut self) -> JsonValue {
        match self.peek().expect("unexpected EOF") {
            b'"' => JsonValue::Str(self.parse_string()),
            b'[' => self.parse_array(),
            b'{' => self.parse_object(),
            b't' => {
                self.cursor += 4;
                JsonValue::Bool(true)
            }
            b'f' => {
                self.cursor += 5;
                JsonValue::Bool(false)
            }
            b'n' => {
                self.cursor += 4;
                JsonValue::Null
            }
            _ => {
                self.skip_whitespace();
                let start = self.cursor;
                if self.bytes.get(self.cursor) == Some(&b'-') {
                    self.cursor += 1;
                }
                while self.cursor < self.bytes.len() && self.bytes[self.cursor].is_ascii_digit() {
                    self.cursor += 1;
                }
                let text = std::str::from_utf8(&self.bytes[start..self.cursor]).unwrap();
                JsonValue::Number(text.parse::<i64>().unwrap())
            }
        }
    }
}

fn parse_json_document(raw_text: &str) -> JsonValue {
    let mut parser = JsonParser::new(raw_text);
    let parsed = parser.parse_value();
    parser.skip_whitespace();
    assert_eq!(parser.cursor, parser.bytes.len());
    parsed
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn load_baseline_object(file_name: &str) -> BTreeMap<String, JsonValue> {
    let file_path = repository_root().join("rig/expected").join(file_name);
    let raw_text = fs::read_to_string(&file_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", file_path.display()));
    assert!(!raw_text.contains('\u{2014}'), "{file_name} has em dash");
    parse_json_document(&raw_text)
        .as_object()
        .expect("root JSON object")
        .clone()
}

fn assert_widths_section(file_name: &str, widths_value: &JsonValue) {
    let widths_obj = widths_value.as_object().expect("widths object");
    assert_eq!(
        widths_obj["all_toolkit_glyphs_single_cell"].as_bool(),
        Some(true)
    );
    let samples_obj = widths_obj["samples"].as_object().expect("samples object");
    assert_eq!(samples_obj.len(), WIDTH_SAMPLES.len());
    for sample in WIDTH_SAMPLES {
        let cells = samples_obj[sample.name].as_i64().expect(sample.name);
        if sample.category == "toolkit_glyph" {
            assert_eq!(cells, 1, "{file_name}: {}", sample.name);
        } else if sample.category == "east_asian" {
            assert_eq!(cells, 2, "{file_name}: {}", sample.name);
        }
    }
}

fn assert_identity_and_sections(file_name: &str, root_obj: &BTreeMap<String, JsonValue>) {
    let actual_keys: Vec<&str> = root_obj.keys().map(String::as_str).collect();
    assert_eq!(actual_keys, REQUIRED_TOP_LEVEL_KEYS, "{file_name}");
    let identity = root_obj["identity"].as_object().expect("identity");
    for forbidden_key in [
        "arch",
        "has_no_color",
        "is_ssh",
        "orca_version",
        "term_program_version",
    ] {
        assert!(!identity.contains_key(forbidden_key), "{file_name}");
    }
    assert_eq!(identity["is_stdin_tty"].as_bool(), Some(true));
    assert_eq!(identity["is_stdout_tty"].as_bool(), Some(true));
    let bg = root_obj["background_color"].as_object().expect("bg");
    assert!(!bg.contains_key("elapsed_ms"));
    assert!(bg["answered"].as_bool().is_some());
    assert!(
        root_obj["cell_size"].as_object().expect("cell_size")["answered"]
            .as_bool()
            .is_some()
    );
    assert_widths_section(file_name, &root_obj["widths"]);
}

#[test]
fn expected_directory_contains_exact_thirteen_valid_baselines() {
    let expected_dir = repository_root().join("rig/expected");
    let mut actual_files: Vec<String> = fs::read_dir(&expected_dir)
        .unwrap_or_else(|err| panic!("missing {}: {err}", expected_dir.display()))
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    actual_files.sort();
    assert_eq!(actual_files, EXPECTED_BASELINE_FILES);
    for file_name in EXPECTED_BASELINE_FILES {
        assert_identity_and_sections(file_name, &load_baseline_object(file_name));
    }
}

#[test]
fn expected_baselines_preserve_empirical_terminal_invariants() {
    let pkg_raw = fs::read_to_string(repository_root().join("rig/xterm/package.json")).unwrap();
    let pinned_xterm = parse_json_document(&pkg_raw).as_object().unwrap()["dependencies"]
        .as_object()
        .unwrap()["@xterm/xterm"]
        .as_str()
        .unwrap()
        .to_owned();

    let orca = load_baseline_object("orca.json");
    assert_eq!(
        orca["identity"].as_object().unwrap()["xtversion"].as_str(),
        Some(format!("xterm.js({pinned_xterm})").as_str())
    );
    assert_eq!(orca["osc52"], JsonValue::Null);
    assert_eq!(orca["interactive"], JsonValue::Null);
    assert_eq!(
        load_baseline_object("orca-tmux.json")["interactive"],
        JsonValue::Null
    );

    let iterm2 = load_baseline_object("iterm2.json");
    assert_eq!(
        iterm2["identity"].as_object().unwrap()["xtversion"].as_str(),
        Some("iTerm2 3.7.7")
    );
    assert_eq!(
        iterm2["kitty_keyboard"].as_object().unwrap()["answered"].as_bool(),
        Some(true)
    );

    let win = load_baseline_object("xterm-windows.json");
    let win_paste = win["interactive"].as_object().unwrap()["paste"]
        .as_object()
        .unwrap();
    assert_eq!(win_paste["hasPasteEvent"].as_bool(), Some(false));
    assert_eq!(win_paste["keyPressCount"].as_i64(), Some(10));
    assert_eq!(win_paste["arrivedInSingleBurst"].as_bool(), Some(true));
}

#[test]
fn ci_workflow_verifies_all_seven_ci_hosted_expected_baselines() {
    let root = repository_root();
    for script_rel in [
        "rig/xterm/src/expected_compare.mjs",
        "rig/xterm/src/expected_normalize.mjs",
    ] {
        let script_text = fs::read_to_string(root.join(script_rel)).unwrap();
        assert!(!script_text.contains('\u{2014}'));
        assert!(!script_text.contains(concat!("sl", "eep")));
    }

    let ci_contents = fs::read_to_string(root.join(".github/workflows/ci.yml")).unwrap();
    for expected_ci_baseline in [
        "rig/expected/xterm-linux.json",
        "rig/expected/xterm-tmux-stock-linux.json",
        "rig/expected/xterm-tmux-full-linux.json",
        "rig/expected/xterm-macos.json",
        "rig/expected/xterm-tmux-stock-macos.json",
        "rig/expected/xterm-tmux-full-macos.json",
        "rig/expected/xterm-windows.json",
    ] {
        assert!(ci_contents.contains(expected_ci_baseline));
    }
}

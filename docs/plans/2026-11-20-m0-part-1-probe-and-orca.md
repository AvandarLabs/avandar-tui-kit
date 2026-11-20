# Milestone 0 (Part 1: Capability Probe, Target Lint, and Orca Runner) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the unpublished `rig/probe` workspace crate (`capability_probe` binary, pure reply parsers, width sample catalog, OSC 11 `Event` parser, and Windows `.cmd` shim check), add Layer 2 (`just lint-targets`), and add Layer 5 (`just orca`) verified against a live unfocused Orca terminal.

**Architecture:** `rig/probe` separates all escape-sequence builders, reply parsers (`DECRPM`, `XTVERSION`, Kitty keyboard flags, cursor position, cell pixel size, `DA1` fence), OSC 11 `crossterm::event::Event` state machine, JSONL formatting, and environment/width calculations into pure library modules tested in `cargo test`. Thin `#[cfg(unix)]` / `#[cfg(windows)]` I/O glue runs raw queries fenced by `DA1` (`CSI c`) with zero fixed sleeps, then switches to `crossterm` event polling for the 200 ms / 2,000 ms OSC 11 listen and interactive input logging. `rig/orca/run.sh` drives `capability_probe` in an unfocused Orca terminal via `orca terminal create` and `orca terminal wait --for exit`.

**Tech Stack:** Rust 1.88+ (edition 2024), `crossterm` 0.29 (the only external crate dependency, inherited from `[workspace.dependencies]`), POSIX `sh` + `orca` CLI for Layer 5, `just` task recipes.

**Spec:** `docs/milestones.md` (Milestone 0), `docs/specs/architecture.md`, `docs/specs/ratatui-mouse.md` (rules M1-M8), `docs/testing.md`, `docs/terminals.md`.

## Global Constraints

- `rust-version = "1.88"`, `edition = "2024"`, `[lints] workspace = true` (`unsafe_code = "forbid"`, `missing_docs = "warn"`, `clippy::pedantic` and `clippy::nursery` denied via `-D warnings`).
- Zero new entries in `[workspace.dependencies]`: `rig/probe` uses `std` and `crossterm.workspace = true` only (`publish = false`).
- One module per file, every production `.rs` file <= 400 lines, every function <= 45 lines (`AGENTS.md`, `docs/rules/rust.md`).
- No em dashes anywhere in comments, docs, strings, or commit messages (`AGENTS.md`).
- Wait on terminal conditions (`DA1` fence reply, `.ready` file, `orca terminal wait --for exit`), never on fixed sleeps (`docs/testing.md` rule 3).
- Mouse setup writes `CSI ?1000h`, `CSI ?1002h`, `CSI ?1006h` in order and never `1003` (rule M1/M2); mouse teardown writes `CSI ?9l`, `?1000l`, `?1002l`, `?1003l`, `?1005l`, `?1006l`, `?1015l`, `?1016l` (rule M5).
- Every platform-specific `#[cfg]` module or test has a counterpart so `just lint-targets` passes cleanly for `x86_64-pc-windows-msvc` and `x86_64-unknown-linux-gnu`.
- When all four parts of Milestone 0 finish, delete the completed plans from `docs/plans/` (`AGENTS.md`).

## Review Focus

1. **Uppercase hex digits in OSC 11 replies carry `KeyModifiers::SHIFT` in `crossterm`:** `crossterm` adds `SHIFT` to `KeyCode::Char('A'..='F')`; `ColorReplyParser` must accept both lowercase and `SHIFT`-modified uppercase hex digits (`1` to `4` digits per channel) and normalize them to identical `u8` channels (pinned in Task 3).
2. **Split `Esc` + `]` and `Esc` + `\` across reads or Windows Console records in `ColorReplyParser`:** OSC 11 may begin with `(Esc, NONE)` then `(']', NONE)` instead of `(']', ALT)`, and end with `(Esc, NONE)` then `('\\', NONE)` instead of `('\\', ALT)`; both must succeed, while `(Esc, NONE)` followed by `'x'` must return `ColorStep::Rejected` with both events in original order (pinned in Task 3).
3. **Unanswered queries coalesced ahead of the `DA1` fence in `tmux`:** `tmux` ignores `CSI ? u` and `CSI 16 t` while coalescing `DECRPM` answers with the trailing `DA1` (`CSI ? ... c`) reply into one buffer; parsers must extract every present reply from a coalesced byte slice and return `None` for missing replies once `has_da1_reply` is true (pinned in Task 2).
4. **Teardown on panic (`F12`) restores every terminal mode before printing:** Triggering the documented panic key must run teardown (M5 mouse reset, kitty pop if pushed, paste/focus/sync reset, raw mode off) before the panic hook finishes (pinned in Task 5).
5. **Burst key counting for Windows paste measurement (M0 row 16):** Draining immediately available events (`poll(Duration::ZERO)`) must record `burst_len` accurately for multi-character paste bursts without dropping or reordering trailing non-key events (pinned in Task 5).

---

### File Structure

- Modify: `Cargo.toml` (add `"rig/probe"` to `workspace.members`)
- Modify: `justfile` (add `lint-targets` and `orca` recipes)
- Create: `rig/probe/Cargo.toml` (`name = "rig-probe"`, `publish = false`)
- Create: `rig/probe/src/lib.rs` (crate docs and module exports)
- Create: `rig/probe/src/sequences.rs` (escape-sequence constants, `DECRPM` query builder, base64 + plain/tmux OSC 52 builders)
- Create: `rig/probe/src/parsers.rs` (pure byte parsers for `DECRPM`, `XTVERSION`, Kitty flags, cursor position, cell pixel size, and `DA1` fence)
- Create: `rig/probe/src/color_reply.rs` (pure `crossterm::event::Event` parser for OSC 11, RGB channel scaling, WCAG luminance + `0.179` dark threshold)
- Create: `rig/probe/src/widths.rs` (canonical width sample catalog for M0 rows 19-20 + cell-width delta calculation)
- Create: `rig/probe/src/environment.rs` (pure environment snapshot reader)
- Create: `rig/probe/src/json_log.rs` (dependency-free JSON escaping, JSONL event/section logger, `.ready` marker creation)
- Create: `rig/probe/src/cmd_shim.rs` (Windows `.cmd` argument quoting builder + `#[cfg(windows)]` `cmd.exe /D /S /C` execution test and Unix sibling test for M0 row 18)
- Create: `rig/probe/src/probe_args.rs` (pure CLI argument parser for `--log`, `--batch`, `--osc52`)
- Create: `rig/probe/src/terminal_io.rs` (`#[cfg(unix)]` / `#[cfg(windows)]` raw query-and-fence transport, teardown guard, burst-aware event drain)
- Create: `rig/probe/src/probe_runner.rs` (executes probe sections in order, logs JSONL records, writes `.ready`, runs interactive loop with `Ctrl+C` exit and `F12` panic)
- Create: `rig/probe/examples/capability_probe.rs` (thin CLI entrypoint calling `rig_probe::probe_runner::run_from_env()`)
- Create: `rig/orca/run.sh` (Layer 5 script creating an unfocused Orca terminal, waiting on exit, closing tab, verifying `.ready` and `summary` records)

---

### Task 1: Scaffold `rig/probe`, Layer 2 (`just lint-targets`), and Escape-Sequence Builders

**Files:**
- Modify: `Cargo.toml:5-7`
- Modify: `justfile:33-38`
- Create: `rig/probe/Cargo.toml`
- Create: `rig/probe/src/lib.rs`
- Create: `rig/probe/src/sequences.rs`

**Interfaces:**
- Consumes: `[workspace.package]`, `[workspace.dependencies]`, `[workspace.lints]` in root `Cargo.toml`.
- Produces (`rig_probe::sequences`):
  - Constants (`&[u8]`): `XTVERSION_QUERY`, `DA1_FENCE_QUERY`, `KITTY_KEYBOARD_QUERY`, `KITTY_KEYBOARD_PUSH_DISAMBIGUATE`, `KITTY_KEYBOARD_POP`, `CELL_SIZE_PIXELS_QUERY`, `CURSOR_POSITION_QUERY`, `OSC_11_BACKGROUND_QUERY`, `MOUSE_SETUP_M1`, `MOUSE_TEARDOWN_M5`, `BRACKETED_PASTE_ON`, `BRACKETED_PASTE_OFF`, `FOCUS_EVENTS_ON`, `FOCUS_EVENTS_OFF`, `SYNC_OUTPUT_ON`, `SYNC_OUTPUT_OFF`.
  - Functions:
    - `pub fn encode_base64(input: &[u8]) -> String`
    - `pub fn build_decrpm_query(mode: u16) -> Vec<u8>`
    - `pub fn build_osc52_plain(payload: &[u8]) -> Vec<u8>`
    - `pub fn build_osc52_tmux(payload: &[u8]) -> Vec<u8>`

- [ ] **Step 1: Wire `rig/probe` into the workspace, add `lint-targets` to `justfile`, and write failing sequence tests**

Add `"rig/probe"` to `members` in `Cargo.toml`, add `lint-targets` to `justfile`:

```just
# Lint Windows and Linux targets from the development machine (Layer 2).
lint-targets:
    cargo clippy --workspace --all-targets --all-features --target x86_64-pc-windows-msvc --target x86_64-unknown-linux-gnu -- -D warnings
```

Create `rig/probe/Cargo.toml` (`name = "rig-probe"`, `publish = false`, inheriting workspace package/lints and `crossterm.workspace = true`), `rig/probe/src/lib.rs` exporting `pub mod sequences;`, and `rig/probe/src/sequences.rs` with stub functions and unit tests asserting:

```rust
#[test]
fn mouse_setup_and_teardown_follow_rules_m1_m2_and_m5() {
    assert_eq!(MOUSE_SETUP_M1, b"\x1b[?1000h\x1b[?1002h\x1b[?1006h");
    assert!(!MOUSE_SETUP_M1.windows(5).any(|w| w == b"1003h"));
    assert_eq!(
        MOUSE_TEARDOWN_M5,
        b"\x1b[?9l\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1005l\x1b[?1006l\x1b[?1015l\x1b[?1016l"
    );
}

#[test]
fn builds_decrpm_and_osc52_sequences_with_padding() {
    assert_eq!(build_decrpm_query(1002), b"\x1b[?1002$p");
    assert_eq!(encode_base64(b""), "");
    assert_eq!(encode_base64(b"f"), "Zg==");
    assert_eq!(encode_base64(b"fo"), "Zm8=");
    assert_eq!(encode_base64(b"foo"), "Zm9v");
    assert_eq!(build_osc52_plain(b"probe-1"), b"\x1b]52;c;cHJvYmUtMQ==\x07");
    assert_eq!(
        build_osc52_tmux(b"probe-1"),
        b"\x1bPtmux;\x1b\x1b]52;c;cHJvYmUtMQ==\x07\x1b\\"
    );
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p rig-probe`
Expected: FAIL (`assertion left == right failed` on stub return values).

- [ ] **Step 3: Implement `rig/probe/src/sequences.rs`**

Define the exact byte constants (`XTVERSION_QUERY = b"\x1b[>0q"`, `DA1_FENCE_QUERY = b"\x1b[c"`, `KITTY_KEYBOARD_QUERY = b"\x1b[?u"`, `KITTY_KEYBOARD_PUSH_DISAMBIGUATE = b"\x1b[>1u"`, `KITTY_KEYBOARD_POP = b"\x1b[<u"`, `CELL_SIZE_PIXELS_QUERY = b"\x1b[16t"`, `CURSOR_POSITION_QUERY = b"\x1b[6n"`, `OSC_11_BACKGROUND_QUERY = b"\x1b]11;?\x07"`, plus mode toggles) and implement `encode_base64`, `build_decrpm_query`, `build_osc52_plain`, and `build_osc52_tmux`.

- [ ] **Step 4: Run `just check` and `just lint-targets`**

Run: `just check && just lint-targets`
Expected: PASS with 0 warnings across native, Windows, and Linux targets.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock justfile rig/probe/Cargo.toml rig/probe/src/lib.rs rig/probe/src/sequences.rs
git commit -m "feat(probe): scaffold rig-probe crate, lint-targets recipe, and sequence builders"
```

---

### Task 2: Pure Raw-Reply Parsers (`DECRPM`, `XTVERSION`, Kitty Flags, Cursor, Cell Size, `DA1` Fence)

**Files:**
- Modify: `rig/probe/src/lib.rs`
- Create: `rig/probe/src/parsers.rs`

**Interfaces:**
- Produces (`rig_probe::parsers`):
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum ModeReportState { NotRecognized, Set, Reset, PermanentlySet, PermanentlyReset, Unknown(u16) }` (with `pub const fn from_code(code: u16) -> Self` and `pub const fn as_str(self) -> &'static str`)
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct ModeReport { pub mode: u16, pub state: ModeReportState }`
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct CursorPosition { pub row: u16, pub column: u16 }`
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct CellPixelSize { pub height_px: u16, pub width_px: u16 }`
  - `pub fn parse_mode_reports(bytes: &[u8]) -> Vec<ModeReport>`
  - `pub fn parse_xtversion_reply(bytes: &[u8]) -> Option<String>`
  - `pub fn parse_kitty_flags_replies(bytes: &[u8]) -> Vec<u16>`
  - `pub fn parse_cursor_position(bytes: &[u8]) -> Option<CursorPosition>`
  - `pub fn parse_cell_pixel_size(bytes: &[u8]) -> Option<CellPixelSize>`
  - `pub fn has_da1_reply(bytes: &[u8]) -> bool`

- [ ] **Step 1: Write failing unit tests in `rig/probe/src/parsers.rs` (including Review Focus #3 coalesced tmux buffer)**

```rust
#[test]
fn parses_coalesced_mode_reports_and_missing_optional_replies_before_da1_fence() {
    // Simulates tmux: answers DECRPM for 1000/1002/1006, ignores Kitty (?u) and cell size (16t),
    // then answers DA1 (\x1b[?1;2c).
    let buf = b"\x1b[?1000;2$y\x1b[?1002;1$y\x1b[?1006;1$y\x1b[?1;2c";
    assert!(has_da1_reply(buf));
    assert_eq!(
        parse_mode_reports(buf),
        vec![
            ModeReport { mode: 1000, state: ModeReportState::Reset },
            ModeReport { mode: 1002, state: ModeReportState::Set },
            ModeReport { mode: 1006, state: ModeReportState::Set },
        ]
    );
    assert!(parse_kitty_flags_replies(buf).is_empty());
    assert_eq!(parse_cell_pixel_size(buf), None);
}

#[test]
fn parses_xtversion_kitty_cursor_and_cell_size_replies() {
    let buf = b"\x1bP>|xterm.js(6.0.0)\x1b\\\x1b[?0u\x1b[?1u\x1b[6;18;9t\x1b[1;3R\x1b[?62;22c";
    assert_eq!(parse_xtversion_reply(buf).as_deref(), Some("xterm.js(6.0.0)"));
    assert_eq!(parse_kitty_flags_replies(buf), vec![0, 1]);
    assert_eq!(parse_cell_pixel_size(buf), Some(CellPixelSize { height_px: 18, width_px: 9 }));
    assert_eq!(parse_cursor_position(buf), Some(CursorPosition { row: 1, column: 3 }));
    assert!(has_da1_reply(buf));
}

#[test]
fn incomplete_da1_or_malformed_csi_does_not_false_trigger_fence() {
    assert!(!has_da1_reply(b"\x1b[?1002;1$y\x1b[?62;"));
    assert!(!has_da1_reply(b"\x1b[?u"));
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p rig-probe`
Expected: FAIL on stubbed parser outputs.

- [ ] **Step 3: Implement `rig/probe/src/parsers.rs`**

Scan `bytes` cleanly without panicking or `unwrap`:
- `parse_mode_reports`: match `\x1b[?`, `<u16>;<u16>$y`.
- `parse_xtversion_reply`: match `\x1bP>|` through `\x1b\\`, converting body via `String::from_utf8_lossy`.
- `parse_kitty_flags_replies`: match `\x1b[?` followed by `<u16>` (optionally `;<u16>`) and final byte `u`.
- `parse_cursor_position`: match `\x1b[` followed by `<u16>;<u16>R` (return the latest match in `bytes` so sequential cursor reads use the newest report).
- `parse_cell_pixel_size`: match `\x1b[6;<u16>;<u16>t`.
- `has_da1_reply`: match `\x1b[?` followed by one or more ASCII digits/semicolons and final byte `c`.

- [ ] **Step 4: Run `just check` and `just lint-targets`**

Run: `just check && just lint-targets`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add rig/probe/src/lib.rs rig/probe/src/parsers.rs
git commit -m "feat(probe): add pure byte parsers for terminal replies and DA1 fence"
```

---

### Task 3: Pure OSC 11 Background Color `Event` Parser and Luminance Math

**Files:**
- Modify: `rig/probe/src/lib.rs`
- Create: `rig/probe/src/color_reply.rs`

**Interfaces:**
- Consumes: `crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers}`
- Produces (`rig_probe::color_reply`):
  - `pub const INITIAL_COLOR_WAIT_MS: u64 = 200;`
  - `pub const ARMED_COLOR_WINDOW_MS: u64 = 2_000;`
  - `pub const DARK_LUMINANCE_THRESHOLD: f64 = 0.179;`
  - `#[derive(Debug, Clone, PartialEq)] pub struct ColorReply { pub r: u8, pub g: u8, pub b: u8, pub channels: [String; 3], pub luminance: f64, pub is_dark: bool }`
  - `#[derive(Debug, Clone, PartialEq)] pub enum ColorStep { Idle(Event), Buffering, Complete(ColorReply), Rejected(Vec<Event>) }`
  - `#[derive(Debug, Default, Clone)] pub struct ColorReplyParser` with:
    - `pub fn new() -> Self`
    - `pub fn feed(&mut self, event: Event) -> ColorStep`
    - `pub fn flush_incomplete(&mut self) -> Vec<Event>`
  - `pub fn relative_luminance(r: u8, g: u8, b: u8) -> f64`

- [ ] **Step 1: Write failing unit tests in `rig/probe/src/color_reply.rs` (including Review Focus #1 and #2)**

```rust
#[test]
fn parses_osc11_with_uppercase_shift_hex_and_bel_or_st_terminators() {
    // Review Focus #1: crossterm sets SHIFT on uppercase 'A'..'F'.
    let mut parser = ColorReplyParser::new();
    assert_eq!(parser.feed(Event::Key(KeyEvent::new(KeyCode::Char(']'), KeyModifiers::ALT))), ColorStep::Buffering);
    for ch in "11;rgb:FFFF/FFFF/FFFF".chars() {
        let mods = if ch.is_ascii_uppercase() { KeyModifiers::SHIFT } else { KeyModifiers::NONE };
        assert_eq!(parser.feed(Event::Key(KeyEvent::new(KeyCode::Char(ch), mods))), ColorStep::Buffering);
    }
    let step = parser.feed(Event::Key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL)));
    match step {
        ColorStep::Complete(reply) => {
            assert_eq!((reply.r, reply.g, reply.b), (255, 255, 255));
            assert!(!reply.is_dark);
            assert!(reply.luminance > DARK_LUMINANCE_THRESHOLD);
        }
        other => panic!("expected Complete, got {other:?}"),
    }
}

#[test]
fn parses_split_esc_prefix_and_st_terminator_and_restores_rejected_events_in_order() {
    // Review Focus #2: split Esc + ']' and Esc + '\' (Windows ConPTY / split reads),
    // plus dark Orca background (1, 2, 3, or 4 hex digits per channel).
    let mut parser = ColorReplyParser::new();
    assert_eq!(parser.feed(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))), ColorStep::Buffering);
    for ch in "]11;rgb:1a/1b/26".chars() {
        assert_eq!(parser.feed(Event::Key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE))), ColorStep::Buffering);
    }
    assert_eq!(parser.feed(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))), ColorStep::Buffering);
    match parser.feed(Event::Key(KeyEvent::new(KeyCode::Char('\\'), KeyModifiers::NONE))) {
        ColorStep::Complete(reply) => {
            assert_eq!((reply.r, reply.g, reply.b), (0x1a, 0x1b, 0x26));
            assert!(reply.is_dark);
        }
        other => panic!("expected Complete, got {other:?}"),
    }

    // Bare Esc followed by 'k' must emit Rejected([Esc, 'k']) in arrival order.
    let esc = Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    let k = Event::Key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
    assert_eq!(parser.feed(esc.clone()), ColorStep::Buffering);
    assert_eq!(parser.feed(k.clone()), ColorStep::Rejected(vec![esc, k]));
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p rig-probe`
Expected: FAIL on stubbed `ColorReplyParser`.

- [ ] **Step 3: Implement `rig/probe/src/color_reply.rs`**

Implement the state machine over `Event`:
- Ignore `KeyEventKind::Release` while buffering (on Windows every key press is paired with a release).
- Recognize start: `Alt+']'` or `Esc` followed by `']'`.
- Match exact prefix `"11;rgb:"`, then three hex channels of `1..=4` ASCII hex digits separated by `'/'` (accepting `NONE` or `SHIFT` modifiers on hex chars).
- Terminate on `Ctrl+'g'`, `Char('\x07')`, `Alt+'\\'`, or `Esc` followed by `'\\'`.
- Scale an `n`-digit hex value `v` (`n` in `1..=4`) to `u8` via `((u32::from(v) * 255 + max / 2) / max) as u8` where `max = (1_u32 << (4 * n)) - 1`.
- Compute WCAG relative luminance (`0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)`) and set `is_dark = luminance < DARK_LUMINANCE_THRESHOLD` (`0.179`).
- On any mismatch, reset to idle and return `ColorStep::Rejected(buffered_events_plus_current)`.

- [ ] **Step 4: Run `just check` and `just lint-targets`**

Run: `just check && just lint-targets`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add rig/probe/src/lib.rs rig/probe/src/color_reply.rs
git commit -m "feat(probe): add pure crossterm Event parser for OSC 11 color replies"
```

---

### Task 4: Width Sample Catalog, Environment Snapshot, JSONL Logger, CLI Args, and `.cmd` Shim Check

**Files:**
- Modify: `rig/probe/src/lib.rs`
- Create: `rig/probe/src/widths.rs`
- Create: `rig/probe/src/environment.rs`
- Create: `rig/probe/src/json_log.rs`
- Create: `rig/probe/src/probe_args.rs`
- Create: `rig/probe/src/cmd_shim.rs`

**Interfaces:**
- Produces:
  - `rig_probe::widths`:
    - `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct WidthSample { pub name: &'static str, pub category: &'static str, pub text: &'static str }`
    - `pub const WIDTH_SAMPLES: &[WidthSample]` (all rows of `terminals.md`, portable-text samples, M0 row 19 rainbow flag `"\u{1F3F3}\u{FE0F}\u{200D}\u{1F308}"`, and M0 row 20 toolkit drawing glyphs: `'…'`, `'·'`, `'›'`, `'▸'`, `'─'`, `'│'`, `'┌'`, `'├'`, `'└'`, `'▏'`, `'▍'`, `'▌'`, `'▊'`, `'█'`, `'⠁'`, `'⣿'`)
    - `pub const fn measured_cell_width(start_col: u16, end_col: u16) -> u16`
  - `rig_probe::environment`:
    - `#[derive(Debug, Clone, PartialEq, Eq)] pub struct EnvironmentSnapshot { pub os: &'static str, pub arch: &'static str, pub term: Option<String>, pub term_program: Option<String>, pub term_program_version: Option<String>, pub is_tmux: bool, pub is_ssh: bool, pub has_no_color: bool, pub orca_version: Option<String>, pub is_stdin_tty: bool, pub is_stdout_tty: bool }`
    - `pub fn read_environment_with<F: Fn(&str) -> Option<String>>(lookup: F, is_stdin_tty: bool, is_stdout_tty: bool) -> EnvironmentSnapshot`
  - `rig_probe::json_log`:
    - `pub fn escape_json_string(value: &str) -> String`
    - `pub fn ready_file_path(log_path: &std::path::Path) -> std::path::PathBuf` (`<path>.ready`)
    - `pub struct JsonLog` with `pub fn create(log_path: &std::path::Path) -> std::io::Result<Self>`, `pub fn write_line(&mut self, json_object_line: &str) -> std::io::Result<()>`, and `pub fn mark_ready(&mut self) -> std::io::Result<()>`
  - `rig_probe::probe_args`:
    - `#[derive(Debug, Clone, PartialEq, Eq)] pub struct ProbeArgs { pub log_path: std::path::PathBuf, pub is_batch: bool, pub osc52_token: Option<String> }`
    - `pub fn parse_probe_args<I, S>(args: I) -> Result<ProbeArgs, String> where I: IntoIterator<Item = S>, S: AsRef<str>`
  - `rig_probe::cmd_shim`:
    - `pub fn quote_cmd_arg(arg: &str) -> String`
    - `pub fn build_cmd_shim_invocation(shim_path: &std::path::Path, args: &[&str]) -> String`

- [ ] **Step 1: Write failing unit tests across `widths.rs`, `environment.rs`, `json_log.rs`, `probe_args.rs`, and `cmd_shim.rs`**

Assert:
- `WIDTH_SAMPLES` has unique names and includes `"rainbow_flag"`, `"heart_vs16"`, `"thumbs_up_skin_tone"`, `"zwsp"`, `"ellipsis"`, `"middle_dot"`, `"chevron"`, `"block_eighth_1"`, `"braille_full"`, `"box_horizontal"`, `"cjk_han"`, `"combining_acute"`.
- `read_environment_with` treats `NO_COLOR=""` as `has_no_color == false` and non-empty `NO_COLOR="1"` as `has_no_color == true` (`specs/architecture.md` Detection rule 2).
- `escape_json_string("a\"b\\c\n\x1b")` returns `"a\\\"b\\\\c\\n\\u001b"`, and `JsonLog::mark_ready` appends a `"kind":"ready"` JSON line and creates `<log_path>.ready`.
- `parse_probe_args(["--log", "/tmp/p.jsonl", "--batch", "--osc52", "tok"])` succeeds and missing `--log` returns `Err`.
- `cmd_shim`: pure test on all platforms verifies `build_cmd_shim_invocation` with spaces, quotes, `%`, `^`, `&`, and `!`; `#[cfg(windows)]` test writes a temp `.cmd` script echoing `%1`..`%6` to a file, executes `cmd.exe /D /S /C`, and asserts the exact arguments received.

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p rig-probe`
Expected: FAIL on stub implementations.

- [ ] **Step 3: Implement the five modules**

Keep each file crisp, documented, and free of `unwrap`/`expect` outside `#[cfg(test)]`.

- [ ] **Step 4: Run `just check` and `just lint-targets`**

Run: `just check && just lint-targets`
Expected: PASS (confirming both `#[cfg(windows)]` and Unix branches of `cmd_shim.rs` compile and lint cleanly under `-D warnings`).

- [ ] **Step 5: Commit**

```bash
git add rig/probe/src/
git commit -m "feat(probe): add width samples, env snapshot, JSONL logger, CLI parser, and cmd shim check"
```

---

### Task 5: Terminal I/O Transport, `capability_probe` Binary, and Panic/Drop Teardown

**Files:**
- Modify: `rig/probe/src/lib.rs`
- Create: `rig/probe/src/terminal_io.rs`
- Create: `rig/probe/src/probe_runner.rs`
- Create: `rig/probe/examples/capability_probe.rs`

**Interfaces:**
- Consumes: `sequences`, `parsers`, `color_reply`, `widths`, `environment`, `json_log`, `probe_args`.
- Produces:
  - `rig_probe::terminal_io`:
    - `pub struct TerminalSessionGuard` (tracks active modes and restores M5 mouse teardown, kitty pop if pushed, paste/focus/sync off, and raw mode off on `Drop`, plus installs a once-only panic hook writing the same reset bytes before panic output).
    - `pub fn drain_event_burst<F>(first: Event, poll_zero: F) -> (Vec<Event>, usize)` where `F: FnMut() -> std::io::Result<Option<Event>>` (pure burst collector returning all drained events and the count of consecutive `KeyEventKind::Press` key events in the burst, for Review Focus #5 / M0 row 16).
    - Platform query runner (`#[cfg(unix)]` raw `/dev/tty` or `stdin` read until `has_da1_reply` with a safety failure timeout of `2_000 ms`; `#[cfg(windows)]` console query/cursor width fallback recording unsupported raw CSI replies cleanly).
  - `rig_probe::probe_runner`:
    - `pub const EXIT_CHORD_DESC: &str = "Ctrl+C";`
    - `pub const PANIC_KEY_DESC: &str = "F12";`
    - `#[derive(Debug, Clone, PartialEq, Eq)] pub enum LoopControl { Continue, CleanExit, TriggerPanic }`
    - `pub fn classify_probe_control(event: &Event) -> LoopControl`
    - `pub fn build_teardown_bytes(was_kitty_pushed: bool) -> Vec<u8>`
    - `pub fn run_from_env() -> std::process::ExitCode`

- [ ] **Step 1: Write failing tests for burst draining, control-key classification, and panic/drop teardown bytes (Review Focus #4 and #5)**

```rust
#[test]
fn drain_event_burst_counts_consecutive_press_keys_and_preserves_trailing_events() {
    // Review Focus #5: paste burst followed by FocusLost must report burst_len == 3 and keep FocusLost.
    let mut queued = VecDeque::from([
        Ok(Some(Event::Key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE)))),
        Ok(Some(Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE)))),
        Ok(Some(Event::FocusLost)),
        Ok(None),
    ]);
    let first = Event::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
    let (events, burst_len) = drain_event_burst(first, || queued.pop_front().unwrap_or(Ok(None))).unwrap();
    assert_eq!(burst_len, 3);
    assert_eq!(events.len(), 4);
    assert_eq!(events[3], Event::FocusLost);
}

#[test]
fn teardown_bytes_and_control_keys_cover_clean_exit_and_panic_paths() {
    // Review Focus #4: teardown resets M5 mouse modes, paste, focus, sync, and pops kitty iff pushed.
    let with_kitty = build_teardown_bytes(true);
    assert!(with_kitty.windows(MOUSE_TEARDOWN_M5.len()).any(|w| w == MOUSE_TEARDOWN_M5));
    assert!(with_kitty.windows(KITTY_KEYBOARD_POP.len()).any(|w| w == KITTY_KEYBOARD_POP));

    let without_kitty = build_teardown_bytes(false);
    assert!(!without_kitty.windows(KITTY_KEYBOARD_POP.len()).any(|w| w == KITTY_KEYBOARD_POP));

    assert_eq!(
        classify_probe_control(&Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL))),
        LoopControl::CleanExit
    );
    assert_eq!(
        classify_probe_control(&Event::Key(KeyEvent::new(KeyCode::F(12), KeyModifiers::NONE))),
        LoopControl::TriggerPanic
    );
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p rig-probe`
Expected: FAIL on stub implementations.

- [ ] **Step 3: Implement `terminal_io.rs`, `probe_runner.rs`, and `examples/capability_probe.rs`**

`probe_runner` runs the sections defined in `docs/milestones.md` in deterministic order, logging each written sequence (`{"kind":"write",...}`) and each section result (`{"kind":"section","section":"...",...}`) to `JsonLog`:
1. `identity` (`XTVERSION` + `EnvironmentSnapshot`)
2. `mouse_modes` (initial `DECRPM` 1000/1002/1003/1006, write `MOUSE_SETUP_M1`, `DECRPM` 1000/1002/1003/1006 proving 1002 & 1006 set and 1000 & 1003 reset, write `MOUSE_TEARDOWN_M5`, `DECRPM` proving all reset)
3. `kitty_keyboard` (query `CSI ? u`, push `CSI > 1 u`, query again, pop `CSI < u`, query again)
4. `modes` (test setting & resetting `2004` bracketed paste, `1004` focus events, and `2026` synchronized output via `DECRPM`)
5. `cell_size` (`CSI 16 t`)
6. `widths` (for each `WidthSample`, `\r`, query start col or `1`, write `sample.text`, query `CSI 6 n`, `\r\x1b[2K` clear line, record measured cell width)
7. `osc52` (if `--osc52 <token>` passed, emit plain and/or tmux-wrapped OSC 52 and log bytes written)
8. `background_color` (enable `crossterm` event reading, send `OSC_11_BACKGROUND_QUERY`, poll up to `200 ms` for initial reply while keeping parser armed up to `2_000 ms`, recording `elapsed_ms` and swallowing any late reply so it never logs as key presses)
9. In `--batch` mode: log `{"kind":"summary",...}`, call `mark_ready()`, exit `0` (dropping `TerminalSessionGuard`). In interactive mode: enable `MOUSE_SETUP_M1`, `BRACKETED_PASTE_ON`, `FOCUS_EVENTS_ON`, call `mark_ready()`, and loop reading bursts via `drain_event_burst` until `LoopControl::CleanExit` (`Ctrl+C`) or `LoopControl::TriggerPanic` (`F12`, `panic!("capability_probe triggered test panic")`).

- [ ] **Step 4: Run `just check` and `just lint-targets`**

Run: `just check && just lint-targets`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add rig/probe/
git commit -m "feat(probe): implement capability_probe runner, terminal guard, and example binary"
```

---

### Task 6: Layer 5 (`just orca` and `rig/orca/run.sh`) Live Verification in Orca

**Files:**
- Create: `rig/orca/run.sh` (`chmod +x`)
- Modify: `justfile`

**Interfaces:**
- Consumes: `capability_probe` example binary (`--log <path> --batch`), `orca terminal create`, `orca terminal wait --for exit`, `orca terminal close --tab`.
- Produces:
  - `just orca <program> [extra_args...]`: one-line recipe calling `./rig/orca/run.sh "$@"`.
  - Live Layer 5 verification proving `xterm.js(6.0.0)` (and Unicode 11 two-cell emoji widths, M1/M5 mouse slot behavior, Kitty keyboard support, dark background color reply) in an unfocused Orca terminal, both standalone and inside `tmux` if `tmux` is installed on the host.

- [ ] **Step 1: Add `orca` recipe to `justfile` and create `rig/orca/run.sh`**

Add to `justfile`:

```just
# Run an example program in an unfocused live Orca terminal (Layer 5).
orca *args:
    ./rig/orca/run.sh "$@"
```

Create executable `rig/orca/run.sh` (`set -eu`):
1. Require `<program>` argument (e.g., `capability_probe`), shift remaining args as extra program flags (defaulting to `--batch` when `--batch` is not already passed and `capability_probe` is run non-interactively, or forwarding explicit args).
2. Pre-build the example binary: `cargo build -p rig-probe --example "$program"`.
3. Create a temp log directory (`mktemp -d "${TMPDIR:-/tmp}/tuikit-orca.XXXXXX"`), register an `EXIT` trap that closes the Orca terminal tab (`orca terminal close --terminal "$handle" --tab --json >/dev/null 2>&1 || true`) and preserves the log path on failure.
4. Launch via `orca terminal create --worktree "path:$PWD" --title "probe:$program" --command "'$bin' --log '$log_file' $extra_flags; exit" --json` (never passing `--focus` so user typing cannot land in it).
5. Parse the terminal handle from the JSON output, run `orca terminal wait --terminal "$handle" --for exit --timeout-ms 20000 --json` (zero `sleep` calls), verify `[ -f "$log_file.ready" ]` and `grep -q '"kind":"ready"' "$log_file"`, print the captured section records to stdout, and close the tab cleanly.

- [ ] **Step 2: Run Layer 5 live against `capability_probe` in Orca (including M0 row 8 OSC 52 check)**

Run: `just orca capability_probe` and `just orca capability_probe --batch --osc52 "tuikit-orca-osc52-probe"` (saving and restoring `pbpaste` around the OSC 52 check on macOS so the user's clipboard is untouched afterward).
Expected:
- Exit code `0`, unfocused Orca tab opens and closes automatically.
- Logged `identity` reports `xtversion` containing `xterm.js(6.0.0)` and `TERM_PROGRAM=Orca`.
- Logged `mouse_modes` reports `1002` and `1006` set and `1000` and `1003` reset after M1, and all reset after M5.
- Logged `kitty_keyboard` reports query answered, push to `1`, and pop back to `0`.
- Logged `background_color` reports dark (`is_dark: true`) within the wait window.
- Logged `widths` matches `docs/terminals.md`'s `xterm.js 6.0.0 in Orca` column (including rainbow flag measurement for M0 row 19 and all toolkit glyphs for M0 row 20).
- M0 row 8 (`OSC 52 in xterm.js in Orca`) is observed via `pbpaste` and recorded in the run output for Part 4's `terminals.md` update.

- [ ] **Step 3: Run `just check` and `just lint-targets`**

Run: `just check && just lint-targets`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add justfile rig/orca/run.sh
git commit -m "feat(rig): add Layer 5 unfocused Orca runner and verify capability_probe"
```

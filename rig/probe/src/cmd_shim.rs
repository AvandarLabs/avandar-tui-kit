//! Windows `cmd.exe` `.cmd` shim argument quoting and invocation builder.

use std::path::Path;

/// Quotes a single argument for `cmd.exe /D /S /C` execution of a `.cmd` shim forwarding `%*` to an executable.
///
/// Embedded `\0`, `\r`, and `\n` characters cannot cross `cmd.exe` without terminating or truncating
/// the command line and are omitted.
#[must_use]
pub fn quote_cmd_arg(argument: &str) -> String {
    let mut quoted = String::with_capacity(argument.len() + 2);
    quoted.push('"');
    let mut pending_backslashes = 0_usize;
    for character in argument.chars() {
        match character {
            '\0' | '\r' | '\n' => {}
            '\\' => pending_backslashes += 1,
            '"' => {
                // UCRT / CommandLineToArgvW halves backslashes only when immediately preceding `"`.
                quoted.extend(std::iter::repeat_n('\\', pending_backslashes * 2));
                pending_backslashes = 0;
                // Close quote, emit `\^^^"` (`cmd /S /C` pass 1 -> `\^"`, `.cmd %*` pass 2 -> `\"`),
                // then reopen quote so both `cmd.exe` passes and `CommandLineToArgvW` stay in-quotes.
                quoted.push_str("\"\\^^^\"\"");
            }
            '%' => {
                // Backslashes before non-quote characters are literal in `CommandLineToArgvW` (1:1).
                quoted.extend(std::iter::repeat_n('\\', pending_backslashes));
                pending_backslashes = 0;
                // Pass 1 expands `%cd:~,%` to `""` leaving literal `%`; `.cmd %*` pass 2 scans past
                // `%*` without re-expanding `%VAR%` or `%1`.
                quoted.push_str("%%cd:~,%");
            }
            plain_char => {
                quoted.extend(std::iter::repeat_n('\\', pending_backslashes));
                pending_backslashes = 0;
                quoted.push(plain_char);
            }
        }
    }
    // Double trailing backslashes so they do not escape the closing quote in `CommandLineToArgvW`.
    quoted.extend(std::iter::repeat_n('\\', pending_backslashes * 2));
    quoted.push('"');
    quoted
}

/// Builds the inner command string for `cmd.exe /D /S /C "<invocation>"` targeting `shim_path`.
#[must_use]
pub fn build_cmd_shim_invocation(shim_path: &Path, arguments: &[&str]) -> String {
    let shim_text = shim_path.to_string_lossy();
    let mut invocation = quote_cmd_arg(&shim_text);
    for &argument in arguments {
        invocation.push(' ');
        invocation.push_str(&quote_cmd_arg(argument));
    }
    invocation
}

#[cfg(test)]
mod tests {
    use super::*;

    fn simulate_cmd_line_pass(raw_command_line: &str) -> String {
        let stripped_line = raw_command_line
            .strip_prefix('"')
            .and_then(|remaining_line| remaining_line.strip_suffix('"'))
            .expect("outer /S quotes required");
        let percent_expanded_line = stripped_line.replace("%%cd:~,%", "%");
        strip_unquoted_carets_and_forbid_operators(&percent_expanded_line)
    }

    fn strip_unquoted_carets_and_forbid_operators(line: &str) -> String {
        let mut output = String::with_capacity(line.len());
        let mut characters = line.chars();
        let mut is_in_quotes = false;
        while let Some(character) = characters.next() {
            if is_in_quotes {
                is_in_quotes = character != '"';
                output.push(character);
                continue;
            }
            match character {
                '"' => {
                    is_in_quotes = true;
                    output.push('"');
                }
                '^' => {
                    if let Some(escaped_char) = characters.next() {
                        output.push(escaped_char);
                    }
                }
                '&' | '|' | '<' | '>' | '(' | ')' => {
                    panic!("unquoted cmd.exe operator leaked: {character} in {line}");
                }
                other_char => output.push(other_char),
            }
        }
        assert!(!is_in_quotes, "unbalanced cmd.exe quotes in {line}");
        output
    }

    #[derive(Default)]
    struct WindowsArgvDecoder {
        parsed_arguments: Vec<String>,
        current_argument: String,
        is_in_quotes: bool,
        was_in_quotes: bool,
        backslash_count: usize,
    }

    impl WindowsArgvDecoder {
        fn feed_quote(&mut self) {
            self.current_argument
                .extend(std::iter::repeat_n('\\', self.backslash_count / 2));
            let is_escaped_quote = !self.backslash_count.is_multiple_of(2);
            self.backslash_count = 0;
            if is_escaped_quote || self.was_in_quotes {
                self.current_argument.push('"');
                self.was_in_quotes = false;
            } else {
                self.was_in_quotes = self.is_in_quotes;
                self.is_in_quotes = !self.is_in_quotes;
            }
        }

        fn feed_char(&mut self, character: char) {
            match character {
                '\\' => {
                    self.backslash_count += 1;
                    self.was_in_quotes = false;
                }
                '"' => self.feed_quote(),
                ' ' | '\t' if !self.is_in_quotes => {
                    self.current_argument
                        .extend(std::iter::repeat_n('\\', self.backslash_count));
                    if !self.current_argument.is_empty() || self.was_in_quotes {
                        self.parsed_arguments
                            .push(std::mem::take(&mut self.current_argument));
                    }
                    self.backslash_count = 0;
                    self.was_in_quotes = false;
                }
                other_char => {
                    self.current_argument
                        .extend(std::iter::repeat_n('\\', self.backslash_count));
                    self.backslash_count = 0;
                    self.was_in_quotes = false;
                    self.current_argument.push(other_char);
                }
            }
        }

        fn finish(mut self) -> Vec<String> {
            self.current_argument
                .extend(std::iter::repeat_n('\\', self.backslash_count));
            if !self.current_argument.is_empty() || self.was_in_quotes || self.is_in_quotes {
                self.parsed_arguments.push(self.current_argument);
            }
            self.parsed_arguments
        }
    }

    fn simulate_windows_argv(command_line: &str) -> Vec<String> {
        let mut decoder = WindowsArgvDecoder::default();
        for character in command_line.chars() {
            decoder.feed_char(character);
        }
        decoder.finish()
    }

    #[test]
    fn quote_cmd_arg_escapes_trailing_backslashes_percent_signs_and_strips_control_chars() {
        assert_eq!(quote_cmd_arg(""), "\"\"");
        assert_eq!(quote_cmd_arg("hello world"), "\"hello world\"");
        assert_eq!(quote_cmd_arg("C:\\dir\\"), "\"C:\\dir\\\\\"");
        assert_eq!(quote_cmd_arg("%PATH%"), "\"%%cd:~,%PATH%%cd:~,%\"");
        assert_eq!(quote_cmd_arg("a\r\nb\0c"), "\"abc\"");
    }

    #[test]
    fn simulated_two_pass_cmd_and_argv_round_trips_all_special_characters() {
        let shim_path = Path::new("C:\\Program Files\\Agents\\codex.cmd");
        let expected_args = [
            "",
            "hello world",
            "say \"hello & world\" now",
            "\"quoted\"",
            "\"\"",
            "a\\\"b",
            "C:\\dir with space\\",
            "%PATH% %CMDCMDLINE:~-1% 100% %1 %*",
            "a^b ^^ c&d && e|f <g >h (i) !TEMP!",
        ];
        let invocation = build_cmd_shim_invocation(shim_path, &expected_args);
        let pass_one = simulate_cmd_line_pass(&format!("\"{invocation}\""));
        let pass_two = strip_unquoted_carets_and_forbid_operators(&pass_one);
        let decoded = simulate_windows_argv(&pass_two);

        assert_eq!(decoded[0], "C:\\Program Files\\Agents\\codex.cmd");
        assert_eq!(&decoded[1..], expected_args);
    }

    #[cfg(windows)]
    mod windows_round_trip {
        use std::ffi::OsString;
        use std::fs;
        use std::os::windows::process::CommandExt;
        use std::path::{Path, PathBuf};
        use std::process::Command;
        use std::sync::atomic::{AtomicUsize, Ordering};

        use super::super::build_cmd_shim_invocation;

        static NEXT_SHIM_TEST_ID: AtomicUsize = AtomicUsize::new(1);

        const ARG_DUMPER_SOURCE: &str = concat!(
            "fn main() {\n",
            "    let dump_path = std::env::var_os(\"RIG_PROBE_DUMP\").unwrap();\n",
            "    let mut encoded = String::new();\n",
            "    for argument in std::env::args().skip(1) {\n",
            "        encoded.push_str(&argument);\n",
            "        encoded.push('\\0');\n",
            "    }\n",
            "    std::fs::write(dump_path, encoded).unwrap();\n",
            "}\n",
        );

        fn compile_arg_dumper(temp_dir: &Path) -> PathBuf {
            let source_path = temp_dir.join("dump_args.rs");
            let binary_path = temp_dir.join("dump_args.exe");
            fs::write(&source_path, ARG_DUMPER_SOURCE).expect("should write dumper source");
            let rustc_bin = std::env::var_os("RUSTC").unwrap_or_else(|| OsString::from("rustc"));
            let output = Command::new(rustc_bin)
                .arg(&source_path)
                .arg("-o")
                .arg(&binary_path)
                .output()
                .expect("should invoke rustc");
            assert!(
                output.status.success(),
                "rustc failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            binary_path
        }

        fn execute_cmd_shim(shim_path: &Path, dump_path: &Path, arguments: &[&str]) -> Vec<String> {
            let invocation = build_cmd_shim_invocation(shim_path, arguments);
            // `raw_arg` is required because `Command::arg` applies MSVCRT `\"` escaping,
            // which breaks `cmd.exe /S /C` outer-quote stripping.
            let status = Command::new("cmd.exe")
                .env("RIG_PROBE_DUMP", dump_path)
                .env("RIG_PROBE_SECRET", "EXPANDED_BY_MISTAKE")
                .raw_arg(format!("/D /S /C \"{invocation}\""))
                .status()
                .expect("should spawn cmd.exe");
            assert!(status.success(), "cmd.exe exited with {status}");
            let encoded_dump =
                fs::read_to_string(dump_path).expect("dumper binary should write output");
            encoded_dump
                .split_terminator('\0')
                .map(str::to_owned)
                .collect()
        }

        #[test]
        fn windows_cmd_shim_round_trips_spaces_quotes_and_metacharacters() {
            let test_id = NEXT_SHIM_TEST_ID.fetch_add(1, Ordering::Relaxed);
            let temp_dir = std::env::temp_dir().join(format!(
                "rig-probe-cmd-shim-{}-{test_id}",
                std::process::id()
            ));
            fs::create_dir_all(&temp_dir).expect("should create temp dir");

            let binary_path = compile_arg_dumper(&temp_dir);
            let shim_path = temp_dir.join("agent shim.cmd");
            let dump_path = temp_dir.join("received.bin");
            let shim_script = format!("@echo off\r\n\"{}\" %*\r\n", binary_path.display());
            fs::write(&shim_path, shim_script).expect("should write .cmd shim");

            let expected_args = [
                "",
                "hello world",
                "say \"hello & world\" now",
                "\"\"",
                "a\\\"b",
                "C:\\dir with space\\",
                "%RIG_PROBE_SECRET% %PATH% 100% %1",
                "a^b ^^ c&d | e < f > g (h) !RIG_PROBE_SECRET!",
            ];
            let actual_args = execute_cmd_shim(&shim_path, &dump_path, &expected_args);
            assert_eq!(actual_args, expected_args);

            fs::remove_dir_all(&temp_dir).expect("should clean up temp dir");
        }
    }
}

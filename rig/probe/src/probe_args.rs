//! Pure command-line argument parser for the `capability_probe` example program.

use std::path::PathBuf;

/// Parsed command-line options for `capability_probe`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeArgs {
    /// Destination JSONL log file path supplied via `--log <path>`.
    pub log_path: PathBuf,
    /// Whether `--batch` was passed to exit immediately after automated sections finish.
    pub is_batch: bool,
    /// Whether `--push-kitty` was passed to push Kitty disambiguate mode during interactive input.
    pub is_push_kitty: bool,
    /// Optional token supplied via `--osc52 <token>` to emit `OSC 52` clipboard probes.
    pub osc52_token: Option<String>,
}

fn take_non_empty_flag_value<I, S>(flag_name: &str, iterator: &mut I) -> Result<String, String>
where
    I: Iterator<Item = S>,
    S: AsRef<str>,
{
    let Some(raw_value) = iterator.next() else {
        return Err(format!("missing value for {flag_name}"));
    };
    let value_text = raw_value.as_ref();
    if value_text.is_empty() {
        return Err(format!("{flag_name} value must not be empty"));
    }
    Ok(value_text.to_owned())
}

#[derive(Default)]
struct PartialProbeArgs {
    log_path: Option<PathBuf>,
    is_batch: bool,
    is_push_kitty: bool,
    osc52_token: Option<String>,
}

impl PartialProbeArgs {
    fn apply_flag<I, S>(&mut self, flag: &str, iterator: &mut I) -> Result<(), String>
    where
        I: Iterator<Item = S>,
        S: AsRef<str>,
    {
        match flag {
            "--log" => {
                if self.log_path.is_some() {
                    return Err("duplicate --log flag".to_owned());
                }
                let path_value = take_non_empty_flag_value("--log", iterator)?;
                self.log_path = Some(PathBuf::from(path_value));
            }
            "--batch" => {
                if self.is_batch {
                    return Err("duplicate --batch flag".to_owned());
                }
                self.is_batch = true;
            }
            "--push-kitty" => {
                if self.is_push_kitty {
                    return Err("duplicate --push-kitty flag".to_owned());
                }
                self.is_push_kitty = true;
            }
            "--osc52" => {
                if self.osc52_token.is_some() {
                    return Err("duplicate --osc52 flag".to_owned());
                }
                self.osc52_token = Some(take_non_empty_flag_value("--osc52", iterator)?);
            }
            unexpected_argument => {
                return Err(format!("unexpected argument: {unexpected_argument}"));
            }
        }
        Ok(())
    }
}

/// Parses `capability_probe` CLI flags from an iterator of arguments (excluding `argv[0]`).
///
/// # Errors
///
/// Returns a human-readable error message when `--log <path>` is missing or empty, when a value-taking
/// flag is missing its argument or repeated, or when an unrecognized argument is encountered.
pub fn parse_probe_args<I, S>(args: I) -> Result<ProbeArgs, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut iterator = args.into_iter();
    let mut partial = PartialProbeArgs::default();
    while let Some(raw_argument) = iterator.next() {
        partial.apply_flag(raw_argument.as_ref(), &mut iterator)?;
    }
    let Some(resolved_log_path) = partial.log_path else {
        return Err("missing required --log <path> flag".to_owned());
    };
    Ok(ProbeArgs {
        log_path: resolved_log_path,
        is_batch: partial.is_batch,
        is_push_kitty: partial.is_push_kitty,
        osc52_token: partial.osc52_token,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_required_log_and_optional_batch_kitty_and_osc52_flags() {
        let minimal = parse_probe_args(["--log", "/tmp/p.jsonl"]).expect("minimal args must parse");
        assert_eq!(
            minimal,
            ProbeArgs {
                log_path: PathBuf::from("/tmp/p.jsonl"),
                is_batch: false,
                is_push_kitty: false,
                osc52_token: None,
            }
        );

        let full = parse_probe_args([
            "--batch",
            "--push-kitty",
            "--osc52",
            "tok-1",
            "--log",
            "/tmp/p.jsonl",
        ])
        .expect("full args must parse in any flag order");
        assert_eq!(
            full,
            ProbeArgs {
                log_path: PathBuf::from("/tmp/p.jsonl"),
                is_batch: true,
                is_push_kitty: true,
                osc52_token: Some("tok-1".to_owned()),
            }
        );
    }

    #[test]
    fn rejects_missing_empty_duplicate_and_unknown_arguments() {
        let invalid_cases: &[(&[&str], &str)] = &[
            (&[], "missing required --log"),
            (&["--batch"], "missing required --log"),
            (&["--log"], "missing value for --log"),
            (&["--log", ""], "--log value must not be empty"),
            (&["--log", "a.jsonl", "--log", "b.jsonl"], "duplicate --log"),
            (
                &["--log", "a.jsonl", "--batch", "--batch"],
                "duplicate --batch",
            ),
            (
                &["--log", "a.jsonl", "--push-kitty", "--push-kitty"],
                "duplicate --push-kitty",
            ),
            (
                &["--log", "a.jsonl", "--osc52"],
                "missing value for --osc52",
            ),
            (
                &["--log", "a.jsonl", "--osc52", ""],
                "--osc52 value must not be empty",
            ),
            (
                &["--log", "a.jsonl", "--osc52", "one", "--osc52", "two"],
                "duplicate --osc52",
            ),
            (
                &["--log", "a.jsonl", "--unknown"],
                "unexpected argument: --unknown",
            ),
            (&["positional"], "unexpected argument: positional"),
        ];
        for &(case_args, expected_substring) in invalid_cases {
            let error_message =
                parse_probe_args(case_args).expect_err("invalid arguments should fail");
            assert!(
                error_message.contains(expected_substring),
                "expected {expected_substring:?} in {error_message:?} for {case_args:?}"
            );
        }
    }
}

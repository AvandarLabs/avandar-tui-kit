//! Pure command-line argument parser for the `capability_probe` example program.

use std::path::PathBuf;

/// Selects which `OSC 52` clipboard escape form(s) `capability_probe` emits when `--osc52` is set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Osc52Form {
    /// Emit plain `OSC 52` outside `tmux`, and both plain and `DCS` passthrough inside `tmux`.
    Auto,
    /// Emit only the plain `OSC 52` sequence.
    Plain,
    /// Emit only the `tmux` `DCS` passthrough `OSC 52` sequence.
    Tmux,
    /// Emit both the plain and `tmux` `DCS` passthrough `OSC 52` sequences.
    Both,
}

impl Osc52Form {
    fn parse(raw_value: &str) -> Result<Self, String> {
        match raw_value {
            "auto" => Ok(Self::Auto),
            "plain" => Ok(Self::Plain),
            "tmux" => Ok(Self::Tmux),
            "both" => Ok(Self::Both),
            other => Err(format!(
                "invalid --osc52-form value '{other}': expected auto, plain, tmux, or both"
            )),
        }
    }
}

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
    /// Clipboard escape form selector supplied via `--osc52-form <auto|plain|tmux|both>`.
    pub osc52_form: Osc52Form,
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

fn set_unique_option<T>(slot: &mut Option<T>, flag_name: &str, value: T) -> Result<(), String> {
    if slot.is_some() {
        return Err(format!("duplicate {flag_name} flag"));
    }
    *slot = Some(value);
    Ok(())
}

fn set_unique_bool(slot: &mut bool, flag_name: &str) -> Result<(), String> {
    if *slot {
        return Err(format!("duplicate {flag_name} flag"));
    }
    *slot = true;
    Ok(())
}

#[derive(Default)]
struct PartialProbeArgs {
    log_path: Option<PathBuf>,
    is_batch: bool,
    is_push_kitty: bool,
    osc52_token: Option<String>,
    osc52_form: Option<Osc52Form>,
}

impl PartialProbeArgs {
    fn apply_flag<I, S>(&mut self, flag: &str, iterator: &mut I) -> Result<(), String>
    where
        I: Iterator<Item = S>,
        S: AsRef<str>,
    {
        match flag {
            "--log" => {
                let path_value = take_non_empty_flag_value("--log", iterator)?;
                set_unique_option(&mut self.log_path, "--log", PathBuf::from(path_value))
            }
            "--batch" => set_unique_bool(&mut self.is_batch, "--batch"),
            "--push-kitty" => set_unique_bool(&mut self.is_push_kitty, "--push-kitty"),
            "--osc52" => {
                let token = take_non_empty_flag_value("--osc52", iterator)?;
                set_unique_option(&mut self.osc52_token, "--osc52", token)
            }
            "--osc52-form" => {
                let raw_form = take_non_empty_flag_value("--osc52-form", iterator)?;
                set_unique_option(
                    &mut self.osc52_form,
                    "--osc52-form",
                    Osc52Form::parse(&raw_form)?,
                )
            }
            unexpected_argument => Err(format!("unexpected argument: {unexpected_argument}")),
        }
    }
}

/// Parses `capability_probe` CLI flags from an iterator of arguments (excluding `argv[0]`).
///
/// # Errors
///
/// Returns a human-readable error message when `--log <path>` is missing or empty, when a value-taking
/// flag is missing its argument, invalid, or repeated, or when an unrecognized argument is encountered.
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
        osc52_form: partial.osc52_form.unwrap_or(Osc52Form::Auto),
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
                osc52_form: Osc52Form::Auto,
            }
        );

        let full = parse_probe_args([
            "--batch",
            "--push-kitty",
            "--osc52",
            "tok-1",
            "--osc52-form",
            "tmux",
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
                osc52_form: Osc52Form::Tmux,
            }
        );
    }

    #[test]
    fn parses_and_rejects_osc52_form_arguments() {
        for (raw_form, expected_form) in [
            ("auto", Osc52Form::Auto),
            ("plain", Osc52Form::Plain),
            ("tmux", Osc52Form::Tmux),
            ("both", Osc52Form::Both),
        ] {
            let parsed = parse_probe_args(["--log", "p.jsonl", "--osc52-form", raw_form])
                .expect("each documented --osc52-form variant must parse");
            assert_eq!(parsed.osc52_form, expected_form);
        }
        let invalid_forms: &[(&[&str], &str)] = &[
            (&["--log", "a", "--osc52-form"], "missing value"),
            (&["--log", "a", "--osc52-form", ""], "must not be empty"),
            (
                &["--log", "a", "--osc52-form", "bogus"],
                "invalid --osc52-form",
            ),
            (
                &[
                    "--log",
                    "a",
                    "--osc52-form",
                    "plain",
                    "--osc52-form",
                    "both",
                ],
                "duplicate --osc52-form",
            ),
        ];
        for &(case_args, needle) in invalid_forms {
            let err = parse_probe_args(case_args).expect_err("invalid --osc52-form should fail");
            assert!(err.contains(needle), "got {err:?} for {case_args:?}");
        }
    }

    #[test]
    fn rejects_missing_empty_duplicate_and_unknown_arguments() {
        let invalid_cases: &[(&[&str], &str)] = &[
            (&[], "missing required --log"),
            (&["--batch"], "missing required --log"),
            (&["--log"], "missing value for --log"),
            (&["--log", ""], "--log value must not be empty"),
            (&["--log", "a", "--log", "b"], "duplicate --log"),
            (&["--log", "a", "--batch", "--batch"], "duplicate --batch"),
            (
                &["--log", "a", "--push-kitty", "--push-kitty"],
                "duplicate --push-kitty",
            ),
            (&["--log", "a", "--osc52"], "missing value for --osc52"),
            (
                &["--log", "a", "--osc52", ""],
                "--osc52 value must not be empty",
            ),
            (
                &["--log", "a", "--osc52", "1", "--osc52", "2"],
                "duplicate --osc52",
            ),
            (
                &["--log", "a", "--unknown"],
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

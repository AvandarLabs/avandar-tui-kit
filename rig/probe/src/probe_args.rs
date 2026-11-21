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

/// Concrete CLI argument parsing failure returned by [`parse_probe_args`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeArgsError {
    /// Required `--log <path>` flag was omitted.
    MissingRequiredLog,
    /// Value-taking flag appeared at the end of the argument stream without a value.
    MissingFlagValue {
        /// Flag name (`"--log"`, `"--osc52"`, or `"--osc52-form"`).
        flag: &'static str,
    },
    /// Value-taking flag received an empty string (`""`).
    EmptyFlagValue {
        /// Flag name (`"--log"`, `"--osc52"`, or `"--osc52-form"`).
        flag: &'static str,
    },
    /// Flag was passed more than once.
    DuplicateFlag {
        /// Repeated flag name.
        flag: &'static str,
    },
    /// `--osc52-form` received a value outside `auto|plain|tmux|both`.
    InvalidOsc52Form {
        /// Unrecognized `--osc52-form` value.
        value: String,
    },
    /// Unrecognized flag or unexpected positional argument.
    UnexpectedArgument {
        /// Offending CLI token.
        argument: String,
    },
}

impl std::fmt::Display for ProbeArgsError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingRequiredLog => formatter.write_str("missing required --log <path> flag"),
            Self::MissingFlagValue { flag } => write!(formatter, "missing value for {flag}"),
            Self::EmptyFlagValue { flag } => write!(formatter, "{flag} value must not be empty"),
            Self::DuplicateFlag { flag } => write!(formatter, "duplicate {flag} flag"),
            Self::InvalidOsc52Form { value } => write!(
                formatter,
                "invalid --osc52-form value '{value}': expected auto, plain, tmux, or both"
            ),
            Self::UnexpectedArgument { argument } => {
                write!(formatter, "unexpected argument: {argument}")
            }
        }
    }
}

impl std::error::Error for ProbeArgsError {}

impl Osc52Form {
    fn parse(raw_value: &str) -> Result<Self, ProbeArgsError> {
        match raw_value {
            "auto" => Ok(Self::Auto),
            "plain" => Ok(Self::Plain),
            "tmux" => Ok(Self::Tmux),
            "both" => Ok(Self::Both),
            other => Err(ProbeArgsError::InvalidOsc52Form {
                value: other.to_owned(),
            }),
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

fn take_non_empty_flag_value<I, S>(
    flag_name: &'static str,
    iterator: &mut I,
) -> Result<String, ProbeArgsError>
where
    I: Iterator<Item = S>,
    S: AsRef<str>,
{
    let Some(raw_value) = iterator.next() else {
        return Err(ProbeArgsError::MissingFlagValue { flag: flag_name });
    };
    let value_text = raw_value.as_ref();
    if value_text.is_empty() {
        return Err(ProbeArgsError::EmptyFlagValue { flag: flag_name });
    }
    Ok(value_text.to_owned())
}

fn set_unique_option<T>(
    slot: &mut Option<T>,
    flag_name: &'static str,
    value: T,
) -> Result<(), ProbeArgsError> {
    if slot.is_some() {
        return Err(ProbeArgsError::DuplicateFlag { flag: flag_name });
    }
    *slot = Some(value);
    Ok(())
}

const fn set_unique_bool(slot: &mut bool, flag_name: &'static str) -> Result<(), ProbeArgsError> {
    if *slot {
        return Err(ProbeArgsError::DuplicateFlag { flag: flag_name });
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
    fn apply_flag<I, S>(&mut self, flag: &str, iterator: &mut I) -> Result<(), ProbeArgsError>
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
            unexpected_argument => Err(ProbeArgsError::UnexpectedArgument {
                argument: unexpected_argument.to_owned(),
            }),
        }
    }
}

/// Parses `capability_probe` CLI flags from an iterator of arguments (excluding `argv[0]`).
///
/// # Errors
///
/// Returns a [`ProbeArgsError`] when `--log <path>` is missing or empty, when a value-taking
/// flag is missing its argument, invalid, or repeated, or when an unrecognized argument is encountered.
pub fn parse_probe_args<I, S>(args: I) -> Result<ProbeArgs, ProbeArgsError>
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
        return Err(ProbeArgsError::MissingRequiredLog);
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
    fn minimal_and_reordered_flags_populate_probe_args_fields() {
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
    fn documented_osc52_form_values_map_to_matching_variants() {
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
    }

    #[test]
    fn invalid_osc52_form_flags_return_typed_errors() {
        let invalid_forms: &[(&[&str], ProbeArgsError, &str)] = &[
            (
                &["--log", "a", "--osc52-form"],
                ProbeArgsError::MissingFlagValue {
                    flag: "--osc52-form",
                },
                "missing value",
            ),
            (
                &["--log", "a", "--osc52-form", ""],
                ProbeArgsError::EmptyFlagValue {
                    flag: "--osc52-form",
                },
                "must not be empty",
            ),
            (
                &["--log", "a", "--osc52-form", "bogus"],
                ProbeArgsError::InvalidOsc52Form {
                    value: "bogus".to_owned(),
                },
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
                ProbeArgsError::DuplicateFlag {
                    flag: "--osc52-form",
                },
                "duplicate --osc52-form",
            ),
        ];
        for (case_args, expected_error, expected_substring) in invalid_forms {
            let parse_error =
                parse_probe_args(*case_args).expect_err("invalid --osc52-form should fail");
            assert_eq!(&parse_error, expected_error);
            assert!(parse_error.to_string().contains(expected_substring));
        }
    }

    #[test]
    fn missing_empty_duplicate_and_unknown_flags_return_typed_errors() {
        let invalid_cases: &[(&[&str], ProbeArgsError, &str)] = &[
            (
                &[],
                ProbeArgsError::MissingRequiredLog,
                "missing required --log",
            ),
            (
                &["--batch"],
                ProbeArgsError::MissingRequiredLog,
                "missing required --log",
            ),
            (
                &["--log"],
                ProbeArgsError::MissingFlagValue { flag: "--log" },
                "missing value for --log",
            ),
            (
                &["--log", ""],
                ProbeArgsError::EmptyFlagValue { flag: "--log" },
                "--log value must not be empty",
            ),
            (
                &["--log", "a", "--log", "b"],
                ProbeArgsError::DuplicateFlag { flag: "--log" },
                "duplicate --log",
            ),
            (
                &["--log", "a", "--batch", "--batch"],
                ProbeArgsError::DuplicateFlag { flag: "--batch" },
                "duplicate --batch",
            ),
            (
                &["--log", "a", "--push-kitty", "--push-kitty"],
                ProbeArgsError::DuplicateFlag {
                    flag: "--push-kitty",
                },
                "duplicate --push-kitty",
            ),
            (
                &["--log", "a", "--osc52"],
                ProbeArgsError::MissingFlagValue { flag: "--osc52" },
                "missing value for --osc52",
            ),
            (
                &["--log", "a", "--osc52", ""],
                ProbeArgsError::EmptyFlagValue { flag: "--osc52" },
                "--osc52 value must not be empty",
            ),
            (
                &["--log", "a", "--osc52", "1", "--osc52", "2"],
                ProbeArgsError::DuplicateFlag { flag: "--osc52" },
                "duplicate --osc52",
            ),
            (
                &["--log", "a", "--unknown"],
                ProbeArgsError::UnexpectedArgument {
                    argument: "--unknown".to_owned(),
                },
                "unexpected argument: --unknown",
            ),
            (
                &["positional"],
                ProbeArgsError::UnexpectedArgument {
                    argument: "positional".to_owned(),
                },
                "unexpected argument: positional",
            ),
        ];
        for (case_args, expected_error, expected_substring) in invalid_cases {
            let parse_error =
                parse_probe_args(*case_args).expect_err("invalid arguments should fail");
            assert_eq!(&parse_error, expected_error);
            assert!(parse_error.to_string().contains(expected_substring));
        }
    }
}

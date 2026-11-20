//! Pure command-line argument parser for the `capability_probe` example program.

use std::path::PathBuf;

/// Parsed command-line options for `capability_probe`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeArgs {
    /// Destination JSONL log file path supplied via `--log <path>`.
    pub log_path: PathBuf,
    /// Whether `--batch` was passed to exit immediately after automated sections finish.
    pub is_batch: bool,
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
    let value_str = raw_value.as_ref();
    if value_str.is_empty() {
        return Err(format!("{flag_name} value must not be empty"));
    }
    Ok(value_str.to_owned())
}

#[derive(Default)]
struct PartialProbeArgs {
    log_path: Option<PathBuf>,
    is_batch: bool,
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
            "--osc52" => {
                if self.osc52_token.is_some() {
                    return Err("duplicate --osc52 flag".to_owned());
                }
                self.osc52_token = Some(take_non_empty_flag_value("--osc52", iterator)?);
            }
            unexpected_arg => return Err(format!("unexpected argument: {unexpected_arg}")),
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
        osc52_token: partial.osc52_token,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_required_log_and_optional_batch_and_osc52_flags() {
        let minimal = parse_probe_args(["--log", "/tmp/p.jsonl"]).expect("minimal args must parse");
        assert_eq!(
            minimal,
            ProbeArgs {
                log_path: PathBuf::from("/tmp/p.jsonl"),
                is_batch: false,
                osc52_token: None,
            }
        );

        let full = parse_probe_args(["--batch", "--osc52", "tok-1", "--log", "/tmp/p.jsonl"])
            .expect("full args must parse in any flag order");
        assert_eq!(
            full,
            ProbeArgs {
                log_path: PathBuf::from("/tmp/p.jsonl"),
                is_batch: true,
                osc52_token: Some("tok-1".to_owned()),
            }
        );
    }

    #[test]
    fn rejects_missing_empty_duplicate_and_unknown_arguments() {
        let invalid_cases: &[&[&str]] = &[
            &[],
            &["--batch"],
            &["--log"],
            &["--log", ""],
            &["--log", "a.jsonl", "--log", "b.jsonl"],
            &["--log", "a.jsonl", "--batch", "--batch"],
            &["--log", "a.jsonl", "--osc52"],
            &["--log", "a.jsonl", "--osc52", ""],
            &["--log", "a.jsonl", "--osc52", "one", "--osc52", "two"],
            &["--log", "a.jsonl", "--unknown"],
            &["positional"],
        ];
        for &case_args in invalid_cases {
            assert!(
                parse_probe_args(case_args).is_err(),
                "expected Err for {case_args:?}"
            );
        }
    }
}

//! Pure and process-backed snapshots of terminal environment variables and TTY streams.

use std::io::IsTerminal;

/// Snapshot of terminal-identifying environment variables and stream TTY flags.
#[expect(
    clippy::struct_excessive_bools,
    reason = "diagnostic snapshot records five independent environment and stream flags"
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentSnapshot {
    /// Operating system name (`std::env::consts::OS`).
    pub os: &'static str,
    /// CPU architecture name (`std::env::consts::ARCH`).
    pub arch: &'static str,
    /// `TERM` variable when set and non-empty.
    pub term: Option<String>,
    /// `TERM_PROGRAM` variable when set and non-empty.
    pub term_program: Option<String>,
    /// `TERM_PROGRAM_VERSION` variable when set and non-empty.
    pub term_program_version: Option<String>,
    /// Whether `TMUX` is present and non-empty.
    pub is_tmux: bool,
    /// Whether any of `SSH_TTY`, `SSH_CONNECTION`, or `SSH_CLIENT` is present and non-empty.
    pub is_ssh: bool,
    /// Whether `NO_COLOR` is present and non-empty per `https://no-color.org`.
    pub has_no_color: bool,
    /// `ORCA_VERSION` variable when set and non-empty.
    pub orca_version: Option<String>,
    /// Whether standard input is a terminal.
    pub is_stdin_tty: bool,
    /// Whether standard output is a terminal.
    pub is_stdout_tty: bool,
}

fn non_empty_variable<F>(lookup: &F, variable_name: &str) -> Option<String>
where
    F: Fn(&str) -> Option<String>,
{
    // Empty values count as absent: `NO_COLOR=""` overrides an inherited `NO_COLOR=1` per
    // no-color.org (Detection Rule 2), and cleared exports like `TMUX=""` mean inactive.
    lookup(variable_name).filter(|variable_value| !variable_value.is_empty())
}

fn has_non_empty_variable<F>(lookup: &F, variable_name: &str) -> bool
where
    F: Fn(&str) -> Option<String>,
{
    non_empty_variable(lookup, variable_name).is_some()
}

/// Builds an [`EnvironmentSnapshot`] from an injected environment lookup closure and TTY flags.
#[must_use]
pub fn read_environment_with<F>(
    lookup: F,
    is_stdin_tty: bool,
    is_stdout_tty: bool,
) -> EnvironmentSnapshot
where
    F: Fn(&str) -> Option<String>,
{
    let is_ssh = ["SSH_TTY", "SSH_CONNECTION", "SSH_CLIENT"]
        .into_iter()
        .any(|ssh_variable| has_non_empty_variable(&lookup, ssh_variable));

    EnvironmentSnapshot {
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        term: non_empty_variable(&lookup, "TERM"),
        term_program: non_empty_variable(&lookup, "TERM_PROGRAM"),
        term_program_version: non_empty_variable(&lookup, "TERM_PROGRAM_VERSION"),
        is_tmux: has_non_empty_variable(&lookup, "TMUX"),
        is_ssh,
        has_no_color: has_non_empty_variable(&lookup, "NO_COLOR"),
        orca_version: non_empty_variable(&lookup, "ORCA_VERSION"),
        is_stdin_tty,
        is_stdout_tty,
    }
}

/// Reads an [`EnvironmentSnapshot`] from `std::env::var` and the current process standard streams.
#[must_use]
pub fn read_process_environment() -> EnvironmentSnapshot {
    let is_stdin_tty = std::io::stdin().is_terminal();
    let is_stdout_tty = std::io::stdout().is_terminal();
    read_environment_with(
        |variable_name| std::env::var(variable_name).ok(),
        is_stdin_tty,
        is_stdout_tty,
    )
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn map_lookup(entries: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let variables: HashMap<String, String> = entries
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect();
        move |requested_key| variables.get(requested_key).cloned()
    }

    #[test]
    fn populated_environment_lookup_sets_terminal_tmux_no_color_orca_and_tty_fields() {
        let snapshot = read_environment_with(
            map_lookup(&[
                ("TERM", "tmux-256color"),
                ("TERM_PROGRAM", "Orca"),
                ("TERM_PROGRAM_VERSION", "1.4.258"),
                ("TMUX", "/private/tmp/tmux-501/default,123,0"),
                ("NO_COLOR", "1"),
                ("ORCA_VERSION", "1.4.258"),
            ]),
            true,
            false,
        );
        assert_eq!(snapshot.os, std::env::consts::OS);
        assert_eq!(snapshot.arch, std::env::consts::ARCH);
        assert_eq!(snapshot.term.as_deref(), Some("tmux-256color"));
        assert_eq!(snapshot.term_program.as_deref(), Some("Orca"));
        assert_eq!(snapshot.term_program_version.as_deref(), Some("1.4.258"));
        assert!(snapshot.is_tmux);
        assert!(!snapshot.is_ssh);
        assert!(snapshot.has_no_color);
        assert_eq!(snapshot.orca_version.as_deref(), Some("1.4.258"));
        assert!(snapshot.is_stdin_tty);
        assert!(!snapshot.is_stdout_tty);
    }

    #[test]
    fn treats_empty_no_color_and_empty_variables_as_absent_per_detection_rule_2() {
        let snapshot = read_environment_with(
            map_lookup(&[
                ("TERM", ""),
                ("TERM_PROGRAM", ""),
                ("TERM_PROGRAM_VERSION", ""),
                ("TMUX", ""),
                ("SSH_TTY", ""),
                ("SSH_CONNECTION", ""),
                ("SSH_CLIENT", ""),
                ("NO_COLOR", ""),
                ("ORCA_VERSION", ""),
            ]),
            false,
            true,
        );
        assert_eq!(snapshot.term, None);
        assert_eq!(snapshot.term_program, None);
        assert_eq!(snapshot.term_program_version, None);
        assert!(!snapshot.is_tmux);
        assert!(!snapshot.is_ssh);
        assert!(!snapshot.has_no_color);
        assert_eq!(snapshot.orca_version, None);
        assert!(!snapshot.is_stdin_tty);
        assert!(snapshot.is_stdout_tty);
    }

    #[test]
    fn detects_ssh_from_any_non_empty_ssh_variable() {
        for ssh_key in ["SSH_TTY", "SSH_CONNECTION", "SSH_CLIENT"] {
            let snapshot = read_environment_with(map_lookup(&[(ssh_key, "present")]), false, false);
            assert!(snapshot.is_ssh, "expected is_ssh for {ssh_key}");
        }
    }

    #[test]
    fn process_environment_snapshot_reflects_live_tty_streams_and_non_empty_term() {
        let snapshot = read_process_environment();
        let expected_term = std::env::var("TERM").ok().filter(|value| !value.is_empty());
        assert_eq!(snapshot.term, expected_term);
        assert_eq!(snapshot.is_stdin_tty, std::io::stdin().is_terminal());
        assert_eq!(snapshot.is_stdout_tty, std::io::stdout().is_terminal());
    }
}

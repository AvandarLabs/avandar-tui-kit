//! Terminal capability probe binary driven by the Milestone 0 test harnesses.

use std::process::ExitCode;

fn main() -> ExitCode {
    rig_probe::probe_runner::run_from_env()
}

//! Terminal capability probe helpers and pure parsers for the Milestone 0 test rig.
//!
//! All escape-sequence builders and reply parsers live in pure modules so
//! terminal dialect edge cases can be verified with unit tests independent of
//! live TTY I/O.

pub mod cmd_shim;
pub mod color_reply;
pub mod environment;
pub mod json_log;
pub mod parsers;
pub mod probe_args;
pub mod sequences;
pub mod widths;

//! Terminal capability probe helpers and pure parsers for the cross-terminal test rig.
//!
//! All escape-sequence builders and reply parsers live in pure modules so
//! terminal dialect edge cases can be verified with unit tests independent of
//! live TTY I/O.

pub mod armed_color;
pub mod cmd_shim;
pub mod color_reply;
pub mod environment;
pub mod event_json;
pub mod json_log;
pub mod parsers;
pub mod probe_args;
pub mod probe_runner;
pub mod probe_sections;
pub mod section_json;
pub mod sequences;
pub mod session_guard;
pub mod terminal_io;
pub mod widths;

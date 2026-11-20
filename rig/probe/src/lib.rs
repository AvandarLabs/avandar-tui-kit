//! Terminal capability probe helpers and pure parsers for the Milestone 0 test rig.
//!
//! All escape-sequence builders and reply parsers live in pure modules so
//! terminal dialect edge cases can be verified with unit tests independent of
//! live TTY I/O.

pub mod parsers;
pub mod sequences;

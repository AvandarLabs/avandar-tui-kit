//! Reusable building blocks for terminal apps built with
//! [ratatui](https://ratatui.rs).
//!
//! This is the umbrella crate. Each building block is its own crate in this
//! workspace, and this crate re-exports each one behind a cargo feature of the
//! same name, so an app pays only for the parts it turns on. Nothing is on by
//! default.
//!
//! The toolkit is a set of parts an app calls, not a framework: it never owns
//! the event loop, the state, or the terminal for longer than a call.

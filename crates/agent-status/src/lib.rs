//! Library face of the `agent-status` crate.
//!
//! Exposes the pieces other workspace members (notably `agent-switcher`) need:
//! the on-disk entry shape and state store, the pure formatting helpers, and
//! the registered-agent registry.

pub mod agents;
pub mod commands;
pub mod state;

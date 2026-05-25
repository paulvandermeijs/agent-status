//! Library face of the `agent-status` crate.

pub mod agents;
pub mod commands;
pub mod state;

pub use agents::{Agent, by_name};
pub use commands::{
    build_entry, build_extension, format_list, format_status, ExtensionFile,
};
pub use state::{AttentionEntry, Event, StateStore};

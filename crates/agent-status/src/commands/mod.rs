//! Helpers used by each `agent-status` subcommand. One file per subcommand
//! (`set`, `status`, `list`, `agent-extension`); `mod.rs` re-exports the
//! public API. The attention filter that both `format_status` and
//! `format_list` consume is `Event::needs_attention` on the entry's event
//! — the switcher reads the store directly and surfaces every event value,
//! so that filter does NOT apply there.

mod agent_extension;
mod list;
mod set;
mod status;

pub use agent_extension::{build_extension, ExtensionFile};
pub use list::format_list;
pub use set::build_entry;
pub use status::format_status;

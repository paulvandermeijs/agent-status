//! Helpers used by each `agent-status` subcommand. One file per subcommand
//! (`set`, `status`, `list`, `agent-extension`); `mod.rs` re-exports the
//! public API and houses the shared `needs_attention` filter consumed by
//! both `format_status` and `format_list`.

mod agent_extension;
mod list;
mod set;
mod status;

pub use agent_extension::{build_extension, ExtensionFile};
pub use list::format_list;
pub use set::build_entry;
pub use status::format_status;

/// Whether an `event` value represents a session that wants the user's eyes
/// right now. `notify` is an explicit "Claude is blocked on you" signal;
/// `done` is the just-finished state that the next prompt will move on from.
/// Other values (`working`, `idle`, or anything the agent layer invents
/// later) are alive-but-not-asking and are hidden from the tmux indicator
/// and the legacy fzf TSV. The switcher reads the store directly and
/// surfaces every event value, so this filter does NOT apply there.
pub(crate) fn needs_attention(event: &str) -> bool {
    !matches!(event, "working" | "idle")
}

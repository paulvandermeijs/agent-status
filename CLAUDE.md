# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

`agent-status` is a small Rust CLI that integrates with Claude Code hooks and tmux's `status-right` to show which AI coding agent sessions are waiting on user input. State is one JSON file per session under `${XDG_RUNTIME_DIR:-/tmp}/agent-status/`, keyed by `session_id`. README.md covers end-user install and config; this file is for working on the binary itself.

## Build / test / lint

This repo is a Cargo workspace. The binaries live in `crates/agent-status` (the
hook-facing CLI, also exposes a library) and `crates/agent-switcher` (the
ratatui TUI popup).

```sh
cargo test                                                            # workspace-wide
cargo clippy --all-targets --all-features --locked -- -D warnings     # required gate
cargo build --release                                                 # both binaries
```

Run a single unit test or a specific integration test:

```sh
cargo test -p agent-status entry_roundtrips_through_json
cargo test -p agent-status --test cli end_to_end_set_status_clear
cargo test -p agent-switcher filter_matches_project_substring
```

Workspace-level `[workspace.lints]` enforce `unsafe_code = "forbid"`,
`nonstandard_style = "deny"`, `clippy::all = "deny"`, `clippy::pedantic = "warn"`
across both crates.

## Module split (load-bearing)

`agent-status` is split into a library and a binary in the same crate:

- `crates/agent-status/src/state.rs` — owns all filesystem I/O. `AttentionEntry`
  and `StateStore`. Tests use `tempfile::TempDir` for isolation.
- `crates/agent-status/src/commands/` — pure helpers organized one file
  per CLI subcommand. `mod.rs` re-exports the public API
  (`build_entry`, `format_status`, `format_list`, `build_extension`,
  `ExtensionFile`) and owns the crate-private `needs_attention` filter
  shared by `format_status` and `format_list`. `set.rs`, `status.rs`,
  `list.rs`, `agent_extension.rs` each implement one subcommand's
  helper, public API at the top, private helpers below. No `std::env`,
  `std::io`, `std::time`, or `std::fs` imports anywhere.
- `crates/agent-status/src/main.rs` — clap glue, impure adapter; imports from
  the library via `agent_status::…`.
- `crates/agent-status/src/lib.rs` — module declarations + crate-root
  re-exports (`AttentionEntry`, `StateStore`, `Agent`, etc.) consumed by
  `agent-switcher`.
- `crates/agent-status/src/agents/…` — per-agent JSON parsing.

`agent-switcher` is binary-only:

- `crates/agent-switcher/src/main.rs` — terminal setup + crossterm event loop.
- `crates/agent-switcher/src/app.rs` — `App` state and key-event reducer.
- `crates/agent-switcher/src/ui.rs` — ratatui rendering (pure, takes `&App`).
- `crates/agent-switcher/src/filter.rs` — pure filter/match logic.

Both share the `agent-status` library, which is the only crate that depends on
serde/clap/serde_json.

## Adding a new agent

Each AI coding agent we integrate with lives in its own file under `crates/agent-status/src/agents/`. To plug in a new one:

1. Create `crates/agent-status/src/agents/<agent>.rs` with a unit struct (e.g. `pub struct CodexCliAgent;`) implementing `agents::Agent` (in `crates/agent-status/src/agents/mod.rs`). Implement `name()` returning the lowercase hyphenated identifier (e.g. `"codex-cli"`) and `extract_session_id()` parsing whatever field the agent's hook payload uses for the session/conversation key.
2. Register the new agent in `agents::by_name` so the CLI's `--agent` flag can resolve it.
3. Add a corresponding variant to the `AgentName` enum in `crates/agent-status/src/agents/mod.rs` (with the `#[value(name = "<wire-name>")]` clap attribute), and add branches in `AgentName::name()`, `AgentName::agent()`, and `commands::build_extension`. The exhaustive match in `build_extension` will produce a compile error until you do.
4. Add unit tests for `extract_session_id` covering the four standard cases (valid id, missing field, empty string, invalid JSON) plus any field-name-specific edge cases (e.g. Cursor's `conversation_id` vs `session_id` switch on `sessionStart`).
5. Document the agent's hook config in README.md alongside the existing Claude Code section.

No changes to `state.rs` or `main.rs` should be needed for a typical new agent — only `commands/agent_extension.rs` changes, to add the new `AgentName` branch in `build_extension`.

Some agents (e.g. pi at `pi.dev`) don't have a shell-hook mechanism — their lifecycle events fire in-process inside the agent's runtime. For those, the Rust `Agent` impl is unchanged (it still reads JSON from stdin), but the integration ships an additional bridge file that runs inside the agent and shells out to `agent-status`. See `extensions/pi-coding-agent.ts` and the pi section of `README.md`. The `Agent::extract_session_id` contract still applies — the bridge constructs the JSON payload, so we control the field name.

Agents that accept a per-launch file-argument (Claude Code's `--settings <file>`, pi's `-e <file>`) can be installed via a shell alias — `alias claude='claude --settings "$(agent-status agent-extension)"'`, `alias pi='pi -e "$(agent-status agent-extension --agent pi-coding-agent)"'`. The `agent-extension` subcommand calls `build_extension` in `commands/agent_extension.rs`, which takes an `AgentName` and returns an `ExtensionFile { filename, content }`. The match is exhaustive over `AgentName`, so adding a new variant requires a corresponding branch (the compiler enforces it). Each branch picks the right filename extension (`.json` / `.ts`) and the right content shape: Claude Code's JSON via `serde_json::json!`, the TypeScript bridges via `include_str!` from `extensions/<agent>.ts` plus a one-line substitution of the `BIN` constant. opencode's plugin loader is directory-based with no per-launch flag, so it's a `cp` install rather than an alias — same `build_extension` path. For TypeScript bridges, point at the corresponding `.ts` file under `extensions/` and rely on `TS_BIN_RESOLUTION_LINE` for the substitution.

## Wire compatibility

`AttentionEntry`'s original five field names (`project`, `cwd`, `event`, `tmux_pane`, `ts`) match the bash precursor of this tool; mixed-version setups must not break. The test `entry_matches_bash_plan_field_names` in `state.rs` is the guard — don't rename fields without updating it deliberately.

The `agent` field was added in the v0.2.0 refactor. It is non-optional in the current schema, so old state files from the bash precursor or pre-v0.2.0 binary (which lack `agent`) will fail to deserialize on the next `list` and be silently skipped. This is acceptable: stale entries are cleaned up naturally on the first `set`/`clear` of each session after upgrading.

The `pid` field was added later still and is also optional in the schema for the same reason — entries written by older binaries simply skip the PID-based auto-prune (`is_pid_alive` is only consulted when `pid` is `Some`).

The `event` field is the `state::Event` enum: `Notify`, `Done`, `Working`,
`Idle`, plus `Unknown(String)` for forward compat. On the wire it
serializes as a plain lowercase string (`"notify"`, `"done"`, …) via a
hand-written Serialize/Deserialize, so the on-disk JSON shape and the
bash precursor's vocabulary are unchanged. Unrecognized event strings
deserialize to `Event::Unknown(s)` and re-serialize verbatim, so a new
hook event added by a future agent does not break older binaries that
read the same state directory. `Event::needs_attention` (on the enum
itself, not a free function) is the single source of truth for which
values surface in the tmux status indicator and legacy fzf
TSV — currently everything except `Working` and `Idle`, which means
`Unknown` is surfaced by default (better to over-report than silently
hide a new event type). `agent-switcher` ignores that filter and reads
the store directly, so every row shows up there regardless of event
value.

The switcher groups rows by event priority for display: `Notify` →
`Done` → `Idle` → `Working` → `Unknown`, with `ts` (then `session_id`)
as the within-group tiebreaker that matches `StateStore::list`'s order.
The sort lives in `app::sort_by_priority` and is applied both in
`App::new` and on every `App::tick` so the order is stable after each
state-directory refresh. A non-selectable bold section header (labels:
`Needs your attention` / `Done` / `Idle` / `Working` / `Other`) is rendered before
the first row of each non-empty group; empty groups produce no header.
The group transitions are detected in `ui::sessions_table` by calling
`app::event_rank` — the single source of truth shared with the sort,
so display order and header placement can't drift. To change the
grouping, update `event_rank` in `agent-switcher/src/app.rs` and the
matching label/color pair in `ui::section_header_label_color`; the
variant order on `Event` itself is not load-bearing.

Hook → event mapping for Claude Code (kept in
`build_claude_code_settings`):
- `SessionStart` → `idle` (placeholder so the switcher sees the session
  from the moment Claude launches, even before the first prompt)
- `UserPromptSubmit`, `PreToolUse` → `working`
- `PermissionRequest` → `notify`
- `Stop` → `done`
- `SessionEnd` → `clear` (the only event that removes the row)

`Notification` is intentionally NOT subscribed to even though it
superficially looks like a `notify` source. Claude Code fires it for
several matchers — `permission_prompt` (duplicates `PermissionRequest`,
which fires first and is the canonical signal), `idle_prompt` (a
periodic "Claude is waiting for your input" reminder fired on a timer),
`auth_success`, `elicitation_*`, etc. Wiring `Notification → notify`
caused freshly-cleared (`/clear`) sessions to flip back from `idle` to
`notify` once the idle reminder timer fired. `PermissionRequest`'s
payload carries `tool_name` + `tool_input`, so the activity-string
synthesizer in `extract_message` produces a more specific message
("Running: cargo test") than Notification's generic text would.

For PreToolUse, the hook payload's `tool_name`/`tool_input` are turned into
a one-line activity string (`format_pre_tool_use_activity` in
`agents/claude_code.rs`) and stored as the entry's `message`, so the
switcher's Activity column shows what the agent is doing in real time.

pi's bridge (`extensions/pi-coding-agent.ts`) mirrors this mapping:
`session_start` → `idle`, `before_agent_start` and
`tool_execution_start` → `working`, `agent_end` → `done`,
`session_shutdown` → `clear`. Activity messages are formatted in the
bridge (not the Rust side) because pi's tool input schemas live in
TypeScript: `before_agent_start` uses the first line of the user
prompt, `tool_execution_start` uses a `formatToolActivity` analogue of
`format_pre_tool_use_activity` keyed by pi's lowercase tool names
(bash/read/edit/write/grep/find/ls), and `agent_end` walks
`event.messages` for the assistant's last `type: "text"` content.
opencode does not yet emit `working` or `idle`; its hook semantics are
unchanged. To promote a new event from `Unknown` to a first-class
variant: add it to `Event` in `state.rs` (plus the `From<&str>` /
`From<String>` / `as_str` branches), update `Event::needs_attention` if
it should be hidden from the tmux indicator, add a rank in
`agent-switcher/src/app::event_rank`, and add a UI arm in
`agent-switcher/src/ui.rs`'s `match &e.event` block (the exhaustiveness
check on `Event` will surface every spot that needs touching).

## Dev / installed binary divergence

Claude Code hooks and tmux's `status-right` invoke whatever `agent-status` resolves to on `$PATH` — typically the `cargo install` copy in `~/.cargo/bin`, NOT the freshly compiled `target/release/agent-status` in this checkout. To exercise source changes against the real hook flow, reinstall:

```sh
cargo install --path crates/agent-status   --force
cargo install --path crates/agent-switcher --force
```

`cargo test` always builds a fresh test binary (via `CARGO_BIN_EXE_agent-status`), so the test suite is unaffected by what's installed.

Historical note: an earlier convention put both binaries at `~/.claude/bin/agent-status` / `~/.claude/bin/agent-switcher` so hook commands could hardcode an absolute path. The current alias-based wiring regenerates `claude-code.json` (and the pi/opencode TS bridges) on every launch with `current_exe()` baked in, so that bespoke directory served no purpose and was dropped — `cargo install` is the only path now.

## Subtle bits

- The state directory suffix (`agent-status`, joined with `XDG_RUNTIME_DIR`) is hardcoded in `StateStore::from_env`. Renaming the binary alone does not move the state dir.
- `StateStore::dir()` is `#[cfg(test)]` only — it exists for the env-path test. Lift the cfg if production code ever needs it.
- `validate_session_id` rejects empty strings, `/`, the platform separator, `.`, and `..` — defense in case Claude Code ever sends adversarial session IDs.
- `refresh_tmux` redirects child stderr/stdout to /dev/null. Hooks fire outside tmux often, and the inherited stderr noise pollutes Claude Code's notification feed.

## Plans / history

Implementation plans live under `docs/superpowers/plans/`. Useful for design context, but they're frozen in time — not authoritative for current code state.

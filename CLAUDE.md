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
- `crates/agent-status/src/commands.rs` — pure helpers (`build_entry`,
  `format_status`, `format_list`, `build_extension`). No `std::env`,
  `std::io`, `std::time`, or `std::fs` imports.
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
3. Add unit tests for `extract_session_id` covering the four standard cases (valid id, missing field, empty string, invalid JSON) plus any field-name-specific edge cases (e.g. Cursor's `conversation_id` vs `session_id` switch on `sessionStart`).
4. Document the agent's hook config in README.md alongside the existing Claude Code section.

No changes to `state.rs`, `commands.rs`, or `main.rs` should be needed for a typical new agent — that's the test of the abstraction.

Some agents (e.g. pi at `pi.dev`) don't have a shell-hook mechanism — their lifecycle events fire in-process inside the agent's runtime. For those, the Rust `Agent` impl is unchanged (it still reads JSON from stdin), but the integration ships an additional bridge file that runs inside the agent and shells out to `agent-status`. See `extensions/pi-coding-agent.ts` and the pi section of `README.md`. The `Agent::extract_session_id` contract still applies — the bridge constructs the JSON payload, so we control the field name.

Agents that accept a per-launch file-argument (Claude Code's `--settings <file>`, pi's `-e <file>`) can be installed via a shell alias — `alias claude='claude --settings "$(agent-status agent-extension)"'`, `alias pi='pi -e "$(agent-status agent-extension --agent pi-coding-agent)"'`. The `agent-extension` subcommand calls `build_extension` in `commands.rs`, which returns `Option<ExtensionFile { filename, content }>`. Each branch in `build_extension`'s match picks the right filename extension (`.json` / `.ts`) and the right content shape: Claude Code's JSON via `serde_json::json!`, the TypeScript bridges via `include_str!` from `extensions/<agent>.ts` plus a one-line substitution of the `BIN` constant. opencode's plugin loader is directory-based with no per-launch flag, so it's a `cp` install rather than an alias — same `build_extension` path. To wire a new alias-friendly agent, add a branch to `build_extension`; for TypeScript bridges, point at the corresponding `.ts` file under `extensions/` and rely on `TS_BIN_RESOLUTION_LINE` for the substitution.

## Wire compatibility

`AttentionEntry`'s original five field names (`project`, `cwd`, `event`, `tmux_pane`, `ts`) match the bash precursor of this tool; mixed-version setups must not break. The test `entry_matches_bash_plan_field_names` in `state.rs` is the guard — don't rename fields without updating it deliberately.

The `agent` field was added in the v0.2.0 refactor. It is non-optional in the current schema, so old state files from the bash precursor or pre-v0.2.0 binary (which lack `agent`) will fail to deserialize on the next `list` and be silently skipped. This is acceptable: stale entries are cleaned up naturally on the first `set`/`clear` of each session after upgrading.

The `pid` field was added later still and is also optional in the schema for the same reason — entries written by older binaries simply skip the PID-based auto-prune (`is_pid_alive` is only consulted when `pid` is `Some`).

The `event` field has four documented values: `notify`, `done`, `working`,
`idle`. The `needs_attention` helper in `commands.rs` is the single source of
truth for which values surface in the tmux status indicator and legacy fzf
TSV — currently `notify` and `done`. `agent-switcher` ignores that filter
and reads the store directly, so every row shows up there regardless of
event value.

Hook → event mapping for Claude Code (kept in
`build_claude_code_settings`):
- `SessionStart` → `idle` (placeholder so the switcher sees the session
  from the moment Claude launches, even before the first prompt)
- `UserPromptSubmit`, `PreToolUse` → `working`
- `Notification`, `PermissionRequest` → `notify`
- `Stop` → `done`
- `SessionEnd` → `clear` (the only event that removes the row)

pi and opencode do not yet emit `working` or `idle`; their hook semantics
are unchanged. New event values should be added to the match in
`needs_attention` and to the switcher's `match e.event.as_str()` block in
`agent-switcher/src/ui.rs`.

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

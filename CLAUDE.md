# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

`agent-status` is a small Rust CLI that integrates with Claude Code hooks and tmux's `status-right` to show which AI coding agent sessions are waiting on user input. State is one JSON file per session under `${XDG_RUNTIME_DIR:-/tmp}/agent-status/`, keyed by `session_id`. README.md covers end-user install and config; this file is for working on the binary itself.

## Build / test / lint

```sh
cargo test                                                            # 95 tests (83 unit + 12 integration)
cargo clippy --all-targets --all-features --locked -- -D warnings     # required gate
cargo build --release                                                 # ~500 KB stripped binary
```

Run a single unit test or a specific integration test:

```sh
cargo test entry_roundtrips_through_json
cargo test --test cli end_to_end_set_status_clear
```

The crate has `[lints.rust]` (`unsafe_code = "forbid"`, `nonstandard_style = "deny"`) and `[lints.clippy]` (`all = "deny"`, `pedantic = "warn"`) in Cargo.toml — clippy must pass with `-D warnings`.

## Module split (load-bearing)

Three files with a deliberate division of labor — preserve it when adding logic:

- `src/state.rs` — owns all filesystem I/O. `AttentionEntry` (the on-disk struct) and `StateStore` (read/write/list state files). Tests use `tempfile::TempDir` for isolation.
- `src/commands.rs` — purely pure helpers (`build_entry`, `format_status`, `format_list`). No `std::env`, `std::io`, `std::time`, or `std::fs` imports — that's what makes the bulk of the logic unit-testable.
- `src/main.rs` — the impure adapter. clap `Parser`/`Subcommand` derives plus `run_*` glue functions that read stdin/env/time and call into the pure helpers.
- `src/agents/mod.rs` — defines the `Agent` trait (`name()` + `extract_session_id()`) and the `by_name` registry that resolves the `--agent` flag to a concrete impl.
- `src/agents/claude_code.rs` — `ClaudeCodeAgent` implementing `Agent` for Claude Code hook payloads.

When adding logic, decide first whether it can live in `commands.rs` (pure → easy unit test) or has to be in `state.rs`/`main.rs` (impure → integration test territory).

## Adding a new agent

Each AI coding agent we integrate with lives in its own file under `src/agents/`. To plug in a new one:

1. Create `src/agents/<agent>.rs` with a unit struct (e.g. `pub struct CodexCliAgent;`) implementing `agents::Agent` (in `src/agents/mod.rs`). Implement `name()` returning the lowercase hyphenated identifier (e.g. `"codex-cli"`) and `extract_session_id()` parsing whatever field the agent's hook payload uses for the session/conversation key.
2. Register the new agent in `agents::by_name` so the CLI's `--agent` flag can resolve it.
3. Add unit tests for `extract_session_id` covering the four standard cases (valid id, missing field, empty string, invalid JSON) plus any field-name-specific edge cases (e.g. Cursor's `conversation_id` vs `session_id` switch on `sessionStart`).
4. Document the agent's hook config in README.md alongside the existing Claude Code section.

No changes to `state.rs`, `commands.rs`, or `main.rs` should be needed for a typical new agent — that's the test of the abstraction.

Some agents (e.g. pi at `pi.dev`) don't have a shell-hook mechanism — their lifecycle events fire in-process inside the agent's runtime. For those, the Rust `Agent` impl is unchanged (it still reads JSON from stdin), but the integration ships an additional bridge file that runs inside the agent and shells out to `agent-status`. See `extensions/pi-coding-agent.ts` and the pi section of `README.md`. The `Agent::extract_session_id` contract still applies — the bridge constructs the JSON payload, so we control the field name.

Agents that accept a `--settings <file>` flag (currently only Claude Code) can be installed via a shell alias instead of manual settings.json editing: `alias claude='claude --settings "$(agent-status agent-settings)"'`. The `agent-settings` subcommand calls `build_settings_json` in `commands.rs` (pure JSON construction using `serde_json::json!`) and writes the result to `${XDG_RUNTIME_DIR:-/tmp}/agent-status/settings/<agent>.json` using `current_exe()` for the binary path. To wire a new agent here, extend `build_settings_json` to match on the agent name and return the agent's hook JSON.

## Wire compatibility

`AttentionEntry`'s original five field names (`project`, `cwd`, `event`, `tmux_pane`, `ts`) match the bash precursor of this tool; mixed-version setups must not break. The test `entry_matches_bash_plan_field_names` in `state.rs` is the guard — don't rename fields without updating it deliberately.

The `agent` field was added in the v0.2.0 refactor. It is non-optional in the current schema, so old state files from the bash precursor or pre-v0.2.0 binary (which lack `agent`) will fail to deserialize on the next `list` and be silently skipped. This is acceptable: stale entries are cleaned up naturally on the first `set`/`clear` of each session after upgrading.

The `pid` field was added later still and is also optional in the schema for the same reason — entries written by older binaries simply skip the PID-based auto-prune (`is_pid_alive` is only consulted when `pid` is `Some`).

## Dev / installed binary divergence

Claude Code hooks and tmux's `status-right` invoke the binary at `~/.claude/bin/agent-status`, NOT the freshly compiled `target/release/agent-status` in this checkout. To exercise source changes against the real hook flow, reinstall:

```sh
cargo build --release && install -m 0755 target/release/agent-status ~/.claude/bin/agent-status
```

`cargo test` always builds a fresh test binary (via `CARGO_BIN_EXE_agent-status`), so the test suite is unaffected by what's installed.

## Subtle bits

- The state directory suffix (`agent-status`, joined with `XDG_RUNTIME_DIR`) is hardcoded in `StateStore::from_env`. Renaming the binary alone does not move the state dir.
- `StateStore::dir()` is `#[cfg(test)]` only — it exists for the env-path test. Lift the cfg if production code ever needs it.
- `validate_session_id` rejects empty strings, `/`, the platform separator, `.`, and `..` — defense in case Claude Code ever sends adversarial session IDs.
- `refresh_tmux` redirects child stderr/stdout to /dev/null. Hooks fire outside tmux often, and the inherited stderr noise pollutes Claude Code's notification feed.

## Plans / history

Implementation plans live under `docs/superpowers/plans/`. Useful for design context, but they're frozen in time — not authoritative for current code state.

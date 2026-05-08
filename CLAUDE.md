# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

`claude-status` is a small Rust CLI that integrates with Claude Code hooks and tmux's `status-right` to show which Claude Code sessions are waiting on user input. State is one JSON file per session under `${XDG_RUNTIME_DIR:-/tmp}/claude-status/`, keyed by `session_id`. README.md covers end-user install and config; this file is for working on the binary itself.

## Build / test / lint

```sh
cargo test                                                            # 21 tests (18 unit + 3 integration)
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
- `src/commands.rs` — purely pure helpers (`extract_session_id`, `build_entry`, `format_status`, `format_list`). No `std::env`, `std::io`, `std::time`, or `std::fs` imports — that's what makes the bulk of the logic unit-testable.
- `src/main.rs` — the impure adapter. clap `Parser`/`Subcommand` derives plus `run_*` glue functions that read stdin/env/time and call into the pure helpers.

When adding logic, decide first whether it can live in `commands.rs` (pure → easy unit test) or has to be in `state.rs`/`main.rs` (impure → integration test territory).

## Wire compatibility

`AttentionEntry`'s field names (`project`, `cwd`, `event`, `tmux_pane`, `ts`) match the bash precursor of this tool; mixed-version setups must not break. The test `entry_matches_bash_plan_field_names` in `state.rs` is the guard — don't rename fields without updating it deliberately.

## Dev / installed binary divergence

Claude Code hooks and tmux's `status-right` invoke the binary at `~/.claude/bin/claude-status`, NOT the freshly compiled `target/release/claude-status` in this checkout. To exercise source changes against the real hook flow, reinstall:

```sh
cargo build --release && install -m 0755 target/release/claude-status ~/.claude/bin/claude-status
```

`cargo test` always builds a fresh test binary (via `CARGO_BIN_EXE_claude-status`), so the test suite is unaffected by what's installed.

## Subtle bits

- The state directory suffix (`claude-status`, joined with `XDG_RUNTIME_DIR`) is hardcoded in `StateStore::from_env`. Renaming the binary alone does not move the state dir.
- `StateStore::dir()` is `#[cfg(test)]` only — it exists for the env-path test. Lift the cfg if production code ever needs it.
- `validate_session_id` rejects empty strings, `/`, the platform separator, `.`, and `..` — defense in case Claude Code ever sends adversarial session IDs.
- `refresh_tmux` redirects child stderr/stdout to /dev/null. Hooks fire outside tmux often, and the inherited stderr noise pollutes Claude Code's notification feed.

## Plans / history

Implementation plans live under `docs/superpowers/plans/`. The original bash precursor is `claude-tmux-attention-plan.md`. Useful for design context, but they're frozen in time — not authoritative for current code state.

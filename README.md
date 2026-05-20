# agent-status

Tmux-integrated indicators showing which AI coding agent sessions are waiting on
user input. Supports [Claude Code], [pi], and [opencode]; the architecture is
set up to plug in additional agents (Codex CLI, Cursor CLI) without
restructuring.

```text
$ agent-status status        # one session waiting
[!] agent-status

$ agent-status status        # multiple sessions waiting
[!] 3 projects waiting
```

This repo publishes two crates:

| Crate                                                   | Description                                                                                                                                   |
| ------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| **[`agent-status`](crates/agent-status/README.md)**     | CLI + library. Records per-session state, renders the tmux `status-right` indicator, and generates the hook/extension configs for each agent. |
| **[`agent-switcher`](crates/agent-switcher/README.md)** | ratatui popup TUI. Lists every recorded session and switches tmux panes on <kbd>Enter</kbd>.                                                  |

## Install

```sh
cargo install agent-status agent-switcher
```

Then follow [`agent-status`'s README](crates/agent-status/README.md) for the
hook wiring (Claude Code / pi / opencode) and tmux configuration, and
[`agent-switcher`'s README](crates/agent-switcher/README.md) for the popup
picker bind.

## Development

```sh
cargo test                                                            # workspace-wide test suite
cargo clippy --all-targets --all-features --locked -- -D warnings     # required gate
cargo build --release                                                 # both binaries, ~1.1 MB combined
```

See [`CLAUDE.md`](CLAUDE.md) for the codebase guide (module layout, adding a new
agent, wire compatibility).

## License

MIT — see [LICENSE](LICENSE).

[Claude Code]: https://claude.com/claude-code
[pi]: https://pi.dev
[opencode]: https://opencode.ai

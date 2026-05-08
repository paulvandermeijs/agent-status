# agent-status

A small Rust CLI that shows in tmux's `status-right` which AI coding agent sessions are waiting on user input. Currently supports Claude Code; the architecture is set up to plug in additional agents (Codex CLI, Cursor CLI, OpenCode) without restructuring.

```text
$ agent-status status        # one session waiting
[!] agent-status

$ agent-status status        # multiple sessions waiting
[!] 3 projects waiting

$ agent-status status        # nothing waiting
                              # (no output, exit 0)
```

## How it works

Claude Code [hooks][hooks] fire `agent-status set` on `Notification` and `Stop`, and `agent-status clear` on `UserPromptSubmit` / `SessionStart` / `SessionEnd`. Each `set` writes one JSON file per session under `${XDG_RUNTIME_DIR:-/tmp}/agent-status/`, keyed by `session_id`. tmux's `status-right` invokes `agent-status status` on its refresh interval; the command lists the state directory and renders the indicator (project name for one waiting session, count for many, nothing for none).

No daemon. The filesystem is the state store; each session writes only its own keyed file, so concurrent writers never contend.

## Install

```sh
cargo build --release
mkdir -p ~/.claude/bin
install -m 0755 target/release/agent-status ~/.claude/bin/agent-status
```

`~/.claude/bin` is one option; any directory works as long as the absolute path matches what you put in the hook commands and tmux config below. The binary is around 500 KB and has no runtime dependencies (tmux is invoked best-effort to refresh the status bar; if it isn't running, the failure is silenced).

## Configure

### Claude Code hooks (`~/.claude/settings.json`)

Merge the following into the top-level `hooks` block:

```json
{
  "hooks": {
    "Notification":     [{ "hooks": [{ "type": "command", "command": "$HOME/.claude/bin/agent-status set --agent claude-code notify" }] }],
    "Stop":             [{ "hooks": [{ "type": "command", "command": "$HOME/.claude/bin/agent-status set --agent claude-code done"   }] }],
    "UserPromptSubmit": [{ "hooks": [{ "type": "command", "command": "$HOME/.claude/bin/agent-status clear --agent claude-code"      }] }],
    "SessionStart":     [{ "hooks": [{ "type": "command", "command": "$HOME/.claude/bin/agent-status clear --agent claude-code"      }] }],
    "SessionEnd":       [{ "hooks": [{ "type": "command", "command": "$HOME/.claude/bin/agent-status clear --agent claude-code"      }] }]
  }
}
```

### tmux (`~/.tmux.conf`)

Drop `#($HOME/.claude/bin/agent-status status)` into your existing `status-right` wherever you want the indicator to appear, and lower the refresh interval so updates feel snappy:

```tmux
set -g status-interval 5
# example: prepend the indicator to whatever status-right you already use
set -g status-right "#($HOME/.claude/bin/agent-status status) <your existing status-right here>"
```

Optional popup picker (prefix + `C-a`) for jumping to the waiting pane — requires `fzf`:

```tmux
bind-key C-a display-popup -E -w 60% -h 40% \
  "$HOME/.claude/bin/agent-status list | fzf --with-nth=2.. --delimiter='\\t' --prompt='Jump to> ' \
    | cut -f1 | xargs -r -I{} tmux switch-client -t {}"
```

Reload with `tmux source-file ~/.tmux.conf`.

## Usage

```sh
agent-status --help                       # top-level help
agent-status set [EVENT] [--agent NAME]   # mark this session as waiting (reads JSON on stdin)
agent-status clear [--agent NAME]         # clear this session's state (reads JSON on stdin)
agent-status status                       # print the status-right line, empty if nothing waiting
agent-status list                         # print TSV (pane, project, event) per waiting session
```

`set` and `clear` expect a JSON object on stdin with at least `{"session_id": "..."}`. Empty or missing `session_id` is a silent no-op.

## State location

`${XDG_RUNTIME_DIR:-/tmp}/agent-status/<session_id>` — one file per active session. Inspectable with `ls`/`cat`. Format:

```json
{"agent":"claude-code","project":"agent-status","cwd":"/path/to/project","event":"notify","tmux_pane":"%17","ts":1778163565}
```

## Development

```sh
cargo test                                                   # 25 tests (22 unit + 3 integration)
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo build --release                                        # ~500 KB stripped binary
```

## Caveats

- **The `Stop` hook fires on every turn end**, so any session that just finished a response shows up as "waiting" until you send the next prompt. Intentional — the whole point is to know which session needs you while you're heads-down elsewhere. Drop the `Stop` line from `settings.json` if it proves too eager.
- **Stale state on abnormal exit.** If a Claude Code process dies without firing its session-end hook, its state file lingers. macOS's tmpwatch and reboots eventually clean `/tmp`; on Linux with `XDG_RUNTIME_DIR`, files vanish at logout.
- **Architecture-specific binary.** The compiled binary is platform-locked. On a new machine, rebuild from source.

[hooks]: https://docs.claude.com/en/docs/claude-code/hooks

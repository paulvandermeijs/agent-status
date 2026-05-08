# claude-status

A small Rust CLI that shows in tmux's `status-right` which Claude Code sessions are waiting on user input.

```text
[!] claude-status   14:23 08-May-26     # one session waiting
[!] 3 projects waiting   14:23 08-May-26 # multiple
```

## How it works

Claude Code [hooks][hooks] fire `claude-status set` on `Notification` and `Stop`, and `claude-status clear` on `UserPromptSubmit` / `SessionStart` / `SessionEnd`. Each `set` writes one JSON file per session under `${XDG_RUNTIME_DIR:-/tmp}/claude-status/`, keyed by `session_id`. tmux's `status-right` runs `claude-status status` every 5 seconds and prints the count of files in that directory.

No daemon. The filesystem is the state store; each session writes only its own keyed file, so concurrent writers never contend.

## Install

```sh
cargo build --release
install -m 0755 target/release/claude-status ~/.claude/bin/claude-status
```

The binary is around 500 KB and has no runtime dependencies (tmux is invoked best-effort to refresh the status bar; if it isn't running, the failure is silenced).

## Configure

### Claude Code hooks (`~/.claude/settings.json`)

Merge the following into the top-level `hooks` block:

```json
{
  "hooks": {
    "Notification":     [{ "hooks": [{ "type": "command", "command": "$HOME/.claude/bin/claude-status set notify" }] }],
    "Stop":             [{ "hooks": [{ "type": "command", "command": "$HOME/.claude/bin/claude-status set done"   }] }],
    "UserPromptSubmit": [{ "hooks": [{ "type": "command", "command": "$HOME/.claude/bin/claude-status clear"      }] }],
    "SessionStart":     [{ "hooks": [{ "type": "command", "command": "$HOME/.claude/bin/claude-status clear"      }] }],
    "SessionEnd":       [{ "hooks": [{ "type": "command", "command": "$HOME/.claude/bin/claude-status clear"      }] }]
  }
}
```

### tmux (`~/.tmux.conf`)

```tmux
set -g status-interval 5
set -g status-right "#($HOME/.claude/bin/claude-status status)  %H:%M %d-%b-%y "

# popup picker (prefix + C-a) — needs fzf
bind-key C-a display-popup -E -w 60% -h 40% \
  "$HOME/.claude/bin/claude-status list | fzf --with-nth=2.. --delimiter='\\t' --prompt='Jump to> ' \
    | cut -f1 | xargs -r -I{} tmux switch-client -t {}"
```

Reload with `tmux source-file ~/.tmux.conf`.

## Usage

```sh
claude-status --help                  # top-level help
claude-status set [EVENT]             # mark this session as waiting (reads JSON on stdin)
claude-status clear                   # clear this session's state (reads JSON on stdin)
claude-status status                  # print the status-right line, empty if nothing waiting
claude-status list                    # print TSV (pane, project, event) per waiting session
```

`set` and `clear` expect a JSON object on stdin with at least `{"session_id": "..."}`. Empty or missing `session_id` is a silent no-op.

## State location

`${XDG_RUNTIME_DIR:-/tmp}/claude-status/<session_id>` — one file per active session. Inspectable with `ls`/`cat`. Format:

```json
{"project":"claude-status","cwd":"/path/to/project","event":"notify","tmux_pane":"%17","ts":1778163565}
```

## Development

```sh
cargo test                                                   # 21 tests (18 unit + 3 integration)
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo build --release                                        # ~500 KB stripped binary
```

## Caveats

- **The `Stop` hook fires on every turn end**, so any session that just finished a response shows up as "waiting" until you send the next prompt. Intentional — the whole point is to know which session needs you while you're heads-down elsewhere. Drop the `Stop` line from `settings.json` if it proves too eager.
- **Stale state on abnormal exit.** If a Claude Code process dies without firing its session-end hook, its state file lingers. macOS's tmpwatch and reboots eventually clean `/tmp`; on Linux with `XDG_RUNTIME_DIR`, files vanish at logout.
- **Architecture-specific binary.** The compiled binary is platform-locked. On a new machine, rebuild from source.

[hooks]: https://docs.claude.com/en/docs/claude-code/hooks

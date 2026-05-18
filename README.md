# agent-status

A small Rust CLI that shows in tmux's `status-right` which AI coding agent sessions are waiting on user input. Supports [Claude Code](https://claude.com/claude-code), [pi](https://pi.dev), and [opencode](https://opencode.ai); the architecture is set up to plug in additional agents (Codex CLI, Cursor CLI) without restructuring.

```text
$ agent-status status        # one session waiting
[!] agent-status

$ agent-status status        # multiple sessions waiting
[!] 3 projects waiting

$ agent-status status        # nothing waiting
                              # (no output, exit 0)
```

## How it works

Claude Code [hooks][hooks] fire `agent-status set` on `Notification` and `Stop`, and `agent-status clear` on `UserPromptSubmit` / `PreToolUse` / `SessionStart` / `SessionEnd`. Each `set` writes one JSON file per session under `${XDG_RUNTIME_DIR:-/tmp}/agent-status/`, keyed by `session_id`. tmux's `status-right` invokes `agent-status status` on its refresh interval; the command lists the state directory and renders the indicator (project name for one waiting session, count for many, nothing for none).

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
    "PreToolUse":       [{ "hooks": [{ "type": "command", "command": "$HOME/.claude/bin/agent-status clear --agent claude-code"      }] }],
    "SessionStart":     [{ "hooks": [{ "type": "command", "command": "$HOME/.claude/bin/agent-status clear --agent claude-code"      }] }],
    "SessionEnd":       [{ "hooks": [{ "type": "command", "command": "$HOME/.claude/bin/agent-status clear --agent claude-code"      }] }]
  }
}
```

The `PreToolUse` hook fires before every tool call Claude makes. The hook issues a `clear` — which is idempotent — so the agent-status indicator correctly transitions out of "Needs Input" the moment Claude resumes work after you grant a permission, instead of staying "Needs Input" until the next `Stop` fires (which may be many tool calls later). The `PreToolUse` hook fires often, but `clear` skips refreshing tmux when there's nothing to remove, so the steady-state cost is one filesystem stat per tool call.

### Claude Code alias (optional, zero-config)

If you'd rather not edit `~/.claude/settings.json` by hand, drop this alias
into your shell rc (`.zshrc`, `.bashrc`, etc.):

```sh
alias claude='claude --settings "$(agent-status agent-settings)"'
```

The alias expands every time you run `claude`: `agent-status agent-settings`
writes a fresh settings JSON wiring the six hooks to its own absolute path
(found via `current_exe()`) and prints the file's path on stdout. Claude
Code then merges `--settings <file>` on top of your user/project settings,
so the alias adds the agent-status hooks without overwriting anything else
you've configured.

The generated file lives at
`${XDG_RUNTIME_DIR:-/tmp}/agent-status/settings/claude-code.json` and is
overwritten on each invocation — no cleanup needed. If you have agent-status
hooks already wired in `~/.claude/settings.json`, remove them before adding
the alias, otherwise each event fires twice (idempotent for `clear`, just
wasteful for `set`).

### pi (`~/.pi/agent/extensions/`)

Pi extensions run in-process, so the integration ships as a single TypeScript file you drop into pi's auto-discovery directory. Copy `extensions/pi-coding-agent.ts` from this repo:

```sh
mkdir -p ~/.pi/agent/extensions
cp extensions/pi-coding-agent.ts ~/.pi/agent/extensions/
```

Pi auto-discovers `~/.pi/agent/extensions/*.ts` on startup; no further configuration is required. The extension fires on these pi lifecycle events:

| pi event              | agent-status call                              |
|-----------------------|------------------------------------------------|
| `before_agent_start`  | `clear --agent pi-coding-agent` (user submitted a prompt) |
| `agent_end`           | `set --agent pi-coding-agent done` (agent finished a turn) |
| `session_start`       | `clear --agent pi-coding-agent`                |
| `session_shutdown`    | `clear --agent pi-coding-agent`                |

If your `agent-status` binary is not at `~/.claude/bin/agent-status`, set `AGENT_STATUS_BIN` in your shell environment before launching pi.

**Known limitation:** pi has no built-in "agent paused waiting for permission" event analogous to Claude Code's `Notification` hook — pi extensions handle confirmations in-process via `ctx.ui.confirm()`. So pi-coding-agent surfaces the "done" state but not a separate "needs attention" state. In practice the dominant signal is "agent finished a turn, waiting on next prompt" anyway.

### opencode (`~/.config/opencode/plugins/`)

opencode plugins run in-process, so the integration ships as a single TypeScript file you drop into opencode's auto-discovery directory. Copy `extensions/opencode.ts` from this repo:

```sh
mkdir -p ~/.config/opencode/plugins
cp extensions/opencode.ts ~/.config/opencode/plugins/
```

opencode auto-discovers files in `~/.config/opencode/plugins/` (global) and `.opencode/plugins/` (per-project) at startup; no further configuration is required. The plugin fires on these opencode events:

| opencode event       | agent-status call                                 |
|----------------------|---------------------------------------------------|
| `session.idle`       | `set --agent opencode done` (agent finished a turn) |
| `permission.updated` | `set --agent opencode notify` (agent paused for permission) |
| `session.created`    | `clear --agent opencode`                          |
| `session.deleted`    | `clear --agent opencode`                          |

In practice opencode persists sessions for resume, so `session.deleted` rarely fires on graceful exit — the `clear` arm is defensive. `session.created` likewise fires once at the start of each new session and resolves to a no-op clear (no state file to remove yet); it exists so a stale state file from a previous crash gets dropped at session start.

If your `agent-status` binary is not at `~/.claude/bin/agent-status`, set `AGENT_STATUS_BIN` in your shell environment before launching opencode.

Unlike pi, opencode emits a `permission.updated` event when an agent pauses for a permission prompt, so opencode supports both `notify` and `done` indicator states (full feature parity with Claude Code). The one wart: opencode has no event for "user submitted a prompt", so after a turn ends the indicator stays on `done` while the user types the next prompt — by design, since the session *is* the one that needs your attention.

### tmux (`~/.tmux.conf`)

Drop `#($HOME/.claude/bin/agent-status status)` into your existing `status-right` wherever you want the indicator to appear, and lower the refresh interval so updates feel snappy:

```tmux
set -g status-interval 5
# example: prepend the indicator to whatever status-right you already use
set -g status-right "#($HOME/.claude/bin/agent-status status) <your existing status-right here>"
```

Optional popup picker (prefix + `C-a`) for jumping to the waiting pane — requires `fzf`:

```tmux
bind-key C-a display-popup -E -w 80% -h 50% \
  "$HOME/.claude/bin/agent-status list | fzf \
     --delimiter='\\t' \
     --with-nth=3 \
     --preview='$HOME/.claude/bin/agent-status preview {1}' \
     --preview-window=right:50%:wrap \
     --prompt='Jump to> ' \
   | cut -f2 | xargs -r -I{} tmux switch-client -t {}"
```

`agent-status list` emits `session_id<TAB>pane<TAB>display` per waiting session.
fzf shows only the third column (`--with-nth=3`), uses the first column as the
preview key (`{1}` → `agent-status preview <session_id>`), and the post-selection
`cut -f2` extracts the pane to feed `tmux switch-client`. The display column
encodes the event as `[!]` (notify) or `[*]` (done) so fuzzy-find matches the
project, agent, and message snippet rather than the bare event word.

Reload with `tmux source-file ~/.tmux.conf`.

## Usage

```sh
agent-status --help                       # top-level help
agent-status set [EVENT] [--agent NAME]   # mark this session as waiting (reads JSON on stdin)
agent-status clear [--agent NAME]         # clear this session's state (reads JSON on stdin)
agent-status status                       # print the status-right line, empty if nothing waiting
agent-status list                         # print TSV (session_id, pane, display) per waiting session
agent-status preview <SESSION_ID>         # multi-line detail for one session (used by fzf --preview)
```

`set` and `clear` expect a JSON object on stdin with at least `{"session_id": "..."}`. Empty or missing `session_id` is a silent no-op.

## State location

`${XDG_RUNTIME_DIR:-/tmp}/agent-status/<session_id>` — one file per active session. Inspectable with `ls`/`cat`. Format:

```json
{"agent":"claude-code","project":"agent-status","cwd":"/path/to/project","event":"notify","tmux_pane":"%17","ts":1778163565,"message":"Permission required","pid":12345}
```

The `message` field is optional and only present when the agent's hook payload
supplies one (e.g. Claude Code's `Notification` event). Older state files written
before this field existed still load — `message` defaults to absent.

The `agent` field is `"claude-code"`, `"opencode"`, or `"pi-coding-agent"`; new agents use their own lowercase-hyphenated name.

The `pid` field records the agent process's PID (typically the claude / opencode / pi binary) so `agent-status status`, `list`, and `preview` can detect and remove entries whose owning process has died without firing its session-end hook. Files written by older binaries or the bash precursor — which lack `pid` — are never auto-pruned; they age out only on tmpfs cleanup. Such entries should disappear naturally after one `set`/`clear` cycle on the affected session.

## Development

```sh
cargo test                                                   # 95 tests (83 unit + 12 integration)
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo build --release                                        # ~500 KB stripped binary
```

## Caveats

- **The `Stop` hook fires on every turn end**, so any session that just finished a response shows up as "waiting" until you send the next prompt. Intentional — the whole point is to know which session needs you while you're heads-down elsewhere. Drop the `Stop` line from `settings.json` if it proves too eager.
- **opencode has no "user submitted a prompt" event**, so once `session.idle` marks an opencode session `done`, the indicator stays on `done` while you type the next prompt and only refreshes its timestamp on the next idle. Same intent as the `Stop` caveat above — the session *is* the one waiting on you.
- **Architecture-specific binary.** The compiled binary is platform-locked. On a new machine, rebuild from source.

[hooks]: https://docs.claude.com/en/docs/claude-code/hooks

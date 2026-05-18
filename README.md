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
install -m 0755 target/release/agent-status   ~/.claude/bin/agent-status
install -m 0755 target/release/agent-switcher ~/.claude/bin/agent-switcher
```

`~/.claude/bin` is one option; any directory works as long as the absolute path matches what you put in the hook commands and tmux config below. Both binaries are around 500 KB combined and have no runtime dependencies (tmux is invoked best-effort to refresh the status bar and switch panes; if it isn't running, the failure is silenced).

## Configure

### Claude Code

Drop this alias into your shell rc (`.zshrc`, `.bashrc`, etc.):

```sh
alias claude='claude --settings "$(agent-status agent-extension)"'
```

That's it. Each time you run `claude`, the alias expands so claude launches with `--settings <generated.json>` — a six-hook file that `agent-status` regenerates on every invocation from its own absolute path. Claude Code merges `--settings` on top of your user/project settings, so nothing you've already configured gets overwritten.

The generated file lives at `${XDG_RUNTIME_DIR:-/tmp}/agent-status/extensions/claude-code.json` and is rewritten every run — no cleanup, no env vars, no PATH manipulation.

#### Wiring the hooks manually (fallback)

If you'd rather see the hooks in your settings file, skip the alias and merge this into the top-level `hooks` block of `~/.claude/settings.json` instead:

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

Pick one route, not both. If you have manual hooks AND the alias, each event fires twice — idempotent for `clear` but wasteful for `set`.

The `PreToolUse` hook fires before every tool call Claude makes. The hook issues a `clear` — which is idempotent — so the agent-status indicator transitions out of "Needs Input" the moment Claude resumes work after you grant a permission, instead of staying "Needs Input" until the next `Stop` fires (which may be many tool calls later). The `PreToolUse` hook fires often, but `clear` skips refreshing tmux when there's nothing to remove, so the steady-state cost is one filesystem stat per tool call.

### pi

Drop this alias into your shell rc (`.zshrc`, `.bashrc`, etc.):

```sh
alias pi='pi -e "$(agent-status agent-extension --agent pi-coding-agent)"'
```

Each `pi` invocation regenerates `${XDG_RUNTIME_DIR:-/tmp}/agent-status/extensions/pi-coding-agent.ts` with the absolute path to the current `agent-status` binary baked into the bridge's `BIN` constant. pi's `-e <path>` flag loads the file as a one-shot extension, alongside whatever else you have under `~/.pi/agent/extensions/`.

The extension fires on these pi lifecycle events:

| pi event              | agent-status call                                 |
|-----------------------|---------------------------------------------------|
| `before_agent_start`  | `clear --agent pi-coding-agent` (user submitted a prompt) |
| `agent_end`           | `set --agent pi-coding-agent done` (agent finished a turn) |
| `session_start`       | `clear --agent pi-coding-agent`                   |
| `session_shutdown`    | `clear --agent pi-coding-agent`                   |

**Known limitation:** pi has no built-in "agent paused waiting for permission" event analogous to Claude Code's `Notification` hook — pi extensions handle confirmations in-process via `ctx.ui.confirm()`. So pi-coding-agent surfaces the "done" state but not a separate "needs attention" state. In practice the dominant signal is "agent finished a turn, waiting on next prompt" anyway.

#### Wiring the extension manually (fallback)

Prefer to drop the bridge into pi's discovery directory? Skip the alias and copy the file once:

```sh
mkdir -p ~/.pi/agent/extensions
cp extensions/pi-coding-agent.ts ~/.pi/agent/extensions/
```

pi auto-discovers `~/.pi/agent/extensions/*.ts` on startup; no further configuration is required. If your `agent-status` binary is not at `~/.claude/bin/agent-status`, set `AGENT_STATUS_BIN` in your shell environment before launching pi — the manual copy uses the env-var fallback the alias bypasses.

### opencode

opencode discovers plugins from `~/.config/opencode/plugins/` (global) or `.opencode/plugins/` (per-project) at startup — there's no per-launch extension flag, so the alias pattern can't apply. Generate the plugin and copy it once:

```sh
mkdir -p ~/.config/opencode/plugins
cp "$(agent-status agent-extension --agent opencode)" ~/.config/opencode/plugins/agent-status.ts
```

`agent-status agent-extension --agent opencode` writes a regenerated `${XDG_RUNTIME_DIR:-/tmp}/agent-status/extensions/opencode.ts` with the absolute path to the current `agent-status` binary baked in, and prints that path. The `cp` lands a fresh copy in opencode's discovery directory. Re-run this whenever you move or rebuild the `agent-status` binary so the baked-in path stays correct.

The plugin fires on these opencode events:

| opencode event       | agent-status call                                 |
|----------------------|---------------------------------------------------|
| `session.idle`       | `set --agent opencode done` (agent finished a turn) |
| `permission.updated` | `set --agent opencode notify` (agent paused for permission) |
| `session.created`    | `clear --agent opencode`                          |
| `session.deleted`    | `clear --agent opencode`                          |

In practice opencode persists sessions for resume, so `session.deleted` rarely fires on graceful exit — the `clear` arm is defensive. `session.created` likewise fires once at the start of each new session and resolves to a no-op clear (no state file to remove yet); it exists so a stale state file from a previous crash gets dropped at session start.

Unlike pi, opencode emits a `permission.updated` event when an agent pauses for a permission prompt, so opencode supports both `notify` and `done` indicator states (full feature parity with Claude Code). The one wart: opencode has no event for "user submitted a prompt", so after a turn ends the indicator stays on `done` while the user types the next prompt — by design, since the session *is* the one that needs your attention.

#### Wiring the plugin manually (fallback)

If you'd rather use the source `.ts` file directly (e.g. for shared dotfiles that bundle this repo as a submodule), copy `extensions/opencode.ts` instead:

```sh
mkdir -p ~/.config/opencode/plugins
cp extensions/opencode.ts ~/.config/opencode/plugins/
```

This version resolves the binary path from the `AGENT_STATUS_BIN` env var, defaulting to `~/.claude/bin/agent-status`. If your binary lives elsewhere, set `AGENT_STATUS_BIN` in your shell environment before launching opencode.

### tmux (`~/.tmux.conf`)

Drop `#($HOME/.claude/bin/agent-status status)` into your existing `status-right` wherever you want the indicator to appear, and lower the refresh interval so updates feel snappy:

```tmux
set -g status-interval 5
# example: prepend the indicator to whatever status-right you already use
set -g status-right "#($HOME/.claude/bin/agent-status status) <your existing status-right here>"
```

Optional popup picker (prefix + `C-a`) for jumping to the waiting pane — uses the bundled `agent-switcher` TUI:

```tmux
bind-key C-a display-popup -E -w 80% -h 50% "$HOME/.claude/bin/agent-switcher"
```

`agent-switcher` opens a small ratatui TUI: filter input at the top, the list of sessions in the middle, a help strip at the bottom. Type to filter (case-insensitive, matches across project / agent / message / session id); <kbd>Ctrl-N</kbd> and <kbd>Ctrl-P</kbd> (or arrow keys) move the selection; <kbd>Enter</kbd> runs `tmux switch-client` to the selected session's pane and exits; <kbd>Esc</kbd> or <kbd>Ctrl-C</kbd> exits without switching.

The list shows every recorded session — including those still working (animated spinner) — not just sessions waiting on your attention. That makes the popup useful as a general session jumper, while the status-bar indicator stays focused on "needs you now" sessions.

Reload with `tmux source-file ~/.tmux.conf`.

## Usage

```sh
agent-status --help                       # top-level help
agent-status set [EVENT] [--agent NAME]   # mark this session as waiting (reads JSON on stdin)
agent-status clear [--agent NAME]         # clear this session's state (reads JSON on stdin)
agent-status status                       # print the status-right line, empty if nothing waiting
agent-status list                         # print TSV (session_id, pane, display) per waiting session
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
cargo test                                                   # 100 tests (87 unit + 13 integration)
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo build --release                                        # ~500 KB stripped binary
```

## Caveats

- **The `Stop` hook fires on every turn end**, so any session that just finished a response shows up as "waiting" until you send the next prompt. Intentional — the whole point is to know which session needs you while you're heads-down elsewhere. Drop the `Stop` line from `settings.json` if it proves too eager.
- **opencode has no "user submitted a prompt" event**, so once `session.idle` marks an opencode session `done`, the indicator stays on `done` while you type the next prompt and only refreshes its timestamp on the next idle. Same intent as the `Stop` caveat above — the session *is* the one waiting on you.
- **Architecture-specific binary.** The compiled binary is platform-locked. On a new machine, rebuild from source.
- **Only Claude Code records a `working` state** today. pi and opencode sessions appear in the switcher when they're waiting (`done` / `notify`) but not while they're mid-turn. The hook semantics for those agents can be extended in a follow-up.

[hooks]: https://docs.claude.com/en/docs/claude-code/hooks

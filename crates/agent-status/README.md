# agent-status

A small Rust CLI + library that shows in tmux's `status-right` which AI coding agent sessions are waiting on user input. Supports [Claude Code], [pi], and [opencode]; the architecture is set up to plug in additional agents (Codex CLI, Cursor CLI) without restructuring.

```text
$ agent-status status        # one session waiting
[!] agent-status

$ agent-status status        # multiple sessions waiting
[!] 3 projects waiting

$ agent-status status        # nothing waiting
                              # (no output, exit 0)
```

## How it works

Claude Code [hooks][hooks] fire `agent-status set notify` on `Notification` and `PermissionRequest`, `set done` on `Stop`, `set working` on `UserPromptSubmit` and `PreToolUse`, `set idle` on `SessionStart`, and `clear` on `SessionEnd`. Each `set` writes one JSON file per session under `${XDG_RUNTIME_DIR:-/tmp}/agent-status/`, keyed by `session_id`. tmux's `status-right` invokes `agent-status status` on its refresh interval; the command lists the state directory and renders the indicator (project name for one waiting session, count for many, nothing for none).

No daemon. The filesystem is the state store; each session writes only its own keyed file, so concurrent writers never contend.

## Install

```sh
cargo install agent-status
```

The binary lands in `~/.cargo/bin`. As long as that's on your `$PATH` (Cargo's installer adds it by default), the alias-based wiring below works without any further configuration. The binary is around 550 KB stripped and has no runtime dependencies (tmux is invoked best-effort to refresh the status bar; if it isn't running, the failure is silenced).

For the popup picker, also install the companion crate:

```sh
cargo install agent-switcher
```

## Configure

### Claude Code

Drop this alias into your shell rc (`.zshrc`, `.bashrc`, etc.):

```sh
alias claude='claude --settings "$(agent-status agent-extension --agent claude-code)"'
```

That's it. Each time you run `claude`, the alias expands so claude launches with `--settings <generated.json>` — a seven-hook file that `agent-status` regenerates on every invocation from its own absolute path. Claude Code merges `--settings` on top of your user/project settings, so nothing you've already configured gets overwritten.

The generated file lives at `${XDG_RUNTIME_DIR:-/tmp}/agent-status/extensions/claude-code.json` and is rewritten every run — no cleanup, no env vars, no PATH manipulation.

#### Wiring the hooks manually (fallback)

If you'd rather see the hooks in your settings file, skip the alias and merge this into the top-level `hooks` block of `~/.claude/settings.json` instead:

```json
{
  "hooks": {
    "Notification":      [{ "hooks": [{ "type": "command", "command": "agent-status set --agent claude-code notify"  }] }],
    "PermissionRequest": [{ "hooks": [{ "type": "command", "command": "agent-status set --agent claude-code notify"  }] }],
    "Stop":              [{ "hooks": [{ "type": "command", "command": "agent-status set --agent claude-code done"    }] }],
    "UserPromptSubmit":  [{ "hooks": [{ "type": "command", "command": "agent-status set --agent claude-code working" }] }],
    "PreToolUse":        [{ "hooks": [{ "type": "command", "command": "agent-status set --agent claude-code working" }] }],
    "SessionStart":      [{ "hooks": [{ "type": "command", "command": "agent-status set --agent claude-code idle"   }] }],
    "SessionEnd":        [{ "hooks": [{ "type": "command", "command": "agent-status clear --agent claude-code"      }] }]
  }
}
```

Pick one route, not both. If you have manual hooks AND the alias, each event fires twice — idempotent for repeats of the same `set` but wasteful.

`SessionStart` writes a placeholder `idle` row so every Claude session appears in the [`agent-switcher`][switcher] popup from the moment it starts — even before you type the first prompt. `UserPromptSubmit` and `PreToolUse` then flip the row to `working` while Claude is mid-turn. The tmux status indicator filters both `idle` and `working` out, so the bar still only shows "needs you now" sessions (`notify` from `Notification`/`PermissionRequest`, `done` from `Stop`). The switcher shows every row, rendering `idle` as a dim dot and `working` as a spinner.

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

**Known limitation:** pi has no built-in "agent paused waiting for permission" event analogous to Claude Code's `PermissionRequest` hook — pi extensions handle confirmations in-process via `ctx.ui.confirm()`. So pi-coding-agent surfaces the "done" state but not a separate "needs attention" state. In practice the dominant signal is "agent finished a turn, waiting on next prompt" anyway.

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

### tmux (`~/.tmux.conf`)

Drop `#(agent-status status)` into your existing `status-right` wherever you want the indicator to appear, and lower the refresh interval so updates feel snappy:

```tmux
set -g status-interval 5
set -g status-right "#(agent-status status) <your existing status-right here>"
```

Reload with `tmux source-file ~/.tmux.conf`.

For the popup picker (prefix + `C-a`), see the [`agent-switcher`][switcher] crate.

## Usage

```sh
agent-status --help                       # top-level help
agent-status set [EVENT] --agent NAME     # mark this session as waiting (reads JSON on stdin)
agent-status clear --agent NAME           # clear this session's state (reads JSON on stdin)
agent-status status                       # print the status-right line, empty if nothing waiting
agent-status list                         # print TSV (session_id, pane, display) per waiting session
```

`set` and `clear` expect a JSON object on stdin with at least `{"session_id": "..."}`. Empty or missing `session_id` is a silent no-op.

## State location

`${XDG_RUNTIME_DIR:-/tmp}/agent-status/<session_id>` — one file per active session. Inspectable with `ls`/`cat`. Format:

```json
{"agent":"claude-code","project":"agent-status","cwd":"/path/to/project","event":"notify","tmux_pane":"%17","ts":1778163565,"message":"Permission required","pid":12345}
```

The `message` field is optional and only present when the agent's hook payload supplies one (e.g. Claude Code's `Notification` event). Older state files written before this field existed still load — `message` defaults to absent.

The `agent` field is `"claude-code"`, `"opencode"`, or `"pi-coding-agent"`; new agents use their own lowercase-hyphenated name.

The `pid` field records the agent process's PID so `agent-status status` and `list` can detect and remove entries whose owning process has died without firing its session-end hook. Files written by older binaries — which lack `pid` — are never auto-pruned; they age out only on tmpfs cleanup. Such entries should disappear naturally after one `set`/`clear` cycle on the affected session.

## Caveats

- **The `Stop` hook fires on every turn end**, so any session that just finished a response shows up as "waiting" until you send the next prompt. Intentional — the whole point is to know which session needs you while you're heads-down elsewhere. Drop the `Stop` line from `settings.json` if it proves too eager.
- **opencode has no "user submitted a prompt" event**, so once `session.idle` marks an opencode session `done`, the indicator stays on `done` while you type the next prompt and only refreshes its timestamp on the next idle. Same intent as the `Stop` caveat above — the session *is* the one waiting on you.
- **Architecture-specific binary.** The compiled binary is platform-locked. On a new machine, rebuild from source (`cargo install agent-status`).
- **Only Claude Code records a `working` state** today. pi and opencode sessions appear in the switcher when they're waiting (`done` / `notify`) but not while they're mid-turn. The hook semantics for those agents can be extended in a follow-up.

## License

MIT. See [LICENSE](LICENSE).

[hooks]: https://docs.claude.com/en/docs/claude-code/hooks
[Claude Code]: https://claude.com/claude-code
[pi]: https://pi.dev
[opencode]: https://opencode.ai
[switcher]: https://crates.io/crates/agent-switcher

# agent-switcher

A ratatui-based tmux popup TUI for switching between waiting AI coding agent
sessions. Companion to [`agent-status`][status] — reads the same
`${XDG_RUNTIME_DIR:-/tmp}/agent-status/` state directory and renders a small
picker. <kbd>Enter</kbd> runs `tmux switch-client` to the selected session's
pane.

```
┌─ Filter ─────────────────────────────────────────────────────────────┐
│ >                                                                    │
└──────────────────────────────────────────────────────────────────────┘
┌─ Sessions ───────────────────────────────────────────────────────────┐
│    Session                 Agent           Activity                  │
│    Notify                                                            │
│  ! agent-status (53fabd56) claude-code     Claude needs permission   │
│    Done                                                              │
│  ✓ docs         (1f33ccee) claude-code     Wrote 4 files             │
│    Idle                                                              │
│  · scratch      (4012a1cd) opencode                                  │
│    Working                                                           │
│  ⠋ playground   (9b73ed57) claude-code     Reading src/main.rs       │
│  ⠋ infra        (8f1ade22) pi-coding-agent Editing terraform/main.tf │
└──────────────────────────────────────────────────────────────────────┘
 Ctrl-N/P or ↓/↑: navigate · Enter: switch pane · Esc / Ctrl-C: cancel
```

## Install

```sh
cargo install agent-switcher
```

You also need [`agent-status`][status] installed and at least one agent's hooks
wired up — without state files in `${XDG_RUNTIME_DIR:-/tmp}/agent-status/`, the
switcher has nothing to display.

```sh
cargo install agent-status agent-switcher
```

See [`agent-status`'s README][status] for hook wiring (Claude Code, pi,
opencode).

## tmux popup

Drop this into `~/.tmux.conf`:

```tmux
bind-key C-a display-popup -E -w 80% -h 50% "agent-switcher"
```

Reload with `tmux source-file ~/.tmux.conf`. Press prefix + <kbd>C-a</kbd> to
open the picker.

## Keybindings inside the switcher

| Key                                | Action                                                                               |
| ---------------------------------- | ------------------------------------------------------------------------------------ |
| Type any char                      | Append to the filter (case-insensitive; matches project, agent, message, session id) |
| <kbd>Backspace</kbd>               | Remove the last filter char                                                          |
| <kbd>Ctrl-N</kbd> / <kbd>↓</kbd>   | Move selection down (wraps at the bottom)                                            |
| <kbd>Ctrl-P</kbd> / <kbd>↑</kbd>   | Move selection up (wraps at the top)                                                 |
| <kbd>Enter</kbd>                   | `tmux switch-client` to the selected session's pane, then exit                       |
| <kbd>Esc</kbd> / <kbd>Ctrl-C</kbd> | Exit without switching                                                               |

## What's in the list

Every recorded session — including those still working (animated spinner) — not
just sessions waiting on your attention. That makes the popup useful as a
general session jumper, while the [`agent-status`][status] tmux indicator stays
focused on "needs you now" sessions.

Rows are grouped under colored section banners in the order **Notify → Done →
Idle → Working → Other**, with the most-attention-needing group at the top.
Empty groups produce no banner. Within a group, rows are sorted by the
timestamp the hook last fired.

The activity column:

- While Claude Code is working: shows the active tool — e.g.
  `Reading src/main.rs`, `Running: git status`, `Searching: fn main`.
- When Claude Code is waiting on you: shows the notification message (e.g.
  `Claude needs your permission to use Bash`).
- For other agents: shows the last-response text if the agent's hook payload
  supplied one.

The marker column:

- `!` (yellow) — `notify`: agent is blocked on you (permission / input prompt).
- `✓` (green) — `done`: agent finished a turn.
- `·` (gray) — `idle`: session is alive but no prompt has arrived yet
  (placeholder so the row is visible from `SessionStart`).
- `⠋` (spinner, cyan) — `working`: agent is mid-turn.
- First char of the event name (white) — any future event type the binary
  doesn't yet recognize, bucketed under the **Other** banner.

## Dependency on `agent-status`

`agent-switcher` reads the state store via the [`agent-status`
library][status-docs] (`StateStore`, `AttentionEntry`). The two crates ship the
same version, but `agent-switcher` declares a flexible version range so you can
upgrade them independently if you need to.

## License

MIT. See [LICENSE](LICENSE).

[status]: https://crates.io/crates/agent-status
[status-docs]: https://docs.rs/agent-status

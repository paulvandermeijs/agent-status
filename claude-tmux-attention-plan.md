# Multi-session Claude Code Attention Indicator — Implementation Plan

## Context

I run multiple Claude Code sessions in tmux at the same time. I want a single indicator in `status-right` that shows how many sessions are currently waiting on me (e.g. `🔔 2 projects waiting`), visible from any tmux window. Hooks fired by Claude Code drop state files on disk; tmux's `status-right` runs a small script that counts them.

No daemon — the filesystem is the state store. One file per waiting session in `$XDG_RUNTIME_DIR/claude-attention/` (falling back to `/tmp` on macOS).

## Architecture

```
~/.claude/hooks/
  attention-set.sh      # Notification + Stop  → write state file
  attention-clear.sh    # UserPromptSubmit / SessionStart / SessionEnd → remove state file
  attention-status.sh   # tmux status-right    → render count
  attention-list.sh     # tmux popup picker    → fzf list + jump to pane

$XDG_RUNTIME_DIR/claude-attention/
  <session_id>          # JSON: {project, cwd, event, tmux_pane, ts}
```

## Prerequisites

- `jq` installed (`brew install jq` / `apt install jq`)
- `fzf` installed (only required for the bonus popup picker)
- tmux ≥ 3.0

Check both and report back if missing before proceeding.

## Tasks

### 1. Create the hook scripts

Create `~/.claude/hooks/` if it doesn't exist. Create the four scripts below and `chmod +x` all of them.

**`~/.claude/hooks/attention-set.sh`**

```bash
#!/usr/bin/env bash
set -e
STATE_DIR="${XDG_RUNTIME_DIR:-/tmp}/claude-attention"
mkdir -p "$STATE_DIR"

INPUT=$(cat)
SESSION_ID=$(jq -r '.session_id // empty' <<<"$INPUT")
[ -z "$SESSION_ID" ] && exit 0

EVENT="${1:-attention}"
PROJECT=$(basename "${CLAUDE_PROJECT_DIR:-$PWD}")

jq -n \
  --arg project "$PROJECT" \
  --arg cwd     "$PWD" \
  --arg event   "$EVENT" \
  --arg pane    "${TMUX_PANE:-}" \
  --argjson ts  "$(date +%s)" \
  '{project:$project, cwd:$cwd, event:$event, tmux_pane:$pane, ts:$ts}' \
  > "$STATE_DIR/$SESSION_ID"

tmux refresh-client -S 2>/dev/null || true
```

**`~/.claude/hooks/attention-clear.sh`**

```bash
#!/usr/bin/env bash
STATE_DIR="${XDG_RUNTIME_DIR:-/tmp}/claude-attention"
SESSION_ID=$(jq -r '.session_id // empty' < /dev/stdin)
[ -n "$SESSION_ID" ] && rm -f "$STATE_DIR/$SESSION_ID"
tmux refresh-client -S 2>/dev/null || true
```

**`~/.claude/hooks/attention-status.sh`**

```bash
#!/usr/bin/env bash
STATE_DIR="${XDG_RUNTIME_DIR:-/tmp}/claude-attention"
[ -d "$STATE_DIR" ] || exit 0

shopt -s nullglob
files=("$STATE_DIR"/*)
n=${#files[@]}
[ "$n" -eq 0 ] && exit 0

if [ "$n" -eq 1 ]; then
  proj=$(jq -r '.project' "${files[0]}" 2>/dev/null || echo "?")
  echo "#[fg=yellow,bold]🔔 $proj"
else
  echo "#[fg=yellow,bold]🔔 $n projects waiting"
fi
```

**`~/.claude/hooks/attention-list.sh`** (bonus — fzf popup picker)

```bash
#!/usr/bin/env bash
STATE_DIR="${XDG_RUNTIME_DIR:-/tmp}/claude-attention"
shopt -s nullglob
files=("$STATE_DIR"/*)
[ ${#files[@]} -eq 0 ] && { echo "No projects waiting."; sleep 1; exit; }

choice=$(for f in "${files[@]}"; do
  jq -r '[.tmux_pane, .project, .event] | @tsv' "$f"
done | fzf --with-nth=2.. --delimiter='\t' --prompt="Jump to> ")

[ -z "$choice" ] && exit 0
pane=$(cut -f1 <<<"$choice")
[ -n "$pane" ] && tmux switch-client -t "$pane"
```

### 2. Wire up Claude Code hooks

Edit `~/.claude/settings.json`. **Merge** with any existing `hooks` block — don't overwrite. If the file doesn't exist, create it with just the contents below.

```json
{
  "hooks": {
    "Notification":     [{ "hooks": [{ "type": "command", "command": "$HOME/.claude/hooks/attention-set.sh notify" }] }],
    "Stop":             [{ "hooks": [{ "type": "command", "command": "$HOME/.claude/hooks/attention-set.sh done"   }] }],
    "UserPromptSubmit": [{ "hooks": [{ "type": "command", "command": "$HOME/.claude/hooks/attention-clear.sh"      }] }],
    "SessionStart":     [{ "hooks": [{ "type": "command", "command": "$HOME/.claude/hooks/attention-clear.sh"      }] }],
    "SessionEnd":       [{ "hooks": [{ "type": "command", "command": "$HOME/.claude/hooks/attention-clear.sh"      }] }]
  }
}
```

### 3. Configure tmux

Append to `~/.tmux.conf`. Preserve any existing `status-right` content by appending to it rather than replacing.

```tmux
set -g status-interval 5
set -ga status-right '#(~/.claude/hooks/attention-status.sh) '

# Bonus: prefix + C-a opens an fzf picker of waiting sessions
bind-key C-a display-popup -E -w 60% -h 40% "~/.claude/hooks/attention-list.sh"
```

If I already have a `status-right` defined, integrate the script call into it instead of using `set -ga`. Show me the merged result before writing.

Reload tmux: `tmux source-file ~/.tmux.conf`.

### 4. Verify

Run these checks and report results:

1. **State directory works.** Manually drop a fake state file and confirm it shows up in the status bar:
   ```bash
   STATE_DIR="${XDG_RUNTIME_DIR:-/tmp}/claude-attention"
   mkdir -p "$STATE_DIR"
   echo '{"project":"test","cwd":"/tmp","event":"notify","tmux_pane":"","ts":0}' > "$STATE_DIR/fake-session"
   tmux refresh-client -S
   ```
   Expected: `🔔 test` in status-right. Then clean up: `rm "$STATE_DIR/fake-session"`.

2. **Status script runs cleanly when empty.** With no state files, `~/.claude/hooks/attention-status.sh` should exit 0 with no output.

3. **Hook scripts are executable.** `ls -l ~/.claude/hooks/` should show `x` permission on all four.

4. **End-to-end test.** Open a new tmux window, run Claude Code, ask it something that triggers a permission prompt (e.g. ask it to run a non-allowlisted bash command). Switch to a different tmux window without responding. The status bar in the second window should show `🔔 1 project waiting` within 5 seconds. Submit the prompt → indicator clears.

### 5. Report back

Tell me:
- What was already in `~/.claude/settings.json` and `~/.tmux.conf` before the change, and how it was merged
- Output of the four verification steps
- Anything that didn't work as expected

## Notes & caveats

- **`Stop` is noisy by design.** Every turn end fires `Stop`, so any finished response counts as "needing attention" until I send the next prompt. That's intentional — the whole point is to know which session finished while I was heads-down elsewhere. If after a few days it feels too eager, I'll drop the `Stop` line.
- **Stale state on crashes.** If a Claude Code process dies abnormally, its file lingers. Don't add pruning logic preemptively — wait until it actually bites.
- **Don't add desktop notifications, sound, or push to phone.** I rejected those earlier; the whole point of this design is tmux-native, integrated into the status bar.
- **macOS vs Linux.** Scripts must work on both. The `${XDG_RUNTIME_DIR:-/tmp}` fallback handles macOS, where `XDG_RUNTIME_DIR` is typically unset.

#!/bin/bash
# agent-status claude wrapper v1
#
# PATH-shadowing shim for Claude Code that auto-injects the agent-status hook
# configuration via `claude --settings <tmp.json>`. Install at the head of
# $PATH (e.g. ~/.claude/bin/claude, symlinked/renamed as just `claude`) and
# the user no longer has to manually edit ~/.claude/settings.json.
#
# Activation rules — the wrapper passes through to the real claude binary
# unmodified when any of the following is true:
#   * AGENT_STATUS_CLAUDE_WRAPPER_DISABLED=1 is set
#   * AGENT_STATUS_CLAUDE_WRAPPER_ACTIVE=1 is set (anti-recursion guard for
#     sub-claude invocations)
#   * Outside tmux (TMUX_PANE empty) and AGENT_STATUS_CLAUDE_WRAPPER_FORCE is
#     not "1"
#   * The agent-status binary at "$AGENT_STATUS_BIN" (default
#     ~/.claude/bin/agent-status) is not executable
#   * $AGENT_STATUS_BIN contains characters that would produce malformed
#     command strings in the generated settings (spaces, quotes, backslashes,
#     control chars) — defensive guard, see below
#
# Environment:
#   AGENT_STATUS_BIN                          override path to agent-status
#   AGENT_STATUS_CLAUDE_WRAPPER_DISABLED      "1" disables the wrapper
#   AGENT_STATUS_CLAUDE_WRAPPER_FORCE         "1" activates outside tmux
#   AGENT_STATUS_CLAUDE_WRAPPER_ACTIVE        set by the wrapper itself to
#                                             prevent nested re-injection

set -u

self_dir="$(cd "$(dirname "$0")" && pwd)"

find_real_claude() {
  local IFS=:
  local dir
  for dir in $PATH; do
    [ -z "$dir" ] && continue
    dir="${dir%/}"
    [ "$dir" = "$self_dir" ] && continue
    if [ -x "$dir/claude" ] && [ ! -d "$dir/claude" ]; then
      # Defense against PATH containing the same physical directory under
      # different names (symlinks). `-ef` compares inode+device.
      if [ "$dir/claude" -ef "$0" ]; then continue; fi
      printf '%s\n' "$dir/claude"
      return 0
    fi
  done
  return 1
}

real_claude="$(find_real_claude)"
if [ -z "$real_claude" ]; then
  echo "agent-status claude wrapper: real claude binary not found in PATH." >&2
  echo "Install Claude Code and ensure its bin dir is on PATH, then retry." >&2
  exit 127
fi

# Decide whether to inject hooks. Any miss → transparent passthrough.
if [ "${AGENT_STATUS_CLAUDE_WRAPPER_DISABLED:-0}" = "1" ] \
   || [ "${AGENT_STATUS_CLAUDE_WRAPPER_ACTIVE:-0}" = "1" ] \
   || { [ -z "${TMUX_PANE:-}" ] && [ "${AGENT_STATUS_CLAUDE_WRAPPER_FORCE:-0}" != "1" ]; }
then
  exec "$real_claude" "$@"
fi

agent_status_bin="${AGENT_STATUS_BIN:-${HOME:-}/.claude/bin/agent-status}"
if [ ! -x "$agent_status_bin" ]; then
  # agent-status isn't installed — passthrough instead of breaking claude.
  exec "$real_claude" "$@"
fi

# Guard against paths that would corrupt the generated JSON or break shell
# expansion when Claude Code runs the hook command. The interpolation below
# embeds $agent_status_bin into a JSON string AND that string is then re-
# parsed by the shell — so a space, quote, backslash, or control char in
# the path produces either malformed JSON or a mis-tokenised hook command.
# Rather than try to escape correctly for both layers, refuse the injection.
case "$agent_status_bin" in
  *[[:space:]\"\\\\]*|*[$'\t\n\r']*)
    exec "$real_claude" "$@" ;;
esac

# Use mktemp -d for portability: BSD mktemp (macOS) and GNU mktemp (Linux)
# disagree on how -t handles a suffix-bearing template. A directory holding
# settings.json is unambiguous on both, and the trap can rm -rf the directory
# without needing two paths to track.
settings_dir="$(mktemp -d "${TMPDIR:-/tmp}/agent-status-claude-settings.XXXXXXXX")" || {
  # mktemp failed (full disk, weird FS) — passthrough.
  exec "$real_claude" "$@"
}
settings_file="$settings_dir/settings.json"

cat > "$settings_file" <<EOF
{
  "hooks": {
    "Notification":     [{ "hooks": [{ "type": "command", "command": "$agent_status_bin set --agent claude-code notify" }] }],
    "Stop":             [{ "hooks": [{ "type": "command", "command": "$agent_status_bin set --agent claude-code done"   }] }],
    "UserPromptSubmit": [{ "hooks": [{ "type": "command", "command": "$agent_status_bin clear --agent claude-code"      }] }],
    "PreToolUse":       [{ "hooks": [{ "type": "command", "command": "$agent_status_bin clear --agent claude-code"      }] }],
    "SessionStart":     [{ "hooks": [{ "type": "command", "command": "$agent_status_bin clear --agent claude-code"      }] }],
    "SessionEnd":       [{ "hooks": [{ "type": "command", "command": "$agent_status_bin clear --agent claude-code"      }] }]
  }
}
EOF

# Anti-recursion for sub-claude invocations spawned by tools, MCP servers, etc.
export AGENT_STATUS_CLAUDE_WRAPPER_ACTIVE=1

# Clean up the temp dir (and its settings.json) when the wrapper exits.
trap 'rm -rf "$settings_dir"' EXIT HUP INT TERM

# Run claude as a child (not exec) so the trap fires when claude exits.
"$real_claude" --settings "$settings_file" "$@"
exit $?

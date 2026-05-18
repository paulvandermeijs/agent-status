# Claude Code Auto-Injecting Wrapper Shim Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a PATH-shadowing wrapper script at `extensions/claude-wrapper.sh` that the user installs at the head of their `$PATH` as `claude`. On invocation it locates the real `claude` binary, generates a temporary settings JSON with all the agent-status hooks pre-wired (with `$AGENT_STATUS_BIN` substituted in), and exec-replaces itself with `claude --settings <tmp> <user-args>`. Result: users no longer need to manually edit `~/.claude/settings.json` to integrate with agent-status. Mirrors cmux's `Resources/bin/claude` shim (Issue #2140, PR #1306), adapted to agent-status's tmux-based delivery.

**Architecture:** Single bash script, no Rust changes. Activates only when (a) the wrapper is on `$PATH` ahead of the real claude and (b) we're inside tmux (`TMUX_PANE` set) — outside tmux, transparent passthrough so the wrapper is safe to leave installed everywhere. Three opt-out paths covered: explicit `AGENT_STATUS_CLAUDE_WRAPPER_DISABLED=1`, anti-recursion when claude spawns sub-claudes (the wrapper exports `AGENT_STATUS_CLAUDE_WRAPPER_ACTIVE=1` before exec), and silent degradation when `agent-status` itself isn't installed. The wrapper itself doesn't depend on `agent-status` — if the binary isn't found at `$AGENT_STATUS_BIN`, it just passes through unmodified. The temporary settings file is cleaned up on wrapper exit via `trap`.

**Tech Stack:** Bash 3.2 (the default on macOS, the project's primary target platform). POSIX-only utilities: `mktemp`, `printf`, `cat`, `rm`, `dirname`, `cd`, `pwd`. No new Rust dependencies, no JS/TS. Settings file format is the same JSON Claude Code's `--settings` flag already accepts (verified against the install snippet in `README.md`).

**Dependency note:** This plan is best applied after `2026-05-18-claude-code-pretooluse-hook.md` lands, since the wrapper bakes in the `PreToolUse` line. If applied first, drop the `PreToolUse` line from the generated settings JSON and add it later when the other plan ships.

---

## File Structure

- **Create** `extensions/claude-wrapper.sh` — the wrapper script. ~80 lines. Executable on installation. Public API at the top (a header comment block describing flags and env vars), helper function `find_real_claude` and main logic at the bottom (per the user's "public API at top, private parts at bottom" rule, treating the doc header as the "API").
- **Modify** `README.md` — new subsection under "Configure" titled "Optional: auto-installing wrapper" explaining how to install and enable it; remind users it's optional and the manual settings.json route still works.
- **Modify** `CLAUDE.md` — extend "Adding a new agent" with a third integration pattern. Currently the doc covers (1) shell-hook agents like Claude Code and (2) in-process extensions like pi/opencode. Add (3): PATH-shadowing wrappers that auto-inject hooks for agents that support `--settings`-style flags. This wrapper is the first instance; a future codex wrapper would follow the same pattern.

The Rust source tree is untouched. The integration test suite is untouched (we can't meaningfully integration-test a shim that depends on a real `claude` binary; the verification in this plan is manual).

---

## Task 1: Write the wrapper script

**Files:**
- Create: `extensions/claude-wrapper.sh`

- [ ] **Step 1: Create the file with the complete content below**

Write `extensions/claude-wrapper.sh` with these contents exactly:

```bash
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
#   * TMUX_PANE is empty AND AGENT_STATUS_CLAUDE_WRAPPER_FORCE=1 is not set
#   * The agent-status binary at "$AGENT_STATUS_BIN" (default
#     ~/.claude/bin/agent-status) is not executable
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

agent_status_bin="${AGENT_STATUS_BIN:-$HOME/.claude/bin/agent-status}"
if [ ! -x "$agent_status_bin" ]; then
  # agent-status isn't installed — passthrough instead of breaking claude.
  exec "$real_claude" "$@"
fi

settings_file="$(mktemp -t agent-status-claude-settings.XXXXXXXX)" || {
  # mktemp failed (full disk, weird FS) — passthrough.
  exec "$real_claude" "$@"
}
# Rename to .json so anything that sniffs the extension is happy.
mv "$settings_file" "$settings_file.json"
settings_file="$settings_file.json"

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

# Clean up the temp file when the wrapper (and thus claude) exits.
trap 'rm -f "$settings_file"' EXIT HUP INT TERM

# Run claude as a child (not exec) so the trap fires when claude exits.
"$real_claude" --settings "$settings_file" "$@"
exit $?
```

- [ ] **Step 2: Make the file executable**

Run: `chmod +x extensions/claude-wrapper.sh`

- [ ] **Step 3: Quick syntax check**

Run: `bash -n extensions/claude-wrapper.sh && echo OK`
Expected: `OK` (bash parses the file without complaining).

- [ ] **Step 4: Commit**

```bash
git add extensions/claude-wrapper.sh
git commit -m "feat(extensions): add Claude Code wrapper shim auto-injecting hooks"
```

---

## Task 2: Verify `find_real_claude` against the real PATH

**Files:** none modified.

- [ ] **Step 1: Identify the real claude binary on this machine**

Run: `which claude`
Expected: an absolute path. Note it down — call it `$REAL`.

- [ ] **Step 2: Source the function and call it**

```bash
self_dir="$PWD/extensions"
. /dev/stdin <<'EOF'
find_real_claude() {
  local IFS=:
  local dir
  for dir in $PATH; do
    [ -z "$dir" ] && continue
    dir="${dir%/}"
    [ "$dir" = "$self_dir" ] && continue
    if [ -x "$dir/claude" ] && [ ! -d "$dir/claude" ]; then
      if [ "$dir/claude" -ef "$self_dir/claude-wrapper.sh" ]; then continue; fi
      printf '%s\n' "$dir/claude"
      return 0
    fi
  done
  return 1
}
EOF
find_real_claude
```

Expected: prints the same path `which claude` printed in Step 1. If they differ, something on `$PATH` ahead of the real claude has been mistaken for it — likely another claude-named alias. Investigate.

- [ ] **Step 3: Verify the wrapper-skip works**

Symlink the wrapper as `claude` into a temp dir and put that dir at the front of PATH:

```bash
tmp_dir="$(mktemp -d)"
ln -s "$PWD/extensions/claude-wrapper.sh" "$tmp_dir/claude"
PATH="$tmp_dir:$PATH" bash -c '
  self_dir="'"$tmp_dir"'"
  find_real_claude() {
    local IFS=:
    local dir
    for dir in $PATH; do
      [ -z "$dir" ] && continue
      dir="${dir%/}"
      [ "$dir" = "$self_dir" ] && continue
      if [ -x "$dir/claude" ] && [ ! -d "$dir/claude" ]; then
        if [ "$dir/claude" -ef "$self_dir/claude" ]; then continue; fi
        printf "%s\n" "$dir/claude"
        return 0
      fi
    done
    return 1
  }
  find_real_claude
'
rm -rf "$tmp_dir"
```

Expected: prints the real claude path (NOT the temp dir's symlink). If it prints the symlink path, the skip logic is broken.

- [ ] **Step 4: No commit — verification only**

---

## Task 3: Test the activation conditions

**Files:** none modified.

- [ ] **Step 1: Test passthrough when TMUX_PANE is unset**

```bash
unset TMUX_PANE AGENT_STATUS_CLAUDE_WRAPPER_FORCE
./extensions/claude-wrapper.sh --version
```

Expected: prints Claude Code's version output unchanged — the wrapper detected it's outside tmux and exec'd directly. There should be no `--settings` file left behind in `/tmp`.

- [ ] **Step 2: Verify no temp settings file was created**

```bash
ls /tmp/agent-status-claude-settings.* 2>/dev/null
```

Expected: no match (the wrapper never created one because passthrough fired before the mktemp block).

- [ ] **Step 3: Test passthrough when explicitly disabled**

```bash
TMUX_PANE=%99 AGENT_STATUS_CLAUDE_WRAPPER_DISABLED=1 ./extensions/claude-wrapper.sh --version
```

Expected: same as Step 1 — Claude version output, no temp file.

- [ ] **Step 4: Test anti-recursion guard**

```bash
TMUX_PANE=%99 AGENT_STATUS_CLAUDE_WRAPPER_ACTIVE=1 ./extensions/claude-wrapper.sh --version
```

Expected: passthrough behavior (no settings injection). This simulates a sub-claude spawned by the outer claude — without the guard it would loop forever.

- [ ] **Step 5: Test activation when TMUX_PANE is set**

```bash
TMUX_PANE=%99 ./extensions/claude-wrapper.sh --version
```

Expected: prints Claude Code's version output — but this time a `/tmp/agent-status-claude-settings.*.json` file existed during the call. Verify the wrapper cleaned it up:

```bash
ls /tmp/agent-status-claude-settings.* 2>/dev/null
```

Expected: no match (the EXIT trap removed it).

- [ ] **Step 6: No commit — verification only**

---

## Task 4: End-to-end smoke test with a real Claude Code session

**Files:** none modified.

This is the integration test for the whole feature. Run it from a tmux pane.

- [ ] **Step 1: Install the wrapper as `claude` at the head of PATH**

```bash
mkdir -p ~/.claude/bin
cp extensions/claude-wrapper.sh ~/.claude/bin/claude
chmod +x ~/.claude/bin/claude
# Ensure ~/.claude/bin is at the FRONT of PATH for this shell. If your shell rc
# already has it on PATH but behind your nodenv/asdf/Homebrew claude, the
# wrapper won't intercept — confirm with `which claude`.
```

- [ ] **Step 2: Verify PATH precedence**

Run: `which claude`
Expected: `/Users/<you>/.claude/bin/claude` (the wrapper), not `/opt/homebrew/bin/claude` or wherever the real one lives. If wrong, adjust your shell rc — move `export PATH="$HOME/.claude/bin:$PATH"` after any other claude-providing PATH manipulation.

- [ ] **Step 3: Temporarily neutralize the user's own `~/.claude/settings.json` hooks**

If the user already has agent-status hooks wired into `~/.claude/settings.json` (per the existing README install instructions), the wrapper will *additionally* wire them, leading to two firings per event. That's idempotent for `clear` but wasteful for `set`. For the smoke test, either back up and clear the user's settings.json hooks block, or accept the duplicate firings and only check the wrapper's contribution.

- [ ] **Step 4: Start a Claude Code session from the tmux pane**

```bash
claude
```

Submit a prompt that takes a few tool calls and produces a Stop. Mid-turn, in another tmux pane:

```bash
ls "${XDG_RUNTIME_DIR:-/tmp}/agent-status/"
cat "${XDG_RUNTIME_DIR:-/tmp}/agent-status/"* 2>/dev/null
```

Expected: a state file appears keyed by the claude session id, with `"agent":"claude-code"` and `"event":"done"` after each Stop. The state should clear and re-set across turn boundaries, exactly as if you'd manually wired the hooks in settings.json.

- [ ] **Step 5: Verify settings file got cleaned up after claude exits**

After exiting claude (`Ctrl+D` or `/exit`):

```bash
ls /tmp/agent-status-claude-settings.* 2>/dev/null
```

Expected: no match — the EXIT trap removed it.

- [ ] **Step 6: Test the opt-out**

```bash
AGENT_STATUS_CLAUDE_WRAPPER_DISABLED=1 claude --version
```

Expected: prints Claude's version cleanly with no settings injection.

- [ ] **Step 7: No commit — verification only**

---

## Task 5: Update README with install instructions

**Files:**
- Modify: `README.md` (add a new subsection under "Configure", or restructure the Claude Code section)

- [ ] **Step 1: Add the new subsection**

Find the existing `### Claude Code hooks (~/.claude/settings.json)` section. Right after it (before the `### pi (~/.pi/agent/extensions/)` section), insert:

```markdown
### Claude Code wrapper (optional, zero-config)

If you'd rather not edit `~/.claude/settings.json` by hand, you can install the
wrapper at `extensions/claude-wrapper.sh` as `claude` at the front of your
`$PATH`. The wrapper finds the real `claude` binary, generates a temporary
settings file with the agent-status hooks pre-wired, and exec-launches claude
with `--settings <tmp>`. No settings.json edits required.

```sh
mkdir -p ~/.claude/bin
cp extensions/claude-wrapper.sh ~/.claude/bin/claude
chmod +x ~/.claude/bin/claude
# Ensure ~/.claude/bin is at the FRONT of your $PATH.
```

The wrapper is **passthrough by default outside tmux** (it gates on `TMUX_PANE`)
so leaving it installed everywhere is safe — only sessions launched from a
tmux pane get the hook injection. Other gates:

| Env var | Effect |
|---|---|
| `AGENT_STATUS_CLAUDE_WRAPPER_DISABLED=1` | Disable the wrapper for this invocation (full passthrough). |
| `AGENT_STATUS_CLAUDE_WRAPPER_FORCE=1` | Activate even outside tmux. |
| `AGENT_STATUS_BIN` | Override the path to the `agent-status` binary the wrapper bakes into the generated settings. Default: `~/.claude/bin/agent-status`. |

The wrapper merges with any hooks already in your `~/.claude/settings.json`
(Claude Code merges `--settings` files on top of user/project settings). If
you have your own agent-status hooks wired there already, remove them
before installing the wrapper — otherwise each event fires twice (idempotent
for `clear`, harmless-but-wasteful for `set`).

If `agent-status` itself isn't installed at `$AGENT_STATUS_BIN`, the wrapper
detects this and silently falls through to the real claude — useful for
shared dotfiles where not every host has agent-status built.
```

- [ ] **Step 2: Verify markdown rendering**

Run: `head -100 README.md`
Expected: the new section reads cleanly. The triple-backtick blocks inside the section render properly (Markdown allows nesting at one level, which works for this section's outer `markdown` block and inner `sh` block).

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs(readme): document the optional Claude Code wrapper shim"
```

---

## Task 6: Update CLAUDE.md with the third integration pattern

**Files:**
- Modify: `CLAUDE.md` (under "Adding a new agent")

- [ ] **Step 1: Append a third integration-pattern note**

The current `CLAUDE.md` lists two integration patterns: (1) shell-hook agents (Claude Code) and (2) in-process bridges (pi, opencode). After the paragraph that begins "Some agents (e.g. pi at `pi.dev`) don't have a shell-hook mechanism...", add a new paragraph:

```markdown
A third pattern is a PATH-shadowing wrapper that auto-injects the hook
configuration so the user doesn't have to touch the agent's settings file
at all. The wrapper lives in `extensions/<agent>-wrapper.sh`, gates on
some environment signal (`TMUX_PANE` for our use case), finds the real
binary on `$PATH` (skipping its own directory by inode comparison), writes
a temp settings file with `agent-status` hook commands and the appropriate
flag (Claude Code's `--settings`, codex's `-c notify=...`, etc.), and
runs the real binary as a child so it can clean up the temp file via
`trap`. The Rust `Agent` impl is unchanged — the wrapper still pipes the
agent's normal hook JSON to `agent-status set`/`clear`, so the
`extract_session_id` contract still applies. See
`extensions/claude-wrapper.sh` for the reference implementation.
```

- [ ] **Step 2: Update the test-count line if changed**

This plan doesn't add any Rust tests. If `cargo test 2>&1 | tail -2` shows a different total than what's currently in `CLAUDE.md`, the wrapper plan didn't cause that — investigate other changes. The expected behavior is no test-count delta from this plan alone.

- [ ] **Step 3: Commit**

```bash
git add CLAUDE.md
git commit -m "docs(claude): document wrapper-shim integration pattern"
```

---

## Task 7: Final cross-check

**Files:** none modified.

- [ ] **Step 1: Run the full test suite and clippy gate**

```bash
cargo test
cargo clippy --all-targets --all-features --locked -- -D warnings
```

Expected: all tests pass, no clippy warnings. This plan didn't touch Rust source, so any failure here is unrelated.

- [ ] **Step 2: Sanity-check the wrapper is committed and executable**

```bash
ls -la extensions/claude-wrapper.sh
git ls-files extensions/claude-wrapper.sh
```

Expected: file present with mode `0755`, tracked by git.

- [ ] **Step 3: Sanity-check the docs cross-reference each other**

```bash
grep -n "claude-wrapper" README.md CLAUDE.md
```

Expected: both files reference `extensions/claude-wrapper.sh`. Good.

- [ ] **Step 4: No further commit**

The plan is complete.

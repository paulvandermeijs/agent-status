# pi.dev (pi-coding-agent) Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add agent-status integration for [pi-coding-agent](https://pi.dev) so pi sessions waiting on user input show up in the tmux status line alongside Claude Code sessions.

**Architecture:** Two parts. (1) A Rust `PiCodingAgent` registered under name `pi-coding-agent` that reads `session_id` from the stdin JSON payload — structurally identical to `ClaudeCodeAgent` because we control the payload format. (2) A TypeScript pi extension (`extensions/pi-coding-agent.ts`) that subscribes to pi's in-process lifecycle events (`before_agent_start`, `agent_end`, `session_start`, `session_shutdown`) and shells out to the `agent-status` binary, piping `{"session_id":"<uuid>"}` to stdin. Pi has no "agent paused for permission" event analogous to Claude Code's `Notification`, so v1 only emits the `done` state — this is documented as a known limitation rather than faked with `tool_call`.

**Tech Stack:** Rust (existing crate), TypeScript (pi extension, single file using `node:fs`, `node:path`, `node:child_process`).

---

## File Structure

- **Create** `src/agents/pi_coding_agent.rs` — `PiCodingAgent` unit struct implementing `agents::Agent`. Reads `session_id` from JSON payload, with the four standard edge cases.
- **Modify** `src/agents/mod.rs` — declare `pub mod pi_coding_agent;` and register `"pi-coding-agent" => PiCodingAgent` in `by_name`. Add a registry test.
- **Create** `extensions/pi-coding-agent.ts` — single-file pi extension. Subscribes to lifecycle events, computes the session UUID from `ctx.sessionManager.getSessionFile()`, and spawns `agent-status` as a fire-and-forget child process with the JSON payload on stdin.
- **Modify** `README.md` — add a "pi (pi-coding-agent)" subsection under "Configure" mirroring the Claude Code section: how to install the TS extension, what events are wired, the known notify-state gap.
- **Modify** `CLAUDE.md` — short note under "Adding a new agent" that some agents (pi) don't have shell-hook integration and require an in-process bridge extension; the Rust `Agent` impl is the same, the configuration story is different.

Note: keep the `src/main.rs`, `src/state.rs`, `src/commands.rs` triplet untouched — the abstraction we set up in the v0.2.0 refactor should absorb a new agent without changes there. CLAUDE.md says "No changes to `state.rs`, `commands.rs`, or `main.rs` should be needed for a typical new agent — that's the test of the abstraction." This task is the test.

---

## Task 1: Rust `PiCodingAgent` impl (TDD)

**Files:**
- Create: `src/agents/pi_coding_agent.rs`
- Modify: `src/agents/mod.rs`

- [ ] **Step 1: Write the failing `name()` test in a new agent file**

Create `src/agents/pi_coding_agent.rs` with just the test, no impl yet:

```rust
use crate::agents::Agent;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_is_pi_coding_agent() {
        assert_eq!(PiCodingAgent.name(), "pi-coding-agent");
    }
}
```

Then declare the module in `src/agents/mod.rs` so the test is discovered. Add `pub mod pi_coding_agent;` immediately after the existing `pub mod claude_code;`:

```rust
pub mod claude_code;
pub mod pi_coding_agent;
```

- [ ] **Step 2: Run the test, confirm it fails**

Run: `cargo test --lib agents::pi_coding_agent`
Expected: FAIL with `cannot find type 'PiCodingAgent' in this scope` (or similar — the unit struct doesn't exist yet).

- [ ] **Step 3: Add the minimal `PiCodingAgent` struct and `name()`**

Edit `src/agents/pi_coding_agent.rs` so the public API sits at the top, tests at the bottom (per the user's global "public API at top" rule). Final shape after this step:

```rust
use crate::agents::Agent;

/// pi-coding-agent ([pi.dev](https://pi.dev)).
///
/// Reads `session_id` from the JSON piped in by the bundled pi extension at
/// `extensions/pi-coding-agent.ts`, which fires on pi's `before_agent_start`,
/// `agent_end`, `session_start`, and `session_shutdown` events.
pub struct PiCodingAgent;

impl Agent for PiCodingAgent {
    fn name(&self) -> &'static str {
        "pi-coding-agent"
    }

    fn extract_session_id(&self, _stdin_json: &str) -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_is_pi_coding_agent() {
        assert_eq!(PiCodingAgent.name(), "pi-coding-agent");
    }
}
```

(`extract_session_id` is stubbed to `None` deliberately — we'll TDD it next.)

- [ ] **Step 4: Run the test, confirm it passes**

Run: `cargo test --lib agents::pi_coding_agent`
Expected: 1 passed.

- [ ] **Step 5: Write the failing `extract_session_id_returns_id` test**

Append to the `tests` module in `src/agents/pi_coding_agent.rs`:

```rust
    #[test]
    fn extract_session_id_returns_id() {
        let json = r#"{"session_id":"abc-123","other":"stuff"}"#;
        assert_eq!(
            PiCodingAgent.extract_session_id(json).as_deref(),
            Some("abc-123")
        );
    }
```

- [ ] **Step 6: Run the test, confirm it fails**

Run: `cargo test --lib agents::pi_coding_agent::tests::extract_session_id_returns_id`
Expected: FAIL — `assertion failed: ... left: None, right: Some("abc-123")` because the stub returns `None`.

- [ ] **Step 7: Implement `extract_session_id`**

Replace the stub body in `src/agents/pi_coding_agent.rs`:

```rust
    fn extract_session_id(&self, stdin_json: &str) -> Option<String> {
        let v: serde_json::Value = serde_json::from_str(stdin_json).ok()?;
        let id = v.get("session_id")?.as_str()?;
        if id.is_empty() {
            None
        } else {
            Some(id.to_string())
        }
    }
```

This is structurally identical to `ClaudeCodeAgent::extract_session_id` because we control the TS extension that constructs the payload — both agents read `session_id`. Don't extract a shared helper: the `Agent` trait was deliberately designed so each agent's field can vary (Cursor will use `conversation_id`), and a shared helper here would be premature abstraction.

- [ ] **Step 8: Run the test, confirm it passes**

Run: `cargo test --lib agents::pi_coding_agent::tests::extract_session_id_returns_id`
Expected: PASS.

- [ ] **Step 9: Add the three remaining edge-case tests**

CLAUDE.md mandates the four standard edge cases for any new agent. We just covered the happy path; add the other three. Append to the `tests` module:

```rust
    #[test]
    fn extract_session_id_returns_none_for_missing_field() {
        assert_eq!(
            PiCodingAgent.extract_session_id(r#"{"other":1}"#),
            None
        );
    }

    #[test]
    fn extract_session_id_returns_none_for_empty_string() {
        assert_eq!(
            PiCodingAgent.extract_session_id(r#"{"session_id":""}"#),
            None
        );
    }

    #[test]
    fn extract_session_id_returns_none_for_invalid_json() {
        assert_eq!(PiCodingAgent.extract_session_id("not json"), None);
    }
```

- [ ] **Step 10: Run all `pi_coding_agent` tests, confirm they pass**

Run: `cargo test --lib agents::pi_coding_agent`
Expected: 4 passed.

- [ ] **Step 11: Add the `by_name` registration test**

Edit `src/agents/mod.rs`. Find the `tests` module at the bottom and append a new test inside it (do NOT touch the existing `by_name_resolves_claude_code` test):

```rust
    #[test]
    fn by_name_resolves_pi_coding_agent() {
        let agent = by_name("pi-coding-agent").expect("pi-coding-agent is a registered agent");
        assert_eq!(agent.name(), "pi-coding-agent");
    }
```

- [ ] **Step 12: Run the test, confirm it fails**

Run: `cargo test --lib agents::tests::by_name_resolves_pi_coding_agent`
Expected: FAIL with `pi-coding-agent is a registered agent` panic — the registry doesn't know about it yet.

- [ ] **Step 13: Register `pi-coding-agent` in the `by_name` match**

Edit `src/agents/mod.rs`. Find the existing `by_name` function:

```rust
pub fn by_name(name: &str) -> Option<Box<dyn Agent>> {
    match name {
        "claude-code" => Some(Box::new(claude_code::ClaudeCodeAgent)),
        _ => None,
    }
}
```

Add the new arm immediately after the `claude-code` arm:

```rust
pub fn by_name(name: &str) -> Option<Box<dyn Agent>> {
    match name {
        "claude-code" => Some(Box::new(claude_code::ClaudeCodeAgent)),
        "pi-coding-agent" => Some(Box::new(pi_coding_agent::PiCodingAgent)),
        _ => None,
    }
}
```

- [ ] **Step 14: Run the registration test, confirm it passes**

Run: `cargo test --lib agents::tests::by_name_resolves_pi_coding_agent`
Expected: PASS.

- [ ] **Step 15: Run the full test suite**

Run: `cargo test`
Expected: 31 tests pass (existing 25, plus 5 new in `pi_coding_agent::tests` — `name_is_pi_coding_agent`, `extract_session_id_returns_id`, `extract_session_id_returns_none_for_missing_field`, `extract_session_id_returns_none_for_empty_string`, `extract_session_id_returns_none_for_invalid_json` — plus 1 new in `agents::tests::by_name_resolves_pi_coding_agent`). Zero failures.

If the existing-tests baseline has shifted since this plan was written, what matters is: zero failures, and all six newly-named tests appear in the output.

- [ ] **Step 16: Run clippy with the project's required gate**

Run: `cargo clippy --all-targets --all-features --locked -- -D warnings`
Expected: no warnings, exit 0. The project's `[lints.clippy] pedantic = "warn"` plus `-D warnings` means even pedantic lints fail the build, so address any complaints before committing.

- [ ] **Step 17: Commit the Rust side**

```bash
git add src/agents/pi_coding_agent.rs src/agents/mod.rs
git commit -m "$(cat <<'EOF'
feat(agents): add pi-coding-agent

- Add `PiCodingAgent` reading `session_id` from stdin JSON payload
- Register `pi-coding-agent` in the `by_name` registry
- Cover the four standard `extract_session_id` edge cases
EOF
)"
```

---

## Task 2: TypeScript pi extension

**Files:**
- Create: `extensions/pi-coding-agent.ts`

This is the bridge that turns pi's in-process events into `agent-status` subprocess invocations. It is shipped as a single file the user drops into `~/.pi/agent/extensions/`.

- [ ] **Step 1: Verify the target dir does not yet exist**

Run: `ls extensions/ 2>/dev/null; echo "exit=$?"`
Expected: `exit=2` (or similar — the dir doesn't exist yet). If it already exists, list contents and proceed without disturbing other files.

- [ ] **Step 2: Create the extension file**

Create `extensions/pi-coding-agent.ts` with this exact content. Public API (the default export) is at the top; helpers below it, per the user's global rule.

```typescript
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { spawn } from "node:child_process";
import { basename } from "node:path";

/**
 * Bridges pi-coding-agent lifecycle events to the `agent-status` CLI so pi
 * sessions waiting on user input show up in tmux's status-right.
 *
 * Install: copy this file to `~/.pi/agent/extensions/pi-coding-agent.ts`.
 * Override the binary path with `AGENT_STATUS_BIN` if not at the default.
 */
export default function (pi: ExtensionAPI) {
  pi.on("session_start", async (_event, ctx) => fire(ctx, "clear"));
  pi.on("session_shutdown", async (_event, ctx) => fire(ctx, "clear"));
  pi.on("before_agent_start", async (_event, ctx) => fire(ctx, "clear"));
  pi.on("agent_end", async (_event, ctx) => fire(ctx, "set", "done"));
}

const BIN =
  process.env.AGENT_STATUS_BIN ?? `${process.env.HOME}/.claude/bin/agent-status`;

type Action = "set" | "clear";
type SetEvent = "notify" | "done";

function fire(ctx: any, action: Action, event?: SetEvent): void {
  const sessionId = sessionIdFromCtx(ctx);
  if (!sessionId) return;

  const args =
    action === "set"
      ? ["set", "--agent", "pi-coding-agent", event!]
      : ["clear", "--agent", "pi-coding-agent"];

  const child = spawn(BIN, args, {
    stdio: ["pipe", "ignore", "ignore"],
  });
  child.on("error", () => {
    // best-effort: agent-status may not be installed; never crash pi
  });
  child.stdin.end(JSON.stringify({ session_id: sessionId }));
}

function sessionIdFromCtx(ctx: any): string | null {
  const file: string | null | undefined = ctx?.sessionManager?.getSessionFile?.();
  if (!file) return null;
  // pi session filenames are "<timestamp>_<uuid>.jsonl" — pull the UUID out.
  const match = basename(file, ".jsonl").match(/_([0-9a-f-]{36})$/i);
  return match ? match[1] : null;
}
```

Notes for the implementer:

- **Why basename-parsing instead of reading the JSONL header?** The basename is canonical (pi uses the same UUID for `--session <id>` resume), and parsing avoids a synchronous file read on every event. The format is documented in `packages/coding-agent/docs/session-format.md`: `~/.pi/agent/sessions/--<path>--/<timestamp>_<uuid>.jsonl`.
- **Why fire-and-forget `spawn` instead of `spawnSync`?** Pi event handlers run on the main loop; blocking them stalls the TUI. The `agent-status` invocation is best-effort — we don't await, don't read stdout, and silence errors, matching the Rust side's tmux-refresh behavior (`refresh_tmux` redirects child stderr/stdout to /dev/null per `CLAUDE.md`).
- **Why no notify state?** Pi has no built-in event for "agent paused waiting for user permission" — extensions implement permission gates with `ctx.ui.confirm()` in-process. `tool_call` fires constantly inside a turn and would thrash the indicator. Document this in the README rather than fake it.
- **`ctx: any`** — pi's TypeScript types for `ExtensionContext` would let us narrow this, but they're not imported by the file as-is. The `any` keeps the file dependency-free for users who drop it into `~/.pi/agent/extensions/` without a `node_modules`. Pi loads extensions via [jiti](https://github.com/unjs/jiti) at runtime, so missing types are runtime-irrelevant.

- [ ] **Step 3: Lightweight syntax/structure verification**

Pi's runtime check is the canonical verification, but it requires a working pi install with a provider. As a cheap alternative, confirm the file parses as valid TypeScript by stripping types:

Run: `node --experimental-strip-types --check extensions/pi-coding-agent.ts 2>&1 || true`
Expected on Node 22+: no syntax errors. If Node < 22 or the flag is unavailable, this step is best-effort — visually re-read the file and confirm:
- The default export is a function taking `pi: ExtensionAPI`
- Each `pi.on(...)` call uses one of the events confirmed in [pi-mono extensions docs](https://github.com/badlogic/pi-mono/blob/main/packages/coding-agent/docs/extensions.md): `session_start`, `session_shutdown`, `before_agent_start`, `agent_end`
- `spawn` from `node:child_process` is imported (not `spawnSync`)
- `child.stdin.end(...)` writes the JSON and closes stdin

End-to-end verification (pi running in interactive mode, watching `agent-status list` reflect the events) is out of scope for the plan — record it as a manual smoke test in the commit message rather than a TDD step.

- [ ] **Step 4: Commit the extension**

```bash
git add extensions/pi-coding-agent.ts
git commit -m "$(cat <<'EOF'
feat(extensions): add pi-coding-agent bridge

- TypeScript pi extension subscribing to `session_start`,
  `session_shutdown`, `before_agent_start`, and `agent_end`
- Shells out to `agent-status set/clear --agent pi-coding-agent`
  with `{"session_id":"<uuid>"}` on stdin
- Fire-and-forget `spawn` so pi's event loop is never blocked;
  errors silenced because the binary may not be installed
EOF
)"
```

---

## Task 3: README and CLAUDE.md docs

**Files:**
- Modify: `README.md`
- Modify: `CLAUDE.md`

- [ ] **Step 1: Update the README intro to mention pi**

Edit `README.md` line 3 (the lead paragraph). Find:

```markdown
A small Rust CLI that shows in tmux's `status-right` which AI coding agent sessions are waiting on user input. Currently supports Claude Code; the architecture is set up to plug in additional agents (Codex CLI, Cursor CLI, OpenCode) without restructuring.
```

Replace with:

```markdown
A small Rust CLI that shows in tmux's `status-right` which AI coding agent sessions are waiting on user input. Supports [Claude Code](https://claude.com/claude-code) and [pi](https://pi.dev); the architecture is set up to plug in additional agents (Codex CLI, Cursor CLI, OpenCode) without restructuring.
```

- [ ] **Step 2: Add the pi config section to the README**

Edit `README.md`. Find the existing `### tmux (~/.tmux.conf)` heading. Insert a new subsection **immediately before** it (i.e. after the closing ``` of the Claude Code JSON block on what is currently line 48, before the blank line that precedes `### tmux`):

```markdown
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

```

(Watch the trailing blank line: keep one blank line between the new pi section and the `### tmux (~/.tmux.conf)` heading that follows.)

- [ ] **Step 3: Update the State location example to show the new agent**

Edit `README.md`. Find the JSON example under `## State location` (currently around line 87):

```json
{"agent":"claude-code","project":"agent-status","cwd":"/path/to/project","event":"notify","tmux_pane":"%17","ts":1778163565}
```

This is fine as-is — the format is identical for pi. Add a one-line callout immediately after the JSON block, before the next heading:

```markdown
The `agent` field is `"claude-code"` or `"pi-coding-agent"`; new agents use their own lowercase-hyphenated name.
```

- [ ] **Step 4: Update CLAUDE.md to note the in-process bridge pattern**

Edit `CLAUDE.md`. Find the section heading `## Adding a new agent` and the paragraph immediately under it. After the existing four numbered steps and the closing line ("No changes to `state.rs`, `commands.rs`, or `main.rs` should be needed for a typical new agent — that's the test of the abstraction."), append a new paragraph:

```markdown
Some agents (e.g. pi at `pi.dev`) don't have a shell-hook mechanism — their lifecycle events fire in-process inside the agent's runtime. For those, the Rust `Agent` impl is unchanged (it still reads JSON from stdin), but the integration ships an additional bridge file that runs inside the agent and shells out to `agent-status`. See `extensions/pi-coding-agent.ts` and the pi section of `README.md`. The `Agent::extract_session_id` contract still applies — the bridge constructs the JSON payload, so we control the field name.
```

- [ ] **Step 5: Verify nothing broke**

Run: `cargo test && cargo clippy --all-targets --all-features --locked -- -D warnings`
Expected: tests still pass, clippy clean. (The doc changes don't touch source, but a sanity check is cheap.)

- [ ] **Step 6: Commit the docs**

```bash
git add README.md CLAUDE.md
git commit -m "$(cat <<'EOF'
docs: document pi-coding-agent integration

- Add pi extension install instructions to README under Configure
- Note pi's lack of a `Notification`-equivalent event as a known limitation
- Update README intro and state-location callout to mention pi
- Document the in-process bridge pattern in CLAUDE.md
EOF
)"
```

---

## Self-Review Checklist (run before handoff)

After all three tasks are committed:

- [ ] Run `cargo test` — every test passes; the test count includes the four new `pi_coding_agent::tests::*` and the new `by_name_resolves_pi_coding_agent`.
- [ ] Run `cargo clippy --all-targets --all-features --locked -- -D warnings` — exit 0.
- [ ] Run `git log --oneline -5` — three commits in order: rust impl, ts extension, docs.
- [ ] Open `README.md` in a viewer and confirm the pi section reads cleanly: it appears before the tmux section, the table renders, the limitation paragraph is intact.
- [ ] Confirm `extensions/pi-coding-agent.ts` exists and the default export is at the top, helpers below it.
- [ ] **Manual end-to-end smoke test (optional, requires a pi install):** copy the extension into `~/.pi/agent/extensions/`, launch pi with a provider configured, send a prompt, watch the agent finish a turn, and confirm `agent-status list` shows the pi session as waiting. Send another prompt; confirm it clears. Quit pi; confirm the entry disappears. If any step fails, file as a follow-up — do not block the plan on this since not every implementer will have a pi-compatible provider configured.

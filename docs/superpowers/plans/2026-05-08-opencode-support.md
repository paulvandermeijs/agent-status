# opencode Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add agent-status integration for [opencode](https://opencode.ai) so opencode sessions waiting on user input show up in the tmux status line alongside Claude Code and pi sessions.

**Architecture:** Two parts. (1) A Rust `OpencodeAgent` registered under name `opencode` that reads `session_id` from the stdin JSON payload — structurally identical to `ClaudeCodeAgent` and `PiCodingAgent` because we control the bridge file's payload format. (2) A TypeScript opencode plugin (`extensions/opencode.ts`) using `@opencode-ai/plugin`'s unified `event` hook to subscribe to opencode's in-process lifecycle events (`session.idle`, `permission.updated`, `session.created`, `session.deleted`) and shell out to the `agent-status` binary, piping `{"session_id":"<sessionID>"}` to stdin. Unlike pi, opencode emits a `permission.updated` event when an agent pauses for permission, so `opencode` supports both `notify` (mid-turn permission prompt) and `done` (turn finished) states — the full Claude Code feature parity.

**Tech Stack:** Rust (existing crate, no new deps), TypeScript (single-file opencode plugin using only `node:child_process`).

---

## Background: opencode's event surface

opencode plugins are JavaScript/TypeScript modules placed in `~/.config/opencode/plugins/` (global) or `.opencode/plugins/` (per-project). They are auto-loaded at startup (per [opencode plugins docs](https://opencode.ai/docs/plugins/)). Each plugin module exports an async function that receives a context object and returns a hooks object. The unified `event` hook receives every lifecycle event and discriminates on `event.type`.

Relevant event variants and their payload shapes (sourced from `packages/sdk/js/src/gen/types.gen.ts` in the upstream opencode repo):

```typescript
type EventSessionIdle = {
  type: "session.idle"
  properties: { sessionID: string }
}
type EventSessionCreated = {
  type: "session.created"
  properties: { info: Session }      // Session has `id: string`
}
type EventSessionDeleted = {
  type: "session.deleted"
  properties: { info: Session }
}
type EventPermissionUpdated = {
  type: "permission.updated"
  properties: Permission             // Permission has `sessionID: string`
}
```

Note the **inconsistent field naming**: `session.idle` and `permission.updated` carry `sessionID` directly on `properties`; `session.created` / `session.deleted` carry the full Session under `properties.info`, with the id at `info.id`. The plugin handles both shapes.

The mapping we'll wire is:

| opencode event       | agent-status call                             | Why                                              |
|----------------------|-----------------------------------------------|--------------------------------------------------|
| `session.idle`       | `set --agent opencode done`                   | Agent finished a turn — analogue of Claude Code's `Stop` and pi's `agent_end`. |
| `permission.updated` | `set --agent opencode notify`                 | Agent paused waiting for permission — analogue of Claude Code's `Notification`. |
| `session.created`    | `clear --agent opencode`                      | Defensive: a fresh session has nothing pending.  |
| `session.deleted`    | `clear --agent opencode`                      | Session went away — drop any state file.         |

There is no opencode event for "user just submitted a prompt" (analogue of Claude Code's `UserPromptSubmit` or pi's `before_agent_start`). We accept the same staleness behavior pi has: after `session.idle` fires, the indicator stays on `done` while the user types the next prompt and continues showing `done` (with refreshed timestamp) on each subsequent `session.idle`. This is correct: the session *is* one that finished a turn and is waiting for you. Documented as a known wart, not a bug.

---

## File Structure

- **Create** `src/agents/opencode.rs` — `OpencodeAgent` unit struct implementing `agents::Agent`. Reads `session_id` from JSON payload, with the four standard edge cases (CLAUDE.md mandate).
- **Modify** `src/agents/mod.rs` — declare `pub mod opencode;` and register `"opencode" => OpencodeAgent` in `by_name`. Add a registry test.
- **Create** `extensions/opencode.ts` — single-file opencode plugin. Subscribes to four lifecycle events, pulls the session id from each (handling both `properties.sessionID` and `properties.info.id`), and spawns `agent-status` as a fire-and-forget child process with the JSON payload on stdin.
- **Modify** `README.md` — update intro (add opencode link, remove "OpenCode" from the "future agents" list); add a "opencode" subsection under "Configure" mirroring the pi section; update the state-location callout to mention `"opencode"` as an `agent` field value; bump test counts in the Development block.
- **Modify** `CLAUDE.md` — bump the test counts in the build-line example (currently `31 tests (28 unit + 3 integration)` → `37 tests (34 unit + 3 integration)`).

Note: keep the `src/main.rs`, `src/state.rs`, `src/commands.rs` triplet untouched. Per CLAUDE.md: "No changes to `state.rs`, `commands.rs`, or `main.rs` should be needed for a typical new agent — that's the test of the abstraction." This is again the test.

---

## Task 1: Rust `OpencodeAgent` impl (TDD)

**Files:**
- Create: `src/agents/opencode.rs`
- Modify: `src/agents/mod.rs`

- [ ] **Step 1: Write the failing `name()` test in a new agent file**

Create `src/agents/opencode.rs` with just the test, no impl yet:

```rust
use crate::agents::Agent;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_is_opencode() {
        assert_eq!(OpencodeAgent.name(), "opencode");
    }
}
```

Then declare the module in `src/agents/mod.rs` so the test is discovered. Add `pub mod opencode;` immediately after the existing `pub mod pi_coding_agent;`:

```rust
pub mod claude_code;
pub mod opencode;
pub mod pi_coding_agent;
```

(Keep the modules alphabetized — `opencode` sorts between `claude_code` and `pi_coding_agent`.)

- [ ] **Step 2: Run the test, confirm it fails**

Run: `cargo test --lib agents::opencode`
Expected: FAIL with `cannot find type 'OpencodeAgent' in this scope` (or similar — the unit struct doesn't exist yet).

- [ ] **Step 3: Add the minimal `OpencodeAgent` struct and `name()`**

Edit `src/agents/opencode.rs` so the public API sits at the top, tests at the bottom (per the user's global "public API at top" rule). Final shape after this step:

```rust
use crate::agents::Agent;

/// opencode ([opencode.ai](https://opencode.ai)).
///
/// Reads `session_id` from the JSON piped in by the bundled opencode plugin at
/// `extensions/opencode.ts`, which fires on opencode's `session.idle`,
/// `permission.updated`, `session.created`, and `session.deleted` events.
pub struct OpencodeAgent;

impl Agent for OpencodeAgent {
    fn name(&self) -> &'static str {
        "opencode"
    }

    fn extract_session_id(&self, _stdin_json: &str) -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_is_opencode() {
        assert_eq!(OpencodeAgent.name(), "opencode");
    }
}
```

(`extract_session_id` is stubbed to `None` deliberately — we'll TDD it next.)

- [ ] **Step 4: Run the test, confirm it passes**

Run: `cargo test --lib agents::opencode`
Expected: 1 passed.

- [ ] **Step 5: Write the failing `extract_session_id_returns_id` test**

Append to the `tests` module in `src/agents/opencode.rs`:

```rust
    #[test]
    fn extract_session_id_returns_id() {
        let json = r#"{"session_id":"abc-123","other":"stuff"}"#;
        assert_eq!(
            OpencodeAgent.extract_session_id(json).as_deref(),
            Some("abc-123")
        );
    }
```

- [ ] **Step 6: Run the test, confirm it fails**

Run: `cargo test --lib agents::opencode::tests::extract_session_id_returns_id`
Expected: FAIL — `assertion failed: ... left: None, right: Some("abc-123")` because the stub returns `None`.

- [ ] **Step 7: Implement `extract_session_id`**

Replace the stub body in `src/agents/opencode.rs`:

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

This is structurally identical to `ClaudeCodeAgent::extract_session_id` and `PiCodingAgent::extract_session_id` because we control the TS plugin that constructs the payload — all three agents read `session_id` from the same on-stdin format. **Don't extract a shared helper:** the `Agent` trait was deliberately designed so each agent's field can vary (e.g. Cursor will use `conversation_id`); a shared helper here would be premature abstraction. The duplication is the point of the trait.

- [ ] **Step 8: Run the test, confirm it passes**

Run: `cargo test --lib agents::opencode::tests::extract_session_id_returns_id`
Expected: PASS.

- [ ] **Step 9: Add the three remaining edge-case tests**

CLAUDE.md mandates the four standard edge cases for any new agent. We just covered the happy path; add the other three. Append to the `tests` module:

```rust
    #[test]
    fn extract_session_id_returns_none_for_missing_field() {
        assert_eq!(
            OpencodeAgent.extract_session_id(r#"{"other":1}"#),
            None
        );
    }

    #[test]
    fn extract_session_id_returns_none_for_empty_string() {
        assert_eq!(
            OpencodeAgent.extract_session_id(r#"{"session_id":""}"#),
            None
        );
    }

    #[test]
    fn extract_session_id_returns_none_for_invalid_json() {
        assert_eq!(OpencodeAgent.extract_session_id("not json"), None);
    }
```

- [ ] **Step 10: Run all `opencode` tests, confirm they pass**

Run: `cargo test --lib agents::opencode`
Expected: 4 passed.

- [ ] **Step 11: Add the `by_name` registration test**

Edit `src/agents/mod.rs`. Find the `tests` module at the bottom and append a new test inside it (do NOT touch the existing `by_name_resolves_*` tests):

```rust
    #[test]
    fn by_name_resolves_opencode() {
        let agent = by_name("opencode").expect("opencode is a registered agent");
        assert_eq!(agent.name(), "opencode");
    }
```

- [ ] **Step 12: Run the test, confirm it fails**

Run: `cargo test --lib agents::tests::by_name_resolves_opencode`
Expected: FAIL with `opencode is a registered agent` panic — the registry doesn't know about it yet.

- [ ] **Step 13: Register `opencode` in the `by_name` match**

Edit `src/agents/mod.rs`. Find the existing `by_name` function:

```rust
pub fn by_name(name: &str) -> Option<Box<dyn Agent>> {
    match name {
        "claude-code" => Some(Box::new(claude_code::ClaudeCodeAgent)),
        "pi-coding-agent" => Some(Box::new(pi_coding_agent::PiCodingAgent)),
        _ => None,
    }
}
```

Add the new arm. Keep arms alphabetized for readability — `opencode` sorts between `claude-code` and `pi-coding-agent`:

```rust
pub fn by_name(name: &str) -> Option<Box<dyn Agent>> {
    match name {
        "claude-code" => Some(Box::new(claude_code::ClaudeCodeAgent)),
        "opencode" => Some(Box::new(opencode::OpencodeAgent)),
        "pi-coding-agent" => Some(Box::new(pi_coding_agent::PiCodingAgent)),
        _ => None,
    }
}
```

- [ ] **Step 14: Run the registration test, confirm it passes**

Run: `cargo test --lib agents::tests::by_name_resolves_opencode`
Expected: PASS.

- [ ] **Step 15: Run the full test suite**

Run: `cargo test`
Expected: zero failures, and the following six newly-named tests appear in the output:
- `agents::opencode::tests::name_is_opencode`
- `agents::opencode::tests::extract_session_id_returns_id`
- `agents::opencode::tests::extract_session_id_returns_none_for_missing_field`
- `agents::opencode::tests::extract_session_id_returns_none_for_empty_string`
- `agents::opencode::tests::extract_session_id_returns_none_for_invalid_json`
- `agents::tests::by_name_resolves_opencode`

The numeric total at this checkpoint should be 37 (34 unit + 3 integration), assuming the baseline before this task was 31. If the baseline drifted, what matters is: zero failures and all six newly-named tests in the report.

- [ ] **Step 16: Run clippy with the project's required gate**

Run: `cargo clippy --all-targets --all-features --locked -- -D warnings`
Expected: no warnings, exit 0. The project's `[lints.clippy] pedantic = "warn"` plus `-D warnings` means even pedantic lints fail the build, so address any complaints before committing.

- [ ] **Step 17: Commit the Rust side**

```bash
git add src/agents/opencode.rs src/agents/mod.rs
git commit -m "$(cat <<'EOF'
feat(agents): add opencode

- Add `OpencodeAgent` reading `session_id` from stdin JSON payload
- Register `opencode` in the `by_name` registry
- Cover the four standard `extract_session_id` edge cases
EOF
)"
```

---

## Task 2: TypeScript opencode plugin

**Files:**
- Create: `extensions/opencode.ts`

This is the bridge that turns opencode's in-process events into `agent-status` subprocess invocations. It is shipped as a single file the user drops into `~/.config/opencode/plugins/`.

- [ ] **Step 1: Confirm the `extensions/` dir exists and inspect its contents**

Run: `ls extensions/`
Expected: `pi-coding-agent.ts` (already there from the pi integration). Add `opencode.ts` alongside it. Do not touch the existing pi file.

- [ ] **Step 2: Create the plugin file**

Create `extensions/opencode.ts` with this exact content. Public API (the named export) is at the top; helpers below it, per the user's global rule.

```typescript
import { spawn } from "node:child_process";

/**
 * Bridges opencode lifecycle events to the `agent-status` CLI so opencode
 * sessions waiting on user input show up in tmux's status-right.
 *
 * Install: copy this file to `~/.config/opencode/plugins/opencode.ts`
 * (or `.opencode/plugins/opencode.ts` for per-project install).
 * Override the binary path with `AGENT_STATUS_BIN` if not at the default.
 */
export const AgentStatusPlugin = async () => {
  return {
    event: async ({ event }: { event: any }) => {
      switch (event?.type) {
        case "session.idle":
          fire(event.properties?.sessionID, "set", "done");
          return;
        case "permission.updated":
          fire(event.properties?.sessionID, "set", "notify");
          return;
        case "session.created":
        case "session.deleted":
          fire(event.properties?.info?.id, "clear");
          return;
      }
    },
  };
};

const BIN =
  process.env.AGENT_STATUS_BIN ?? `${process.env.HOME}/.claude/bin/agent-status`;

type Action = "set" | "clear";
type SetEvent = "notify" | "done";

function fire(
  sessionId: string | undefined,
  action: Action,
  event?: SetEvent,
): void {
  if (!sessionId) return;

  const args =
    action === "set"
      ? ["set", "--agent", "opencode", event!]
      : ["clear", "--agent", "opencode"];

  const child = spawn(BIN, args, {
    stdio: ["pipe", "ignore", "ignore"],
  });
  child.on("error", () => {
    // best-effort: agent-status may not be installed; never crash opencode
  });
  child.stdin.end(JSON.stringify({ session_id: sessionId }));
}
```

Notes for the implementer:

- **Why no `import type { Plugin } from "@opencode-ai/plugin"`?** The plugin file is dropped into `~/.config/opencode/plugins/` as a single standalone TypeScript file with no `node_modules` adjacent to it. opencode's plugin loader handles type-stripped TS at runtime, but adding a type-only import means users with older loaders or stricter TS configs hit module-resolution errors. The plugin's runtime behavior is unaffected by the missing types — `event: any` is the same trade-off pi's bridge made (`ctx: any`). If a user wants strong typing they can add the import in their local copy.
- **Why two different paths to the session id?** opencode's event payloads are heterogeneous: `session.idle` and `permission.updated` carry the id directly as `properties.sessionID`, while `session.created` / `session.deleted` carry the full `Session` object as `properties.info`, with the id at `info.id`. The bridge handles both. (Source: `packages/sdk/js/src/gen/types.gen.ts` in the upstream opencode repo.)
- **Why fire-and-forget `spawn` instead of `spawnSync`?** opencode event handlers are awaited, but our handler immediately returns after spawning the child without awaiting it; the child runs alongside opencode without blocking the event loop. This matches the `refresh_tmux` pattern on the Rust side (per `CLAUDE.md`: child stderr/stdout redirected to /dev/null, errors silenced) and the pi bridge.
- **Why `permission.updated` instead of `permission.replied`?** `permission.updated` fires when a permission state changes — including the initial creation, which is the moment we want the indicator to show "needs attention". `permission.replied` fires after the user has answered, which is too late. If `permission.updated` over-fires on resolution (firing again when the user clicks "allow"), the next `session.idle` will overwrite to `done` — minor flicker, correct end state.
- **Why no event for "user submitted a prompt"?** opencode does not expose a clean lifecycle event for that point in time (the closest would be filtering `message.updated` to `properties.info.role === "user"`, but that depends on Message-shape internals we don't want to bind to). The result: after a turn ends, the indicator stays on `done` while the user types the next prompt, and continues showing `done` (with a fresh timestamp) on each subsequent `session.idle`. That is correct behavior — the session *is* the one waiting for you.

- [ ] **Step 3: Lightweight syntax/structure verification**

opencode's runtime check is the canonical verification, but it requires a working opencode install with a provider. As a cheap alternative, confirm the file parses as valid TypeScript by stripping types:

Run: `node --experimental-strip-types --check extensions/opencode.ts 2>&1 || true`
Expected on Node 22+: no syntax errors. If Node < 22 or the flag is unavailable, this step is best-effort — visually re-read the file and confirm:
- The named export `AgentStatusPlugin` is at the top of the file
- The handler returns an object with an `event` key whose value is an async function taking `({ event })`
- Each `case` in the switch matches one of: `session.idle`, `permission.updated`, `session.created`, `session.deleted`
- `spawn` from `node:child_process` is imported (not `spawnSync`)
- `child.stdin.end(...)` writes the JSON and closes stdin
- The handler returns `void` (after `fire(...)`) without awaiting the child

End-to-end verification (opencode running interactively, watching `agent-status list` reflect the events) is out of scope for the plan — record it as a manual smoke test in the commit message rather than a TDD step.

- [ ] **Step 4: Commit the plugin**

```bash
git add extensions/opencode.ts
git commit -m "$(cat <<'EOF'
feat(extensions): add opencode bridge

- TypeScript opencode plugin subscribing to `session.idle`,
  `permission.updated`, `session.created`, and `session.deleted`
- Shells out to `agent-status set/clear --agent opencode`
  with `{"session_id":"<sessionID>"}` on stdin
- Handles the heterogeneous event payload shapes (sessionID on
  session.idle / permission.updated; info.id on session.created /
  session.deleted)
- Fire-and-forget `spawn` so opencode's event loop is never blocked;
  errors silenced because the binary may not be installed
EOF
)"
```

---

## Task 3: README and CLAUDE.md docs

**Files:**
- Modify: `README.md`
- Modify: `CLAUDE.md`

- [ ] **Step 1: Update the README intro to mention opencode**

Edit `README.md` line 3 (the lead paragraph). Find:

```markdown
A small Rust CLI that shows in tmux's `status-right` which AI coding agent sessions are waiting on user input. Supports [Claude Code](https://claude.com/claude-code) and [pi](https://pi.dev); the architecture is set up to plug in additional agents (Codex CLI, Cursor CLI, OpenCode) without restructuring.
```

Replace with (note: removed "OpenCode" from the future-agents list since it is now supported, and added the link as the third comma-separated supported agent):

```markdown
A small Rust CLI that shows in tmux's `status-right` which AI coding agent sessions are waiting on user input. Supports [Claude Code](https://claude.com/claude-code), [pi](https://pi.dev), and [opencode](https://opencode.ai); the architecture is set up to plug in additional agents (Codex CLI, Cursor CLI) without restructuring.
```

- [ ] **Step 2: Add the opencode config section to the README**

Edit `README.md`. Find the existing `### tmux (`~/.tmux.conf`)` heading. Insert a new subsection **immediately before** it (i.e. after the closing of the pi section's "Known limitation" paragraph, before the blank line that precedes `### tmux`):

````markdown
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

If your `agent-status` binary is not at `~/.claude/bin/agent-status`, set `AGENT_STATUS_BIN` in your shell environment before launching opencode.

Unlike pi, opencode emits a `permission.updated` event when an agent pauses for a permission prompt, so opencode supports both `notify` and `done` indicator states (full feature parity with Claude Code). The one wart: opencode has no event for "user submitted a prompt", so after a turn ends the indicator stays on `done` while the user types the next prompt — by design, since the session *is* the one that needs your attention.

````

(Watch the trailing blank line: keep one blank line between the new opencode section and the `### tmux (`~/.tmux.conf`)` heading that follows.)

- [ ] **Step 3: Update the State location callout to include opencode**

Edit `README.md`. Find the existing one-line callout under `## State location` (currently right after the JSON example):

```markdown
The `agent` field is `"claude-code"` or `"pi-coding-agent"`; new agents use their own lowercase-hyphenated name.
```

Replace with:

```markdown
The `agent` field is `"claude-code"`, `"opencode"`, or `"pi-coding-agent"`; new agents use their own lowercase-hyphenated name.
```

- [ ] **Step 4: Update the test count in the README's Development block**

Edit `README.md`. Find the line in the `## Development` block:

```sh
cargo test                                                   # 31 tests (28 unit + 3 integration)
```

Replace the count with the post-opencode total — 6 new unit tests (5 in `agents::opencode::tests` + 1 in `agents::tests`) bring it from 28 → 34 unit tests, integration unchanged at 3:

```sh
cargo test                                                   # 37 tests (34 unit + 3 integration)
```

- [ ] **Step 5: Update CLAUDE.md test count**

Edit `CLAUDE.md`. Find the same comment in the `## Build / test / lint` block:

```sh
cargo test                                                            # 31 tests (28 unit + 3 integration)
```

Replace with:

```sh
cargo test                                                            # 37 tests (34 unit + 3 integration)
```

(No other CLAUDE.md changes needed — the "Adding a new agent" section already documents the in-process bridge pattern from the pi integration; that note covers opencode without modification.)

- [ ] **Step 6: Verify nothing broke**

Run: `cargo test && cargo clippy --all-targets --all-features --locked -- -D warnings`
Expected: 37 tests pass, clippy clean. (The doc changes don't touch source, but a sanity check is cheap, and re-running clippy catches any drift if Task 1's commit was rebased.)

- [ ] **Step 7: Commit the docs**

```bash
git add README.md CLAUDE.md
git commit -m "$(cat <<'EOF'
docs: document opencode integration

- Add opencode plugin install instructions to README under Configure
- Note the lack of a "user submitted" event as a known wart
- Update README intro and state-location callout to mention opencode
- Bump test counts in README and CLAUDE.md (31 -> 37)
EOF
)"
```

---

## Self-Review Checklist (run before handoff)

After all three tasks are committed:

- [ ] Run `cargo test` — every test passes; the test count is 37 and includes the four new `agents::opencode::tests::*` plus `agents::tests::by_name_resolves_opencode`.
- [ ] Run `cargo clippy --all-targets --all-features --locked -- -D warnings` — exit 0.
- [ ] Run `git log --oneline -5` — three new commits in order: rust impl, ts plugin, docs.
- [ ] Open `README.md` in a viewer and confirm the opencode section reads cleanly: it appears between the pi section and the tmux section, the table renders, the limitation paragraph is intact, and the intro lists three supported agents.
- [ ] Confirm `extensions/opencode.ts` exists and the named export `AgentStatusPlugin` is at the top, helpers below it.
- [ ] Confirm `extensions/pi-coding-agent.ts` is unchanged (a quick `git diff HEAD~3 -- extensions/pi-coding-agent.ts` should report no diff).
- [ ] **Manual end-to-end smoke test (optional, requires an opencode install with a provider):** copy the plugin into `~/.config/opencode/plugins/`, launch opencode, send a prompt that triggers a tool requiring permission, and confirm `agent-status list` shows the opencode session as `notify` while the permission prompt is open. Approve the permission, let the turn finish, and confirm the entry transitions to `done`. Send another prompt; confirm it eventually returns to `done` after the next idle. Quit opencode; confirm the entry disappears via `session.deleted`. If any step fails, file as a follow-up — do not block the plan on this since not every implementer will have opencode configured.

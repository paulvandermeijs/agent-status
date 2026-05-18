# Claude Code PreToolUse Hook Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Clear the `notify` ("Needs Input") state as soon as Claude Code resumes work after the user grants a permission — instead of waiting for the next `Stop` hook to fire, which can be many tool calls later. This matches the fix cmux applied in PR #1306 ("Fix stale Claude sidebar status: add missing hooks"): a `PreToolUse` hook clears the "Needs Input" indicator on the first tool call after a permission grant. Pure UX win, no behavior change for sessions that never hit a permission prompt.

**Architecture:** Two parts. (1) Recommend a new `PreToolUse` hook line in `README.md`'s install snippet that calls `agent-status clear --agent claude-code`. (2) Make `clear` not refresh tmux when nothing was removed, so that the new `PreToolUse` hook (which fires on every tool call) doesn't trigger a tmux status redraw on every tool call. The current `run_clear` unconditionally calls `refresh_tmux()` after `store.remove()`, even when no file was present — fine for the today's hooks (`UserPromptSubmit` etc. fire rarely) but wasteful when `PreToolUse` is in the mix.

**Tech Stack:** Rust (existing crate). No new dependencies. No changes to the wire format.

---

## File Structure

- **Modify** `src/state.rs` — change `StateStore::remove` from `io::Result<()>` to `io::Result<bool>`, where `true` means a file was actually deleted and `false` means it was already absent. The `NotFound` arm just returns `Ok(false)` instead of `Ok(())`.
- **Modify** `src/main.rs:125-141` (`run_clear`) — gate `refresh_tmux()` on the bool returned from `store.remove()`, so a no-op `clear` doesn't refresh tmux.
- **Modify** `tests/cli.rs` — add an integration test that two consecutive `clear`s produce only one tmux-refresh-able event. (We can't observe `refresh_tmux` directly in tests since the call happens regardless of whether tmux is running, but we can observe the public contract: idempotency.)
- **Modify** `README.md` — add the `PreToolUse` line to the Claude Code settings snippet; document the rationale in a short paragraph.

The change to `StateStore::remove`'s return type is a breaking change to an internal API surface, but `StateStore` is not exposed outside the crate (`lib.rs` doesn't exist — this is a binary crate), so there's only one caller (`run_clear`). The blast radius is one file.

---

## Task 1: Change `StateStore::remove` to return whether a file was actually deleted

**Files:**
- Modify: `src/state.rs:80-87` (`remove` method)
- Modify: `src/state.rs` tests (one existing test asserts `Ok(())` shape implicitly)

- [ ] **Step 1: Write the failing test**

Add to the `tests` mod in `src/state.rs`:

```rust
#[test]
fn remove_returns_true_when_file_was_present() {
    let dir = TempDir::new().unwrap();
    let store = StateStore::new(dir.path().into());
    store.write("s1", &sample_entry("p")).unwrap();
    assert!(store.remove("s1").unwrap(), "first remove should report deletion");
}

#[test]
fn remove_returns_false_when_file_was_already_absent() {
    let dir = TempDir::new().unwrap();
    let store = StateStore::new(dir.path().into());
    assert!(!store.remove("never-existed").unwrap());
    store.write("s1", &sample_entry("p")).unwrap();
    store.remove("s1").unwrap();
    assert!(!store.remove("s1").unwrap(), "second remove should report no-op");
}
```

- [ ] **Step 2: Run, confirm it fails to compile**

Run: `cargo test --lib state::tests::remove_returns`
Expected: FAIL with `expected bool, found ()` or similar — `remove` currently returns `()`.

- [ ] **Step 3: Update `StateStore::remove`'s signature and body**

Replace the existing method in `src/state.rs` with:

```rust
/// Remove the entry for `session_id`. Idempotent: returns `Ok(false)` when
/// the file is absent and `Ok(true)` when a file was actually deleted.
///
/// Callers can use the bool to skip side effects (e.g. tmux refresh) on
/// no-op clears — relevant for hooks like Claude Code's `PreToolUse` that
/// fire on every tool call and would otherwise generate excessive refreshes.
///
/// # Errors
/// Returns the underlying I/O error if removal fails for a reason other
/// than `NotFound`. Returns [`io::ErrorKind::InvalidInput`] when
/// `session_id` is empty or contains a path separator.
pub fn remove(&self, session_id: &str) -> io::Result<bool> {
    validate_session_id(session_id)?;
    match fs::remove_file(self.dir.join(session_id)) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}
```

- [ ] **Step 4: Update the existing `remove_is_idempotent` test**

The existing test relies on `remove` returning `()`. It still passes semantically (it just ignores the new bool) because of how `.unwrap()` works on `Result<bool, _>`, but explicitly assert the bools to keep the test informative:

Replace:

```rust
#[test]
fn remove_is_idempotent() {
    let dir = TempDir::new().unwrap();
    let store = StateStore::new(dir.path().into());
    store.remove("never-existed").unwrap();
    store.write("s1", &sample_entry("p")).unwrap();
    store.remove("s1").unwrap();
    store.remove("s1").unwrap();
    assert_eq!(store.list().unwrap().len(), 0);
}
```

with:

```rust
#[test]
fn remove_is_idempotent() {
    let dir = TempDir::new().unwrap();
    let store = StateStore::new(dir.path().into());
    assert!(!store.remove("never-existed").unwrap());
    store.write("s1", &sample_entry("p")).unwrap();
    assert!(store.remove("s1").unwrap());
    assert!(!store.remove("s1").unwrap());
    assert_eq!(store.list().unwrap().len(), 0);
}
```

- [ ] **Step 5: Update `write_rejects_path_traversal_session_id`'s use of `remove`**

Find:

```rust
let err = store.remove("../escape").unwrap_err();
assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
```

This already returns `Result<bool, io::Error>` so `unwrap_err()` still works. No change needed — verify by re-reading.

- [ ] **Step 6: Run all tests**

Run: `cargo test`
Expected: all green, including the two new tests.

- [ ] **Step 7: Run clippy**

Run: `cargo clippy --all-targets --all-features --locked -- -D warnings`
Expected: no warnings.

- [ ] **Step 8: Commit**

```bash
git add src/state.rs
git commit -m "feat(state): StateStore::remove returns whether a file was actually deleted"
```

---

## Task 2: Gate `refresh_tmux()` in `run_clear` on the new bool

**Files:**
- Modify: `src/main.rs:125-141` (`run_clear`)

- [ ] **Step 1: Update `run_clear`**

Replace the existing function body with:

```rust
fn run_clear(store: &StateStore, agent_name: &str) -> io::Result<()> {
    let Some(agent) = agents::by_name(agent_name) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unknown agent: {agent_name}"),
        ));
    };

    let mut buf = String::new();
    io::stdin().read_to_string(&mut buf)?;
    let Some(session_id) = agent.extract_session_id(&buf) else {
        return Ok(());
    };
    // Only refresh tmux when we actually removed something. `PreToolUse` fires
    // on every tool call; the typical case is "no state file present, this is
    // a no-op clear" and we shouldn't redraw the status bar for that.
    if store.remove(&session_id)? {
        refresh_tmux();
    }
    Ok(())
}
```

The change is one if-let against the bool. `run_set` is unchanged.

- [ ] **Step 2: Run all tests**

Run: `cargo test`
Expected: all green. The existing `end_to_end_set_status_clear` integration test passes because it exercises the case where state is present and clear should refresh — both branches of the if remain reachable.

- [ ] **Step 3: Run clippy**

Run: `cargo clippy --all-targets --all-features --locked -- -D warnings`
Expected: no warnings.

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "perf(main): skip tmux refresh on no-op clear"
```

---

## Task 3: Add integration test for repeated-clear no-op

**Files:**
- Modify: `tests/cli.rs` (add new test)

- [ ] **Step 1: Add the test**

Append to `tests/cli.rs`:

```rust
#[test]
fn repeated_clear_is_idempotent_and_silent() {
    let tmp = TempDir::new().unwrap();
    let state_dir = tmp.path().join("agent-status");

    // First clear of a never-set session: should be a clean no-op.
    let (stdout, stderr, code) = run(
        &state_dir,
        &["clear"],
        Some(r#"{"session_id":"ghost"}"#),
    );
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "");

    // Second clear of the same session: also no-op.
    let (_, _, code) = run(
        &state_dir,
        &["clear"],
        Some(r#"{"session_id":"ghost"}"#),
    );
    assert_eq!(code, 0);

    // After a set, a clear should still work and a second clear is a no-op.
    let (_, _, code) = run(
        &state_dir,
        &["set", "notify"],
        Some(r#"{"session_id":"s"}"#),
    );
    assert_eq!(code, 0);
    let (_, _, code) = run(
        &state_dir,
        &["clear"],
        Some(r#"{"session_id":"s"}"#),
    );
    assert_eq!(code, 0);
    let (_, _, code) = run(
        &state_dir,
        &["clear"],
        Some(r#"{"session_id":"s"}"#),
    );
    assert_eq!(code, 0);
}
```

- [ ] **Step 2: Run the integration tests**

Run: `cargo test --test cli`
Expected: all green.

- [ ] **Step 3: Commit**

```bash
git add tests/cli.rs
git commit -m "test(cli): repeated clear is idempotent and silent"
```

---

## Task 4: Add the `PreToolUse` hook to the README snippet

**Files:**
- Modify: `README.md` (the Claude Code settings snippet)

- [ ] **Step 1: Update the JSON snippet**

Find the Claude Code hook block:

```json
{
  "hooks": {
    "Notification":     [{ "hooks": [{ "type": "command", "command": "$HOME/.claude/bin/agent-status set --agent claude-code notify" }] }],
    "Stop":             [{ "hooks": [{ "type": "command", "command": "$HOME/.claude/bin/agent-status set --agent claude-code done"   }] }],
    "UserPromptSubmit": [{ "hooks": [{ "type": "command", "command": "$HOME/.claude/bin/agent-status clear --agent claude-code"      }] }],
    "SessionStart":     [{ "hooks": [{ "type": "command", "command": "$HOME/.claude/bin/agent-status clear --agent claude-code"      }] }],
    "SessionEnd":       [{ "hooks": [{ "type": "command", "command": "$HOME/.claude/bin/agent-status clear --agent claude-code"      }] }]
  }
}
```

Replace with:

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

The `PreToolUse` line sits between `UserPromptSubmit` and `SessionStart` so the visual order reads "prompt-submit → pre-tool-use → session-start/end" — left-to-right in lifecycle order.

- [ ] **Step 2: Add the rationale paragraph**

Find the paragraph in README that follows the snippet (the "Merge the following into the top-level `hooks` block" section). Immediately after the JSON, before the next subsection heading, add:

```markdown
The `PreToolUse` hook fires before every tool call Claude makes. The hook
issues a `clear` — which is idempotent — so the agent-status indicator
correctly transitions out of "Needs Input" the moment Claude resumes work
after you grant a permission, instead of staying "Needs Input" until the
next `Stop` fires (which may be many tool calls later). The `PreToolUse`
hook fires often, but `clear` skips refreshing tmux when there's nothing
to remove, so the steady-state cost is one filesystem stat per tool call.
```

- [ ] **Step 3: Verify the README renders sanely**

Run: `head -60 README.md`
Expected: the new line in the JSON block is properly aligned with the others; the rationale paragraph is well-formed Markdown.

- [ ] **Step 4: Commit**

```bash
git add README.md
git commit -m "docs(readme): wire PreToolUse hook to clear stale Needs Input state"
```

---

## Task 5: Smoke-test against a real Claude Code session

**Files:** none modified.

- [ ] **Step 1: Install the freshly-built binary and the updated settings.json hook block**

```bash
cargo build --release
install -m 0755 target/release/agent-status ~/.claude/bin/agent-status
# Then manually edit ~/.claude/settings.json to add the PreToolUse line.
```

- [ ] **Step 2: Trigger a permission prompt mid-turn**

In a fresh `claude` session inside a tmux pane, ask for something that requires permission (e.g. `write a file to /etc/test.txt`). When the permission prompt appears, observe that `agent-status status` shows `[!] <project>` (the `notify` event), confirming the `Notification` hook fired.

- [ ] **Step 3: Grant the permission and observe the indicator clear**

Approve the permission. As Claude resumes and makes its first tool call, the `PreToolUse` hook should fire and `clear` the state file. Within a tmux refresh interval (default we recommend is `set -g status-interval 5`), the `[!] <project>` indicator should disappear — even though Claude is still mid-turn and hasn't yet fired `Stop`.

Verify directly:

```bash
ls "${XDG_RUNTIME_DIR:-/tmp}/agent-status/"
~/.claude/bin/agent-status status
```

Both should show no entry for the active claude session as soon as the next tool call fires.

- [ ] **Step 4: No commit needed for smoke test**

If the indicator clears as expected, the plan is complete.

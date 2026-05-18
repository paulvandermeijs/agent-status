# Claude Code PID Liveness Sweep Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Automatically prune state files whose owning agent process is dead, so the tmux indicator stops showing stale "waiting" entries after a Claude Code SIGKILL, crash, or `pkill`. Eliminates the "Stale state on abnormal exit" caveat in `README.md`.

**Architecture:** Record the hook-caller's parent PID (the claude binary's PID — claude exec's the hook script, so `getppid()` inside the hook is claude itself) on every `set`. Add a `pid: Option<u32>` field to `AttentionEntry` (optional for wire-compat with files written by older versions and by the bash precursor). Add a `prune_dead` step at the top of `list()` that walks the directory, asks `kill -0 <pid>` whether each owning process is alive, and removes entries whose owner is dead. The `status`, `list`, and `preview` subcommands all funnel through `list()`, so they all benefit. Borrowed in spirit from cmux's 30-second PID sweep (PR #1306), but folded into the existing on-read path instead of a background timer — we have no daemon, and `status` is already polled every few seconds by tmux's `status-interval`, so the existing polling cadence becomes our sweep cadence.

**Tech Stack:** Rust (existing crate). No new dependencies — we use `std::os::unix::process::parent_id()` (stable since Rust 1.27) to read PPID, and shell out to `kill -0 <pid>` via `std::process::Command` for liveness checks (avoids needing `libc` and keeps `unsafe_code = "forbid"`).

---

## File Structure

- **Modify** `src/state.rs` — add `pid: Option<u32>` to `AttentionEntry`; add `fn is_pid_alive(pid: u32) -> bool` and a `prune_dead` helper on `StateStore` that uses it; call `prune_dead` from `list()` before returning.
- **Modify** `src/commands.rs` — extend `build_entry` to accept `pid: Option<u32>`; update all unit-test call sites (they currently pass nothing for it; default to `None`).
- **Modify** `src/main.rs` — capture `parent_id()` in `run_set` and pass it to `build_entry`. On macOS/Linux this is the claude binary's PID; in tests where `agent-status` is spawned directly, it's the test harness PID (still useful — the entry will be pruned when the test process exits, which is the desired "auto-clean" property).
- **Modify** `tests/cli.rs` — add an integration test that writes a state entry with a fake PID (use a PID that's guaranteed dead, e.g. `1_000_000_000` which is above the kernel's `pid_max`), then runs `status` and confirms the entry is gone and the directory entry has been removed.
- **Modify** `README.md` — delete the "Stale state on abnormal exit" bullet from the Caveats section (it's no longer true after this plan lands).

---

## Task 1: Add `pid` field to `AttentionEntry` with wire-compat default

**Files:**
- Modify: `src/state.rs:11-29` (the `AttentionEntry` struct)
- Modify: `src/state.rs` tests (a few struct literals will need updating)

- [ ] **Step 1: Write the failing roundtrip test**

Add to the `tests` mod in `src/state.rs`, right after `entry_message_field_omitted_from_json_when_none`:

```rust
#[test]
fn entry_pid_field_roundtrips_when_set() {
    let entry = AttentionEntry {
        agent: "claude-code".into(),
        project: "p".into(),
        cwd: "/c".into(),
        event: "notify".into(),
        tmux_pane: "%1".into(),
        ts: 1,
        message: None,
        pid: Some(42_000),
    };
    let json = serde_json::to_string(&entry).unwrap();
    assert!(json.contains(r#""pid":42000"#));
    let parsed: AttentionEntry = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.pid, Some(42_000));
}

#[test]
fn entry_pid_field_omitted_from_json_when_none() {
    let entry = AttentionEntry {
        agent: "claude-code".into(),
        project: "p".into(),
        cwd: "/c".into(),
        event: "done".into(),
        tmux_pane: "%1".into(),
        ts: 1,
        message: None,
        pid: None,
    };
    let json = serde_json::to_string(&entry).unwrap();
    assert!(!json.contains("pid"), "got: {json}");
}

#[test]
fn entry_deserializes_when_pid_field_absent() {
    // Older state files (no pid field) must still load.
    let json = r#"{"agent":"claude-code","project":"p","cwd":"/c","event":"done","tmux_pane":"%1","ts":1}"#;
    let parsed: AttentionEntry = serde_json::from_str(json).unwrap();
    assert!(parsed.pid.is_none());
}
```

- [ ] **Step 2: Run the tests, confirm they fail**

Run: `cargo test --lib state::tests::entry_pid_field`
Expected: FAIL with `no field 'pid' on type 'AttentionEntry'` (the struct doesn't have the field yet).

- [ ] **Step 3: Add the `pid` field with the same skip-if-none pattern as `message`**

In `src/state.rs`, modify the `AttentionEntry` struct to add the new field right after `message`:

```rust
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct AttentionEntry {
    pub agent: String,
    pub project: String,
    pub cwd: String,
    pub event: String,
    pub tmux_pane: String,
    pub ts: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// PID of the agent process at the time the hook fired (typically `getppid()`
    /// from inside the hook script — the claude/opencode/pi binary). Used by
    /// [`StateStore::prune_dead`] to clean up state files whose owning process
    /// has exited without firing its session-end hook. Absent in entries written
    /// by older binaries; entries without a pid are never auto-pruned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
}
```

- [ ] **Step 4: Update the existing struct-literal call sites in this file's tests**

Several existing tests build `AttentionEntry` literals without `pid`. Each will now fail to compile until updated. The list (search for `AttentionEntry {` in `src/state.rs`):
- `entry_roundtrips_through_json`
- `entry_matches_bash_plan_field_names`
- `sample_entry`
- `entry_message_field_roundtrips_when_set`
- `entry_message_field_omitted_from_json_when_none`

For each, add `pid: None,` immediately after `message: ...`. Example diff for `sample_entry`:

```rust
fn sample_entry(project: &str) -> AttentionEntry {
    AttentionEntry {
        agent: "claude-code".into(),
        project: project.into(),
        cwd: format!("/x/{project}"),
        event: "notify".into(),
        tmux_pane: "%1".into(),
        ts: 1,
        message: None,
        pid: None,
    }
}
```

The `entry_matches_bash_plan_field_names` test deserves a deliberate look: it asserts the bash-precursor field names are still present in the JSON, and `pid` was not in the bash version. The test should *not* assert `pid` is present (it can be absent for compat). The current assertions are existence-only, not exhaustive, so adding `pid: None` to the literal is enough — the test still passes because `skip_serializing_if` keeps `pid` out of the JSON.

- [ ] **Step 5: Run the new tests, confirm they pass**

Run: `cargo test --lib state::tests`
Expected: all green, including the three new pid tests and all existing tests.

- [ ] **Step 6: Run clippy gate**

Run: `cargo clippy --all-targets --all-features --locked -- -D warnings`
Expected: no warnings.

- [ ] **Step 7: Commit**

```bash
git add src/state.rs
git commit -m "feat(state): add optional pid field to AttentionEntry for liveness tracking"
```

---

## Task 2: Thread `pid` through `build_entry` and `run_set`

**Files:**
- Modify: `src/commands.rs:9-29` (`build_entry` signature and body)
- Modify: `src/commands.rs` tests (call sites)
- Modify: `src/main.rs:88-123` (`run_set`)

- [ ] **Step 1: Write the failing test for `build_entry`**

Add to the `tests` mod in `src/commands.rs`:

```rust
#[test]
fn build_entry_stores_pid_when_some() {
    let e = build_entry(
        "claude-code",
        "notify",
        "/Users/me/work/app",
        "%5",
        42,
        Some("Permission required"),
        Some(12345),
    );
    assert_eq!(e.pid, Some(12345));
}

#[test]
fn build_entry_leaves_pid_none_when_none() {
    let e = build_entry("claude-code", "done", "/x/p", "%1", 1, None, None);
    assert!(e.pid.is_none());
}
```

- [ ] **Step 2: Run the test, confirm it fails to compile**

Run: `cargo test --lib commands::tests::build_entry_stores_pid_when_some`
Expected: FAIL with "this function takes 6 arguments but 7 arguments were supplied".

- [ ] **Step 3: Extend `build_entry` to accept `pid: Option<u32>`**

Replace the existing signature and body in `src/commands.rs`:

```rust
pub fn build_entry(
    agent: &str,
    event: &str,
    cwd: &str,
    tmux_pane: &str,
    ts: u64,
    message: Option<&str>,
    pid: Option<u32>,
) -> AttentionEntry {
    let project = Path::new(cwd)
        .file_name()
        .map_or_else(|| cwd.to_string(), |s| s.to_string_lossy().into_owned());
    AttentionEntry {
        agent: agent.to_string(),
        project,
        cwd: cwd.to_string(),
        event: event.to_string(),
        tmux_pane: tmux_pane.to_string(),
        ts,
        message: message.map(str::to_string),
        pid,
    }
}
```

- [ ] **Step 4: Update existing `build_entry` test call sites**

Search for `build_entry(` in `src/commands.rs` tests. Each call gets `, None` appended before the closing paren. Example:

```rust
let e = build_entry("claude-code", "notify", "/Users/me/work/claude-status", "%5", 42, None, None);
```

There are four such test call sites: `build_entry_uses_basename_of_cwd_as_project`, `build_entry_falls_back_to_cwd_when_no_basename`, `build_entry_stores_message_when_some`, `build_entry_leaves_message_none_when_none`. Each just needs `, None` appended.

- [ ] **Step 5: Update the one production call site in `run_set`**

In `src/main.rs`, find the line `let entry = build_entry(agent.name(), event, &cwd, &pane, ts, message.as_deref());` and replace with:

```rust
let pid = std::os::unix::process::parent_id();
let entry = build_entry(
    agent.name(),
    event,
    &cwd,
    &pane,
    ts,
    message.as_deref(),
    Some(pid),
);
```

`std::os::unix::process::parent_id()` returns `u32` (never panics, always available on Unix). This is the PID of the process that exec'd us — for Claude Code hooks, that's claude itself; for the pi/opencode TS extensions, that's the Node/Bun runtime.

- [ ] **Step 6: Run all tests**

Run: `cargo test`
Expected: all green.

- [ ] **Step 7: Run clippy**

Run: `cargo clippy --all-targets --all-features --locked -- -D warnings`
Expected: no warnings.

- [ ] **Step 8: Commit**

```bash
git add src/commands.rs src/main.rs
git commit -m "feat(commands): capture parent pid on agent-status set"
```

---

## Task 3: Add `is_pid_alive` helper

**Files:**
- Modify: `src/state.rs` (add helper near the bottom, before `validate_session_id`)

- [ ] **Step 1: Write the failing test**

Add to `src/state.rs`'s `tests` mod:

```rust
#[test]
fn is_pid_alive_returns_true_for_self() {
    let me = std::process::id();
    assert!(is_pid_alive(me), "kill -0 of own pid should succeed");
}

#[test]
fn is_pid_alive_returns_false_for_impossible_pid() {
    // pid_max on Linux is typically 4194304 (2^22); macOS 99998. Both well below 1_000_000_000.
    assert!(!is_pid_alive(1_000_000_000));
}

#[test]
fn is_pid_alive_returns_false_for_pid_zero() {
    // kill(0, 0) signals the whole process group — not what we want. The helper
    // must reject pid 0 explicitly so a corrupted state file with pid:0 doesn't
    // accidentally keep itself alive.
    assert!(!is_pid_alive(0));
}
```

- [ ] **Step 2: Run, confirm it fails**

Run: `cargo test --lib state::tests::is_pid_alive`
Expected: FAIL with `cannot find function 'is_pid_alive' in this scope`.

- [ ] **Step 3: Implement `is_pid_alive`**

Add to `src/state.rs`, just above `fn validate_session_id`:

```rust
/// Returns whether `pid` is a live process the current user can signal.
///
/// Uses `kill -0 <pid>` (POSIX). Returns `true` iff the command exits 0, which
/// means: the pid exists, and the caller has permission to send it a signal. A
/// dead pid, a pid in another user's namespace, or `pid == 0` (which `kill(2)`
/// treats as the whole process group — not what we want) all return `false`.
///
/// We deliberately do not use `libc::kill` directly so the crate keeps
/// `unsafe_code = "forbid"`. The cost is one fork+exec of `/bin/kill` per
/// entry checked; with the typical handful of waiting sessions this is well
/// under a millisecond and fires only on `agent-status status`/`list`/`preview`.
fn is_pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test --lib state::tests::is_pid_alive`
Expected: all three pass.

- [ ] **Step 5: Commit**

```bash
git add src/state.rs
git commit -m "feat(state): add is_pid_alive helper using POSIX kill -0"
```

---

## Task 4: Add `prune_dead` to `StateStore` and call it from `list`

**Files:**
- Modify: `src/state.rs` (add `prune_dead` method on `StateStore`, call from `list`)

- [ ] **Step 1: Write the failing test**

Add to `src/state.rs`'s `tests` mod:

```rust
#[test]
fn list_prunes_entries_with_dead_pid() {
    let dir = TempDir::new().unwrap();
    let store = StateStore::new(dir.path().into());

    // Entry with our own pid stays.
    let mut alive = sample_entry("alive");
    alive.pid = Some(std::process::id());
    store.write("session-alive", &alive).unwrap();

    // Entry with an impossible pid is pruned.
    let mut dead = sample_entry("dead");
    dead.pid = Some(1_000_000_000);
    store.write("session-dead", &dead).unwrap();

    let listed = store.list().unwrap();
    assert_eq!(listed.len(), 1, "should keep only the alive entry");
    assert_eq!(listed[0].0, "session-alive");

    // The dead entry's file should actually be gone from disk, not just filtered
    // from the returned Vec — otherwise it would resurface on the next call
    // until the user manually cleaned /tmp.
    assert!(!dir.path().join("session-dead").exists());
}

#[test]
fn list_keeps_entries_without_pid() {
    // Old-format entries (no pid field) must not be pruned — we have no way to
    // verify their liveness and pruning them blindly would lose state on every
    // upgrade.
    let dir = TempDir::new().unwrap();
    let store = StateStore::new(dir.path().into());
    let no_pid_entry = sample_entry("legacy"); // sample_entry sets pid: None
    store.write("session-legacy", &no_pid_entry).unwrap();

    let listed = store.list().unwrap();
    assert_eq!(listed.len(), 1);
}
```

- [ ] **Step 2: Run the tests, confirm they fail**

Run: `cargo test --lib state::tests::list_prunes`
Expected: FAIL — the dead entry is still in the list.

- [ ] **Step 3: Modify `list()` to prune dead entries inline**

In `src/state.rs`, replace the body of `list()` (the part that pushes into `out` and sorts) with a version that filters and removes dead-pid entries as it walks. The full new body:

```rust
pub fn list(&self) -> io::Result<Vec<(String, AttentionEntry)>> {
    let iter = match fs::read_dir(&self.dir) {
        Ok(it) => it,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut out = Vec::new();
    for entry in iter {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        let Ok(parsed) = serde_json::from_slice::<AttentionEntry>(&bytes) else {
            continue;
        };
        // Auto-prune entries whose owning process is dead. Entries with no
        // recorded pid (older binaries; bash precursor) are kept as-is — we
        // have no way to verify their liveness.
        if let Some(pid) = parsed.pid {
            if !is_pid_alive(pid) {
                let _ = fs::remove_file(&path);
                continue;
            }
        }
        out.push((name, parsed));
    }
    out.sort_by(|a, b| a.1.ts.cmp(&b.1.ts).then_with(|| a.0.cmp(&b.0)));
    Ok(out)
}
```

The change is a single new block between `let Ok(parsed) = ...` and `out.push(...)`. Everything else is unchanged.

- [ ] **Step 4: Run all tests**

Run: `cargo test`
Expected: all green, including the two new prune tests.

- [ ] **Step 5: Run clippy**

Run: `cargo clippy --all-targets --all-features --locked -- -D warnings`
Expected: no warnings.

- [ ] **Step 6: Commit**

```bash
git add src/state.rs
git commit -m "feat(state): prune state files with dead pid in StateStore::list"
```

---

## Task 5: End-to-end integration test

**Files:**
- Modify: `tests/cli.rs` (add new test)

- [ ] **Step 1: Add the integration test**

Append to `tests/cli.rs`:

```rust
#[test]
fn status_prunes_state_file_with_dead_pid() {
    let tmp = TempDir::new().unwrap();
    let state_dir = tmp.path().join("agent-status");
    std::fs::create_dir_all(&state_dir).unwrap();

    // Hand-write a state file with an impossible pid — kill -0 will never
    // succeed for it, so the next `status` invocation must remove it.
    let json = r#"{"agent":"claude-code","project":"ghost","cwd":"/x","event":"notify","tmux_pane":"","ts":1,"pid":1000000000}"#;
    std::fs::write(state_dir.join("ghost-session"), json).unwrap();
    assert!(state_dir.join("ghost-session").exists());

    let (stdout, _, code) = run(&state_dir, &["status"], None);
    assert_eq!(code, 0);
    assert_eq!(stdout, "", "status should report no waiting sessions");
    assert!(
        !state_dir.join("ghost-session").exists(),
        "stale state file should have been pruned by the status read",
    );
}
```

- [ ] **Step 2: Run the integration tests**

Run: `cargo test --test cli`
Expected: all green, including the new test.

- [ ] **Step 3: Commit**

```bash
git add tests/cli.rs
git commit -m "test(cli): status command prunes stale state files with dead pid"
```

---

## Task 6: Update README — remove the stale caveat, document the auto-prune

**Files:**
- Modify: `README.md` (Caveats section + State location section)

- [ ] **Step 1: Update the State location section to mention `pid`**

Find the JSON example under "State location":

```json
{"agent":"claude-code","project":"agent-status","cwd":"/path/to/project","event":"notify","tmux_pane":"%17","ts":1778163565,"message":"Permission required"}
```

Replace with:

```json
{"agent":"claude-code","project":"agent-status","cwd":"/path/to/project","event":"notify","tmux_pane":"%17","ts":1778163565,"message":"Permission required","pid":12345}
```

And append a new paragraph right after the existing "The `agent` field is..." sentence:

```markdown
The `pid` field records the agent process's PID (typically the claude / opencode / pi binary) so `agent-status status`, `list`, and `preview` can detect and remove entries whose owning process has died without firing its session-end hook. Files written by older binaries or the bash precursor — which lack `pid` — are never auto-pruned; they age out only on tmpfs cleanup. Such entries should disappear naturally after one `set`/`clear` cycle on the affected session.
```

- [ ] **Step 2: Delete the "Stale state on abnormal exit" bullet**

In the Caveats section, find:

```markdown
- **Stale state on abnormal exit.** If a Claude Code process dies without firing its session-end hook, its state file lingers. macOS's tmpwatch and reboots eventually clean `/tmp`; on Linux with `XDG_RUNTIME_DIR`, files vanish at logout.
```

Delete the entire bullet. The other three caveats stay.

- [ ] **Step 3: Update CLAUDE.md "Wire compatibility" section**

In `CLAUDE.md`, find the paragraph "The `agent` field was added in the v0.2.0 refactor..." and append a sentence to it:

```markdown
The `pid` field was added later still and is also optional in the schema for the same reason — entries written by older binaries simply skip the PID-based auto-prune (`is_pid_alive` is only consulted when `pid` is `Some`).
```

- [ ] **Step 4: Update the test-count line in CLAUDE.md and README**

Both files mention `# 72 tests (66 unit + 6 integration)`. This plan adds:
- `entry_pid_field_roundtrips_when_set`, `entry_pid_field_omitted_from_json_when_none`, `entry_deserializes_when_pid_field_absent` (3 unit)
- `is_pid_alive_returns_true_for_self`, `is_pid_alive_returns_false_for_impossible_pid`, `is_pid_alive_returns_false_for_pid_zero` (3 unit)
- `list_prunes_entries_with_dead_pid`, `list_keeps_entries_without_pid` (2 unit)
- `build_entry_stores_pid_when_some`, `build_entry_leaves_pid_none_when_none` (2 unit)
- `status_prunes_state_file_with_dead_pid` (1 integration)

Total: +10 unit, +1 integration → 76 unit + 7 integration = 83 tests.

Run `cargo test 2>&1 | tail -10` to confirm the exact count, then update both `CLAUDE.md`'s and `README.md`'s test-count line to match. The CLAUDE.md location:

```sh
cargo test                                                            # 83 tests (76 unit + 7 integration)
```

And README.md's Development section:

```sh
cargo test                                                   # 83 tests (76 unit + 7 integration)
```

- [ ] **Step 5: Commit**

```bash
git add README.md CLAUDE.md
git commit -m "docs: document pid-based auto-prune; drop stale-state caveat"
```

---

## Task 7: Smoke-test against a real Claude Code session

**Files:** none modified.

- [ ] **Step 1: Install the freshly-built binary**

```bash
cargo build --release
install -m 0755 target/release/agent-status ~/.claude/bin/agent-status
```

- [ ] **Step 2: Start a Claude Code session and let it `Stop`**

In a tmux pane, run `claude` and submit one short prompt. After the agent finishes the turn (firing `Stop`), there should be a state file:

```bash
ls "${XDG_RUNTIME_DIR:-/tmp}/agent-status/"
cat "${XDG_RUNTIME_DIR:-/tmp}/agent-status/"*
```

Confirm the JSON includes `"pid":<some-pid>` and `<some-pid>` matches the PID of the running `claude` process (verify with `pgrep -fl '^claude'`).

- [ ] **Step 3: SIGKILL claude and verify auto-prune**

```bash
pkill -KILL -f '^claude'
ls "${XDG_RUNTIME_DIR:-/tmp}/agent-status/"   # file still present — Stop didn't fire
~/.claude/bin/agent-status status              # should print nothing
ls "${XDG_RUNTIME_DIR:-/tmp}/agent-status/"   # file is now gone
```

If the second `ls` still shows the state file, the prune didn't fire — investigate. Most likely cause is the PID being from a sub-shell rather than the claude binary; check the JSON's `pid` field and `ps -p <pid>` to see who actually owned it.

- [ ] **Step 4: No commit needed for smoke test**

If everything passes, the plan is complete. The full work is captured in the seven commits above.

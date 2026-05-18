# `AgentName` Enum Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace `--agent <String>` in the CLI subcommands with a typed `AgentName` enum that derives `clap::ValueEnum`. Compile-time validation that the agent flag matches a registered agent, exhaustive match in `build_extension`, free shell-completion of `--agent <TAB>`, and the "unknown agent" runtime error becomes a clap parse error.

**Architecture:** Add a new `AgentName` enum next to the existing `Agent` trait in `src/agents/mod.rs`. The enum has one variant per registered agent and implements `clap::ValueEnum` (for CLI parsing) and a `name() -> &'static str` method that returns the wire string used in state files and on-disk filenames. The `agents::by_name(&str) -> Option<Box<dyn Agent>>` lookup stays — it's still needed for round-tripping state files that record the agent name as a string — but the CLI no longer goes through it; the CLI dispatches from `AgentName` directly via `AgentName::agent() -> Box<dyn Agent>`. `build_extension` takes `AgentName` instead of `&str`; its match becomes exhaustive (the compiler enforces a branch per variant).

**Tech Stack:** Rust (existing crate). No new dependencies — `clap` already in tree with the `derive` feature, which exports `ValueEnum`.

**Dependency:** This plan is best applied AFTER `2026-05-18-agent-extension-pi-opencode.md` lands, since that plan widens `build_extension`'s match from `claude-code` only to `claude-code`/`pi-coding-agent`/`opencode`. If applied first, drop the `PiCodingAgent` and `Opencode` variants from the enum and add them later.

---

## File Structure

- **Modify** `src/agents/mod.rs` — add `pub enum AgentName { ClaudeCode, PiCodingAgent, Opencode }` with `#[derive(Clone, Copy, Debug, Eq, PartialEq, clap::ValueEnum)]`. Implement `name() -> &'static str` (returns the wire string, kebab-case) and `agent() -> Box<dyn Agent>` (dispatches to the existing Agent impls). The `#[value(name = "kebab-case")]` clap attribute on each variant maps the enum to the wire string for `--agent` parsing. Add unit tests covering value_enum parsing, `name()`, and `agent()`.
- **Modify** `src/main.rs` — change all three `#[arg(long, default_value = "claude-code")] agent: String` to `#[arg(long, default_value_t = AgentName::ClaudeCode, value_enum)] agent: AgentName`. Update `run_set`, `run_clear`, `run_agent_extension` to accept `AgentName` and use `.agent()` instead of `agents::by_name(&str)`.
- **Modify** `src/commands.rs` — change `build_extension(bin_path: &str, agent_name: &str) -> Option<ExtensionFile>` to `build_extension(bin_path: &str, agent: AgentName) -> ExtensionFile` (note: drops `Option<>` because every variant has a branch — the compiler enforces it). Update each `build_*_extension` helper if needed.
- **Modify** `src/commands.rs` tests — call sites that pass `"claude-code"` etc. as strings now pass `AgentName::ClaudeCode` etc. The tests that asserted `None` for an unknown agent get deleted (the enum makes that case impossible by construction).
- **Modify** `tests/cli.rs` — CLI tests passing `"--agent", "frobnicator"` now expect a different error message (clap parse error: "invalid value 'frobnicator'"); update the assertion. The `agent_extension_unsupported_agent_exits_nonzero` test goes away (all enum variants are supported by construction).
- **Modify** `README.md` & `CLAUDE.md` — `--agent` flag's accepted values are now visible in `--help` output; reference that. No URL or hook content changes needed.

---

## Task 1: Define `AgentName` enum and its methods

**Files:**
- Modify: `src/agents/mod.rs`

- [ ] **Step 1: Write failing tests in `src/agents/mod.rs`'s `tests` mod**

Add to the existing `tests` mod, after `by_name_resolves_opencode`:

```rust
#[test]
fn agent_name_returns_wire_string_for_each_variant() {
    assert_eq!(AgentName::ClaudeCode.name(), "claude-code");
    assert_eq!(AgentName::PiCodingAgent.name(), "pi-coding-agent");
    assert_eq!(AgentName::Opencode.name(), "opencode");
}

#[test]
fn agent_name_dispatch_returns_matching_trait_object() {
    assert_eq!(AgentName::ClaudeCode.agent().name(), "claude-code");
    assert_eq!(AgentName::PiCodingAgent.agent().name(), "pi-coding-agent");
    assert_eq!(AgentName::Opencode.agent().name(), "opencode");
}

#[test]
fn agent_name_parses_kebab_case_via_value_enum() {
    use clap::ValueEnum;
    assert_eq!(
        AgentName::from_str("claude-code", true).unwrap(),
        AgentName::ClaudeCode,
    );
    assert_eq!(
        AgentName::from_str("pi-coding-agent", true).unwrap(),
        AgentName::PiCodingAgent,
    );
    assert_eq!(
        AgentName::from_str("opencode", true).unwrap(),
        AgentName::Opencode,
    );
    assert!(AgentName::from_str("frobnicator", true).is_err());
}
```

- [ ] **Step 2: Run, confirm fails**

```bash
cargo test --lib agents::tests::agent_name
```

Expected: FAIL — `AgentName` type doesn't exist.

- [ ] **Step 3: Add the enum and its methods**

In `src/agents/mod.rs`, just above the `Agent` trait definition, add:

```rust
/// CLI-facing enumeration of every registered agent.
///
/// Mirrors the [`Agent`] trait registry but lives in the type system so the
/// `--agent` flag, [`AgentName::agent`] dispatch, and the
/// [`crate::commands::build_extension`] match are all compile-time checked.
/// Wire strings (`claude-code`, etc.) match each variant's `Agent::name()`
/// return value so on-disk state files and CLI flags use the same identifiers.
#[derive(Clone, Copy, Debug, Eq, PartialEq, clap::ValueEnum)]
pub enum AgentName {
    #[value(name = "claude-code")]
    ClaudeCode,
    #[value(name = "pi-coding-agent")]
    PiCodingAgent,
    #[value(name = "opencode")]
    Opencode,
}

impl AgentName {
    /// Stable wire string for this agent (matches `Agent::name()`).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude-code",
            Self::PiCodingAgent => "pi-coding-agent",
            Self::Opencode => "opencode",
        }
    }

    /// Dispatch to the trait object for this agent. Equivalent to
    /// `by_name(self.name()).expect("AgentName must always resolve")` but
    /// the compiler knows it can't fail.
    #[must_use]
    pub fn agent(self) -> Box<dyn Agent> {
        match self {
            Self::ClaudeCode => Box::new(claude_code::ClaudeCodeAgent),
            Self::PiCodingAgent => Box::new(pi_coding_agent::PiCodingAgent),
            Self::Opencode => Box::new(opencode::OpencodeAgent),
        }
    }
}
```

- [ ] **Step 4: Run the tests**

```bash
cargo test --lib agents::tests::agent_name
```

Expected: all three new tests pass.

- [ ] **Step 5: Run clippy**

```bash
cargo clippy --all-targets --all-features --locked -- -D warnings
```

Expected: clean. If clippy complains about pedantic lints (e.g. `must_use_candidate` since I've already annotated, or missing docs), address inline.

- [ ] **Step 6: Commit**

```bash
git add src/agents/mod.rs
git commit -m "feat(agents): add AgentName enum with clap::ValueEnum derive"
```

---

## Task 2: Switch CLI subcommands to use `AgentName`

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Update imports**

In `src/main.rs`, change:

```rust
use commands::{build_entry, build_extension, format_list, format_preview, format_status};
```

to add the agent enum:

```rust
use agents::AgentName;
use commands::{build_entry, build_extension, format_list, format_preview, format_status};
```

- [ ] **Step 2: Update the three `Cmd` variants**

Find `Cmd::Set`:

```rust
    Set {
        /// Event label stored with the entry (e.g. `notify`, `done`).
        #[arg(default_value = "attention")]
        event: String,
        /// Identifier of the agent invoking the hook (e.g. `claude-code`).
        #[arg(long, default_value = "claude-code")]
        agent: String,
    },
```

Replace the `agent` field:

```rust
    Set {
        /// Event label stored with the entry (e.g. `notify`, `done`).
        #[arg(default_value = "attention")]
        event: String,
        /// Identifier of the agent invoking the hook.
        #[arg(long, default_value_t = AgentName::ClaudeCode, value_enum)]
        agent: AgentName,
    },
```

Do the same for `Cmd::Clear`:

```rust
    Clear {
        /// Identifier of the agent invoking the hook.
        #[arg(long, default_value_t = AgentName::ClaudeCode, value_enum)]
        agent: AgentName,
    },
```

And `Cmd::AgentExtension`:

```rust
    AgentExtension {
        /// Identifier of the agent the extension file should target.
        #[arg(long, default_value_t = AgentName::ClaudeCode, value_enum)]
        agent: AgentName,
    },
```

- [ ] **Step 3: Update the `match` in `main`**

The dispatch arms previously took `agent: String`. They now take `agent: AgentName`. Update each:

```rust
    let result = match cli.command {
        Cmd::Set { event, agent } => run_set(&store, agent, &event),
        Cmd::Clear { agent } => run_clear(&store, agent),
        Cmd::Status => run_status(&store, &mut io::stdout().lock()),
        Cmd::List => run_list(&store, &mut io::stdout().lock()),
        Cmd::Preview { session_id } => {
            run_preview(&store, &session_id, &mut io::stdout().lock())
        }
        Cmd::AgentExtension { agent } => run_agent_extension(agent, &mut io::stdout().lock()),
    };
```

Note: `AgentName` is `Copy`, so passing by value is cheap and ergonomic.

- [ ] **Step 4: Update the three `run_*` function signatures and bodies**

Replace `run_set`:

```rust
fn run_set(store: &StateStore, agent_name: AgentName, event: &str) -> io::Result<()> {
    let agent = agent_name.agent();

    let mut buf = String::new();
    io::stdin().read_to_string(&mut buf)?;

    let Some(session_id) = agent.extract_session_id(&buf) else {
        return Ok(());
    };

    let cwd = std::env::var("CLAUDE_PROJECT_DIR")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            std::env::current_dir()
                .ok()
                .map(|p| p.to_string_lossy().into_owned())
        })
        .unwrap_or_default();
    let pane = std::env::var("TMUX_PANE").unwrap_or_default();
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());

    let message = agent.extract_message(&buf);
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
    store.write(&session_id, &entry)?;
    refresh_tmux();
    Ok(())
}
```

The `agents::by_name(&agent_name)` lookup and its associated `unknown agent` error path are gone — clap rejects bad values at parse time.

Replace `run_clear`:

```rust
fn run_clear(store: &StateStore, agent_name: AgentName) -> io::Result<()> {
    let agent = agent_name.agent();

    let mut buf = String::new();
    io::stdin().read_to_string(&mut buf)?;
    let Some(session_id) = agent.extract_session_id(&buf) else {
        return Ok(());
    };
    if store.remove(&session_id)? {
        refresh_tmux();
    }
    Ok(())
}
```

Replace `run_agent_extension`:

```rust
fn run_agent_extension(agent_name: AgentName, out: &mut impl Write) -> io::Result<()> {
    let bin_path = std::env::current_exe()?;
    let bin_str = bin_path.to_string_lossy();
    let extension = build_extension(&bin_str, agent_name);
    let extension_path = extension_path_for(&extension.filename);
    if let Some(parent) = extension_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&extension_path, extension.content)?;
    writeln!(out, "{}", extension_path.display())?;
    Ok(())
}
```

Note `build_extension` now returns `ExtensionFile` directly (not `Option<ExtensionFile>`) — Task 3 changes the signature.

- [ ] **Step 5: Build, expect failure that points at `build_extension`**

```bash
cargo build
```

Expected: compile error on `run_agent_extension`'s `let extension = build_extension(&bin_str, agent_name);` line because `build_extension` still takes `&str`. Task 3 fixes this.

- [ ] **Step 6: No commit yet — fail-state from Step 5 is intentional. Move to Task 3.**

---

## Task 3: Make `build_extension` enum-typed and exhaustive

**Files:**
- Modify: `src/commands.rs`
- Modify: `src/commands.rs` tests

- [ ] **Step 1: Update `build_extension`'s signature and body**

In `src/commands.rs`, change:

```rust
pub fn build_extension(bin_path: &str, agent_name: &str) -> Option<ExtensionFile> {
    match agent_name {
        "claude-code" => Some(ExtensionFile {
            filename: "claude-code.json".to_string(),
            content: build_claude_code_settings(bin_path),
        }),
        "pi-coding-agent" => Some(ExtensionFile {
            filename: "pi-coding-agent.ts".to_string(),
            content: build_pi_extension(bin_path),
        }),
        "opencode" => Some(ExtensionFile {
            filename: "opencode.ts".to_string(),
            content: build_opencode_extension(bin_path),
        }),
        _ => None,
    }
}
```

to:

```rust
use crate::agents::AgentName;

pub fn build_extension(bin_path: &str, agent: AgentName) -> ExtensionFile {
    match agent {
        AgentName::ClaudeCode => ExtensionFile {
            filename: "claude-code.json".to_string(),
            content: build_claude_code_settings(bin_path),
        },
        AgentName::PiCodingAgent => ExtensionFile {
            filename: "pi-coding-agent.ts".to_string(),
            content: build_pi_extension(bin_path),
        },
        AgentName::Opencode => ExtensionFile {
            filename: "opencode.ts".to_string(),
            content: build_opencode_extension(bin_path),
        },
    }
}
```

The match is now exhaustive — adding a new `AgentName` variant in the future will produce a compile error here, exactly the win the refactor is meant to provide. The `Option<>` wrapper is gone for the same reason.

- [ ] **Step 2: Update the existing `build_extension_*` tests in `src/commands.rs`**

The `build_extension_returns_none_for_unsupported_agent` test becomes impossible (no unsupported variant exists) — DELETE it.

All the other `build_extension_*` tests in commands.rs:
- `build_extension_returns_some_for_claude_code`
- `build_extension_claude_code_wires_all_six_hook_events`
- `build_extension_claude_code_uses_set_and_clear_correctly`
- `build_extension_escapes_unsafe_chars_in_bin_path`
- `build_extension_returns_pi_coding_agent_extension`
- `build_extension_pi_extension_json_escapes_bin_path`
- `build_extension_returns_opencode_extension`
- `build_extension_opencode_extension_json_escapes_bin_path`

…each calls `build_extension(path, "claude-code").unwrap()` or similar. Update each call site:
- `"claude-code"` → `AgentName::ClaudeCode`
- `"pi-coding-agent"` → `AgentName::PiCodingAgent`
- `"opencode"` → `AgentName::Opencode`

Drop the `.unwrap()` / `.expect()` since `build_extension` no longer returns `Option<>`.

Example diff for `build_extension_returns_some_for_claude_code`:

```rust
    #[test]
    fn build_extension_returns_extension_for_claude_code() {
        let ext = build_extension("/x/agent-status", AgentName::ClaudeCode);
        assert_eq!(ext.filename, "claude-code.json");
        let parsed: serde_json::Value = serde_json::from_str(&ext.content).unwrap();
        assert!(parsed.get("hooks").is_some(), "missing top-level hooks key");
    }
```

(Optionally rename `_returns_some_` to `_returns_extension_` since it doesn't return `Some` anymore. Apply the same rename to the pi and opencode equivalents.)

Add `use crate::agents::AgentName;` to the `tests` mod's imports (right after the existing `use super::*;`).

- [ ] **Step 3: Run tests**

```bash
cargo test
```

Expected: all green. The `run_agent_extension` glue in `main.rs` should now compile (Task 2's Step 5 fail-state is resolved).

- [ ] **Step 4: Run clippy**

```bash
cargo clippy --all-targets --all-features --locked -- -D warnings
```

Expected: clean.

- [ ] **Step 5: Commit (the Task 2 + Task 3 changes together)**

```bash
git add src/main.rs src/commands.rs
git commit -m "refactor: switch --agent flag to AgentName enum; build_extension match is now exhaustive"
```

---

## Task 4: Update integration tests for the new clap error message

**Files:**
- Modify: `tests/cli.rs`

- [ ] **Step 1: Update the unknown-agent error assertion**

The existing test `agent_extension_unknown_agent_exits_nonzero` asserts:

```rust
    let (_, stderr, code) = run(&state_dir, &["agent-extension", "--agent", "frobnicator"], None);
    assert_ne!(code, 0, "should exit non-zero for unknown agent");
    assert!(stderr.contains("unknown agent"), "stderr: {stderr:?}");
```

With clap's `ValueEnum`, the bad value now fails at parse time with a different message and exit code 2 (clap's standard parse-error code). Update:

```rust
    let (_, stderr, code) = run(&state_dir, &["agent-extension", "--agent", "frobnicator"], None);
    assert_eq!(code, 2, "clap parse error should exit 2");
    assert!(
        stderr.contains("invalid value 'frobnicator'") || stderr.contains("possible values"),
        "stderr: {stderr:?}",
    );
```

- [ ] **Step 2: Delete the unsupported-agent test**

The test `agent_extension_unsupported_agent_exits_nonzero` exercised the case where the agent was known to `by_name` but had no `build_extension` branch. After this refactor, every `AgentName` variant has a branch (compiler enforced), so this case is impossible. Delete the test entirely.

- [ ] **Step 3: Run tests**

```bash
cargo test --test cli
```

Expected: all green; one fewer integration test than before.

- [ ] **Step 4: Run clippy and the full test suite**

```bash
cargo test
cargo clippy --all-targets --all-features --locked -- -D warnings
```

Expected: all green, clippy clean.

- [ ] **Step 5: Commit**

```bash
git add tests/cli.rs
git commit -m "test(cli): update --agent error expectations after enum refactor"
```

---

## Task 5: Test-count update and documentation

**Files:**
- Modify: `CLAUDE.md`
- Modify: `README.md`

- [ ] **Step 1: Confirm the new test count**

```bash
cargo test 2>&1 | grep "test result"
```

Expected change from baseline (101 = 87 unit + 14 integration after the agent-extension plan):
- Task 1 adds 3 new unit tests (`agent_name_returns_wire_string_for_each_variant`, `agent_name_dispatch_returns_matching_trait_object`, `agent_name_parses_kebab_case_via_value_enum`). Net +3.
- Task 3 deletes 1 unit test (`build_extension_returns_none_for_unsupported_agent`). Net -1.
- Task 4 deletes 1 integration test (`agent_extension_unsupported_agent_exits_nonzero`). Net -1.

Predicted final: 102 tests (89 unit + 13 integration). Use the actual count from the command above.

- [ ] **Step 2: Update the test-count lines**

In `CLAUDE.md` (around line 12) and `README.md`'s `## Development` section, update the `cargo test` comment to match the actual count. For example: `# 102 tests (89 unit + 13 integration)`.

- [ ] **Step 3: Mention the typed `--agent` flag in CLAUDE.md's "Adding a new agent" section**

Find the existing list of steps for adding a new agent (the four-step numbered list under "Adding a new agent"). After step 2 ("Register the new agent in `agents::by_name` so the CLI's `--agent` flag can resolve it."), add a new step:

```markdown
3. Add a corresponding variant to the `AgentName` enum in `src/agents/mod.rs` (with the `#[value(name = "<wire-name>")]` clap attribute), and add branches in `AgentName::name()`, `AgentName::agent()`, and `commands::build_extension`. The exhaustive match in `build_extension` will produce a compile error until you do.
```

Renumber the subsequent items (the previous step 3 becomes 4, and so on).

- [ ] **Step 4: Run final verification**

```bash
cargo test
cargo clippy --all-targets --all-features --locked -- -D warnings
```

Expected: tests green, clippy clean.

- [ ] **Step 5: Commit**

```bash
git add CLAUDE.md README.md
git commit -m "docs: document the AgentName enum and refresh test counts"
```

---

## Task 6: Smoke test the CLI

**Files:** none modified.

- [ ] **Step 1: Install the release binary**

```bash
cargo build --release
install -m 0755 target/release/agent-status ~/.claude/bin/agent-status
```

- [ ] **Step 2: Verify `--agent <TAB>` shell-completes via `--help`**

```bash
~/.claude/bin/agent-status agent-extension --help
```

Expected: the `--agent` line shows `[possible values: claude-code, pi-coding-agent, opencode]` (clap derives this from the `ValueEnum` attributes).

- [ ] **Step 3: Verify good values still work**

```bash
~/.claude/bin/agent-status agent-extension --agent claude-code
~/.claude/bin/agent-status agent-extension --agent pi-coding-agent
~/.claude/bin/agent-status agent-extension --agent opencode
```

Expected: each prints the corresponding path under `${XDG_RUNTIME_DIR:-/tmp}/agent-status/extensions/`.

- [ ] **Step 4: Verify a bad value rejects at parse time**

```bash
~/.claude/bin/agent-status agent-extension --agent frobnicator; echo "exit=$?"
```

Expected: clap parse error mentioning `invalid value 'frobnicator'` (or `possible values`) and `exit=2`.

- [ ] **Step 5: No commit — sanity check only.**

If all three commands behave as expected, the plan is complete.

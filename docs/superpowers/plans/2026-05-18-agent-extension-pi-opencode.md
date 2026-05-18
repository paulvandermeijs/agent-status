# `agent-extension` Rename + pi/opencode Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rename the `agent-status agent-settings` subcommand to `agent-extension` and add `pi-coding-agent` and `opencode` branches that emit ready-to-load TypeScript extension files. After this lands, the alias-driven install also works for pi (`pi -e "$(agent-status agent-extension --agent pi-coding-agent)"`); opencode users do a one-time `cp` since opencode has no per-launch extension flag.

**Architecture:** Generalize `commands::build_settings_json` into `commands::build_extension`, returning a typed `ExtensionFile { filename, content }` so different agents can emit different file kinds (`.json` for Claude Code, `.ts` for pi/opencode). The `.ts` files are produced by `include_str!`-embedding the existing `extensions/pi-coding-agent.ts` and `extensions/opencode.ts` source files and string-replacing the BIN-resolution line with a literal-path assignment (`current_exe()` resolved at command time). If the template line shape ever changes and the substitution silently no-ops, the embedded TS still works at runtime because the original line falls back to `process.env.AGENT_STATUS_BIN` / `$HOME/.claude/bin/agent-status` — degraded but functional.

**Tech Stack:** Rust (existing crate). No new dependencies. `include_str!` for embedding the TypeScript templates. `serde_json::to_string` to JSON-quote the binary path safely when inlining it into the generated TypeScript.

---

## File Structure

- **Modify** `src/commands.rs` — introduce `pub struct ExtensionFile { pub filename: String, pub content: String }`. Rename `build_settings_json` → `build_extension` returning `Option<ExtensionFile>`. Keep the Claude Code branch's JSON construction; add `pi-coding-agent` and `opencode` branches that call new `build_pi_extension` / `build_opencode_extension` private helpers. Each helper uses `include_str!` + `str::replacen` to substitute the BIN line.
- **Modify** `extensions/pi-coding-agent.ts` — flatten the two-line `const BIN = …` block to a single line so the Rust substitution pattern is a single exact string match. No behavior change.
- **Modify** `extensions/opencode.ts` — same flatten.
- **Modify** `src/main.rs` — rename `Cmd::AgentSettings` → `Cmd::AgentExtension`, `run_agent_settings` → `run_agent_extension`. The function uses `ExtensionFile.filename` rather than the hardcoded `<agent>.json`. Settings/extensions are now written under `${XDG_RUNTIME_DIR:-/tmp}/agent-status/extensions/` (renamed from `settings/`).
- **Modify** `tests/cli.rs` — update existing tests that reference `agent-settings` to `agent-extension` (subcommand name) and the renamed `settings/` directory to `extensions/`. Add two new integration tests covering pi-coding-agent and opencode.
- **Modify** `README.md` — primary Claude Code section: rename subcommand. pi section: restructure to lead with the alias (`pi -e "$(agent-status agent-extension --agent pi-coding-agent)"`) and demote the manual extension-file copy to a `#### Wiring the extension manually (fallback)` subsection. opencode section: restructure to lead with `cp "$(agent-status agent-extension --agent opencode)" ~/.config/opencode/plugins/agent-status.ts` and demote the source-file copy to a `#### Wiring the plugin manually (fallback)` subsection.
- **Modify** `CLAUDE.md` — update references to the renamed subcommand. Document the `ExtensionFile` dispatch pattern in `build_extension` as the extension point for new alias-friendly agents.

---

## Task 1: Rename `Cmd::AgentSettings` → `Cmd::AgentExtension`

**Files:**
- Modify: `src/main.rs:63-73` (`Cmd::AgentSettings` variant + dispatch arm + `run_agent_settings` function)
- Modify: `tests/cli.rs` (three tests that pass `"agent-settings"` as a subcommand arg + the expected output paths)

- [ ] **Step 1: Rename in `src/main.rs`**

In `src/main.rs`, find this block:

```rust
    /// Generate the agent's hook-settings JSON and print its path.
    ///
    /// Intended for use as a shell alias: `alias claude='claude --settings
    /// "$(agent-status agent-settings)"'`. Writes a fresh JSON file (using
    /// the current `agent-status` binary's absolute path) to
    /// `${XDG_RUNTIME_DIR:-/tmp}/agent-status/settings/<agent>.json` and
    /// prints that path on stdout. Only `claude-code` supports `--settings`-
    /// style injection today; other agents return an error.
    AgentSettings {
        /// Identifier of the agent the settings file should target.
        #[arg(long, default_value = "claude-code")]
        agent: String,
    },
```

Replace with:

```rust
    /// Generate the agent's extension/settings file and print its path.
    ///
    /// Intended for use as a shell alias (Claude Code: `alias claude='claude
    /// --settings "$(agent-status agent-extension)"'`; pi: `alias pi='pi -e
    /// "$(agent-status agent-extension --agent pi-coding-agent)"'`). Writes
    /// a fresh file (using the current `agent-status` binary's absolute path)
    /// to `${XDG_RUNTIME_DIR:-/tmp}/agent-status/extensions/<agent>.<ext>`
    /// and prints that path on stdout. Each agent emits the file type its
    /// loader expects (`.json` for Claude Code, `.ts` for pi/opencode).
    AgentExtension {
        /// Identifier of the agent the extension file should target.
        #[arg(long, default_value = "claude-code")]
        agent: String,
    },
```

Then find the dispatch arm:

```rust
        Cmd::AgentSettings { agent } => run_agent_settings(&agent, &mut io::stdout().lock()),
```

Replace with:

```rust
        Cmd::AgentExtension { agent } => run_agent_extension(&agent, &mut io::stdout().lock()),
```

Then find the function definition:

```rust
fn run_agent_settings(agent_name: &str, out: &mut impl Write) -> io::Result<()> {
```

Replace with:

```rust
fn run_agent_extension(agent_name: &str, out: &mut impl Write) -> io::Result<()> {
```

Also change the `settings_path_for` helper's directory segment from `"settings"` to `"extensions"`:

```rust
fn settings_path_for(agent_name: &str) -> std::path::PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map_or_else(|| std::path::PathBuf::from("/tmp"), std::path::PathBuf::from);
    base.join("agent-status")
        .join("extensions")
        .join(format!("{agent_name}.json"))
}
```

(The hardcoded `.json` here will be replaced in Task 2 once `build_extension` returns the filename.)

- [ ] **Step 2: Rename in `tests/cli.rs`**

Find each occurrence of `"agent-settings"` (subcommand arg) and `"settings"` (directory segment) in test bodies and rename:

```rust
let (stdout, stderr, code) = run(&state_dir, &["agent-settings"], None);
// ...
let expected = state_dir.join("settings").join("claude-code.json");
```

becomes:

```rust
let (stdout, stderr, code) = run(&state_dir, &["agent-extension"], None);
// ...
let expected = state_dir.join("extensions").join("claude-code.json");
```

The three tests that need updating: `agent_settings_writes_file_and_prints_path`, `agent_settings_unknown_agent_exits_nonzero`, `agent_settings_unsupported_agent_exits_nonzero`. Also rename the test function names themselves: `agent_settings_*` → `agent_extension_*`.

- [ ] **Step 3: Run tests, confirm they pass**

```bash
cargo test
```

Expected: all 95 tests pass. (The rename is purely textual; no behavior change.)

- [ ] **Step 4: Run clippy**

```bash
cargo clippy --all-targets --all-features --locked -- -D warnings
```

Expected: no warnings.

- [ ] **Step 5: Commit**

```bash
git add src/main.rs tests/cli.rs
git commit -m "refactor: rename agent-settings subcommand to agent-extension"
```

---

## Task 2: Introduce `ExtensionFile` struct and refactor `build_settings_json` → `build_extension`

**Files:**
- Modify: `src/commands.rs` (the `build_settings_json` function + its tests)
- Modify: `src/main.rs` (the `run_agent_extension` function uses the new return type)

- [ ] **Step 1: Write the failing TDD-driver test**

Add a single new test to the `tests` mod in `src/commands.rs`, right after `build_settings_json_escapes_unsafe_chars_in_bin_path`. This one drives the TDD cycle for the new function; Step 4 will do the bulk rename of the existing `build_settings_json_*` tests into their `build_extension_*` shape.

```rust
#[test]
fn build_extension_returns_filename_and_content_for_claude_code() {
    let ext = build_extension("/x/agent-status", "claude-code").expect("supported");
    assert_eq!(ext.filename, "claude-code.json");
    let parsed: serde_json::Value = serde_json::from_str(&ext.content).unwrap();
    assert!(parsed.get("hooks").is_some());
}
```

- [ ] **Step 2: Run, confirm fails**

```bash
cargo test --lib commands::tests::build_extension_returns_filename_and_content_for_claude_code
```

Expected: FAIL — `build_extension` does not exist yet.

- [ ] **Step 3: Introduce `ExtensionFile` and rename `build_settings_json`**

In `src/commands.rs`, just below `use std::path::Path;`, add:

```rust
/// One generated extension/settings file: the filename to write it as and the
/// content to fill it with. Returned by [`build_extension`] for agents that
/// support a per-launch file-loaded integration (Claude Code's `--settings`,
/// pi's `-e <path>`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionFile {
    pub filename: String,
    pub content: String,
}
```

Then rename `build_settings_json` to `build_extension` and change its return type. Replace the existing function with:

```rust
/// Build the extension/settings file an alias-installed agent loads at launch.
///
/// Returns `Some(ExtensionFile)` for agents that support a file-argument
/// integration (`claude-code` uses `--settings <file>`, `pi-coding-agent`
/// uses `-e <file>`, and `opencode`'s in-process plugin file can be copied
/// once); returns `None` for any other agent name. The `filename` member is
/// the basename to write as (`claude-code.json`, `pi-coding-agent.ts`,
/// `opencode.ts`); the `content` member is the file body.
///
/// For Claude Code the body is JSON wiring the six hooks (see existing
/// behaviour). The `bin_path` is embedded into hook commands via
/// `serde_json::json!` so quotes/backslashes are escaped safely.
pub fn build_extension(bin_path: &str, agent_name: &str) -> Option<ExtensionFile> {
    match agent_name {
        "claude-code" => Some(ExtensionFile {
            filename: "claude-code.json".to_string(),
            content: build_claude_code_settings(bin_path),
        }),
        _ => None,
    }
}

fn build_claude_code_settings(bin_path: &str) -> String {
    let set_notify = format!("{bin_path} set --agent claude-code notify");
    let set_done = format!("{bin_path} set --agent claude-code done");
    let clear = format!("{bin_path} clear --agent claude-code");

    let value = serde_json::json!({
        "hooks": {
            "Notification":     [{"hooks": [{"type": "command", "command": set_notify}]}],
            "Stop":             [{"hooks": [{"type": "command", "command": set_done}]}],
            "UserPromptSubmit": [{"hooks": [{"type": "command", "command": clear}]}],
            "PreToolUse":       [{"hooks": [{"type": "command", "command": clear}]}],
            "SessionStart":     [{"hooks": [{"type": "command", "command": clear}]}],
            "SessionEnd":       [{"hooks": [{"type": "command", "command": clear}]}],
        }
    });
    serde_json::to_string_pretty(&value).expect("serde_json::Value always serializes")
}
```

The existing `build_settings_json` is replaced by this `build_extension` + `build_claude_code_settings` pair.

- [ ] **Step 4: Update the existing `build_settings_json` tests to use `build_extension`**

In `src/commands.rs`'s `tests` mod, the five tests that reference `build_settings_json` need their assertions updated. The simplest path: rename them to `build_extension_*` and assert on the returned `ExtensionFile`'s `content` field instead of the bare string. Replace:

```rust
    #[test]
    fn build_settings_json_returns_none_for_unknown_agent() {
        assert!(build_settings_json("/x/agent-status", "pi-coding-agent").is_none());
        assert!(build_settings_json("/x/agent-status", "opencode").is_none());
        assert!(build_settings_json("/x/agent-status", "frobnicator").is_none());
    }

    #[test]
    fn build_settings_json_returns_some_for_claude_code() {
        let json = build_settings_json("/x/agent-status", "claude-code")
            .expect("claude-code is supported");
        // Parse-back roundtrip — output must be valid JSON.
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.get("hooks").is_some(), "missing top-level hooks key");
    }

    #[test]
    fn build_settings_json_wires_all_six_hook_events() {
        let json = build_settings_json("/x/agent-status", "claude-code").unwrap();
        for event in [
            "Notification",
            "Stop",
            "UserPromptSubmit",
            "PreToolUse",
            "SessionStart",
            "SessionEnd",
        ] {
            assert!(json.contains(event), "missing hook event {event} in: {json}");
        }
    }

    #[test]
    fn build_settings_json_uses_set_and_clear_correctly() {
        let json = build_settings_json("/path/to/agent-status", "claude-code").unwrap();
        // Notification → notify, Stop → done.
        assert!(json.contains("set --agent claude-code notify"));
        assert!(json.contains("set --agent claude-code done"));
        // The four clear events all share one command string.
        assert!(json.contains("clear --agent claude-code"));
        // Sanity: the binary path is embedded verbatim.
        assert!(json.contains("/path/to/agent-status"));
    }

    #[test]
    fn build_settings_json_escapes_unsafe_chars_in_bin_path() {
        // A path with a quote and a backslash would corrupt JSON if interpolated raw.
        // serde_json::json! handles the escaping for us; verify the output round-trips.
        let json = build_settings_json(r#"/x/has"quote\and-backslash/agent-status"#, "claude-code")
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let command = parsed
            .pointer("/hooks/Notification/0/hooks/0/command")
            .and_then(serde_json::Value::as_str)
            .expect("notification command string");
        assert!(command.contains(r#"has"quote\and-backslash"#), "got: {command}");
    }
```

with the `build_extension`-based versions:

```rust
    #[test]
    fn build_extension_returns_none_for_unsupported_agent() {
        assert!(build_extension("/x/agent-status", "frobnicator").is_none());
    }

    #[test]
    fn build_extension_returns_some_for_claude_code() {
        let ext = build_extension("/x/agent-status", "claude-code")
            .expect("claude-code is supported");
        assert_eq!(ext.filename, "claude-code.json");
        let parsed: serde_json::Value = serde_json::from_str(&ext.content).unwrap();
        assert!(parsed.get("hooks").is_some(), "missing top-level hooks key");
    }

    #[test]
    fn build_extension_claude_code_wires_all_six_hook_events() {
        let ext = build_extension("/x/agent-status", "claude-code").unwrap();
        for event in [
            "Notification",
            "Stop",
            "UserPromptSubmit",
            "PreToolUse",
            "SessionStart",
            "SessionEnd",
        ] {
            assert!(ext.content.contains(event), "missing hook event {event}");
        }
    }

    #[test]
    fn build_extension_claude_code_uses_set_and_clear_correctly() {
        let ext = build_extension("/path/to/agent-status", "claude-code").unwrap();
        assert!(ext.content.contains("set --agent claude-code notify"));
        assert!(ext.content.contains("set --agent claude-code done"));
        assert!(ext.content.contains("clear --agent claude-code"));
        assert!(ext.content.contains("/path/to/agent-status"));
    }

    #[test]
    fn build_extension_escapes_unsafe_chars_in_bin_path() {
        let ext = build_extension(r#"/x/has"quote\and-backslash/agent-status"#, "claude-code")
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&ext.content).unwrap();
        let command = parsed
            .pointer("/hooks/Notification/0/hooks/0/command")
            .and_then(serde_json::Value::as_str)
            .expect("notification command string");
        assert!(command.contains(r#"has"quote\and-backslash"#), "got: {command}");
    }
```

(Drop the `build_extension_returns_filename_and_content_for_claude_code` test added in Step 1 — `build_extension_returns_some_for_claude_code` now covers it.)

- [ ] **Step 5: Update `run_agent_extension` in `src/main.rs` to use `ExtensionFile.filename`**

Change the import line:

```rust
use commands::{build_entry, build_settings_json, format_list, format_preview, format_status};
```

to:

```rust
use commands::{build_entry, build_extension, format_list, format_preview, format_status};
```

Then replace the body of `run_agent_extension`:

```rust
fn run_agent_extension(agent_name: &str, out: &mut impl Write) -> io::Result<()> {
    let Some(agent) = agents::by_name(agent_name) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unknown agent: {agent_name}"),
        ));
    };
    let bin_path = std::env::current_exe()?;
    let bin_str = bin_path.to_string_lossy();
    let Some(extension) = build_extension(&bin_str, agent.name()) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("agent has no alias-style integration: {agent_name}"),
        ));
    };
    let extension_path = extension_path_for(&extension.filename);
    if let Some(parent) = extension_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&extension_path, extension.content)?;
    writeln!(out, "{}", extension_path.display())?;
    Ok(())
}

fn extension_path_for(filename: &str) -> std::path::PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map_or_else(|| std::path::PathBuf::from("/tmp"), std::path::PathBuf::from);
    base.join("agent-status").join("extensions").join(filename)
}
```

Delete the old `settings_path_for` function — it's replaced by `extension_path_for`.

- [ ] **Step 6: Update the unsupported-agent error message expectation in tests**

In `tests/cli.rs`, the test `agent_extension_unsupported_agent_exits_nonzero` (renamed in Task 1) asserts `stderr.contains("--settings")`. The new error message is `"agent has no alias-style integration: …"`. Update the assertion:

```rust
    assert!(stderr.contains("alias-style integration"), "stderr: {stderr:?}");
```

- [ ] **Step 7: Run tests**

```bash
cargo test
```

Expected: all green.

- [ ] **Step 8: Run clippy**

```bash
cargo clippy --all-targets --all-features --locked -- -D warnings
```

Expected: no warnings.

- [ ] **Step 9: Commit**

```bash
git add src/commands.rs src/main.rs tests/cli.rs
git commit -m "refactor(commands): generalize build_settings_json into build_extension returning typed ExtensionFile"
```

---

## Task 3: Flatten the BIN-resolution line in the two TypeScript files

**Files:**
- Modify: `extensions/pi-coding-agent.ts:27-28`
- Modify: `extensions/opencode.ts:42-43`

This is a pure formatting change so the Rust substitution can use a single-line exact-match pattern.

- [ ] **Step 1: Flatten `extensions/pi-coding-agent.ts`**

Find these two lines:

```ts
const BIN =
  process.env.AGENT_STATUS_BIN ?? `${process.env.HOME}/.claude/bin/agent-status`;
```

Replace with one line:

```ts
const BIN = process.env.AGENT_STATUS_BIN ?? `${process.env.HOME}/.claude/bin/agent-status`;
```

- [ ] **Step 2: Flatten `extensions/opencode.ts`**

Same change. Find:

```ts
const BIN =
  process.env.AGENT_STATUS_BIN ?? `${process.env.HOME}/.claude/bin/agent-status`;
```

Replace with:

```ts
const BIN = process.env.AGENT_STATUS_BIN ?? `${process.env.HOME}/.claude/bin/agent-status`;
```

- [ ] **Step 3: Verify both files still parse (visual + cargo build)**

```bash
cargo build
```

The `.ts` files aren't compiled by cargo, but the files will be embedded via `include_str!` in the next task. Visual sanity check: open each file and confirm the line replaced is a complete TypeScript const declaration.

- [ ] **Step 4: Commit**

```bash
git add extensions/pi-coding-agent.ts extensions/opencode.ts
git commit -m "refactor(extensions): flatten two-line BIN constant declarations to one line

Prep for build_pi_extension / build_opencode_extension in Rust, which
substitute the BIN line via str::replacen using a single-line pattern."
```

---

## Task 4: Add `build_pi_extension` and the `pi-coding-agent` branch

**Files:**
- Modify: `src/commands.rs` (add helper + extend `build_extension`'s match)
- Modify: `src/commands.rs` tests

- [ ] **Step 1: Write the failing tests**

Add to the `tests` mod in `src/commands.rs`:

```rust
    #[test]
    fn build_extension_returns_pi_coding_agent_extension() {
        let ext = build_extension("/abs/path/agent-status", "pi-coding-agent")
            .expect("pi-coding-agent is supported");
        assert_eq!(ext.filename, "pi-coding-agent.ts");
        // Substituted line must be present.
        assert!(
            ext.content.contains(r#"const BIN = "/abs/path/agent-status";"#),
            "missing substituted BIN; got:\n{}",
            ext.content,
        );
        // Original env-fallback line must be gone.
        assert!(
            !ext.content.contains("process.env.AGENT_STATUS_BIN ??"),
            "env-fallback line should have been replaced",
        );
        // The rest of the .ts source should still be present (sanity).
        assert!(ext.content.contains("export default function"));
    }

    #[test]
    fn build_extension_pi_extension_json_escapes_bin_path() {
        // Backslash and quote must round-trip through serde_json::to_string into
        // a valid TypeScript string literal.
        let ext = build_extension(r#"/x/has"quote\and-backslash/agent-status"#, "pi-coding-agent")
            .unwrap();
        // serde_json::to_string of "/x/has\"quote\\and-backslash/agent-status"
        // yields `"\"/x/has\\\"quote\\\\and-backslash/agent-status\""` (the
        // outer quotes are part of the JSON string literal).
        assert!(
            ext.content.contains(r#"const BIN = "/x/has\"quote\\and-backslash/agent-status";"#),
            "BIN line not escaped correctly; got:\n{}",
            ext.content,
        );
    }
```

- [ ] **Step 2: Run, confirm fails**

```bash
cargo test --lib commands::tests::build_extension_returns_pi_coding_agent_extension
```

Expected: FAIL — `build_extension("/abs/path/agent-status", "pi-coding-agent")` returns `None`.

- [ ] **Step 3: Add the helper and wire the branch**

In `src/commands.rs`, immediately below `build_claude_code_settings`, add:

```rust
/// The exact BIN-resolution line shared by `extensions/pi-coding-agent.ts`
/// and `extensions/opencode.ts`. Matched verbatim by `str::replacen` so the
/// embedded template can be specialized with an absolute path. If this line
/// drifts in the .ts source, the substitution silently no-ops and the file
/// keeps its env-fallback resolution at runtime — still functional, just
/// not alias-optimized.
const TS_BIN_RESOLUTION_LINE: &str =
    "const BIN = process.env.AGENT_STATUS_BIN ?? `${process.env.HOME}/.claude/bin/agent-status`;";

fn build_pi_extension(bin_path: &str) -> String {
    let template = include_str!("../extensions/pi-coding-agent.ts");
    let serialized = serde_json::to_string(bin_path).expect("path serializes");
    let replacement = format!("const BIN = {serialized};");
    template.replacen(TS_BIN_RESOLUTION_LINE, &replacement, 1)
}
```

Then extend the `build_extension` match:

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
        _ => None,
    }
}
```

- [ ] **Step 4: Run tests, confirm green**

```bash
cargo test --lib commands::tests::build_extension
```

Expected: all the `build_extension_*` tests pass, including the two new pi tests.

- [ ] **Step 5: Run clippy**

```bash
cargo clippy --all-targets --all-features --locked -- -D warnings
```

Expected: no warnings.

- [ ] **Step 6: Commit**

```bash
git add src/commands.rs
git commit -m "feat(commands): add pi-coding-agent branch to build_extension"
```

---

## Task 5: Integration test for `agent-extension --agent pi-coding-agent`

**Files:**
- Modify: `tests/cli.rs`

- [ ] **Step 1: Add the test**

Append to `tests/cli.rs`:

```rust
#[test]
fn agent_extension_pi_coding_agent_writes_ts_file() {
    let tmp = TempDir::new().unwrap();
    let state_dir = tmp.path().join("agent-status");

    let (stdout, stderr, code) = run(
        &state_dir,
        &["agent-extension", "--agent", "pi-coding-agent"],
        None,
    );
    assert_eq!(code, 0, "stderr: {stderr}");

    let printed_path = stdout.trim_end_matches('\n');
    let expected = state_dir.join("extensions").join("pi-coding-agent.ts");
    assert_eq!(printed_path, expected.to_string_lossy());

    let contents = std::fs::read_to_string(&expected).expect("extension file written");
    // Substituted BIN line — must be a single quoted string literal, not the
    // env-fallback expression.
    assert!(
        contents.contains(r#"const BIN = ""#),
        "expected substituted BIN, got:\n{contents}",
    );
    assert!(
        !contents.contains("process.env.AGENT_STATUS_BIN ??"),
        "env-fallback should have been replaced",
    );
    // The rest of the .ts source survives.
    assert!(contents.contains("export default function"));
    assert!(contents.contains("pi.on(\"agent_end\""));
}
```

- [ ] **Step 2: Run the integration tests**

```bash
cargo test --test cli
```

Expected: all green, including the new test.

- [ ] **Step 3: Commit**

```bash
git add tests/cli.rs
git commit -m "test(cli): agent-extension --agent pi-coding-agent writes the .ts file"
```

---

## Task 6: README — restructure the pi section to lead with the alias

**Files:**
- Modify: `README.md` (the `### pi (~/.pi/agent/extensions/)` section)

- [ ] **Step 1: Replace the section**

Find the existing `### pi (~/.pi/agent/extensions/)` section in `README.md` (it spans roughly from the `### pi` heading to the next `###` heading for opencode).

The current content is the manual-install instructions ("Pi extensions run in-process … Copy `extensions/pi-coding-agent.ts` …"). Replace the entire section through the closing paragraph about session-id with:

```markdown
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

**Known limitation:** pi has no built-in "agent paused waiting for permission" event analogous to Claude Code's `Notification` hook — pi extensions handle confirmations in-process via `ctx.ui.confirm()`. So pi-coding-agent surfaces the "done" state but not a separate "needs attention" state. In practice the dominant signal is "agent finished a turn, waiting on next prompt" anyway.

#### Wiring the extension manually (fallback)

Prefer to drop the bridge into pi's discovery directory? Skip the alias and copy the file once:

```sh
mkdir -p ~/.pi/agent/extensions
cp extensions/pi-coding-agent.ts ~/.pi/agent/extensions/
```

pi auto-discovers `~/.pi/agent/extensions/*.ts` on startup; no further configuration is required. If your `agent-status` binary is not at `~/.claude/bin/agent-status`, set `AGENT_STATUS_BIN` in your shell environment before launching pi — the manual copy uses the env-var fallback the alias bypasses.
```

Note the triple-backtick block-inside-section markdown nesting (same shape as the Claude Code section).

- [ ] **Step 2: Verify the README still renders sanely**

```bash
head -130 README.md
```

Expected: the pi section reads cleanly end-to-end; the alias snippet is the primary call-to-action; the manual fallback is clearly demoted.

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs(readme): lead the pi section with the agent-extension alias"
```

---

## Task 7: Add `build_opencode_extension` and the `opencode` branch

**Files:**
- Modify: `src/commands.rs`

- [ ] **Step 1: Write the failing tests**

Add to the `tests` mod in `src/commands.rs`:

```rust
    #[test]
    fn build_extension_returns_opencode_extension() {
        let ext = build_extension("/abs/path/agent-status", "opencode")
            .expect("opencode is supported");
        assert_eq!(ext.filename, "opencode.ts");
        assert!(
            ext.content.contains(r#"const BIN = "/abs/path/agent-status";"#),
            "missing substituted BIN; got:\n{}",
            ext.content,
        );
        assert!(
            !ext.content.contains("process.env.AGENT_STATUS_BIN ??"),
            "env-fallback line should have been replaced",
        );
        // The rest of the .ts source should still be present (sanity).
        assert!(ext.content.contains("AgentStatusPlugin"));
    }

    #[test]
    fn build_extension_opencode_extension_json_escapes_bin_path() {
        let ext = build_extension(r#"/x/has"quote\and-backslash/agent-status"#, "opencode")
            .unwrap();
        assert!(
            ext.content.contains(r#"const BIN = "/x/has\"quote\\and-backslash/agent-status";"#),
            "BIN line not escaped correctly; got:\n{}",
            ext.content,
        );
    }
```

- [ ] **Step 2: Run, confirm fails**

```bash
cargo test --lib commands::tests::build_extension_returns_opencode_extension
```

Expected: FAIL — `build_extension("/abs/path/agent-status", "opencode")` returns `None`.

- [ ] **Step 3: Add the helper and wire the branch**

In `src/commands.rs`, immediately below `build_pi_extension`, add:

```rust
fn build_opencode_extension(bin_path: &str) -> String {
    let template = include_str!("../extensions/opencode.ts");
    let serialized = serde_json::to_string(bin_path).expect("path serializes");
    let replacement = format!("const BIN = {serialized};");
    template.replacen(TS_BIN_RESOLUTION_LINE, &replacement, 1)
}
```

Extend the `build_extension` match by adding the `opencode` arm:

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

- [ ] **Step 4: Run tests**

```bash
cargo test --lib commands::tests::build_extension
```

Expected: all green.

- [ ] **Step 5: Run clippy**

```bash
cargo clippy --all-targets --all-features --locked -- -D warnings
```

Expected: no warnings.

- [ ] **Step 6: Commit**

```bash
git add src/commands.rs
git commit -m "feat(commands): add opencode branch to build_extension"
```

---

## Task 8: Integration test for `agent-extension --agent opencode`

**Files:**
- Modify: `tests/cli.rs`

- [ ] **Step 1: Add the test**

Append to `tests/cli.rs`:

```rust
#[test]
fn agent_extension_opencode_writes_ts_file() {
    let tmp = TempDir::new().unwrap();
    let state_dir = tmp.path().join("agent-status");

    let (stdout, stderr, code) = run(
        &state_dir,
        &["agent-extension", "--agent", "opencode"],
        None,
    );
    assert_eq!(code, 0, "stderr: {stderr}");

    let printed_path = stdout.trim_end_matches('\n');
    let expected = state_dir.join("extensions").join("opencode.ts");
    assert_eq!(printed_path, expected.to_string_lossy());

    let contents = std::fs::read_to_string(&expected).expect("extension file written");
    assert!(
        contents.contains(r#"const BIN = ""#),
        "expected substituted BIN, got:\n{contents}",
    );
    assert!(
        !contents.contains("process.env.AGENT_STATUS_BIN ??"),
        "env-fallback should have been replaced",
    );
    assert!(contents.contains("AgentStatusPlugin"));
}
```

- [ ] **Step 2: Run the integration tests**

```bash
cargo test --test cli
```

Expected: all green.

- [ ] **Step 3: Commit**

```bash
git add tests/cli.rs
git commit -m "test(cli): agent-extension --agent opencode writes the .ts file"
```

---

## Task 9: README — restructure the opencode section to lead with the `cp` install

**Files:**
- Modify: `README.md` (the `### opencode (~/.config/opencode/plugins/)` section)

opencode doesn't accept a per-launch `-e <path>` flag (its plugin discovery is directory-based), so the alias pattern doesn't fit. The closest equivalent is a one-time `cp` from the generated path into opencode's plugins directory; the existing manual install is the same shape with a different source path. Both routes write the same plugin.

- [ ] **Step 1: Replace the section**

Find the existing `### opencode (~/.config/opencode/plugins/)` section. Replace through its trailing paragraph with:

```markdown
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

#### Wiring the plugin manually (fallback)

If you'd rather use the source `.ts` file directly (e.g. for shared dotfiles that bundle this repo as a submodule), copy `extensions/opencode.ts` instead:

```sh
mkdir -p ~/.config/opencode/plugins
cp extensions/opencode.ts ~/.config/opencode/plugins/
```

This version uses `process.env.AGENT_STATUS_BIN ?? \`${process.env.HOME}/.claude/bin/agent-status\`` for the binary path. If your `agent-status` binary is not at `~/.claude/bin/agent-status`, set `AGENT_STATUS_BIN` in your shell environment before launching opencode.
```

- [ ] **Step 2: Verify the README still renders sanely**

```bash
head -180 README.md
```

Expected: opencode section is well-formed; the primary install instruction is the `cp` with the generated path; the manual route is clearly demoted.

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs(readme): lead the opencode section with the agent-extension cp install"
```

---

## Task 10: Update CLAUDE.md

**Files:**
- Modify: `CLAUDE.md` (the paragraph that mentions `agent-settings` + the integration-pattern note)

- [ ] **Step 1: Update the subcommand-name paragraph**

Find the existing paragraph in CLAUDE.md that mentions `agent-status agent-settings`:

```markdown
Agents that accept a `--settings <file>` flag (currently only Claude Code) can be installed via a shell alias instead of manual settings.json editing: `alias claude='claude --settings "$(agent-status agent-settings)"'`. The `agent-settings` subcommand calls `build_settings_json` in `commands.rs` (pure JSON construction using `serde_json::json!`) and writes the result to `${XDG_RUNTIME_DIR:-/tmp}/agent-status/settings/<agent>.json` using `current_exe()` for the binary path. To wire a new agent here, extend `build_settings_json` to match on the agent name and return the agent's hook JSON.
```

Replace with:

```markdown
Agents that accept a per-launch file-argument (Claude Code's `--settings <file>`, pi's `-e <file>`) can be installed via a shell alias — `alias claude='claude --settings "$(agent-status agent-extension)"'`, `alias pi='pi -e "$(agent-status agent-extension --agent pi-coding-agent)"'`. The `agent-extension` subcommand calls `build_extension` in `commands.rs`, which returns `Option<ExtensionFile { filename, content }>`. Each branch in `build_extension`'s match picks the right filename extension (`.json` / `.ts`) and the right content shape: Claude Code's JSON via `serde_json::json!`, the TypeScript bridges via `include_str!` from `extensions/<agent>.ts` plus a one-line substitution of the `BIN` constant. opencode's plugin loader is directory-based with no per-launch flag, so it's a `cp` install rather than an alias — same `build_extension` path. To wire a new alias-friendly agent, add a branch to `build_extension`; for TypeScript bridges, point at the corresponding `.ts` file under `extensions/` and rely on `TS_BIN_RESOLUTION_LINE` for the substitution.
```

- [ ] **Step 2: Run the full test suite and clippy gate one final time**

```bash
cargo test 2>&1 | grep "test result"
cargo clippy --all-targets --all-features --locked -- -D warnings
```

Expected: tests all green; clippy clean.

Test count math from baseline (95 = 83 unit + 12 integration before this plan):
- Task 2 replaces 5 `build_settings_json_*` unit tests with 5 `build_extension_*` unit tests (Step 1's TDD-driver test is dropped in Step 4 as redundant). Net 0.
- Task 4 adds 2 unit tests (`build_extension_returns_pi_coding_agent_extension`, `build_extension_pi_extension_json_escapes_bin_path`). Net +2.
- Task 5 adds 1 integration test (`agent_extension_pi_coding_agent_writes_ts_file`). Net +1.
- Task 7 adds 2 unit tests (`build_extension_returns_opencode_extension`, `build_extension_opencode_extension_json_escapes_bin_path`). Net +2.
- Task 8 adds 1 integration test (`agent_extension_opencode_writes_ts_file`). Net +1.

Predicted final: 101 tests (87 unit + 14 integration). Use `cargo test 2>&1 | grep "test result"` to confirm the actual count and use that if the math differs.

- [ ] **Step 3: Update the test-count line in `CLAUDE.md` and `README.md`**

Both files mention the test count under `## Build / test / lint` (CLAUDE.md) and `## Development` (README.md). Update to match what `cargo test` actually reports. The predicted line is `# 101 tests (87 unit + 14 integration)`; use the actual numbers from the previous step.

- [ ] **Step 4: Commit**

```bash
git add CLAUDE.md README.md
git commit -m "docs: document the build_extension dispatch pattern; bump test count"
```

---

## Task 11: End-to-end smoke test

**Files:** none modified.

- [ ] **Step 1: Install the freshly-built binary**

```bash
cargo build --release
install -m 0755 target/release/agent-status ~/.claude/bin/agent-status
```

- [ ] **Step 2: Run each `agent-extension` invocation and inspect the output**

```bash
~/.claude/bin/agent-status agent-extension --agent claude-code
cat "$(~/.claude/bin/agent-status agent-extension --agent claude-code)" | head -10
echo "---"
~/.claude/bin/agent-status agent-extension --agent pi-coding-agent
grep '^const BIN' "$(~/.claude/bin/agent-status agent-extension --agent pi-coding-agent)"
echo "---"
~/.claude/bin/agent-status agent-extension --agent opencode
grep '^const BIN' "$(~/.claude/bin/agent-status agent-extension --agent opencode)"
```

Expected:
- Each invocation prints a path ending in `extensions/<agent>.{json|ts}`.
- The Claude Code JSON's first lines contain the `hooks` object.
- The pi/opencode `.ts` files' `const BIN = ...` line is a quoted-string literal containing `/Users/<you>/.claude/bin/agent-status`, NOT the env-fallback expression.

- [ ] **Step 3: Verify the pi alias works locally (if you have pi installed)**

Add the alias to a transient shell:

```bash
alias pi='pi -e "$(~/.claude/bin/agent-status agent-extension --agent pi-coding-agent)"'
type pi
pi --version 2>&1 | head -3
```

Expected: `type pi` prints the alias body; `pi --version` runs normally (the extension is loaded but doesn't change `--version` output). Examine `${XDG_RUNTIME_DIR:-/tmp}/agent-status/extensions/pi-coding-agent.ts` — it should be newly mtime'd.

- [ ] **Step 4: No further commit**

If all three sanity checks pass, the plan is complete.

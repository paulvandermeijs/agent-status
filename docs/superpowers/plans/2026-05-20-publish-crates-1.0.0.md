# Publish `agent-status` and `agent-switcher` to crates.io at 1.0.0 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prepare both workspace crates for publication to crates.io at version 1.0.0 — add the required Cargo.toml metadata (license, repository, readme, keywords, categories), give each crate its own README that's self-contained for the crates.io / docs.rs view, simplify the workspace README to a project-level overview, and verify both crates pass `cargo publish --dry-run`.

**Architecture:** The repo is a Cargo workspace with two crates. `agent-status` (lib + bin) is the foundation — records session state, renders the tmux indicator, generates hook configs. `agent-switcher` (bin) depends on `agent-status`'s library to read state and renders a ratatui popup. For crates.io: `agent-status` must be published first (it's a dep of `agent-switcher`), and `agent-switcher`'s `[dependencies]` entry needs both `path` (for local dev builds) and `version` (for the published crate to resolve the dependency). License is MIT; the LICENSE text lives at the workspace root and is copied into each crate's directory so `cargo publish`'s per-crate tarball includes it.

**Tech Stack:** Cargo workspace (`Cargo.toml` with `[workspace.package]` for shared version). crates.io publishing via `cargo publish` (this plan stops at `--dry-run`; the actual `cargo publish` is left for the operator after the user verifies name availability). License: MIT.

---

## Pre-flight context

Current state at the start of this plan (master at `e772868`):

- Workspace version: `0.3.0` (in workspace root `Cargo.toml` under `[workspace.package]`)
- Per-crate `Cargo.toml`s have only `name`, `version.workspace = true`, `edition.workspace = true`, `description`. Missing for crates.io: `license`, `repository`, `homepage`, `readme`, `authors`, `keywords`, `categories`.
- No `LICENSE` file anywhere in the repo.
- One workspace-level `README.md` (205 lines) contains everything. No crate-level READMEs.
- `agent-switcher`'s dependency on `agent-status` is `agent-status = { path = "../agent-status" }` — missing `version`, which `cargo publish` requires.
- Git remote: `git@github.com:paulvandermeijs/agent-status.git` → repository URL is `https://github.com/paulvandermeijs/agent-status`.
- Author from `git config`: Paul van der Meijs <vandermeijs@redkiwi.nl>.

After this plan:

```
.
├── Cargo.toml                          # workspace, version 1.0.0
├── LICENSE                             # MIT text (new)
├── README.md                           # simplified, project-level overview (modified)
├── crates/
│   ├── agent-status/
│   │   ├── Cargo.toml                  # full crates.io metadata (modified)
│   │   ├── LICENSE                     # MIT text (new — copy of root)
│   │   ├── README.md                   # crate-specific docs (new — most of current root README)
│   │   ├── extensions/...
│   │   └── src/...
│   └── agent-switcher/
│       ├── Cargo.toml                  # full crates.io metadata, agent-status path+version dep (modified)
│       ├── LICENSE                     # MIT text (new — copy of root)
│       ├── README.md                   # crate-specific docs (new)
│       └── src/...
```

---

## License text (used in three places)

The exact MIT text to put into each `LICENSE` file (workspace root, and one inside each crate directory):

```
MIT License

Copyright (c) 2026 Paul van der Meijs

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

(Note: cargo packages only files inside the crate's directory at publish time, so the workspace-root `LICENSE` alone is invisible to crates.io. Each crate needs its own copy. The two copies are byte-identical to the root.)

---

### Task 1: Add MIT LICENSE file to workspace root and both crate directories

**Files:**
- Create: `LICENSE`
- Create: `crates/agent-status/LICENSE`
- Create: `crates/agent-switcher/LICENSE`

- [ ] **Step 1: Create the workspace-root LICENSE**

Create `LICENSE` (at the repo root) with the verbatim MIT text from the "License text" section above. Year `2026`, copyright holder `Paul van der Meijs`.

- [ ] **Step 2: Create `crates/agent-status/LICENSE` with the same byte-identical text**

Same content as Step 1's file. Cargo packages only files inside the crate's directory, so this copy is what crates.io / docs.rs / downstream users see.

- [ ] **Step 3: Create `crates/agent-switcher/LICENSE` with the same byte-identical text**

Same content as Step 1's file.

- [ ] **Step 4: Verify byte-identical text across the three files**

Run: `md5 LICENSE crates/agent-status/LICENSE crates/agent-switcher/LICENSE`
Expected: three identical MD5 hashes. (On Linux use `md5sum` instead.)

- [ ] **Step 5: Commit**

```bash
git add LICENSE crates/agent-status/LICENSE crates/agent-switcher/LICENSE
git commit -m "$(cat <<'EOF'
chore: add MIT LICENSE file at workspace root and inside each crate

cargo publish packages only files inside the published crate's
directory, so each crate needs its own LICENSE copy alongside the
workspace-root one. All three files are byte-identical (MIT text,
year 2026, copyright Paul van der Meijs).
EOF
)"
```

---

### Task 2: Bump the workspace version to 1.0.0

**Files:**
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Update the workspace package version**

In `Cargo.toml` at the workspace root, find:

```toml
[workspace.package]
version = "0.3.0"
edition = "2021"
```

Replace with:

```toml
[workspace.package]
version = "1.0.0"
edition = "2021"
```

Both crates inherit this via `version.workspace = true` in their per-crate `Cargo.toml`, so no per-crate edit is needed for this step.

- [ ] **Step 2: Verify the workspace still builds**

Run: `cargo build --release`
Expected: clean build. Both `target/release/agent-status` and `target/release/agent-switcher` produced.

- [ ] **Step 3: Verify the version is reflected**

Run: `./target/release/agent-status --version`
Expected output: `agent-status 1.0.0`

- [ ] **Step 4: Run the test gate**

Run: `cargo test`
Expected: 149 tests pass (same set as before — version change is pure metadata).

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml
git commit -m "chore: bump workspace version to 1.0.0 for crates.io release"
```

---

### Task 3: Prep `agent-status` for publishing (crate README + Cargo.toml metadata)

**Files:**
- Create: `crates/agent-status/README.md`
- Modify: `crates/agent-status/Cargo.toml`

- [ ] **Step 1: Create `crates/agent-status/README.md`**

Full content (this is the crates.io / docs.rs landing page for the `agent-status` crate — make it self-contained):

````markdown
# agent-status

A small Rust CLI + library that shows in tmux's `status-right` which AI coding agent sessions are waiting on user input. Supports [Claude Code], [pi], and [opencode]; the architecture is set up to plug in additional agents (Codex CLI, Cursor CLI) without restructuring.

```text
$ agent-status status        # one session waiting
[!] agent-status

$ agent-status status        # multiple sessions waiting
[!] 3 projects waiting

$ agent-status status        # nothing waiting
                              # (no output, exit 0)
```

## How it works

Claude Code [hooks][hooks] fire `agent-status set notify` on `Notification` and `PermissionRequest`, `set done` on `Stop`, `set working` on `UserPromptSubmit` and `PreToolUse`, `set idle` on `SessionStart`, and `clear` on `SessionEnd`. Each `set` writes one JSON file per session under `${XDG_RUNTIME_DIR:-/tmp}/agent-status/`, keyed by `session_id`. tmux's `status-right` invokes `agent-status status` on its refresh interval; the command lists the state directory and renders the indicator (project name for one waiting session, count for many, nothing for none).

No daemon. The filesystem is the state store; each session writes only its own keyed file, so concurrent writers never contend.

## Install

```sh
cargo install agent-status
```

The binary lands in `~/.cargo/bin`. As long as that's on your `$PATH` (Cargo's installer adds it by default), the alias-based wiring below works without any further configuration. The binary is around 550 KB stripped and has no runtime dependencies (tmux is invoked best-effort to refresh the status bar; if it isn't running, the failure is silenced).

For the popup picker, also install the companion crate:

```sh
cargo install agent-switcher
```

## Configure

### Claude Code

Drop this alias into your shell rc (`.zshrc`, `.bashrc`, etc.):

```sh
alias claude='claude --settings "$(agent-status agent-extension --agent claude-code)"'
```

That's it. Each time you run `claude`, the alias expands so claude launches with `--settings <generated.json>` — a seven-hook file that `agent-status` regenerates on every invocation from its own absolute path. Claude Code merges `--settings` on top of your user/project settings, so nothing you've already configured gets overwritten.

The generated file lives at `${XDG_RUNTIME_DIR:-/tmp}/agent-status/extensions/claude-code.json` and is rewritten every run — no cleanup, no env vars, no PATH manipulation.

#### Wiring the hooks manually (fallback)

If you'd rather see the hooks in your settings file, skip the alias and merge this into the top-level `hooks` block of `~/.claude/settings.json` instead:

```json
{
  "hooks": {
    "Notification":      [{ "hooks": [{ "type": "command", "command": "agent-status set --agent claude-code notify"  }] }],
    "PermissionRequest": [{ "hooks": [{ "type": "command", "command": "agent-status set --agent claude-code notify"  }] }],
    "Stop":              [{ "hooks": [{ "type": "command", "command": "agent-status set --agent claude-code done"    }] }],
    "UserPromptSubmit":  [{ "hooks": [{ "type": "command", "command": "agent-status set --agent claude-code working" }] }],
    "PreToolUse":        [{ "hooks": [{ "type": "command", "command": "agent-status set --agent claude-code working" }] }],
    "SessionStart":      [{ "hooks": [{ "type": "command", "command": "agent-status set --agent claude-code idle"   }] }],
    "SessionEnd":        [{ "hooks": [{ "type": "command", "command": "agent-status clear --agent claude-code"      }] }]
  }
}
```

Pick one route, not both. If you have manual hooks AND the alias, each event fires twice — idempotent for repeats of the same `set` but wasteful.

`SessionStart` writes a placeholder `idle` row so every Claude session appears in the [`agent-switcher`][switcher] popup from the moment it starts — even before you type the first prompt. `UserPromptSubmit` and `PreToolUse` then flip the row to `working` while Claude is mid-turn. The tmux status indicator filters both `idle` and `working` out, so the bar still only shows "needs you now" sessions (`notify` from `Notification`/`PermissionRequest`, `done` from `Stop`). The switcher shows every row, rendering `idle` as a dim dot and `working` as a spinner.

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

**Known limitation:** pi has no built-in "agent paused waiting for permission" event analogous to Claude Code's `PermissionRequest` hook — pi extensions handle confirmations in-process via `ctx.ui.confirm()`. So pi-coding-agent surfaces the "done" state but not a separate "needs attention" state. In practice the dominant signal is "agent finished a turn, waiting on next prompt" anyway.

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

### tmux (`~/.tmux.conf`)

Drop `#(agent-status status)` into your existing `status-right` wherever you want the indicator to appear, and lower the refresh interval so updates feel snappy:

```tmux
set -g status-interval 5
set -g status-right "#(agent-status status) <your existing status-right here>"
```

Reload with `tmux source-file ~/.tmux.conf`.

For the popup picker (prefix + `C-a`), see the [`agent-switcher`][switcher] crate.

## Usage

```sh
agent-status --help                       # top-level help
agent-status set [EVENT] --agent NAME     # mark this session as waiting (reads JSON on stdin)
agent-status clear --agent NAME           # clear this session's state (reads JSON on stdin)
agent-status status                       # print the status-right line, empty if nothing waiting
agent-status list                         # print TSV (session_id, pane, display) per waiting session
```

`set` and `clear` expect a JSON object on stdin with at least `{"session_id": "..."}`. Empty or missing `session_id` is a silent no-op.

## State location

`${XDG_RUNTIME_DIR:-/tmp}/agent-status/<session_id>` — one file per active session. Inspectable with `ls`/`cat`. Format:

```json
{"agent":"claude-code","project":"agent-status","cwd":"/path/to/project","event":"notify","tmux_pane":"%17","ts":1778163565,"message":"Permission required","pid":12345}
```

The `message` field is optional and only present when the agent's hook payload supplies one (e.g. Claude Code's `Notification` event). Older state files written before this field existed still load — `message` defaults to absent.

The `agent` field is `"claude-code"`, `"opencode"`, or `"pi-coding-agent"`; new agents use their own lowercase-hyphenated name.

The `pid` field records the agent process's PID so `agent-status status` and `list` can detect and remove entries whose owning process has died without firing its session-end hook. Files written by older binaries — which lack `pid` — are never auto-pruned; they age out only on tmpfs cleanup. Such entries should disappear naturally after one `set`/`clear` cycle on the affected session.

## Caveats

- **The `Stop` hook fires on every turn end**, so any session that just finished a response shows up as "waiting" until you send the next prompt. Intentional — the whole point is to know which session needs you while you're heads-down elsewhere. Drop the `Stop` line from `settings.json` if it proves too eager.
- **opencode has no "user submitted a prompt" event**, so once `session.idle` marks an opencode session `done`, the indicator stays on `done` while you type the next prompt and only refreshes its timestamp on the next idle. Same intent as the `Stop` caveat above — the session *is* the one waiting on you.
- **Architecture-specific binary.** The compiled binary is platform-locked. On a new machine, rebuild from source (`cargo install agent-status`).
- **Only Claude Code records a `working` state** today. pi and opencode sessions appear in the switcher when they're waiting (`done` / `notify`) but not while they're mid-turn. The hook semantics for those agents can be extended in a follow-up.

## License

MIT. See [LICENSE](LICENSE).

[hooks]: https://docs.claude.com/en/docs/claude-code/hooks
[Claude Code]: https://claude.com/claude-code
[pi]: https://pi.dev
[opencode]: https://opencode.ai
[switcher]: https://crates.io/crates/agent-switcher
````

- [ ] **Step 2: Update `crates/agent-status/Cargo.toml` with full crates.io metadata**

Replace the entire current content of `crates/agent-status/Cargo.toml` with:

```toml
[package]
name = "agent-status"
version.workspace = true
edition.workspace = true
description = "Tmux-integrated indicator showing which AI coding agent sessions are waiting on user input."
authors = ["Paul van der Meijs <vandermeijs@redkiwi.nl>"]
license = "MIT"
repository = "https://github.com/paulvandermeijs/agent-status"
homepage = "https://github.com/paulvandermeijs/agent-status"
documentation = "https://docs.rs/agent-status"
readme = "README.md"
keywords = ["tmux", "ai-agent", "claude-code", "status-bar", "hook"]
categories = ["command-line-utilities", "development-tools"]

[lints]
workspace = true

[lib]
path = "src/lib.rs"

[[bin]]
name = "agent-status"
path = "src/main.rs"

[dependencies]
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"

[dev-dependencies]
tempfile = "3"
```

Field-by-field rationale (no need to action; for reviewer reference):
- `license = "MIT"` — SPDX expression, paired with the per-crate `LICENSE` file from Task 1.
- `repository` / `homepage` — both point at the GitHub repo (canonical home for now).
- `documentation` — explicit `https://docs.rs/agent-status`; docs.rs would auto-set this but explicit is clearer.
- `readme = "README.md"` — relative to this `Cargo.toml`, so points at `crates/agent-status/README.md` from Step 1.
- `keywords` — 5 (the crates.io max), lowercase, hyphenated.
- `categories` — `command-line-utilities` (it's a CLI tool) and `development-tools` (it's dev-flow tooling).

- [ ] **Step 3: Verify the workspace still builds and tests**

Run: `cargo build --release && cargo test`
Expected: clean build + 149 tests pass.

- [ ] **Step 4: Verify `cargo publish --dry-run` succeeds for `agent-status`**

Run: `cargo publish -p agent-status --dry-run --allow-dirty`
Expected: command exits 0. You should see lines like:
```
Packaging agent-status v1.0.0 (...)
Verifying agent-status v1.0.0 (...)
Compiling agent-status v1.0.0 (...)
    Finished `release` profile [optimized] target(s) in ...
Packaged ... files, ... (... compressed)
```

`--allow-dirty` is needed because Task 4 hasn't landed yet — the workspace still has uncommitted README/Cargo.toml changes for `agent-switcher` until that task runs.

If `cargo publish` complains about a missing field or invalid value, re-read the field-by-field rationale above and the Cargo.toml content from Step 2 to spot the divergence.

- [ ] **Step 5: Commit**

```bash
git add crates/agent-status/README.md crates/agent-status/Cargo.toml
git commit -m "$(cat <<'EOF'
feat(agent-status): add crate README and crates.io publishing metadata

Adds the per-crate README that crates.io and docs.rs render as the
landing page, plus the Cargo.toml metadata cargo publish requires
(license, repository, homepage, documentation, readme, keywords,
categories, authors).

The README is largely the agent-status-specific portion of the old
workspace-root README, made self-contained: install via `cargo install
agent-status`, hook configuration for Claude Code / pi / opencode,
tmux status-right wiring, CLI usage, state-file format, caveats. The
agent-switcher popup section is moved to that crate's own README and
referenced via a crates.io link.

`cargo publish -p agent-status --dry-run --allow-dirty` passes.
EOF
)"
```

---

### Task 4: Prep `agent-switcher` for publishing (crate README + Cargo.toml metadata, with versioned dep)

**Files:**
- Create: `crates/agent-switcher/README.md`
- Modify: `crates/agent-switcher/Cargo.toml`

- [ ] **Step 1: Create `crates/agent-switcher/README.md`**

Full content:

````markdown
# agent-switcher

A ratatui-based tmux popup TUI for switching between waiting AI coding agent sessions. Companion to [`agent-status`][status] — reads the same `${XDG_RUNTIME_DIR:-/tmp}/agent-status/` state directory and renders a small picker. <kbd>Enter</kbd> runs `tmux switch-client` to the selected session's pane.

```
┌─Filter────────────────────┐
│ >                          │
└───────────────────────────┘
┌─Sessions──────────────────────────────────────────────────────────┐
│   Session                Agent           Activity                │
│ ! agent-status (53fabd56) claude-code     Claude needs permission │
│ ⠋ shai-hulud   (9b73ed57) claude-code     Reading src/main.rs     │
│ · scratch      (4012a1cd) opencode                                │
│ ✓ docs         (1f33ccee) claude-code     Wrote 4 files           │
└───────────────────────────────────────────────────────────────────┘
 Ctrl-N/P or ↓/↑: navigate · Enter: switch pane · Esc / Ctrl-C: cancel
```

## Install

```sh
cargo install agent-switcher
```

You also need [`agent-status`][status] installed and at least one agent's hooks wired up — without state files in `${XDG_RUNTIME_DIR:-/tmp}/agent-status/`, the switcher has nothing to display.

```sh
cargo install agent-status agent-switcher
```

See [`agent-status`'s README][status] for hook wiring (Claude Code, pi, opencode).

## tmux popup

Drop this into `~/.tmux.conf`:

```tmux
bind-key C-a display-popup -E -w 80% -h 50% "agent-switcher"
```

Reload with `tmux source-file ~/.tmux.conf`. Press prefix + <kbd>C-a</kbd> to open the picker.

## Keybindings inside the switcher

| Key                       | Action                                                                |
|---------------------------|-----------------------------------------------------------------------|
| Type any char             | Append to the filter (case-insensitive; matches project, agent, message, session id) |
| <kbd>Backspace</kbd>      | Remove the last filter char                                           |
| <kbd>Ctrl-N</kbd> / <kbd>↓</kbd> | Move selection down (wraps at the bottom)                       |
| <kbd>Ctrl-P</kbd> / <kbd>↑</kbd> | Move selection up (wraps at the top)                            |
| <kbd>Enter</kbd>          | `tmux switch-client` to the selected session's pane, then exit         |
| <kbd>Esc</kbd> / <kbd>Ctrl-C</kbd> | Exit without switching                                        |

## What's in the list

Every recorded session — including those still working (animated spinner) — not just sessions waiting on your attention. That makes the popup useful as a general session jumper, while the [`agent-status`][status] tmux indicator stays focused on "needs you now" sessions.

The activity column:
- While Claude Code is working: shows the active tool — e.g. `Reading src/main.rs`, `Running: git status`, `Searching: fn main`.
- When Claude Code is waiting on you: shows the notification message (e.g. `Claude needs your permission to use Bash`).
- For other agents: shows the last-response text if the agent's hook payload supplied one.

The marker column:
- `⠋` (spinner, cyan) — `working`: agent is mid-turn.
- `!` (yellow) — `notify`: agent is blocked on you (permission / input prompt).
- `✓` (green) — `done`: agent finished a turn.
- `·` (dim) — `idle`: session is alive but no prompt has arrived yet (placeholder so the row is visible from `SessionStart`).

## Dependency on `agent-status`

`agent-switcher` reads the state store via the [`agent-status` library][status-docs] (`StateStore`, `AttentionEntry`). The two crates ship the same version, but `agent-switcher` declares a flexible version range so you can upgrade them independently if you need to.

## License

MIT. See [LICENSE](LICENSE).

[status]: https://crates.io/crates/agent-status
[status-docs]: https://docs.rs/agent-status
````

- [ ] **Step 2: Update `crates/agent-switcher/Cargo.toml` with full crates.io metadata and a versioned `agent-status` dep**

Replace the entire current content of `crates/agent-switcher/Cargo.toml` with:

```toml
[package]
name = "agent-switcher"
version.workspace = true
edition.workspace = true
description = "Tmux popup TUI for switching between waiting AI coding agent sessions."
authors = ["Paul van der Meijs <vandermeijs@redkiwi.nl>"]
license = "MIT"
repository = "https://github.com/paulvandermeijs/agent-status"
homepage = "https://github.com/paulvandermeijs/agent-status"
readme = "README.md"
keywords = ["tmux", "ai-agent", "claude-code", "tui", "ratatui"]
categories = ["command-line-utilities"]

[lints]
workspace = true

[[bin]]
name = "agent-switcher"
path = "src/main.rs"

[dependencies]
agent-status = { path = "../agent-status", version = "1.0.0" }
ratatui = "0.29"
crossterm = "0.28"

[dev-dependencies]
tempfile = "3"
```

Key change beyond the metadata block: `agent-status = { path = "../agent-status", version = "1.0.0" }`. Cargo uses `path` for local workspace builds and `version` for the published crate. `cargo publish` validates that both are consistent — without the `version`, `cargo publish` rejects the manifest with an error like `all path dependencies must have a version specified`.

There's no `documentation` field for this crate because it has no library — `agent-switcher` is binary-only, so docs.rs has nothing to render.

- [ ] **Step 3: Verify the workspace still builds and tests**

Run: `cargo build --release && cargo test`
Expected: clean build + 149 tests pass. The `path` dependency means the local build resolves `agent-status` from `crates/agent-status/`, not from crates.io.

- [ ] **Step 4: Verify `cargo publish --dry-run` succeeds for `agent-switcher`**

Run: `cargo publish -p agent-switcher --dry-run --allow-dirty`

This will FAIL initially because `agent-status` v1.0.0 hasn't been published to crates.io yet — `cargo publish`'s dry-run still tries to resolve the version constraint against the live registry. Expected error:

```
error: failed to verify package tarball
Caused by:
  no matching package named `agent-status` found
```

That's the expected pre-publish state. The `--dry-run` won't succeed for `agent-switcher` until `agent-status` v1.0.0 is actually published. Two ways to handle this:

**(a) Skip the dry-run for now.** Document in the commit message that `agent-switcher`'s dry-run will only pass after `agent-status` is published. Move on.

**(b) Temporarily verify with `cargo package`** instead, which packages the crate without trying to resolve published deps:

```sh
cargo package -p agent-switcher --allow-dirty
```

Expected: exits 0, produces `target/package/agent-switcher-1.0.0.crate`. This proves the manifest is well-formed even if the cross-crate version resolution can't be checked locally.

Use option (b) — it actually verifies something useful (the manifest passes `cargo`'s validation, the README path is correct, all files are includable).

- [ ] **Step 5: Commit**

```bash
git add crates/agent-switcher/README.md crates/agent-switcher/Cargo.toml
git commit -m "$(cat <<'EOF'
feat(agent-switcher): add crate README and crates.io publishing metadata

Adds the per-crate README (crates.io / docs.rs landing page) and the
Cargo.toml metadata cargo publish requires. The agent-status path
dependency gains an explicit version = "1.0.0" alongside the path so
cargo can resolve it both for local workspace builds and for the
published crate.

`cargo package -p agent-switcher --allow-dirty` passes; full
`cargo publish --dry-run` will only succeed once agent-status v1.0.0
is actually published to crates.io.
EOF
)"
```

---

### Task 5: Simplify the workspace `README.md` to a project-level overview

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Replace the entire content of the workspace-root `README.md`**

Replace ALL existing content with:

````markdown
# agent-status

Tmux-integrated indicators showing which AI coding agent sessions are waiting on user input. Supports [Claude Code], [pi], and [opencode]; the architecture is set up to plug in additional agents (Codex CLI, Cursor CLI) without restructuring.

```text
$ agent-status status        # one session waiting
[!] agent-status

$ agent-status status        # multiple sessions waiting
[!] 3 projects waiting
```

This repo publishes two crates:

| Crate | Description |
|---|---|
| **[`agent-status`](crates/agent-status/README.md)** | CLI + library. Records per-session state, renders the tmux `status-right` indicator, and generates the hook/extension configs for each agent. |
| **[`agent-switcher`](crates/agent-switcher/README.md)** | ratatui popup TUI. Lists every recorded session and switches tmux panes on <kbd>Enter</kbd>. |

## Install

```sh
cargo install agent-status agent-switcher
```

Then follow [`agent-status`'s README](crates/agent-status/README.md) for the hook wiring (Claude Code / pi / opencode) and tmux configuration, and [`agent-switcher`'s README](crates/agent-switcher/README.md) for the popup picker bind.

## Development

```sh
cargo test                                                            # workspace-wide test suite
cargo clippy --all-targets --all-features --locked -- -D warnings     # required gate
cargo build --release                                                 # both binaries, ~1.1 MB combined
```

See [`CLAUDE.md`](CLAUDE.md) for the codebase guide (module layout, adding a new agent, wire compatibility).

## License

MIT — see [LICENSE](LICENSE).

[Claude Code]: https://claude.com/claude-code
[pi]: https://pi.dev
[opencode]: https://opencode.ai
````

Rationale for what got removed:
- The "How it works", install-hook-by-hook details, and the full Claude Code / pi / opencode / tmux configure sections moved to `crates/agent-status/README.md` (Task 3).
- The "popup picker" / keybindings / activity-column section moved to `crates/agent-switcher/README.md` (Task 4).
- "Usage" (`agent-status` CLI subcommands), "State location", and operational caveats moved to `crates/agent-status/README.md`.
- The "Development" section stays because it's about working on this repo (workspace-level), not about using either crate.

- [ ] **Step 2: Sanity-check the markdown links**

Run: `grep -nE '\[.+\]\(\S+\)' README.md`
Expected output: links to `crates/agent-status/README.md`, `crates/agent-switcher/README.md`, `CLAUDE.md`, `LICENSE`, plus the three reference-style links to claude.com, pi.dev, opencode.ai at the bottom. All paths should exist on disk (verify with `ls crates/agent-status/README.md crates/agent-switcher/README.md CLAUDE.md LICENSE`).

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "$(cat <<'EOF'
docs(readme): simplify the workspace README to a project-level overview

Crate-specific content (hook wiring, tmux configure, CLI usage, state
format, caveats, popup keybindings, activity column) moved to the
respective crate READMEs in the previous two commits. The workspace
README is now a high-level overview pointing at the two crate READMEs
and the development gates.
EOF
)"
```

---

### Task 6: Verify publish-readiness of both crates

**Files:** none modified — this is a verification task only.

- [ ] **Step 1: Verify `agent-status` dry-run still passes after all the above commits**

Run: `cargo publish -p agent-status --dry-run`
Expected: exits 0 (no `--allow-dirty` needed — the working tree is clean by this point).

If it fails: read the error and back-patch the relevant Cargo.toml or README. The most common failure modes are:
- `failed to verify package tarball: could not find LICENSE` — Task 1 didn't land or the path is wrong.
- `error: invalid value for field 'keywords'` — keyword too long or contains an invalid char.
- `error: this file is too large` — unlikely here (the largest packaged file is the README + extensions/*.ts, well under the limit).

- [ ] **Step 2: Verify `agent-switcher` packages cleanly**

Run: `cargo package -p agent-switcher`
Expected: exits 0, produces `target/package/agent-switcher-1.0.0.crate`.

(As noted in Task 4: full `cargo publish --dry-run` for `agent-switcher` requires `agent-status` to already be on crates.io. `cargo package` is the verification we can run locally.)

- [ ] **Step 3: Re-run the workspace test + lint gates one last time**

Run:
```sh
cargo test
cargo clippy --all-targets --all-features --locked -- -D warnings
```
Expected: 149 tests pass, clippy clean.

- [ ] **Step 4: Inspect the contents of each packaged crate**

Run:
```sh
tar -tzf target/package/agent-status-1.0.0.crate | sort
tar -tzf target/package/agent-switcher-1.0.0.crate | sort
```

Expected `agent-status-1.0.0.crate` contents: `Cargo.toml`, `Cargo.toml.orig`, `LICENSE`, `README.md`, `extensions/opencode.ts`, `extensions/pi-coding-agent.ts`, `src/agents/*.rs`, `src/commands/*.rs`, `src/lib.rs`, `src/main.rs`, `src/state.rs`, `tests/cli.rs`.

Expected `agent-switcher-1.0.0.crate` contents: `Cargo.toml`, `Cargo.toml.orig`, `LICENSE`, `README.md`, `src/app.rs`, `src/filter.rs`, `src/main.rs`, `src/ui.rs`.

If either tarball is missing `LICENSE` or `README.md`, the corresponding crate-directory file from Task 1 / 3 / 4 didn't land. Investigate.

- [ ] **Step 5: No commit needed (verification only)**

This task makes no source changes. If you ran `cargo package` and it left `target/package/`, that's ignored by `.gitignore` (or by `target/` being globally ignored) and doesn't show up in `git status`.

---

## Out of scope (the actual `cargo publish` run)

This plan stops at "ready to publish". The actual publish workflow — which the operator runs interactively after the plan completes — is roughly:

```sh
# 1. Confirm the names are available on crates.io
curl -s https://crates.io/api/v1/crates/agent-status   | jq '.errors[0].detail'  # expect: "Not Found"
curl -s https://crates.io/api/v1/crates/agent-switcher | jq '.errors[0].detail'  # expect: "Not Found"

# 2. Make sure you're logged into crates.io
cargo login

# 3. Publish agent-status FIRST (agent-switcher depends on it)
cargo publish -p agent-status

# 4. Wait ~1 minute for the index to refresh, then publish agent-switcher
cargo publish -p agent-switcher

# 5. Tag the release
git tag v1.0.0
git push origin v1.0.0
```

If either name is taken on crates.io, the operator picks a new name (e.g. `tmux-agent-status`) and updates `crates/<crate>/Cargo.toml`'s `name = ...` field before publishing. That's not predictable from this side — leave for the operator.

---

## Self-review

**1. Spec coverage:**
- "Each crate has a README" — Task 3 (agent-status README) + Task 4 (agent-switcher README). ✓
- "Keep the main README simple, move crate specific info to those individual READMEs" — Task 5. ✓
- "Bump the version to 1.0.0" — Task 2. ✓
- "We want to publish these crates" — Tasks 1 (LICENSE), 3 (agent-status metadata), 4 (agent-switcher metadata + versioned dep), 6 (verification). ✓

**2. Placeholder scan:**
- No "TBD", "implement later", "fill in details", or "similar to Task N" anywhere. Each task embeds the verbatim content for every file it creates or modifies.
- License text is the literal MIT text with year and copyright holder filled in.
- Cargo.toml content is fully specified (every field shown).
- README content is verbatim.

**3. Type / field consistency:**
- The version is `1.0.0` everywhere (Task 2's workspace package, Task 4's `agent-status = { ..., version = "1.0.0" }` dep). The `version.workspace = true` inheritance pattern carries the bump into both crates' `Cargo.toml` without per-crate edits.
- The license string `"MIT"` is consistent across both `Cargo.toml`s; the LICENSE file content matches the SPDX expression.
- The repository / homepage URL is `https://github.com/paulvandermeijs/agent-status` in both manifests.
- The author string is `"Paul van der Meijs <vandermeijs@redkiwi.nl>"` in both manifests (matches `git config user.name` / `user.email`).
- README internal links: workspace README → `crates/agent-status/README.md` and `crates/agent-switcher/README.md` (paths that exist after Tasks 3-4); `crates/agent-status/README.md` → crates.io for `agent-switcher` (external link, no path dependency); `crates/agent-switcher/README.md` → crates.io for `agent-status`. No broken relative paths.

**Plan-level edge cases acknowledged:**
- `agent-switcher`'s `cargo publish --dry-run` cannot succeed before `agent-status` is published. Task 4 Step 4 explicitly uses `cargo package` instead, which validates the manifest without round-tripping the dep through crates.io.
- Crate name availability on crates.io can't be checked from this side without an API call (which the "Out of scope" section documents). If either name is taken, the user adjusts before the actual publish.

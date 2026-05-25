use serde::de::{self, Deserializer};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::PathBuf;

/// Lifecycle state the producing hook reported for a session.
///
/// On the wire this is a plain JSON string — the four known values
/// (`notify`, `done`, `working`, `idle`) round-trip through their named
/// variants, and anything else is preserved verbatim in `Unknown(String)`
/// so new hook event types added by future agents don't break older
/// binaries. The variant order matches the switcher's display priority
/// (most-attention-needing first): `Notify`, `Done`, `Idle`, `Working`,
/// `Unknown`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Event {
    /// Agent explicitly signals the user (Claude Code `Notification` /
    /// `PermissionRequest`, pi `before_user_input` analogues).
    Notify,
    /// Agent just finished a turn (Claude Code `Stop`, pi `agent_end`).
    Done,
    /// Session is alive but not interacting (Claude Code `SessionStart`
    /// placeholder, pi `session_start`).
    Idle,
    /// Agent is in the middle of working — typing, calling tools
    /// (Claude Code `UserPromptSubmit` / `PreToolUse`, pi
    /// `before_agent_start` / `tool_execution_start`).
    Working,
    /// Forward-compat: a hook reported an event string we don't recognize.
    /// Kept verbatim so re-serialization is lossless.
    Unknown(String),
}

impl Event {
    /// Borrow this event as the wire string the hooks use.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Notify => "notify",
            Self::Done => "done",
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Unknown(s) => s.as_str(),
        }
    }

    /// Whether this event represents a session asking for the user's eyes
    /// right now. `Notify` is an explicit "blocked on you" signal; `Done`
    /// is the just-finished state the next prompt will move on from. Any
    /// future / unknown event value is treated as attention-worthy so a
    /// new hook type added by an agent does not silently disappear from
    /// the tmux indicator. `Working` and `Idle` are alive-but-not-asking.
    #[must_use]
    pub fn needs_attention(&self) -> bool {
        !matches!(self, Self::Working | Self::Idle)
    }
}

impl From<&str> for Event {
    fn from(s: &str) -> Self {
        match s {
            "notify" => Self::Notify,
            "done" => Self::Done,
            "idle" => Self::Idle,
            "working" => Self::Working,
            _ => Self::Unknown(s.to_string()),
        }
    }
}

impl From<String> for Event {
    fn from(s: String) -> Self {
        match s.as_str() {
            "notify" => Self::Notify,
            "done" => Self::Done,
            "idle" => Self::Idle,
            "working" => Self::Working,
            _ => Self::Unknown(s),
        }
    }
}

impl Serialize for Event {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Event {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d).map_err(de::Error::custom)?;
        Ok(Self::from(s))
    }
}

/// One entry stored per active agent session that is waiting on user attention.
///
/// Serialized as compact JSON to one file per session (keyed by `session_id`) under
/// `${XDG_RUNTIME_DIR:-/tmp}/agent-status/`. The field shape is wire-compatible with
/// the bash version of this tool.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct AttentionEntry {
    /// Stable identifier of the agent that wrote this entry (e.g. `"claude-code"`).
    pub agent: String,
    /// Basename of the project directory (typically the cwd's last component).
    pub project: String,
    /// Absolute path of the project directory at the time the hook fired.
    pub cwd: String,
    /// Hook event the producing agent reported.
    pub event: Event,
    /// Tmux pane id (such as `%17`), or empty if the hook fired outside tmux.
    pub tmux_pane: String,
    /// Unix timestamp (seconds) when the entry was written.
    pub ts: u64,
    /// Optional last-message text from the agent (e.g. Claude Code Notification's `message`
    /// field). Absent in the JSON when `None`; absent on entries written by older binaries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// PID of the agent process at the time the hook fired (typically `getppid()`
    /// from inside the hook script — the claude/opencode/pi binary). Used to clean
    /// up state files whose owning process has exited without firing its
    /// session-end hook. Absent in entries written by older binaries; entries
    /// without a pid are never auto-pruned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
}

/// Reads, writes and lists [`AttentionEntry`] files under a single state directory.
///
/// Each session writes one file keyed by its `session_id`, so concurrent writers from
/// different sessions never contend on the same path — no locking is required.
pub struct StateStore {
    dir: PathBuf,
}

impl StateStore {
    /// Construct a store backed by `dir`.
    ///
    /// The directory does not need to exist yet — [`write`](Self::write) creates it on demand.
    #[must_use]
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Construct a store under `${XDG_RUNTIME_DIR:-/tmp}/agent-status/`.
    pub fn from_env() -> Self {
        let base = std::env::var_os("XDG_RUNTIME_DIR")
            .map_or_else(|| PathBuf::from("/tmp"), PathBuf::from);
        Self::new(base.join("agent-status"))
    }

    /// Path of the state directory.
    #[cfg(test)]
    #[must_use]
    pub fn dir(&self) -> &std::path::Path {
        &self.dir
    }

    /// Write an entry for `session_id`, creating the state directory if needed.
    ///
    /// # Errors
    /// Returns the underlying I/O error if the directory cannot be created or the file cannot
    /// be written. Returns [`io::ErrorKind::InvalidInput`] when `session_id` is empty or
    /// contains a path separator (defense against path-traversal).
    pub fn write(&self, session_id: &str, entry: &AttentionEntry) -> io::Result<()> {
        validate_session_id(session_id)?;
        fs::create_dir_all(&self.dir)?;
        let json = serde_json::to_vec(entry)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        fs::write(self.dir.join(session_id), json)
    }

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

    /// List all entries in the state directory, sorted by timestamp ascending then `session_id`.
    ///
    /// Files with invalid JSON or unreadable content are silently skipped — they are treated
    /// as if absent. Returns an empty `Vec` when the directory does not exist.
    ///
    /// # Errors
    /// Returns the underlying I/O error if `read_dir` or per-entry metadata access fails for
    /// a reason other than `NotFound`.
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
}

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
/// under a millisecond and fires only on `agent-status status`/`list` and
/// `agent-switcher`'s tick (state-directory refresh).
///
/// Fails open: if the `kill` command can't be spawned at all (no `/bin/kill`,
/// stripped `$PATH` in a hardened user-service env, …), we return `true` so
/// the caller keeps the state file. Pruning every live entry on an unrelated
/// platform misconfiguration would be much worse than skipping the prune.
fn is_pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    // Absolute path so a stripped or hostile $PATH can't shadow us with a
    // fake `kill`. /bin/kill is present on every POSIX target this crate
    // supports (Darwin, Linux). Falls through to a $PATH lookup only if the
    // absolute path doesn't exist.
    let status = std::process::Command::new("/bin/kill")
        .args(["-0", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .status();
    match status {
        Ok(s) => s.success(),
        Err(_) => true,
    }
}

fn validate_session_id(session_id: &str) -> io::Result<()> {
    if session_id.is_empty()
        || session_id.contains('/')
        || session_id.contains(std::path::MAIN_SEPARATOR)
        || session_id == "."
        || session_id == ".."
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid session_id",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn entry_roundtrips_through_json() {
        let entry = AttentionEntry {
            agent: "claude-code".into(),
            project: "claude-status".into(),
            cwd: "/Users/x/work/claude-status".into(),
            event: Event::Notify,
            tmux_pane: "%42".into(),
            ts: 1_700_000_000,
            message: None,
            pid: None,
        };
        let json = serde_json::to_string(&entry).unwrap();
        let parsed: AttentionEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, entry);
    }

    #[test]
    fn event_known_values_serialize_as_plain_strings() {
        for (evt, wire) in [
            (Event::Notify, "\"notify\""),
            (Event::Done, "\"done\""),
            (Event::Idle, "\"idle\""),
            (Event::Working, "\"working\""),
        ] {
            assert_eq!(serde_json::to_string(&evt).unwrap(), wire);
            let parsed: Event = serde_json::from_str(wire).unwrap();
            assert_eq!(parsed, evt);
        }
    }

    #[test]
    fn event_unknown_value_roundtrips_verbatim() {
        // Forward compat: a future agent emitting a new event string must
        // deserialize cleanly and re-serialize without losing the original
        // text, so a mixed-version setup doesn't silently rewrite state.
        let json = r#""compacting""#;
        let parsed: Event = serde_json::from_str(json).unwrap();
        assert_eq!(parsed, Event::Unknown("compacting".to_string()));
        assert_eq!(serde_json::to_string(&parsed).unwrap(), json);
    }

    #[test]
    fn event_needs_attention_matches_legacy_filter() {
        // notify + done + Unknown(future event) → surface in tmux/list;
        // working + idle → hide. This is the contract the bash precursor
        // and the v0.2.0+ binary share.
        assert!(Event::Notify.needs_attention());
        assert!(Event::Done.needs_attention());
        assert!(Event::Unknown("anything-new".into()).needs_attention());
        assert!(!Event::Working.needs_attention());
        assert!(!Event::Idle.needs_attention());
    }

    #[test]
    fn entry_matches_bash_plan_field_names() {
        let entry = AttentionEntry {
            agent: "claude-code".into(),
            project: "p".into(),
            cwd: "/c".into(),
            event: Event::Done,
            tmux_pane: "%1".into(),
            ts: 1,
            message: None,
            pid: None,
        };
        let v: serde_json::Value = serde_json::to_value(&entry).unwrap();
        // Original fields from the bash precursor — must not be renamed/removed.
        assert!(v.get("project").is_some());
        assert!(v.get("cwd").is_some());
        assert!(v.get("event").is_some());
        assert!(v.get("tmux_pane").is_some());
        assert!(v.get("ts").is_some());
        // New attribution field added when this CLI grew multi-agent support.
        assert!(v.get("agent").is_some());
    }

    fn sample_entry(project: &str) -> AttentionEntry {
        AttentionEntry {
            agent: "claude-code".into(),
            project: project.into(),
            cwd: format!("/x/{project}"),
            event: Event::Notify,
            tmux_pane: "%1".into(),
            ts: 1,
            message: None,
            pid: None,
        }
    }

    #[test]
    fn write_then_list_returns_entry() {
        let dir = TempDir::new().unwrap();
        let store = StateStore::new(dir.path().into());
        store.write("session-a", &sample_entry("alpha")).unwrap();
        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].0, "session-a");
        assert_eq!(listed[0].1.project, "alpha");
    }

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

    #[test]
    fn list_on_missing_dir_returns_empty() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("does-not-exist");
        let store = StateStore::new(path);
        assert_eq!(store.list().unwrap().len(), 0);
    }

    #[test]
    fn list_skips_files_with_invalid_json() {
        let dir = TempDir::new().unwrap();
        let store = StateStore::new(dir.path().into());
        store.write("good", &sample_entry("p")).unwrap();
        std::fs::write(dir.path().join("bad"), "not json").unwrap();
        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].0, "good");
    }

    #[test]
    fn from_env_path_ends_with_agent_status() {
        let store = StateStore::from_env();
        assert!(store.dir().ends_with("agent-status"));
    }

    #[test]
    fn entry_message_field_roundtrips_when_set() {
        let entry = AttentionEntry {
            agent: "claude-code".into(),
            project: "p".into(),
            cwd: "/c".into(),
            event: Event::Notify,
            tmux_pane: "%1".into(),
            ts: 1,
            message: Some("Permission required".into()),
            pid: None,
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains(r#""message":"Permission required""#));
        let parsed: AttentionEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.message.as_deref(), Some("Permission required"));
    }

    #[test]
    fn entry_message_field_omitted_from_json_when_none() {
        let entry = AttentionEntry {
            agent: "claude-code".into(),
            project: "p".into(),
            cwd: "/c".into(),
            event: Event::Done,
            tmux_pane: "%1".into(),
            ts: 1,
            message: None,
            pid: None,
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(!json.contains("message"), "got: {json}");
    }

    #[test]
    fn entry_pid_field_roundtrips_when_set() {
        let entry = AttentionEntry {
            agent: "claude-code".into(),
            project: "p".into(),
            cwd: "/c".into(),
            event: Event::Notify,
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
            event: Event::Done,
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

    #[test]
    fn entry_deserializes_when_message_field_absent() {
        // Old state files written before this field was added must still load.
        let json = r#"{"agent":"claude-code","project":"p","cwd":"/c","event":"done","tmux_pane":"%1","ts":1}"#;
        let parsed: AttentionEntry = serde_json::from_str(json).unwrap();
        assert!(parsed.message.is_none());
    }

    #[test]
    fn write_rejects_path_traversal_session_id() {
        let dir = TempDir::new().unwrap();
        let store = StateStore::new(dir.path().into());
        let entry = sample_entry("p");
        for bad in ["../escape", "a/b", "..", ".", ""] {
            let err = store.write(bad, &entry).unwrap_err();
            assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput, "bad id: {bad:?}");
        }
        let err = store.remove("../escape").unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    }

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

    #[test]
    fn list_prunes_entries_with_dead_pid() {
        let dir = TempDir::new().unwrap();
        let store = StateStore::new(dir.path().into());

        let mut alive = sample_entry("alive");
        alive.pid = Some(std::process::id());
        store.write("session-alive", &alive).unwrap();

        let mut dead = sample_entry("dead");
        dead.pid = Some(1_000_000_000);
        store.write("session-dead", &dead).unwrap();

        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1, "should keep only the alive entry");
        assert_eq!(listed[0].0, "session-alive");

        assert!(!dir.path().join("session-dead").exists());
    }

    #[test]
    fn list_keeps_entries_without_pid() {
        let dir = TempDir::new().unwrap();
        let store = StateStore::new(dir.path().into());
        let no_pid_entry = sample_entry("legacy");
        store.write("session-legacy", &no_pid_entry).unwrap();

        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);
    }
}

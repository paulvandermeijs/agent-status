use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::PathBuf;

/// One entry stored per active agent session that is waiting on user attention.
///
/// Serialized as compact JSON to one file per session (keyed by `session_id`) under
/// `${XDG_RUNTIME_DIR:-/tmp}/agent-status/`. The field shape is wire-compatible with
/// the bash version of this tool.
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

    /// Remove the entry for `session_id`. Idempotent: returns `Ok(())` when the file is absent.
    ///
    /// # Errors
    /// Returns the underlying I/O error if removal fails for a reason other than `NotFound`.
    /// Returns [`io::ErrorKind::InvalidInput`] when `session_id` is empty or contains a
    /// path separator.
    pub fn remove(&self, session_id: &str) -> io::Result<()> {
        validate_session_id(session_id)?;
        match fs::remove_file(self.dir.join(session_id)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
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
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(bytes) = fs::read(entry.path()) else {
                continue;
            };
            if let Ok(parsed) = serde_json::from_slice::<AttentionEntry>(&bytes) {
                out.push((name, parsed));
            }
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
/// under a millisecond and fires only on `agent-status status`/`list`/`preview`.
#[allow(dead_code)] // wired into StateStore::list in the next commit
fn is_pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
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
            event: "notify".into(),
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
    fn entry_matches_bash_plan_field_names() {
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
            event: "notify".into(),
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
        store.remove("never-existed").unwrap();
        store.write("s1", &sample_entry("p")).unwrap();
        store.remove("s1").unwrap();
        store.remove("s1").unwrap();
        assert_eq!(store.list().unwrap().len(), 0);
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
            event: "notify".into(),
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
            event: "done".into(),
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
}

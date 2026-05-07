use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::PathBuf;

/// One entry stored per active Claude Code session that is waiting on user attention.
///
/// Serialized as compact JSON to one file per session (keyed by `session_id`) under
/// `${XDG_RUNTIME_DIR:-/tmp}/claude-status/`. The field shape is wire-compatible with
/// the bash version of this tool.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct AttentionEntry {
    /// Basename of the project directory (typically the cwd's last component).
    pub project: String,
    /// Absolute path of the project directory at the time the hook fired.
    pub cwd: String,
    /// Hook event label, for example `notify` or `done`.
    pub event: String,
    /// Tmux pane id (such as `%17`), or empty if the hook fired outside tmux.
    pub tmux_pane: String,
    /// Unix timestamp (seconds) when the entry was written.
    pub ts: u64,
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

    /// Construct a store under `${XDG_RUNTIME_DIR:-/tmp}/claude-status/`.
    pub fn from_env() -> Self {
        let base = std::env::var_os("XDG_RUNTIME_DIR")
            .map_or_else(|| PathBuf::from("/tmp"), PathBuf::from);
        Self::new(base.join("claude-status"))
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
            project: "claude-status".into(),
            cwd: "/Users/x/work/claude-status".into(),
            event: "notify".into(),
            tmux_pane: "%42".into(),
            ts: 1_700_000_000,
        };
        let json = serde_json::to_string(&entry).unwrap();
        let parsed: AttentionEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, entry);
    }

    #[test]
    fn entry_matches_bash_plan_field_names() {
        let entry = AttentionEntry {
            project: "p".into(),
            cwd: "/c".into(),
            event: "done".into(),
            tmux_pane: "%1".into(),
            ts: 1,
        };
        let v: serde_json::Value = serde_json::to_value(&entry).unwrap();
        assert!(v.get("project").is_some());
        assert!(v.get("cwd").is_some());
        assert!(v.get("event").is_some());
        assert!(v.get("tmux_pane").is_some());
        assert!(v.get("ts").is_some());
    }

    fn sample_entry(project: &str) -> AttentionEntry {
        AttentionEntry {
            project: project.into(),
            cwd: format!("/x/{project}"),
            event: "notify".into(),
            tmux_pane: "%1".into(),
            ts: 1,
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
    fn from_env_path_ends_with_claude_status() {
        let store = StateStore::from_env();
        assert!(store.dir().ends_with("claude-status"));
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
}

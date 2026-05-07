use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct AttentionEntry {
    pub project: String,
    pub cwd: String,
    pub event: String,
    pub tmux_pane: String,
    pub ts: u64,
}

pub struct StateStore {
    dir: PathBuf,
}

impl StateStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn from_env() -> Self {
        let base = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/tmp"));
        Self::new(base.join("claude-attention"))
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn write(&self, session_id: &str, entry: &AttentionEntry) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        let json = serde_json::to_vec(entry)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        fs::write(self.dir.join(session_id), json)
    }

    pub fn remove(&self, session_id: &str) -> io::Result<()> {
        match fs::remove_file(self.dir.join(session_id)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }

    pub fn list(&self) -> io::Result<Vec<(String, AttentionEntry)>> {
        if !self.dir.exists() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let bytes = match fs::read(entry.path()) {
                Ok(b) => b,
                Err(_) => continue,
            };
            if let Ok(parsed) = serde_json::from_slice::<AttentionEntry>(&bytes) {
                out.push((name, parsed));
            }
        }
        out.sort_by(|a, b| a.1.ts.cmp(&b.1.ts).then_with(|| a.0.cmp(&b.0)));
        Ok(out)
    }
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
    fn from_env_uses_xdg_runtime_dir() {
        let store = StateStore::new(std::path::PathBuf::from("/tmp/foo/claude-attention"));
        assert!(store.dir().ends_with("claude-attention"));
    }
}

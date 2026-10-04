//! The changes git shows: [`Changes`] for a project's working tree.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::domain::hook::Changes;

/// A project's working tree, from its root directory.
#[derive(Debug, Clone)]
pub struct Git {
    root: PathBuf,
}

impl Git {
    pub fn new(root: &Path) -> Self {
        Git {
            root: root.to_path_buf(),
        }
    }
}

impl Changes for Git {
    /// Whether `git status --porcelain` shows anything in `dir`.
    fn changed(&self, dir: &str) -> Result<bool, String> {
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(["status", "--porcelain", "--", dir])
            .output()
            .map_err(|e| format!("git could not run: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "git status failed in {}: {}",
                self.root.display(),
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Ok(!out.stdout.iter().all(u8::is_ascii_whitespace))
    }
}

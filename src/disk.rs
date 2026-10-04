//! The tree on the file system: [`Tree`] for a project's root directory.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::source::{relative_path, within};
use crate::tree::Tree;

/// A project's files, from its root directory.
#[derive(Debug, Clone)]
pub struct Disk {
    root: PathBuf,
}

impl Disk {
    pub fn new(root: &Path) -> Self {
        Disk {
            root: root.to_path_buf(),
        }
    }
}

impl Tree for Disk {
    fn read(&self, path: &str) -> io::Result<String> {
        fs::read_to_string(self.root.join(path))
    }

    fn entries(&self, dir: &str) -> io::Result<Vec<(String, bool)>> {
        fs::read_dir(self.root.join(dir))?
            .map(|entry| {
                let entry = entry?;
                // A link counts as what it points to, as the checks that follow it read it
                Ok((
                    entry.file_name().to_string_lossy().into_owned(),
                    entry.path().is_dir(),
                ))
            })
            .collect()
    }

    fn files(&self, dir: &str) -> io::Result<Vec<String>> {
        let root = &self.root;
        let walk = ignore::WalkBuilder::new(root)
            .require_git(false)
            .parents(false)
            .ignore(false)
            .git_global(false)
            .git_exclude(false)
            // From the root, so its `.gitignore` applies to `dir` too; into `dir` only
            .filter_entry({
                let root = root.clone();
                let dir = dir.to_string();
                move |entry| {
                    let path = relative_path(entry.path(), &root);
                    dir.is_empty() || path.is_empty() || within(&path, &dir) || within(&dir, &path)
                }
            })
            .build();
        let mut found = Vec::new();
        for entry in walk {
            let entry = entry.map_err(|e| io::Error::other(e.to_string()))?;
            if entry.file_type().is_some_and(|t| t.is_file()) {
                found.push(relative_path(entry.path(), root));
            }
        }
        Ok(found)
    }
}

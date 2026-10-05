//! The tree on the file system: [`Tree`] for a project's root directory.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use domain::layers::DECLARATION;
use domain::tree::{Tree, Writer};
use utils::paths::{relative_path, within};

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

    fn landed(&self, dir: &str, rel: &str) -> Option<String> {
        let real = fs::canonicalize(&self.root).ok()?;
        let full = fs::canonicalize(self.root.join(dir).join(rel)).ok()?;
        Some(relative_path(full.strip_prefix(real).ok()?, Path::new("")))
    }

    fn found(&self, path: &str) -> Option<bool> {
        let full = self.root.join(path);
        if full.is_dir() {
            Some(true)
        } else {
            full.exists().then_some(false)
        }
    }
}

impl Writer for Disk {
    fn write(&self, path: &str, text: &str) -> io::Result<()> {
        let full = self.root.join(path);
        if let Some(dir) = full.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(full, text)
    }

    fn make_dir(&self, dir: &str) -> io::Result<()> {
        fs::create_dir_all(self.root.join(dir))
    }
}

impl Disk {
    /// The project's name: the name of its root directory, which `--root .` gives only once resolved.
    pub fn name(&self) -> Result<String, String> {
        let full = self
            .root
            .canonicalize()
            .map_err(|e| format!("{}: {e}", self.root.display()))?;
        Ok(full.file_name().map_or_else(
            // The root of a drive has no name of its own
            || "project".to_string(),
            |name| name.to_string_lossy().into_owned(),
        ))
    }
}

/// The project `start` sits in: the nearest directory from it upwards that holds the declaration.
pub fn project_root(start: &Path) -> Option<PathBuf> {
    let start = start.canonicalize().ok()?;
    start
        .ancestors()
        .find(|dir| dir.join(DECLARATION).is_file())
        .map(Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_disk_lists_names_as_they_are_spelled() {
        // On Windows the operating system would open README.md as readme.md too; the listing has only its own name
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("docs/backlog")).unwrap();
        fs::write(dir.path().join("README.md"), "# Readme\n").unwrap();
        let disk = Disk::new(dir.path());
        let mut root = disk.entries("").unwrap();
        root.sort();
        assert_eq!(
            root,
            [("README.md".to_string(), false), ("docs".to_string(), true)]
        );
        assert_eq!(
            disk.entries("docs").unwrap(),
            [("backlog".to_string(), true)]
        );
        assert!(disk.entries("README.md").is_err());
    }

    #[test]
    fn what_is_written_is_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let disk = Disk::new(dir.path());
        disk.write("a/b/c.md", "text\n").unwrap();
        assert_eq!(disk.read("a/b/c.md").unwrap(), "text\n");
        assert_eq!(disk.found("a/b"), Some(true));
        assert_eq!(disk.found("a/b/c.md"), Some(false));
        assert_eq!(disk.found("a/x"), None);
    }
}

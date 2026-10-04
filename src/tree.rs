//! The tree a check reads: the port to a project's files, and the rules Rotproof keeps on how a path names a file.
//!
//! A check never asks the operating system itself: it takes a [`Tree`], which `disk.rs` implements on the file system
//! and the tests implement in memory. Paths are from the root, with `/`, and "" is the root.

use std::io;

/// The files of a project, as a check reads them.
pub trait Tree {
    /// The text of the file at `path`, as it is stored. `InvalidData` when it is not UTF-8.
    fn read(&self, path: &str) -> io::Result<String>;
    /// The entries of the directory `dir`: each name, and whether it is a directory. An error when `dir` cannot be
    /// read as a directory.
    fn entries(&self, dir: &str) -> io::Result<Vec<(String, bool)>>;
    /// Every file in `dir` and below, except the ones the project's own `.gitignore` files exclude and hidden ones.
    ///
    /// Only the `.gitignore` files inside the project count. A global excludes file, `.git/info/exclude` and a
    /// `.gitignore` above the root differ from one machine to another, and would hide code on one machine that fails on
    /// another.
    fn files(&self, dir: &str) -> io::Result<Vec<String>>;
    /// Where the path `rel`, named from the directory `dir`, lands as the operating system resolves it: from the root,
    /// or `None` when it lands nowhere or outside the root. On Windows, another case, dots and spaces at the end and
    /// short names land too; links are followed everywhere.
    fn landed(&self, dir: &str, rel: &str) -> Option<String>;
    /// Whether the operating system finds a file at `path`, as it reads names (on Windows, in any case).
    fn is_file(&self, path: &str) -> bool;
}

/// The text of the file at `path`, with every line ending as `\n`.
///
/// Line endings are normalized as Python's text mode does, which the original checks read with: a file checked out with
/// CRLF would otherwise keep `\r` at the end of each heading, and its anchors would differ.
pub fn read_text(tree: &dyn Tree, path: &str) -> io::Result<String> {
    Ok(tree.read(path)?.replace("\r\n", "\n").replace('\r', "\n"))
}

/// Read a code file for a check: `None`, with a finding that its `what` are not checked, when it is not UTF-8. An
/// error is a file that could not be read at all.
pub fn read_code(
    tree: &dyn Tree,
    path: &str,
    what: &str,
    found: &mut Vec<String>,
) -> io::Result<Option<String>> {
    match read_text(tree, path) {
        Ok(source) => Ok(Some(source)),
        Err(e) if e.kind() == io::ErrorKind::InvalidData => {
            found.push(format!(
                "{path}: cannot be read as UTF-8, so its {what} are not checked"
            ));
            Ok(None)
        }
        Err(e) => Err(io::Error::new(e.kind(), format!("{path}: {e}"))),
    }
}

/// What a path names in a tree, read name by name.
#[derive(Debug, PartialEq, Eq)]
pub enum Lookup {
    /// Every part is the exact name of an entry: the path from the root, and whether it is a directory
    Found {
        path: String,
        is_dir: bool,
    },
    /// A part names an entry only as Windows reads names: in another case, or with dots or spaces at its end. The
    /// path up to that part as the tree spells it
    Spelled(String),
    Missing,
}

/// The entry `rel` (parts separated by `/`, `.` and `..` allowed) names from the directory `start`, compared with the
/// names the directories hold, exactly. A path that climbs above the root is missing.
///
/// Asking the operating system would answer as the machine reads names: Windows finds `README.md` for `readme.md`,
/// `README.md.` and `README.md ` too, a stream such as `README.md:secret`, and a short name such as `README~1.MD`.
/// Linux and GitHub find none of them, so a link or a file that passes here would be missing there. Reading the
/// directories gives the same answer on every machine.
pub fn lookup(tree: &dyn Tree, start: &str, rel: &str) -> Lookup {
    let mut at: Vec<String> = start
        .split('/')
        .filter(|p| !p.is_empty())
        .map(String::from)
        .collect();
    let mut spelled: Vec<String> = Vec::new();
    let mut is_dir = true;
    for part in rel.split('/') {
        match part {
            "" | "." => continue,
            ".." => {
                if at.pop().is_none() {
                    return Lookup::Missing;
                }
                spelled.push(part.to_string());
                is_dir = true;
                continue;
            }
            _ => {}
        }
        let Ok(entries) = tree.entries(&at.join("/")) else {
            return Lookup::Missing;
        };
        if let Some((name, dir)) = entries.iter().find(|(name, _)| name == part) {
            at.push(name.clone());
            spelled.push(name.clone());
            is_dir = *dir;
            continue;
        }
        // How Windows reads a name: case does not count, and dots and spaces at the end are dropped
        let windows = |name: &str| name.trim_end_matches(['.', ' ']).to_lowercase();
        return match entries
            .iter()
            .find(|(name, _)| windows(name) == windows(part))
        {
            Some((name, _)) => {
                spelled.push(name.clone());
                Lookup::Spelled(spelled.join("/"))
            }
            None => Lookup::Missing,
        };
    }
    Lookup::Found {
        path: at.join("/"),
        is_dir,
    }
}

/// The path `rel` names from the root when it names an entry exactly, with whether it is a directory; or what to say
/// when it does not.
pub fn exactly(tree: &dyn Tree, rel: &str) -> Result<(String, bool), String> {
    match lookup(tree, "", rel) {
        Lookup::Found { path, is_dir } => Ok((path, is_dir)),
        Lookup::Spelled(on_disk) => Err(format!(
            "missing: {rel} ({on_disk} is there: names are compared exactly, as Linux and GitHub compare them)"
        )),
        Lookup::Missing => Err(format!("missing: {rel}")),
    }
}

/// Every file in the directory `dir` whose name `is_code` accepts, as [`Tree::files`] finds them. None when `dir` is
/// not a directory by its exact name.
pub fn code_files(
    tree: &dyn Tree,
    dir: &str,
    is_code: impl Fn(&str) -> bool,
) -> io::Result<Vec<String>> {
    if !exactly(tree, dir).is_ok_and(|(_, is_dir)| is_dir) {
        return Ok(Vec::new());
    }
    Ok(tree
        .files(dir)?
        .into_iter()
        .filter(|path| is_code(path.rsplit('/').next().unwrap_or(path)))
        .collect())
}

/// A tree in memory, for the tests: files by their path, every directory made by the paths of its files.
#[cfg(test)]
pub mod fake {
    use std::collections::{BTreeMap, BTreeSet};
    use std::io;

    use super::Tree;

    #[derive(Debug, Default)]
    pub struct Fake(pub BTreeMap<String, String>);

    impl Fake {
        pub fn new(files: &[(&str, &str)]) -> Self {
            Fake(
                files
                    .iter()
                    .map(|(path, text)| (path.to_string(), text.to_string()))
                    .collect(),
            )
        }
    }

    impl Tree for Fake {
        fn read(&self, path: &str) -> io::Result<String> {
            self.0
                .get(path)
                .cloned()
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, path.to_string()))
        }

        fn entries(&self, dir: &str) -> io::Result<Vec<(String, bool)>> {
            let prefix = if dir.is_empty() {
                String::new()
            } else {
                format!("{dir}/")
            };
            let mut found = BTreeSet::new();
            for path in self.0.keys() {
                if let Some(rest) = path.strip_prefix(&prefix) {
                    match rest.split_once('/') {
                        Some((name, _)) => found.insert((name.to_string(), true)),
                        None => found.insert((rest.to_string(), false)),
                    };
                }
            }
            if found.is_empty() {
                return Err(io::Error::new(io::ErrorKind::NotFound, dir.to_string()));
            }
            Ok(found.into_iter().collect())
        }

        fn files(&self, dir: &str) -> io::Result<Vec<String>> {
            Ok(self
                .0
                .keys()
                .filter(|path| dir.is_empty() || path.starts_with(&format!("{dir}/")))
                .cloned()
                .collect())
        }

        /// By the exact names only: the fake has no operating system to read names otherwise
        fn landed(&self, dir: &str, rel: &str) -> Option<String> {
            match super::lookup(self, dir, rel) {
                super::Lookup::Found { path, .. } => Some(path),
                _ => None,
            }
        }

        fn is_file(&self, path: &str) -> bool {
            self.0.contains_key(path)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::Fake;
    use super::*;

    #[test]
    fn a_path_is_found_only_by_its_exact_names() {
        let tree = Fake::new(&[("README.md", "# Readme\n"), ("docs/backlog/a.md", "")]);
        let found = |path: &str, is_dir| Lookup::Found {
            path: path.into(),
            is_dir,
        };
        assert_eq!(
            lookup(&tree, "docs/backlog", "../../README.md"),
            found("README.md", false)
        );
        assert_eq!(
            lookup(&tree, "", "./docs//backlog/"),
            found("docs/backlog", true)
        );
        assert_eq!(lookup(&tree, "", ""), found("", true));
        // Each of these opens README.md on Windows, and nothing on Linux or GitHub
        for (rel, on_disk) in [
            ("../../readme.md", "../../README.md"),
            ("../../README.md.", "../../README.md"),
            ("../../README.md ", "../../README.md"),
            ("../../Docs/backlog", "../../docs"),
            ("../../docs./backlog", "../../docs"),
        ] {
            assert_eq!(
                lookup(&tree, "docs/backlog", rel),
                Lookup::Spelled(on_disk.into()),
                "{rel}"
            );
        }
        for rel in [
            "../../README.md:secret",
            "../../README~1.MD",
            "CON",
            "../../nothing.md",
            "../../../README.md",
        ] {
            assert_eq!(lookup(&tree, "docs/backlog", rel), Lookup::Missing, "{rel}");
        }
        assert!(
            exactly(&tree, "readme.md")
                .unwrap_err()
                .contains("README.md is there")
        );
    }

    #[test]
    fn code_files_are_the_files_of_a_directory_by_its_exact_name() {
        let tree = Fake::new(&[
            ("domain/a.py", ""),
            ("domain/b.txt", ""),
            ("domains/c.py", ""),
        ]);
        let py = |name: &str| name.ends_with(".py");
        assert_eq!(code_files(&tree, "domain", py).unwrap(), ["domain/a.py"]);
        assert!(code_files(&tree, "Domain", py).unwrap().is_empty());
        assert!(code_files(&tree, "domain/a.py", py).unwrap().is_empty());
        assert_eq!(
            code_files(&tree, "", py).unwrap(),
            ["domain/a.py", "domains/c.py"]
        );
    }
}

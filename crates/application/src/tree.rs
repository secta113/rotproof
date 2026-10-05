//! Reading a project's files through the [`Tree`] port: a path read name by name, the code files of a directory, and
//! the text of a file as the checks read it.

use std::io;

use domain::tree::{Tree, as_windows_reads, with_lf};
use utils::paths::{file_name, join, parent};

/// The text of the file at `path`, as Rotproof reads it ([`with_lf`]).
pub fn read_text(tree: &dyn Tree, path: &str) -> io::Result<String> {
    Ok(with_lf(&tree.read(path)?))
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
        return match entries
            .iter()
            .find(|(name, _)| as_windows_reads(name) == as_windows_reads(part))
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

/// Where `path` (from the root) lands as the operating system resolves it ([`Tree::landed`]): the longest beginning of
/// it that lands inside the root, with the rest as written, so an import that names a module without its extension, or
/// a name inside a file, lands by the directory it is in. On Windows, a directory in another case, with dots or spaces
/// at its end or with a short name lands where it is; links are followed everywhere. The path as written when no
/// beginning of it lands.
pub fn resolved(tree: &dyn Tree, path: &str) -> String {
    let mut at = path;
    let mut rest: Vec<&str> = Vec::new();
    loop {
        if let Some(real) = tree.landed("", at) {
            return rest.iter().rev().fold(real, |dir, name| join(&dir, name));
        }
        if at.is_empty() {
            return path.to_string();
        }
        rest.push(file_name(at));
        at = parent(at);
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
        .filter(|path| is_code(file_name(path)))
        .collect())
}

/// A tree in memory, for the tests: files by their path, and the directories their paths make or
/// [`Writer`](domain::tree::Writer) made.
#[cfg(test)]
pub mod fake {
    use std::cell::RefCell;
    use std::collections::{BTreeMap, BTreeSet};
    use std::io;

    use domain::tree::{Tree, Writer};

    #[derive(Debug, Default)]
    pub struct Fake {
        files: RefCell<BTreeMap<String, String>>,
        dirs: RefCell<BTreeSet<String>>,
    }

    impl Fake {
        pub fn new(files: &[(&str, &str)]) -> Self {
            let fake = Fake::default();
            for (path, text) in files {
                fake.write(path, text).unwrap();
            }
            fake
        }

        /// The text of a file, for a test to compare
        pub fn text(&self, path: &str) -> Option<String> {
            self.files.borrow().get(path).cloned()
        }
    }

    impl Tree for Fake {
        fn read(&self, path: &str) -> io::Result<String> {
            self.text(path)
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, path.to_string()))
        }

        fn entries(&self, dir: &str) -> io::Result<Vec<(String, bool)>> {
            if !dir.is_empty() && !self.dirs.borrow().contains(dir) {
                return Err(io::Error::new(io::ErrorKind::NotFound, dir.to_string()));
            }
            let prefix = if dir.is_empty() {
                String::new()
            } else {
                format!("{dir}/")
            };
            let files = self.files.borrow();
            let dirs = self.dirs.borrow();
            let mut found = BTreeSet::new();
            for (path, is_dir) in files
                .keys()
                .map(|p| (p, false))
                .chain(dirs.iter().map(|d| (d, true)))
            {
                if let Some(rest) = path.strip_prefix(&prefix) {
                    match rest.split_once('/') {
                        Some((name, _)) => found.insert((name.to_string(), true)),
                        None => found.insert((rest.to_string(), is_dir)),
                    };
                }
            }
            Ok(found.into_iter().collect())
        }

        fn files(&self, dir: &str) -> io::Result<Vec<String>> {
            Ok(self
                .files
                .borrow()
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

        fn found(&self, path: &str) -> Option<bool> {
            if self.files.borrow().contains_key(path) {
                Some(false)
            } else {
                self.dirs.borrow().contains(path).then_some(true)
            }
        }
    }

    impl Writer for Fake {
        fn write(&self, path: &str, text: &str) -> io::Result<()> {
            if let Some((dir, _)) = path.rsplit_once('/') {
                self.make_dir(dir)?;
            }
            self.files
                .borrow_mut()
                .insert(path.to_string(), text.to_string());
            Ok(())
        }

        fn make_dir(&self, dir: &str) -> io::Result<()> {
            let mut at = String::new();
            for part in dir.split('/') {
                if !at.is_empty() {
                    at.push('/');
                }
                at.push_str(part);
                self.dirs.borrow_mut().insert(at.clone());
            }
            Ok(())
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
    fn line_endings_become_lf() {
        let tree = Fake::new(&[("a.md", "# A\r\nb\rc\n")]);
        assert_eq!(read_text(&tree, "a.md").unwrap(), "# A\nb\nc\n");
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

    #[test]
    fn a_path_on_the_disk_is_found_only_by_its_exact_names() {
        // On Windows the operating system would open README.md by each of these; the listing has only its own name
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("docs/backlog")).unwrap();
        std::fs::write(dir.path().join("README.md"), "# Readme\n").unwrap();
        let disk = infrastructure::disk::Disk::new(dir.path());
        assert_eq!(
            lookup(&disk, "docs/backlog", "../../README.md"),
            Lookup::Found {
                path: "README.md".into(),
                is_dir: false
            }
        );
        for (rel, on_disk) in [
            ("../../readme.md", "../../README.md"),
            ("../../README.md.", "../../README.md"),
            ("../../Docs/backlog", "../../docs"),
        ] {
            assert_eq!(
                lookup(&disk, "docs/backlog", rel),
                Lookup::Spelled(on_disk.into()),
                "{rel}"
            );
        }
        for rel in ["../../README.md:secret", "../../README~1.MD", "CON"] {
            assert_eq!(lookup(&disk, "docs/backlog", rel), Lookup::Missing, "{rel}");
        }
    }
}

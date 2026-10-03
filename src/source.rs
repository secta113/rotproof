//! Reading the files the checks look at: finding the code files in a tree, reading a file, and the paths and lines in
//! messages.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Read a file as UTF-8, with every line ending as `\n`.
///
/// Line endings are normalized as Python's text mode does, which the original checks read with: a file checked out with
/// CRLF would otherwise keep `\r` at the end of each heading, and its anchors would differ.
pub fn read_source(path: &Path) -> io::Result<String> {
    Ok(fs::read_to_string(path)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}

/// Read a code file, `path` from the root, for a check: `None`, with a finding that its `what` are not checked, when it
/// is not UTF-8. An error is a file that could not be read at all.
pub fn read_code(
    root: &Path,
    path: &str,
    what: &str,
    found: &mut Vec<String>,
) -> io::Result<Option<String>> {
    match read_source(&root.join(path)) {
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

/// The line (from 1) at a byte offset.
pub fn line_of(source: &str, at: usize) -> usize {
    source.as_bytes()[..at.min(source.len())]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
        + 1
}

/// Whether `path` is `prefix` or inside it. Both are from the root, with `/`.
pub fn within(path: &str, prefix: &str) -> bool {
    path == prefix || path.starts_with(&format!("{prefix}/"))
}

/// Every file in `dir` (from the root, "" for the root) whose name `is_code` accepts, from the root with `/`. Files the
/// project's `.gitignore` files exclude, and hidden ones, are not looked at.
///
/// Only the `.gitignore` files inside the project count. A global excludes file, `.git/info/exclude` and a
/// `.gitignore` above the root differ from one machine to another, and would hide code on one machine that fails on
/// another.
pub fn code_files(
    root: &Path,
    dir: &str,
    is_code: impl Fn(&str) -> bool,
) -> io::Result<Vec<String>> {
    let mut found = Vec::new();
    if !exactly(root, dir).is_ok_and(|path| path.is_dir()) {
        return Ok(found);
    }
    let walk = ignore::WalkBuilder::new(root)
        .require_git(false)
        .parents(false)
        .ignore(false)
        .git_global(false)
        .git_exclude(false)
        // From the root, so its `.gitignore` applies to `dir` too; into `dir` only
        .filter_entry({
            let root = root.to_path_buf();
            let dir = dir.to_string();
            move |entry| {
                let path = relative_path(entry.path(), &root);
                dir.is_empty() || path.is_empty() || within(&path, &dir) || within(&dir, &path)
            }
        })
        .build();
    for entry in walk {
        let entry = entry.map_err(|e| io::Error::other(e.to_string()))?;
        if entry.file_type().is_some_and(|t| t.is_file())
            && is_code(&entry.file_name().to_string_lossy())
        {
            found.push(relative_path(entry.path(), root));
        }
    }
    Ok(found)
}

/// The path from the root, for messages: with `/` on every platform, so the output is the same on Windows.
pub fn relative_path(path: &Path, root: &Path) -> String {
    let path = path.strip_prefix(root).unwrap_or(path);
    path.components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// What a path names on disk, read name by name.
#[derive(Debug, PartialEq, Eq)]
pub enum Lookup {
    /// Every part is the exact name of an entry: the path, from where the lookup started
    Found(PathBuf),
    /// A part names an entry only as Windows reads names: in another case, or with dots or spaces at its end. The
    /// path up to that part as the disk spells it
    Spelled(String),
    Missing,
}

/// The entry `rel` (parts separated by `/`, `.` and `..` allowed) names from `start`, compared with the names the
/// directories hold, exactly.
///
/// Asking the operating system would answer as the machine reads names: Windows finds `README.md` for `readme.md`,
/// `README.md.` and `README.md ` too, a stream such as `README.md:secret`, and a short name such as `README~1.MD`.
/// Linux and GitHub find none of them, so a link or a file that passes here would be missing there. Reading the
/// directories gives the same answer on every machine.
pub fn lookup(start: &Path, rel: &str) -> Lookup {
    let mut at = start.to_path_buf();
    let mut spelled: Vec<String> = Vec::new();
    for part in rel.split('/') {
        match part {
            "" | "." => continue,
            ".." => {
                if !at.pop() {
                    return Lookup::Missing;
                }
                spelled.push(part.to_string());
                continue;
            }
            _ => {}
        }
        let Ok(entries) = fs::read_dir(&at) else {
            return Lookup::Missing;
        };
        let names: Vec<String> = entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        if names.iter().any(|name| name == part) {
            at.push(part);
            spelled.push(part.to_string());
            continue;
        }
        // How Windows reads a name: case does not count, and dots and spaces at the end are dropped
        let windows = |name: &str| name.trim_end_matches(['.', ' ']).to_lowercase();
        return match names.iter().find(|name| windows(name) == windows(part)) {
            Some(name) => {
                spelled.push(name.clone());
                Lookup::Spelled(spelled.join("/"))
            }
            None => Lookup::Missing,
        };
    }
    Lookup::Found(at)
}

/// Whether `rel` (from `root`, with `/`) names an entry exactly, or `Err` with what to say when it does not.
pub fn exactly(root: &Path, rel: &str) -> Result<PathBuf, String> {
    match lookup(root, rel) {
        Lookup::Found(path) => Ok(path),
        Lookup::Spelled(on_disk) => Err(format!(
            "missing: {rel} ({on_disk} is there: names are compared exactly, as Linux and GitHub compare them)"
        )),
        Lookup::Missing => Err(format!("missing: {rel}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_is_found_only_by_its_exact_names() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("docs/backlog")).unwrap();
        fs::write(root.join("README.md"), "# Readme\n").unwrap();
        let here = root.join("docs/backlog");
        assert_eq!(
            lookup(&here, "../../README.md"),
            Lookup::Found(root.join("README.md"))
        );
        assert_eq!(
            lookup(root, "./docs//backlog/"),
            Lookup::Found(root.join("docs/backlog"))
        );
        // Each of these opens README.md on Windows, and nothing on Linux or GitHub
        for (rel, on_disk) in [
            ("../../readme.md", "../../README.md"),
            ("../../README.md.", "../../README.md"),
            ("../../README.md ", "../../README.md"),
            ("../../Docs/backlog", "../../docs"),
            ("../../docs./backlog", "../../docs"),
        ] {
            assert_eq!(lookup(&here, rel), Lookup::Spelled(on_disk.into()), "{rel}");
        }
        for rel in [
            "../../README.md:secret",
            "../../README~1.MD",
            "CON",
            "../../nothing.md",
        ] {
            assert_eq!(lookup(&here, rel), Lookup::Missing, "{rel}");
        }
        assert!(
            exactly(root, "readme.md")
                .unwrap_err()
                .contains("README.md is there")
        );
    }

    #[test]
    fn within_compares_whole_names() {
        assert!(within("domain", "domain"));
        assert!(within("domain/x.py", "domain"));
        assert!(!within("domains/x.py", "domain"));
        assert!(!within("domain", "domain/x.py"));
    }

    #[test]
    fn a_relative_path_uses_slashes() {
        let root = Path::new("repo");
        assert_eq!(
            relative_path(&root.join("docs").join("backlog").join("index.md"), root),
            "docs/backlog/index.md"
        );
    }

    #[test]
    fn line_endings_become_lf() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        fs::write(&path, "# A\r\nb\rc\n").unwrap();
        assert_eq!(read_source(&path).unwrap(), "# A\nb\nc\n");
    }
}

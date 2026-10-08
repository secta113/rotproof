//! The tree a check reads: the ports to a project's files, and the rules Rotproof keeps on how a file's text and name
//! are read.
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
    /// What the operating system finds at `path`, as it reads names (on Windows, in any case): `Some(true)` for a
    /// directory, `Some(false)` for a file, `None` for nothing.
    fn found(&self, path: &str) -> Option<bool>;
}

/// The files a command writes into a project.
pub trait Writer {
    /// Write `text` to the file at `path`, making the directories it sits in.
    fn write(&self, path: &str, text: &str) -> io::Result<()>;
    /// Make the directory `dir`, and the ones it sits in.
    fn make_dir(&self, dir: &str) -> io::Result<()>;
    /// Remove the file at `path`. Only `rotproof init` removes, when it moves a project's records.
    fn remove(&self, path: &str) -> io::Result<()>;
    /// Remove the directory `dir`, which is empty: an error when it is not, so nothing is removed unseen.
    fn remove_dir(&self, dir: &str) -> io::Result<()>;
}

/// The text of a file as Rotproof reads it: every line ending as `\n`, and no byte order mark.
///
/// Line endings are normalized as Python's text mode does, which the original checks read with: a file checked out with
/// CRLF would otherwise keep `\r` at the end of each heading, and its anchors would differ. A byte order mark, which
/// Windows tools write before UTF-8 (PowerShell 5.1's `-Encoding UTF8`), is dropped as Python's `utf-8-sig` drops it:
/// a record would otherwise not start with `---`, and fail as one with no frontmatter.
pub fn with_lf(text: &str) -> String {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// A name as Windows reads it: case does not count, and dots and spaces at the end are dropped. Two names that read
/// the same open the same entry on Windows, and different ones on Linux and GitHub.
pub fn as_windows_reads(name: &str) -> String {
    name.trim_end_matches(['.', ' ']).to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_endings_become_lf() {
        assert_eq!(with_lf("# A\r\nb\rc\n"), "# A\nb\nc\n");
    }

    #[test]
    fn a_byte_order_mark_at_the_start_is_dropped() {
        assert_eq!(with_lf("\u{feff}---\r\n"), "---\n");
        // Anywhere else it is text
        assert_eq!(with_lf("a\u{feff}"), "a\u{feff}");
    }

    #[test]
    fn windows_reads_a_name_in_any_case_and_without_dots_or_spaces_at_its_end() {
        for name in ["readme.md", "README.md.", "README.md ", "Readme.MD. ."] {
            assert_eq!(
                as_windows_reads(name),
                as_windows_reads("README.md"),
                "{name}"
            );
        }
        assert_ne!(
            as_windows_reads("README.md:secret"),
            as_windows_reads("README.md")
        );
    }
}

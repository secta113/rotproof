//! Output ports: the interfaces to the outside that `application` calls and `infrastructure.outbound` implements.
//! Each has only the methods `application` calls.
//!
//! Paths are from the root, with `/`, and "" is the root. A check never asks the operating system itself: it takes a
//! [`Tree`], which the disk implements on the file system and the tests implement in memory.

use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};

use domain::code::{Aliases, Manifest, Source};
use domain::schema::Time;

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

/// The readers of code: the port the checks read code through.
pub trait Parsers {
    /// A Python file, whose path from the root names the package its relative imports start from: every module it
    /// imports, as parts.
    fn python(&self, source: &str, path: &str) -> Source<Vec<String>>;
    /// The names of the functions and classes a Python file defines, at any depth.
    fn python_definitions(&self, source: &str) -> BTreeSet<String>;
    /// The source text of the definition `dotted` names in a Python file (`f`, `Class`, `Class.method`), from its
    /// first decorator to its end: `None` when the file does not define it so.
    fn python_definition(&self, source: &str, dotted: &str) -> Option<String>;
    /// A TypeScript or JavaScript file, whose path from the root says how to read it: every specifier it imports.
    fn typescript(&self, source: &str, path: &str) -> Source<String>;
    /// The aliases of the TypeScript project in `tree`, from its `tsconfig*.json` files. An error is a config that
    /// cannot be read at all.
    fn typescript_aliases(&self, tree: &dyn Tree) -> io::Result<Aliases>;
    /// A crate's `Cargo.toml`, or `Err` with the line of the first error and what the parser says.
    fn manifest(&self, source: &str) -> Result<Manifest, (usize, String)>;
}

/// The changes a project's version control shows: the port the hook asks whether anything was recorded.
pub trait Changes {
    /// Whether anything in `dir` (from the root) is changed, staged or new.
    fn changed(&self, dir: &str) -> Result<bool, String>;
}

/// A project's files, read and written: what a command that writes takes.
pub trait Files: Tree + Writer {
    /// The name of the project's root directory, which the files Rotproof writes once are named after.
    fn name(&self) -> Result<String, String>;
}

/// The machine a command runs on: the project at a root, the readers of code, version control and the clock.
pub trait Machine {
    /// The files of the project at `root`.
    fn files(&self, root: &Path) -> Box<dyn Files>;
    /// The readers of code.
    fn parsers(&self) -> &dyn Parsers;
    /// git's `user.name` in the repository at `root`, when it is set.
    fn user_name(&self, root: &Path) -> Option<String>;
    /// The changes of the repository that holds `root`, or `None` when no repository holds it.
    fn changes(&self, root: &Path) -> Option<Box<dyn Changes>>;
    /// The project that holds `start`: the nearest directory upwards with a declaration, or `None` outside one.
    fn project(&self, start: &Path) -> Option<PathBuf>;
    /// The time now, with the machine's offset from UTC.
    fn now(&self) -> Time;
}

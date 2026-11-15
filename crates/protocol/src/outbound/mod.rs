//! Output ports: the interfaces to the outside that `application` calls and `infrastructure.outbound` implements.
//! Each has only the methods `application` calls.
//!
//! Paths are from the root, with `/`, and "" is the root. A check never asks the operating system itself: it takes a
//! [`Tree`], which the disk implements on the file system and the tests implement in memory.

use std::io;
use std::path::Path;

use domain::code::{Aliases, Import, Manifest, Source};
use domain::definitions::Definition;
use domain::dependencies::Listing;
use domain::layers::Language;
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
    ///
    /// The paths, from the root with `/`, come in the byte order of the whole path (as `String` compares), whatever
    /// order the file system lists them in, so what a check finds file by file is printed in the same order on every
    /// machine. A check relies on this and does not sort again.
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
    /// Remove the file at `path`. Only `rotproof init` removes, when it moves a project's records, and `rotproof index`
    /// and `rotproof create`, the list of who reads the shared units once no unit is shared.
    fn remove(&self, path: &str) -> io::Result<()>;
    /// Remove the directory `dir`, which is empty: an error when it is not, so nothing is removed unseen.
    fn remove_dir(&self, dir: &str) -> io::Result<()>;
}

/// The readers of code: the port the checks read code through.
pub trait Parsers {
    /// A Python file, whose path from the root names the package its relative imports start from: every module it
    /// imports, as parts.
    fn python(&self, source: &str, path: &str) -> Source<Vec<String>>;
    /// The outline of a file of `language`, whose path from the root is `path`: every definition in it, each with its
    /// names from the outermost and the range of its source, from its first decorator or attribute to its end.
    /// Which key names a definition, and how the pieces of a name are joined, is the rule of
    /// `domain::definitions::selected`, the same for every language. `Err` with the line of the first error and
    /// what the parser says when the file cannot be parsed far enough to read any definition in it.
    fn outline(
        &self,
        language: Language,
        source: &str,
        path: &str,
    ) -> Result<Vec<Definition>, (usize, String)>;
    /// A TypeScript or JavaScript file, whose path from the root says how to read it: every specifier it imports, with
    /// the names it takes from each.
    fn typescript(&self, source: &str, path: &str) -> Source<Import>;
    /// The aliases of the TypeScript project in `tree`, from its `tsconfig*.json` files, and the packages of its units,
    /// from the `package.json` in each of `roots` (directories from the root). An error is a file that cannot be read
    /// at all.
    fn typescript_aliases(&self, tree: &dyn Tree, roots: &[String]) -> io::Result<Aliases>;
    /// A crate's `Cargo.toml`, or `Err` with the line of the first error and what the parser says.
    fn manifest(&self, source: &str) -> Result<Manifest, (usize, String)>;
    /// What the manifest of a package of `language` lists as dependencies, and its package's name: a `pyproject.toml`,
    /// a `package.json` or a `Cargo.toml`. `Err` with the line of the first error and what the parser says.
    fn listing(&self, language: Language, source: &str) -> Result<Listing, (usize, String)>;
}

/// A project's files, read and written: what a command that writes takes.
pub trait Files: Tree + Writer {
    /// The name of the project's root directory, which the files Rotproof writes once are named after.
    fn name(&self) -> Result<String, String>;
}

/// The machine a command runs on: the project at a root, the readers of code, git's `user.name` and the clock.
pub trait Machine {
    /// The files of the project at `root`.
    fn files(&self, root: &Path) -> Box<dyn Files>;
    /// The readers of code.
    fn parsers(&self) -> &dyn Parsers;
    /// git's `user.name` in the repository at `root`, when it is set.
    fn user_name(&self, root: &Path) -> Option<String>;
    /// The time now, with the machine's offset from UTC.
    fn now(&self) -> Time;
}

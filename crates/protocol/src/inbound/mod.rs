//! Input ports: what an entry point asks of Rotproof, one method per command, with the data each takes and returns.
//! That data is plain (text, numbers and lists), with no `domain` type, so an entry point knows nothing of `domain`.

use std::borrow::Cow;
use std::path::Path;

/// What an entry point names in its help and its messages, as the rules give them.
#[derive(Debug, Clone)]
pub struct Names {
    /// The running version of Rotproof
    pub version: &'static str,
    /// Where a project declares its structure, from the root
    pub declaration: &'static str,
    /// Where the approvals of forbidden imports are, from the root
    pub approvals: &'static str,
    /// The stack of a repository that keeps records only
    pub records_only: &'static str,
    /// The words the marker check fails on, joined as a sentence ("A, B or C")
    pub markers: String,
}

/// What `rotproof create` did.
#[derive(Debug, Default)]
pub struct Made {
    /// The files written, from the root, with `/`: made new or rewritten with a change
    pub written: Vec<String>,
    /// The files taken away, from the root: the list of who reads the shared units, once no unit is shared
    pub removed: Vec<String>,
    /// Documents left out of the index files: name -> why
    pub left_out: Vec<(String, String)>,
    /// The fields added to the declaration, as `name = value`
    pub added: Vec<String>,
    /// What was changed in a file that was there, from the root: the one change
    /// `rotproof create` makes to a file it wrote once (the members of a Rust workspace), as a line to tell
    pub changed: Vec<(String, String)>,
    /// What the stack does not have Rotproof write yet, and why
    pub not_written: Option<String>,
    /// The question the project has not answered, whether its agent reads Rotproof's guide, as lines to print: empty
    /// once it is answered
    pub question: Vec<String>,
}

/// What `rotproof init` did.
#[derive(Debug)]
pub enum Initialized {
    /// The declaration was written, at this path
    Declared(&'static str),
    /// Boxed: it holds what `rotproof create` made too, and is far the larger of the two
    Upgraded(Box<Upgraded>),
}

/// What an upgrade of the project's files did.
#[derive(Debug, Default)]
pub struct Upgraded {
    /// The version the project's files were up to
    pub from: String,
    /// Each update applied, with the file it changed
    pub applied: Vec<(&'static str, &'static str)>,
    /// Each field of the declaration renamed, as `old to new`: a declaration written before a field had its name keeps
    /// its value under the new one
    pub renamed: Vec<String>,
    /// What each update that could not be applied leaves to a person
    pub by_hand: Vec<String>,
    /// How to move the project's code where the new version's layers want it, which no machine can do: said once,
    /// and it keeps nothing behind (`rotproof check` names what is not moved)
    pub moves: Vec<String>,
    /// What `rotproof create` would have done after the upgrade, without a layer; its `added` also holds the fields
    /// added with `files` (`files` itself when the declaration lacked it, and `declined`)
    pub made: Made,
    /// Whether `files` was set to the running version
    pub files_set: bool,
    /// The knowledge document of the units, from the root, written as a draft: the project declares units, and no
    /// document follows them
    pub drafted: Option<String>,
    /// Why that document was not drafted, when it would have been: it cannot be written without an area to tag it
    /// with, or the units cannot be followed
    pub not_drafted: Option<String>,
    /// What is out of date in the map of the project's own `AGENTS.md` against the layers the declaration has: a layer
    /// with no row, a row for a layer declared absent. Said on every run until it is fixed, and it fails nothing: the
    /// file is the project's
    pub stale_map: Vec<String>,
}

/// One broken rule: which check found it, and what is wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The heading the finding is printed under. Fixed for most checks; built for those that name what they found
    pub check: Cow<'static, str>,
    pub detail: String,
}

/// One warning: which check raised it, and what it says. A warning is printed on every run and fails nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    /// The heading the warning is printed under, without the count and the words that say it fails nothing, which
    /// the entry point adds
    pub check: Cow<'static, str>,
    pub detail: String,
}

/// A comment that keeps the first marker of the marker check with the reference of the issue that tracks the work, in a
/// project that declares `todo_link`. It is no finding: it is listed on every run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedTodo {
    /// The file, from the root
    pub path: String,
    /// Counted from 1
    pub line: usize,
    /// The reference as written: `#123` for a number, `ABC-123` for a key
    pub reference: String,
}

/// A file to move next, and the layers it could go in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovingFile {
    /// From the root, with `/`
    pub path: String,
    /// The layers and levels in which the direction check would pass it and, in Python and TypeScript, the check of
    /// the I/O of the inner layers would too, innermost first, as a finding names a place; none when empty. `None`:
    /// every one there is, as nothing it imports, does, or is imported by says against any
    pub candidates: Option<Vec<String>>,
}

/// What `rotproof check` says of the code in the paths the declaration lists in `moving`, which no check judges: how
/// much there is, and which files to move first. It is no finding, and fails nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Moving {
    /// How many files are moving
    pub total: usize,
    /// Whether the files were put in an order. False for a language whose files' imports are not read: they are
    /// counted, and none is named
    pub ordered: bool,
    /// The files to move next, in groups: a file that imports no other file still moving alone, or the files that
    /// import each other, which are moved together
    pub next: Vec<Vec<MovingFile>>,
    /// How many files wait for no other file still moving and are not named in `next`
    pub more: usize,
}

/// How `rotproof check` read the references between templates, in a project whose views are templates. It is no
/// finding: it is said on every run, so that a reader knows what the check assumed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Templates {
    /// The template languages and the versions the references are read as: "Jinja2 3 and Django 5/6"
    pub read_as: String,
    /// One line for each unit whose references are not judged, or not read as a language, and why
    pub not_judged: Vec<String>,
}

/// How many files `rotproof check` read as tests, in a stack that tells a test by its name. It is no finding: it is
/// said on every run, so that a reader sees what the names matched, and notices when they matched nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestFiles {
    /// How many source files are read as tests, outside `unchecked` and `moving`
    pub count: usize,
    /// What makes a file a test, as the layout writes it: `*.test.*`, `__tests__/`
    pub patterns: Vec<String>,
}

/// What `rotproof check` found.
#[derive(Debug, Default)]
pub struct Report {
    pub findings: Vec<Finding>,
    /// The check that did not run, and why: the structure check says it of the layers of a repository of records only
    pub skipped: Option<String>,
    /// Whether the tree was checked against a stack's layers (not for `stack = "none"`)
    pub layers_checked: bool,
    /// Every forbidden import that was approved, as printed on every run: one line per entry that approves something
    pub approved: Vec<String>,
    /// Every comment that keeps the first marker with the reference of an issue, sorted by path and then by
    /// line: printed on every run apart from the findings and the warnings, and failing nothing. Empty unless the
    /// declaration has `todo_link`
    pub linked: Vec<LinkedTodo>,
    /// The code that is not in a layer yet, printed on every run apart from the findings and the warnings. `None`
    /// unless the declaration lists a path in `moving`
    pub moving: Option<Moving>,
    /// Every import from the code of an inner layer (domain, protocol, application, presentation) into a path of
    /// `unchecked` or `moving`, which no check judges, one line each as `file:line imports <import>, in a path of
    /// <field>`, in the order of the file and the line: said on every run apart from the findings and the warnings,
    /// failing nothing. Empty when there are none
    pub outside: Vec<String>,
    /// How the references between templates were read, said on every run apart from the findings and the warnings.
    /// `None` unless the project declares a UI whose views are templates
    pub templates: Option<Templates>,
    /// How many files were read as tests, said on every run apart from the findings and the warnings. `None` unless
    /// the stack tells a test by its name (TypeScript)
    pub tests: Option<TestFiles>,
    /// The methods the net of method names leaves out because the project defines one of the same name and number of
    /// parameters, as one line said on every run apart from the findings and the warnings, failing nothing. `None` when
    /// the project defines none that the net judges
    pub io_net: Option<String>,
    /// Every warning, in the order it is printed, each kind under its own heading: a knowledge document whose
    /// `stale_after` has come, with what to do, and a comment that may make the type checker check less
    /// (`# pyright: basic`), as `path:line` and the line under it
    pub warnings: Vec<Warning>,
}

/// What `rotproof index` did.
#[derive(Debug, Default)]
pub struct Indexed {
    /// The files written, from the root
    pub written: Vec<String>,
    /// The files taken away, from the root: the list of who reads the shared units, once no unit is shared
    pub removed: Vec<String>,
    /// Documents left out of the index files: name -> why
    pub left_out: Vec<(String, String)>,
}

/// One key of a document's `follows` whose code is not as pinned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drift {
    /// The document's file name in `docs/knowledge/`
    pub doc: String,
    pub key: String,
    pub pinned: String,
    /// Its hash now: `None` when it is gone. `Err` when it cannot be read
    pub now: Result<Option<String>, String>,
}

/// What `rotproof follows` pinned in a knowledge document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pinned {
    /// The document's file name in `docs/knowledge/`
    pub doc: String,
    /// Each key: its hash as pinned before (`None` when it was added), and as the code is now
    pub keys: Vec<(String, Option<String>, String)>,
    /// Whether the document was written: not when every key was pinned as it is now
    pub written: bool,
}

/// A new knowledge document, as `rotproof new` is asked for it.
#[derive(Debug, Clone, Default)]
pub struct NewDocument {
    /// The file's name in `docs/knowledge/` without `.md`
    pub slug: String,
    /// The area it is tagged with. `None`: the only area the declaration lists
    pub area: Option<String>,
    /// What it follows, each key as `rotproof follows` takes it, pinned at its hash now
    pub follows: Vec<String>,
    /// Whether it is an alarm, which gets `# When` and `# Do`
    pub alarm: bool,
    /// The strings of an alarm's `match`
    pub matches: Vec<String>,
}

/// What `rotproof new` wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Started {
    /// The document, from the root, with `/`
    pub path: String,
    /// The area it is tagged with
    pub area: String,
    /// Each key it follows, with the hash it was pinned at
    pub follows: Vec<(String, String)>,
}

/// One entry of the log, as `rotproof log` is given it.
#[derive(Debug, Clone, Default)]
pub struct LogEntry {
    /// The task
    pub title: String,
    /// The task's issue, `owner/repo#n`, when it has one
    pub issue: Option<String>,
    /// What went wrong, the cause and the workaround; `None` when nothing did. At least one
    pub problems: Vec<String>,
    /// What to do differently next time, and where it was recorded if it became a rule. At least one
    pub lessons: Vec<String>,
}

/// What `rotproof log` wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Logged {
    /// The log, from the root, with `/`
    pub path: String,
    /// The day the entry is under, as the clock gives it: `YYYY-MM-DD`
    pub day: String,
}

/// An approval `rotproof approve --prune` removed: what it kept, and in which file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Removed {
    pub import: String,
    pub from: String,
}

/// Every command of Rotproof, on the project at `root`. `Err` is what to say when a file cannot be read or written,
/// or a command cannot do its work.
pub trait Commands {
    /// What the entry point names in its help and its messages.
    fn names(&self) -> Names;
    /// Write the declaration for `stack` and `ui`, or upgrade the project's files to the running version.
    fn init(
        &self,
        root: &Path,
        stack: Option<&str>,
        ui: Option<&str>,
    ) -> Result<Initialized, String>;
    /// Make what the declaration names and the tree lacks; layers only when `yes`.
    fn create(&self, root: &Path, yes: bool) -> Result<Made, String>;
    /// Every broken rule.
    fn check(&self, root: &Path) -> Result<Report, String>;
    /// The guide of `stack`, with the layers of `ui` when it is named too, or of the stack the project declares.
    fn guide(&self, root: &Path, stack: Option<&str>, ui: Option<&str>) -> Result<String, String>;
    /// Write every index file, and find the items to measure again now.
    fn index(&self, root: &Path) -> Result<Indexed, String>;
    /// The findings an approval of `import` by `file` would keep, as `rotproof check` says them. `Err` when the import
    /// is not forbidden, or is approved already.
    fn pending(&self, root: &Path, file: &str, import: &str) -> Result<Vec<String>, String>;
    /// The name to sign an approval with: git's `user.name`, when it is set.
    fn user_name(&self, root: &Path) -> Option<String>;
    /// Keep the import of `import` by `file`, approved now by `by` for `reason`. The path of the file written.
    fn approve(
        &self,
        root: &Path,
        file: &str,
        import: &str,
        reason: &str,
        by: &str,
    ) -> Result<String, String>;
    /// Remove every approval that matches no forbidden import.
    fn prune(&self, root: &Path) -> Result<Vec<Removed>, String>;
    /// Pin each of `keys` in the knowledge document `document` names, at its hash now: all of them, or none.
    fn pin(&self, root: &Path, document: &str, keys: &[String]) -> Result<Pinned, String>;
    /// Write a new knowledge document with its frontmatter and nothing else but an alarm's two headings, the
    /// `description` empty so that the check fails until it is written, and each key it follows pinned. `Err`, writing
    /// nothing, when the slug is not one, the file is there already, the area is not one, or a key is not there.
    fn start(&self, root: &Path, document: &NewDocument) -> Result<Started, String>;
    /// Add `entry` to `docs/log.md`, at the top under the heading of today, the day the clock gives, made when it is
    /// not there. `Err`, writing nothing, when a field is empty or not in the form the log has it, the log is not
    /// there, or the entry would break a rule the log kept.
    fn log(&self, root: &Path, entry: &LogEntry) -> Result<Logged, String>;
}

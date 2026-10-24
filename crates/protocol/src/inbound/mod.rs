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
    /// Documents left out of the index files: name -> why
    pub left_out: Vec<(String, String)>,
    /// The fields added to the declaration, as `name = value`
    pub added: Vec<String>,
    /// What the stack does not have Rotproof write yet, and why
    pub not_written: Option<String>,
}

/// What `rotproof init` did.
#[derive(Debug)]
pub enum Initialized {
    /// The declaration was written, at this path
    Declared(&'static str),
    Upgraded(Upgraded),
}

/// What an upgrade of the project's files did.
#[derive(Debug, Default)]
pub struct Upgraded {
    /// The version the project's files were up to
    pub from: String,
    /// Each update applied, with the file it changed
    pub applied: Vec<(&'static str, &'static str)>,
    /// How many records moved into `docs/work/`
    pub moved: usize,
    /// What each update that could not be applied leaves to a person
    pub by_hand: Vec<String>,
    /// What `rotproof create` would have done after the upgrade, without a layer
    pub made: Made,
    /// Whether `files` was set to the running version
    pub files_set: bool,
}

/// One broken rule: which check found it, and what is wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The heading the finding is printed under. Fixed for most checks; built for those that name what they found
    pub check: Cow<'static, str>,
    pub detail: String,
}

/// What `rotproof check` found.
#[derive(Debug, Default)]
pub struct Report {
    pub findings: Vec<Finding>,
    /// The check that did not run, and why: the structure check says it of the layers of a repository of records only
    pub skipped: Option<String>,
    /// Whether the tree was checked against a stack's layers (not for `stack = "none"`)
    pub layers_checked: bool,
    /// Every forbidden import a person approved, as printed on every run: one line per entry that approves something
    pub approved: Vec<String>,
}

/// What `rotproof index` did.
#[derive(Debug, Default)]
pub struct Indexed {
    /// The files written, from the root
    pub written: Vec<String>,
    /// Documents left out of the index files: name -> why
    pub left_out: Vec<(String, String)>,
    /// The open work items past their `stale_after`, each with it
    pub stale: Vec<(String, String)>,
    /// The open milestones past their `date`, each with it
    pub past_date: Vec<(String, String)>,
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
    /// Write the declaration for `stack`, or upgrade the project's files to the running version.
    fn init(&self, root: &Path, stack: Option<&str>) -> Result<Initialized, String>;
    /// Make what the declaration names and the tree lacks; layers only when `yes`.
    fn create(&self, root: &Path, yes: bool) -> Result<Made, String>;
    /// Every broken rule.
    fn check(&self, root: &Path) -> Result<Report, String>;
    /// The guide of `stack`, or of the stack the project declares.
    fn guide(&self, root: &Path, stack: Option<&str>) -> Result<String, String>;
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
    /// Every key of the knowledge documents whose code is not as pinned.
    fn changed(&self, root: &Path) -> Result<Vec<Drift>, String>;
    /// Pin each of `drifts` as reviewed: each document written, with its hash for the log.
    fn repin(&self, root: &Path, drifts: &[Drift]) -> Result<Vec<(String, String)>, String>;
    /// The hash of what the key of `follows` names now: `None` when it is not there.
    fn current(&self, root: &Path, key: &str) -> Result<Option<String>, String>;
    /// The answer to the agent's stop hook, whose input is `input`: `None` to let it stop.
    fn stop_hook(&self, root: &Path, input: &str) -> Result<Option<String>, String>;
}

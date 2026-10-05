//! The approvals file, `.config/rotproof-approved.toml`, read from the tree and judged by the rules in `domain`, and
//! `rotproof approve`: an entry added for a forbidden import, and the entries that match nothing removed.
//!
//! Whether a person is at a terminal is the command line's to ask; these take what the person answered.

use std::io;

use crate::tree::{Lookup, lookup, read_text};
use domain::approvals::{APPROVALS, Kept, parse, problems, sort, with_kept, without};
use domain::code::Parsers;
use domain::direction::Forbidden;
use domain::tree::{Tree, Writer};

/// What the approvals file holds.
#[derive(Debug, Default)]
pub struct Approvals {
    /// Its text, with every line ending as `\n`: `None` when there is no file
    pub text: Option<String>,
    pub entries: Vec<Kept>,
    /// What is wrong with it: a name spelled otherwise, a file that is not TOML, an entry broken
    pub problems: Vec<String>,
}

/// The approvals of the project `tree` holds. An error is a file that is there and could not be read.
pub fn read(tree: &dyn Tree) -> io::Result<Approvals> {
    let path = match lookup(tree, "", APPROVALS) {
        Lookup::Found {
            path,
            is_dir: false,
        } => path,
        Lookup::Found { is_dir: true, .. } => {
            return Ok(Approvals {
                problems: vec![format!("{APPROVALS} is a directory")],
                ..Approvals::default()
            });
        }
        // An approvals file Linux would not find approves nothing there, so it approves nothing anywhere
        Lookup::Spelled(on_disk) => {
            return Ok(Approvals {
                problems: vec![format!(
                    "{on_disk} is not read: the approvals file is {APPROVALS}, by its exact name"
                )],
                ..Approvals::default()
            });
        }
        Lookup::Missing => return Ok(Approvals::default()),
    };
    let text = read_text(tree, &path)?;
    let (entries, problems) = match parse(&text) {
        Ok(entries) => {
            let found = problems(&entries);
            (entries, found)
        }
        Err(why) => (Vec::new(), vec![why]),
    };
    Ok(Approvals {
        text: Some(text),
        entries,
        problems,
    })
}

/// Every import the table forbids in the project, approved or not: `None` for a repository of records only, which
/// has no layers. `Err` when the declaration cannot be read.
fn forbidden(tree: &dyn Tree, parsers: &dyn Parsers) -> Result<Option<Vec<Forbidden>>, String> {
    let structure = crate::structure::problems(tree).map_err(|e| e.to_string())?;
    let Some(declared) = structure.declared else {
        return Err(structure
            .found
            .first()
            .cloned()
            .unwrap_or_else(|| "the declaration cannot be read".into()));
    };
    if declared.layout.is_none() {
        return Ok(None);
    }
    Ok(Some(
        crate::direction::problems(tree, parsers, &declared)
            .map_err(|e| e.to_string())?
            .forbidden,
    ))
}

/// The forbidden imports of `import` by `file` that no entry approves yet, for the person to see before approving.
/// `Err` when there is none: `rotproof approve` approves only what the check forbids, and once.
pub fn pending(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    file: &str,
    import: &str,
) -> Result<Vec<Forbidden>, String> {
    let found = forbidden(tree, parsers)?.ok_or_else(|| {
        "a repository of records only has no layers, so no import to approve".to_string()
    })?;
    let approvals = read(tree).map_err(|e| e.to_string())?;
    if let Some(why) = approvals.problems.first() {
        return Err(format!("fix the approvals file first: {why}"));
    }
    let sorted = sort(&found, &approvals.entries);
    if sorted
        .approved
        .iter()
        .any(|(f, _)| f.file == file && f.import == import)
    {
        return Err(format!(
            "{import} in {file} is approved already, in {APPROVALS}"
        ));
    }
    let pending: Vec<Forbidden> = sorted
        .forbidden
        .into_iter()
        .filter(|f| f.file == file && f.import == import)
        .cloned()
        .collect();
    if pending.is_empty() {
        return Err(format!(
            "no forbidden import of {import} in {file}: `rotproof approve` approves only what `rotproof check` \
             forbids, named as it names them (the file from the root, and the import as the check writes it)"
        ));
    }
    Ok(pending)
}

/// Add `entry` to the approvals file of `tree` through `out`, and return the file's path.
pub fn approve(tree: &dyn Tree, out: &dyn Writer, entry: &Kept) -> Result<&'static str, String> {
    let approvals = read(tree).map_err(|e| e.to_string())?;
    if let Some(why) = approvals.problems.first() {
        return Err(format!("fix the approvals file first: {why}"));
    }
    let text = with_kept(approvals.text.as_deref(), entry)?;
    out.write(APPROVALS, &text)
        .map_err(|e| format!("{APPROVALS}: {e}"))?;
    Ok(APPROVALS)
}

/// Remove every entry that matches no forbidden import from the approvals file of `tree` through `out`, and return
/// them. It only takes approvals away, so it lets nothing pass that failed before.
pub fn prune(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    out: &dyn Writer,
) -> Result<Vec<Kept>, String> {
    let approvals = read(tree).map_err(|e| e.to_string())?;
    let Some(text) = &approvals.text else {
        return Ok(Vec::new());
    };
    if let Some(why) = approvals.problems.first() {
        return Err(format!("fix the approvals file first: {why}"));
    }
    // A repository of records only forbids nothing, so every entry there is stale
    let found = forbidden(tree, parsers)?.unwrap_or_default();
    let stale: Vec<&Kept> = sort(&found, &approvals.entries).stale;
    if stale.is_empty() {
        return Ok(Vec::new());
    }
    out.write(APPROVALS, &without(text, &stale)?)
        .map_err(|e| format!("{APPROVALS}: {e}"))?;
    Ok(stale.into_iter().cloned().collect())
}

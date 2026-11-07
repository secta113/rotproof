//! The approvals file, `.config/rotproof-approved.toml`, read from the tree and judged by the rules in `domain`, and
//! `rotproof approve`: an entry added for a forbidden import, and the entries that match nothing removed.
//!
//! Whether a person is at a terminal is the command line's to ask; these take what the person answered.

use std::io;

use crate::tree::{Lookup, lookup, read_written};
use domain::approvals::{APPROVALS, Kept, parse, problems, sort, with_kept, without};
use domain::direction::Forbidden;
use domain::tree::Written;
use protocol::outbound::Parsers;
use protocol::outbound::{Tree, Writer};

/// What the approvals file holds.
#[derive(Debug, Default)]
pub struct Approvals {
    /// Its text, with every line ending as `\n` and no byte order mark: `None` when there is no file
    pub text: Option<String>,
    /// How the file was written, for the text written back to go through [`Written::restore`]
    pub was: Written,
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
    let (text, was) = read_written(tree, &path)?;
    let (entries, problems) = match parse(&text) {
        Ok(entries) => {
            let found = problems(&entries);
            (entries, found)
        }
        Err(why) => (Vec::new(), vec![why]),
    };
    Ok(Approvals {
        text: Some(text),
        was,
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

/// Add `entry` to the approvals file of `tree` through `out`, and return the file's path. The file keeps its line
/// endings and its byte order mark.
pub fn approve(tree: &dyn Tree, out: &dyn Writer, entry: &Kept) -> Result<&'static str, String> {
    let approvals = read(tree).map_err(|e| e.to_string())?;
    if let Some(why) = approvals.problems.first() {
        return Err(format!("fix the approvals file first: {why}"));
    }
    let text = with_kept(approvals.text.as_deref(), entry)?;
    out.write(APPROVALS, &approvals.was.restore(&text))
        .map_err(|e| format!("{APPROVALS}: {e}"))?;
    Ok(APPROVALS)
}

/// Remove every entry that matches no forbidden import from the approvals file of `tree` through `out`, and return
/// them. It only takes approvals away, so it lets nothing pass that failed before. The file keeps its line endings and
/// its byte order mark.
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
    out.write(APPROVALS, &approvals.was.restore(&without(text, &stale)?))
        .map_err(|e| format!("{APPROVALS}: {e}"))?;
    Ok(stale.into_iter().cloned().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::fake::Fake;
    use domain::approvals::Approved;
    use infrastructure::outbound::readers::Readers;

    const KEPT: &str = "[[kept]]\nfrom = \"domain/gone.py\"\nimport = \"infrastructure.db\"\nreason = \"r\"\n\
                        approved = { by = \"someone\", at = \"2026-10-06T09:00:00+09:00\" }\n";

    fn entry() -> Kept {
        Kept {
            from: "domain/new.py".into(),
            import: "infrastructure.db".into(),
            reason: "a reason".into(),
            approved: Approved {
                by: "someone".into(),
                at: "2026-10-07T09:00:00+09:00".into(),
            },
        }
    }

    /// `text` as a file of that form is written: every line ending as `\r\n` for `crlf`, after a byte order mark for
    /// `bom`
    fn as_written(text: &str, crlf: bool, bom: bool) -> String {
        let text = if crlf {
            text.replace('\n', "\r\n")
        } else {
            text.to_string()
        };
        if bom { format!("\u{feff}{text}") } else { text }
    }

    const FORMS: [(bool, bool); 4] = [(true, true), (true, false), (false, true), (false, false)];

    #[test]
    fn approve_keeps_the_line_endings_and_the_byte_order_mark() {
        let before = format!("# our note\n{KEPT}");
        for (crlf, bom) in FORMS {
            let tree = Fake::new(&[(APPROVALS, &as_written(&before, crlf, bom))]);
            approve(&tree, &tree, &entry()).unwrap();
            let written = tree.text(APPROVALS).unwrap();
            // Only the entry was added: the file as read is what with_kept gives, and every line ends as before
            let added = with_kept(Some(&before), &entry()).unwrap();
            assert!(added.contains("domain/new.py") && added.starts_with(&before));
            assert_eq!(
                written,
                as_written(&added, crlf, bom),
                "crlf {crlf}, bom {bom}"
            );
        }
    }

    #[test]
    fn prune_keeps_the_line_endings_and_the_byte_order_mark() {
        let before = format!("# our note\n{KEPT}");
        for (crlf, bom) in FORMS {
            // A repository of records only forbids nothing, so every entry is stale
            let tree = Fake::new(&[
                (
                    ".config/rotproof.toml",
                    "stack = \"none\"\nareas = [\"a\"]\n",
                ),
                (APPROVALS, &as_written(&before, crlf, bom)),
            ]);
            let removed = prune(&tree, &Readers, &tree).unwrap();
            assert_eq!(removed.len(), 1);
            let written = tree.text(APPROVALS).unwrap();
            let pruned = without(&before, &[&removed[0]]).unwrap();
            assert!(!pruned.contains("domain/gone.py") && pruned.starts_with("# our note\n"));
            assert_eq!(
                written,
                as_written(&pruned, crlf, bom),
                "crlf {crlf}, bom {bom}"
            );
        }
    }
}

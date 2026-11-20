//! The baseline file, `.config/rotproof-baseline.toml`, read from the tree and judged by the rules in `domain`, and
//! `rotproof baseline`: the violations there are written once, and the entries that match nothing removed.
//!
//! It is kept as the approvals file is: the same findings are matched by the same key, and what an approval covers is
//! left out first.

use std::io;

use crate::approvals::{project, read as read_approvals};
use crate::tree::{Lookup, lookup, read_written};
use domain::approvals::{APPROVALS, sort as sort_approved};
use domain::baseline::{BASELINE, Baseline, Entry, parse, problems, sort, without, written};
use domain::direction::Forbidden;
use domain::tree::Written;
use protocol::outbound::Parsers;
use protocol::outbound::{Tree, Writer};

/// What the baseline file holds.
#[derive(Debug, Default)]
pub struct Frozen {
    /// Its text, with every line ending as `\n` and no byte order mark: `None` when there is no file
    pub text: Option<String>,
    /// How the file was written, for the text written back to go through [`Written::restore`]
    pub was: Written,
    /// The count it was written with, and the entries (none when the file could not be read)
    pub baseline: Option<Baseline>,
    /// What is wrong with it: a name spelled otherwise, a file that is not TOML, a count missing, an entry broken
    pub problems: Vec<String>,
}

impl Frozen {
    /// The entries, none when there is no file or it could not be read
    pub fn entries(&self) -> &[Entry] {
        self.baseline.as_ref().map_or(&[], |b| &b.entries)
    }
}

/// The baseline of the project `tree` holds. An error is a file that is there and could not be read.
pub fn read(tree: &dyn Tree) -> io::Result<Frozen> {
    let path = match lookup(tree, "", BASELINE) {
        Lookup::Found {
            path,
            is_dir: false,
        } => path,
        Lookup::Found { is_dir: true, .. } => {
            return Ok(Frozen {
                problems: vec![format!("{BASELINE} is a directory")],
                ..Frozen::default()
            });
        }
        // A baseline Linux would not find freezes nothing there, so it freezes nothing anywhere
        Lookup::Spelled(on_disk) => {
            return Ok(Frozen {
                problems: vec![format!(
                    "{on_disk} is not read: the baseline file is {BASELINE}, by its exact name"
                )],
                ..Frozen::default()
            });
        }
        Lookup::Missing => return Ok(Frozen::default()),
    };
    let (text, was) = read_written(tree, &path)?;
    let (baseline, problems) = match parse(&text) {
        Ok(baseline) => {
            let found = problems(&baseline.entries);
            (Some(baseline), found)
        }
        Err(why) => (None, vec![why]),
    };
    Ok(Frozen {
        text: Some(text),
        was,
        baseline,
        problems,
    })
}

/// What `rotproof baseline` wrote.
#[derive(Debug, PartialEq, Eq)]
pub struct Froze {
    /// The file, from the root
    pub path: &'static str,
    /// How many entries it holds, each a file and an import
    pub entries: usize,
}

/// Freeze every violation of the project `tree` holds that an approval does not cover, in a file that is not there
/// yet, through `out`. `Err`, writing nothing, when the file is there already, `moving` still lists a path, the
/// approvals file cannot be read, or there is nothing to freeze.
pub fn baseline(tree: &dyn Tree, parsers: &dyn Parsers, out: &dyn Writer) -> Result<Froze, String> {
    let frozen = read(tree).map_err(|e| e.to_string())?;
    if frozen.text.is_some() {
        return Err(format!(
            "{BASELINE} is there already: a baseline is written once and only shrinks, so run `rotproof baseline \
             --prune` to remove its entries that match nothing"
        ));
    }
    if let Some(why) = frozen.problems.first() {
        return Err(format!("fix the baseline file first: {why}"));
    }
    let Some((declared, found)) = project(tree, parsers)? else {
        return Err(
            "a repository of records only has no layers, so no violation to baseline".to_string(),
        );
    };
    if !declared.declaration.moving.is_empty() {
        return Err(format!(
            "`moving` in {} still lists {}: move the code into the layers first, as the baseline freezes what is left \
             once everything is in one",
            domain::declaration::DECLARATION,
            declared.declaration.moving.join(", ")
        ));
    }
    let approvals = read_approvals(tree).map_err(|e| e.to_string())?;
    if let Some(why) = approvals.problems.first() {
        return Err(format!("fix the approvals file first: {why}"));
    }
    let sorted = sort_approved(&found, &approvals.entries);
    let Some((text, entries)) = written(&sorted.forbidden) else {
        return Err(format!(
            "nothing to baseline: no forbidden import or use of I/O of the inner layers is left that {APPROVALS} does \
             not approve"
        ));
    };
    out.write(BASELINE, &text)
        .map_err(|e| format!("{BASELINE}: {e}"))?;
    Ok(Froze {
        path: BASELINE,
        entries,
    })
}

/// What `rotproof baseline --prune` did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Pruned {
    /// The entries removed
    pub removed: Vec<Entry>,
    /// The count before and after: the entries left, which may be fewer than the count was when one was deleted by hand
    pub count: (usize, usize),
}

/// Remove every entry that matches no forbidden import from the baseline of `tree` through `out`, and lower `count` to
/// the entries left. It only takes entries away, so it lets nothing pass that failed before. The file keeps its line
/// endings and its byte order mark.
pub fn prune(tree: &dyn Tree, parsers: &dyn Parsers, out: &dyn Writer) -> Result<Pruned, String> {
    let frozen = read(tree).map_err(|e| e.to_string())?;
    let (Some(text), Some(baseline)) = (&frozen.text, &frozen.baseline) else {
        return match frozen.problems.first() {
            Some(why) => Err(format!("fix the baseline file first: {why}")),
            None => Ok(Pruned::default()),
        };
    };
    if let Some(why) = frozen.problems.first() {
        return Err(format!("fix the baseline file first: {why}"));
    }
    let approvals = read_approvals(tree).map_err(|e| e.to_string())?;
    if let Some(why) = approvals.problems.first() {
        return Err(format!("fix the approvals file first: {why}"));
    }
    // A repository of records only forbids nothing, so every entry there is stale
    let found: Vec<Forbidden> = project(tree, parsers)?
        .map(|(_, found)| found)
        .unwrap_or_default();
    let sorted = sort_approved(&found, &approvals.entries);
    let stale: Vec<&Entry> = sort(&sorted.forbidden, &baseline.entries).stale;
    let pruned = without(text, &stale)?;
    let left = baseline.entries.len() - stale.len();
    if &pruned != text {
        out.write(BASELINE, &frozen.was.restore(&pruned))
            .map_err(|e| format!("{BASELINE}: {e}"))?;
    }
    Ok(Pruned {
        removed: stale.into_iter().cloned().collect(),
        count: (baseline.count, left),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::fake::Fake;
    use infrastructure::outbound::readers::Readers;

    const DECLARATION: &str = "stack = \"python\"\nareas = []\nui_design = \"none\"\nmoving = []\n";
    const BASE: &str = "count = 2\n\n[[entry]]\nfrom = \"domain/gone.py\"\nimport = \"infrastructure\"\n\n\
                        [[entry]]\nfrom = \"domain/model.py\"\nimport = \"infrastructure\"\n";

    fn project_with(files: &[(&str, &str)]) -> Fake {
        let mut all = vec![(".config/rotproof.toml", DECLARATION)];
        all.extend_from_slice(files);
        Fake::new(&all)
    }

    #[test]
    fn it_freezes_what_the_approvals_leave_once_and_refuses_the_next_time() {
        let tree = project_with(&[
            (
                "domain/model.py",
                "import infrastructure.db\nimport application.x\n",
            ),
            (
                APPROVALS,
                "[[kept]]\nfrom = \"domain/model.py\"\nimport = \"application\"\nreason = \"r\"\n\
                 approved = { by = \"someone\", at = \"2026-10-06T09:00:00+09:00\" }\n",
            ),
        ]);
        let done = baseline(&tree, &Readers, &tree).unwrap();
        assert_eq!(done.entries, 1, "{done:?}");
        let written = tree.text(BASELINE).unwrap();
        let read = parse(&written).unwrap();
        assert_eq!(
            read.entries,
            [Entry {
                from: "domain/model.py".into(),
                import: "infrastructure".into()
            }]
        );
        let again = baseline(&tree, &Readers, &tree).unwrap_err();
        assert!(again.contains("is there already"), "{again}");
    }

    #[test]
    fn it_refuses_while_code_is_moving_and_when_there_is_nothing_to_freeze() {
        let moving = Fake::new(&[
            (
                ".config/rotproof.toml",
                "stack = \"python\"\nareas = []\nui_design = \"none\"\nmoving = [\"legacy\"]\n",
            ),
            ("legacy/a.py", "x = 1\n"),
            ("domain/model.py", "import infrastructure.db\n"),
        ]);
        let why = baseline(&moving, &Readers, &moving).unwrap_err();
        assert!(why.contains("still lists legacy"), "{why}");
        assert!(moving.text(BASELINE).is_none());
        let clean = project_with(&[("domain/model.py", "x = 1\n")]);
        let why = baseline(&clean, &Readers, &clean).unwrap_err();
        assert!(why.contains("nothing to baseline"), "{why}");
        assert!(clean.text(BASELINE).is_none());
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

    #[test]
    fn prune_removes_the_entry_that_matches_nothing_lowers_the_count_and_keeps_the_line_endings() {
        for (crlf, bom) in [(true, true), (true, false), (false, true), (false, false)] {
            let tree = project_with(&[
                ("domain/model.py", "import infrastructure.db\n"),
                (BASELINE, &as_written(BASE, crlf, bom)),
            ]);
            let done = prune(&tree, &Readers, &tree).unwrap();
            assert_eq!(done.removed.len(), 1);
            assert_eq!(done.count, (2, 1));
            let pruned = without(BASE, &[&done.removed[0]]).unwrap();
            assert!(pruned.starts_with("count = 1\n"), "{pruned}");
            assert_eq!(
                tree.text(BASELINE).unwrap(),
                as_written(&pruned, crlf, bom),
                "crlf {crlf}, bom {bom}"
            );
        }
    }
}

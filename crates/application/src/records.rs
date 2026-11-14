//! The checks of the records, read from the tree: the areas they are grouped by, the floor, the links a reader may
//! follow, the log, the knowledge documents and the files `rotproof index` writes.

use std::collections::BTreeMap;
use std::io;

use crate::bundle::Bundle;
use crate::finding::under;
use crate::links::broken;
use crate::tree::{exactly, read_text};
use domain::bundle::{DOCS, Docs, in_docs, is_document};
use domain::declaration::{DECLARATION, area_problems};
use domain::markdown;
use domain::paths::join;
use domain::records::{KNOWLEDGE, floor, log_days_to_come, log_problems, unread_paths};
use domain::schema::Time;
use protocol::inbound::Finding;
use protocol::outbound::{Parsers, Tree};

/// The check that says the records could not be checked at all
const SEEN: &str = "the bundle is seen";
/// The check of the structure of the log, which also says why the log could not be read
const STRUCTURE: &str = "the log keeps its structure";
/// The check of the days written in the records
const NOT_COME: &str = "no day in the log is later than today";
const LOG: &str = "docs/log.md";

/// What the checks of the records found.
#[derive(Debug, Default)]
pub struct Records {
    /// Every broken rule, under its check, in the order it is printed
    pub found: Vec<Finding>,
    /// Every knowledge document whose `stale_after` has come, as a warning
    pub due: Vec<String>,
}

/// Every broken rule of the records at `now`, grouped by `areas`: `None` when the declaration cannot be read.
pub fn problems(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    areas: Option<Vec<String>>,
    now: Time,
) -> io::Result<Records> {
    // Every record names an area, so nothing below can be judged without them. The structure check names what is
    // wrong with the declaration; this says that the records were not checked because of it
    let Some(areas) = areas else {
        let why = format!(
            "the records are not checked: they are grouped by the areas in {DECLARATION}, which cannot be read"
        );
        return Ok(Records {
            found: under(SEEN, [why]).collect(),
            ..Records::default()
        });
    };
    let mut found: Vec<Finding> =
        under("the areas are distinct headings", area_problems(&areas)).collect();

    // The floor: the directory of knowledge and the root index exist, and the rules are found as a document. If a move
    // or a rename makes the scan come back empty, the checks below see nothing and pass. Each is found by its exact
    // name: `Rules.md` is found as `rules.md` on Windows, and missing on Linux and GitHub
    let missing: Vec<String> = floor()
        .into_iter()
        .filter_map(|rel| exactly(tree, rel.trim_end_matches('/')).err())
        .collect();
    if !missing.is_empty() {
        found.extend(under(SEEN, missing));
        return Ok(Records {
            found,
            ..Records::default()
        });
    }

    let written = read_log(tree)?;
    found.extend(links(tree, parsers, written.as_ref().ok())?);
    found.extend(log(&written, now));
    let bundle = Bundle::new(tree, areas);
    let knowledge = knowledge(tree, parsers, &bundle, now)?;
    found.extend(knowledge.found);
    found.extend(generated(tree, &bundle)?);
    Ok(Records {
        found,
        due: knowledge.due,
    })
}

/// The text of the log, or why it cannot be read as a file.
fn read_log(tree: &dyn Tree) -> io::Result<Result<String, String>> {
    Ok(match exactly(tree, LOG) {
        Ok((path, false)) => Ok(read_text(tree, &path)?),
        Ok((_, true)) => Err(format!("{LOG} is not a file")),
        Err(why) => Err(why),
    })
}

/// Every link a reader may follow in what Rotproof keeps that does not resolve: the knowledge documents, and the
/// `log` when it could be read.
fn links(tree: &dyn Tree, parsers: &dyn Parsers, log: Option<&String>) -> io::Result<Vec<Finding>> {
    let mut documents = concepts(tree, DOCS)?;
    if let Some(log) = log {
        documents.insert("log.md".into(), log.clone());
    }
    Ok(under(
        "every link in docs/knowledge/ and the log resolves",
        pairs(&unresolved(&documents, tree, parsers)),
    )
    .collect())
}

/// Every broken rule of the `log` at `now`, or why it could not be read.
fn log(log: &Result<String, String>, now: Time) -> Vec<Finding> {
    match log {
        Ok(log) => under(STRUCTURE, log_problems(log))
            .chain(under(NOT_COME, log_days_to_come(log, now)))
            .collect(),
        Err(why) => under(STRUCTURE, [why.clone()]).collect(),
    }
}

/// Every broken rule of the knowledge documents `bundle` reads, and those whose `stale_after` has come at `now`.
fn knowledge(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    bundle: &Bundle,
    now: Time,
) -> io::Result<Records> {
    let read = bundle.read_knowledge()?;
    let held = read
        .documents
        .into_iter()
        .map(|(name, (doc, _))| (name, doc))
        .collect();
    let due = domain::records::due(&held, now);
    let found = under(
        "every knowledge document matches the code it follows",
        crate::follows::drift(tree, parsers, &held),
    )
    .chain(under(
        "every file in docs/knowledge/ is read",
        pairs(&unread(tree, DOCS)?),
    ))
    .chain(under(
        "every knowledge document keeps the format",
        pairs(&read.problems),
    ))
    .collect();
    Ok(Records { found, due })
}

/// The files `rotproof index` writes that are not as it would write them now.
fn generated(tree: &dyn Tree, bundle: &Bundle) -> io::Result<Vec<Finding>> {
    let (files, _) = bundle.expected()?;
    let stale: Vec<String> = files
        .into_iter()
        // Compared by the exact name, as the floor is: a `Rules.md` holding the rules is not `rules.md`
        .filter(|(path, text)| {
            let found = exactly(tree, path).ok();
            found
                .and_then(|(path, _)| read_text(tree, &path).ok())
                .as_ref()
                != Some(text)
        })
        .map(|(path, _)| format!("out of date, run `rotproof index`: {path}"))
        .collect();
    Ok(under("every generated file is up to date", stale).collect())
}

fn pairs(problems: &BTreeMap<String, String>) -> Vec<String> {
    problems
        .iter()
        .map(|(name, why)| format!("{name}: {why}"))
        .collect()
}

/// Document -> why, for the documents with a link that does not resolve, in the repository's `tree`. `documents` is
/// every document read, by its path in `docs/`.
///
/// A relative link resolves from the document's directory, and one starting with `/` from the bundle root (`docs/`).
fn unresolved(
    documents: &BTreeMap<String, String>,
    tree: &dyn Tree,
    parsers: &dyn Parsers,
) -> BTreeMap<String, String> {
    let mut bad = BTreeMap::new();
    for (path, text) in documents {
        let here = match path.rsplit_once('/') {
            Some((dir, _)) => in_docs(dir),
            None => DOCS.to_string(),
        };
        let reasons: Vec<String> = markdown::links(text)
            .iter()
            .filter_map(|(text, target)| broken(tree, parsers, text, target, &here, DOCS))
            .collect();
        if !reasons.is_empty() {
            bad.insert(
                path.clone(),
                format!("links that do not resolve: {reasons:?}"),
            );
        }
    }
    bad
}

/// Every knowledge document, directly in `knowledge/` of the bundle directory `docs`: its path relative to `docs` (with
/// `/`) -> text. The rest of `docs` is the project's, and is not read.
fn concepts(tree: &dyn Tree, docs: &str) -> io::Result<Docs> {
    let dir = join(docs, KNOWLEDGE);
    let mut out = Docs::new();
    for (name, is_dir) in tree.entries(&dir)? {
        if is_document(&name, is_dir) {
            out.insert(
                format!("{KNOWLEDGE}/{name}"),
                read_text(tree, &join(&dir, &name))?,
            );
        }
    }
    Ok(out)
}

/// Path -> why, for the files under `knowledge/` of the bundle directory `docs` that Rotproof would not read (see
/// [`unread_paths`]), each by its path relative to `docs`.
fn unread(tree: &dyn Tree, docs: &str) -> io::Result<BTreeMap<String, String>> {
    let mut found = Vec::new();
    walk(
        tree,
        &join(docs, KNOWLEDGE),
        &format!("{KNOWLEDGE}/"),
        &mut found,
    )?;
    let paths: Vec<String> = found.into_iter().map(|(path, _)| path).collect();
    Ok(unread_paths(&paths))
}

fn walk(
    tree: &dyn Tree,
    dir: &str,
    prefix: &str,
    out: &mut Vec<(String, String)>,
) -> io::Result<()> {
    for (name, is_dir) in tree.entries(dir)? {
        let path = format!("{prefix}{name}");
        let full = join(dir, &name);
        if is_dir {
            walk(tree, &full, &format!("{path}/"), out)?;
        } else {
            out.push((path, full));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use infrastructure::outbound::disk::Disk;
    use infrastructure::outbound::readers::Readers;

    fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn a_broken_link_is_caught_in_any_document_and_resolved_from_its_directory() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("docs/work")).unwrap();
        fs::create_dir_all(root.path().join("docs/knowledge")).unwrap();
        fs::write(root.path().join("docs/log.md"), "# Log\n").unwrap();
        fs::write(root.path().join("docs/work/a.md"), "# A\n").unwrap();
        let documents = map(&[
            ("work/linked.md", "[a](a.md) and [log](/log.md)"),
            (
                "work/only the second link is broken.md",
                "[a](/log.md), [b](/no_such_file.md)",
            ),
            // Relative from its own directory: a.md is in work/, not in knowledge/
            ("knowledge/k.md", "[a](a.md)"),
            ("log.md", "* [a](/work/a.md) and [gone](/work/gone.md)"),
        ]);
        let bad = unresolved(&documents, &Disk::new(root.path()), &Readers);
        assert_eq!(
            bad.keys().collect::<Vec<_>>(),
            [
                "knowledge/k.md",
                "log.md",
                "work/only the second link is broken.md"
            ]
        );
    }

    #[test]
    fn a_link_in_any_form_is_resolved() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("docs/work")).unwrap();
        fs::write(root.path().join("docs/log.md"), "# Log\n").unwrap();
        fs::write(root.path().join("docs/work/a (1).md"), "# A\n").unwrap();
        let good = "[log](/log.md)";
        let resolving = [
            "[log](/log.md \"the log\")",
            "[log](</log.md>)",
            "[a](<a (1).md>)",
            "[log][ref]\n\n[ref]: /log.md",
            "<a href=\"/log.md\">log</a>",
        ];
        for detail in resolving {
            let bad = unresolved(
                &map(&[("work/x.md", detail)]),
                &Disk::new(root.path()),
                &Readers,
            );
            assert!(bad.is_empty(), "{detail}: {bad:?}");
        }
        // Next to a link that resolves, a broken one in each form is still found
        let dangling = [
            "[gone](gone.md \"title\")",
            "[gone](gone.md 'title')",
            "[gone](<gone file.md>)",
            "[gone][ref]\n\n[ref]: gone.md",
            "[gone][]\n\n[gone]: gone.md",
            "<a href=\"gone.md\">gone</a>",
        ];
        for form in dangling {
            let detail = format!("{good} {form}");
            let bad = unresolved(
                &map(&[("work/x.md", &detail)]),
                &Disk::new(root.path()),
                &Readers,
            );
            assert!(bad.contains_key("work/x.md"), "{form} passed");
        }
    }

    /// A `docs/` with the given files, each empty.
    fn docs_with(paths: &[&str]) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        for path in paths {
            let full = root.path().join(path);
            fs::create_dir_all(full.parent().unwrap()).unwrap();
            fs::write(full, "").unwrap();
        }
        root
    }

    #[test]
    fn the_files_rotproof_reads_pass_and_the_rest_of_docs_is_not_read() {
        let docs = docs_with(&[
            "index.md",
            "log.md",
            "knowledge/index.md",
            "knowledge/a.md",
            "knowledge/diagram.png",
            "knowledge/assets/diagram.png",
            // The project's own, and the records of work of a project made before 1.0
            "work/index.md",
            "work/item.MD",
            "work/log.md",
            "plans/index.md",
            "notes.Md",
        ]);
        assert_eq!(
            unread(&Disk::new(docs.path()), "").unwrap(),
            BTreeMap::new()
        );
    }

    #[test]
    fn a_file_rotproof_would_not_read_is_caught() {
        let bad = [
            // A reserved name where Rotproof neither writes nor reads it
            "knowledge/log.md",
            // Markdown that is not named .md
            "knowledge/item.MD",
            // A document in a directory under docs/knowledge/, which would go unchecked
            "knowledge/deeper/x.md",
            "knowledge/deeper/index.md",
        ];
        let docs = docs_with(&bad);
        let found = unread(&Disk::new(docs.path()), "").unwrap();
        assert_eq!(found.keys().collect::<Vec<_>>(), {
            let mut sorted = bad.to_vec();
            sorted.sort();
            sorted
        });
    }
}

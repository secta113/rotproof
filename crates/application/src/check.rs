//! `rotproof check`: every rule of the structure and the records, run against one repository.
//!
//! - **The tree matches `.config/rotproof.toml`**: the layers of the stack are present or declared absent, and no code
//!   sits outside them (`structure.rs`).
//! - **The layers import only what the table allows** (`direction.rs`): the imports of `python` and `typescript`, and
//!   the dependencies each crate of `rust` declares. A forbidden import a person approved in
//!   `.config/rotproof-approved.toml` passes and is printed on every run, and an approval that matches no forbidden
//!   import fails (`approvals.rs`).
//! - **No comment holds a marker**, one of the words in `markers::MARKERS` (`markers.rs`): work left to do belongs in
//!   a work item in `docs/work/`, where it is listed and closed.
//! - **Rotproof's guide is up to date**: `.rotproof/AGENTS.md` equals what `rotproof create` writes for the stack with
//!   this version of Rotproof (`project.rs`).
//! - **The records in `docs/work/` keep their format**: every document keeps the format of its type, every work
//!   item's `# Details` has a link, and every work item the log points to exists.
//! - **Every link in `docs/` resolves**: in every document under `docs/` and in the log, not only in `# Details`.
//! - **Every record has one declared area**: the areas in `.config/rotproof.toml` are distinct headings, and the one
//!   tag of every work item, spec and knowledge document is one of them.
//! - **A parent closes after its children**: a spec's `parent` names an epic, which has no parent of its own, a work
//!   item's `parent` names a spec, and no closed spec has an open child.
//! - **The arrows keep their order** (`arrows.rs`): an arrow is written once, the later record closes after the earlier
//!   one, nothing waits for or is bounded by a dropped record, and the arrows and the parents make no cycle.
//! - **Every open record has a parent or something after it** (`arrows.rs`): an open spec or work item without a
//!   parent is bounded by an arrow, its own `until` or another record's `after`, so all open work reaches a milestone.
//! - **A milestone is open**: there is always a next moment for the work to wait for. A milestone whose `date` has
//!   passed fails nothing; `rotproof index` lists it.
//! - **`docs/` is one OKF bundle**: every document is a known type in the directory for its type, every file Rotproof
//!   generates (the index files and the rules) equals what `rotproof index` writes, and no spec sits at the root.
//! - **The log keeps the OKF log structure**: every second-level heading is a date, newest first, and the entries
//!   are a flat list of list items under those dates.
//! - **No day or time in the records is later than now**: a work item's `filed` and `verified`, and the log's date
//!   headings, are read from the clock when written. A day is compared with the latest day anywhere, so CI in UTC
//!   passes a day written in Tokyo. Time alone only makes one pass, so this never turns CI red with no change.
//!
//! Each check has a floor: when the scan finds nothing at all, it fails instead of passing with nothing checked.

use std::collections::BTreeMap;
use std::io;

use crate::bundle::Bundle;

use crate::links::broken;
use crate::tree::{exactly, read_text};
use domain::approvals::{shown, sort, stale};
use domain::bundle::{DOCS, Docs, in_docs, is_document};
use domain::layers::{DECLARATION, area_problems};
use domain::markdown::links;
use domain::migrate::{OLD, WORK};
use domain::paths::{file_name, join};
use domain::project::GUIDE;
use domain::records::{
    dangling_item_refs, dangling_knowledge_refs, floor, knowledge_refs, log_days_to_come,
    log_problems, misplaced, root_specs, unlogged, unread_paths,
};
use domain::schema::Time;
use domain::upgrade::VERSION;
use protocol::inbound::{Finding, Report};
use protocol::outbound::Parsers;
use protocol::outbound::Tree;

/// The check of the days and times written in the records
const NOT_COME: &str = "no day or time in the records is later than now";

/// Every broken rule in the repository `tree` holds, at `now`. An error is a file that could not be read at all.
pub fn check(tree: &dyn Tree, parsers: &dyn Parsers, now: Time) -> io::Result<Report> {
    let structure = crate::structure::problems(tree)?;
    let mut findings: Vec<Finding> = structure
        .found
        .into_iter()
        .map(|detail| Finding {
            check: "the tree matches .config/rotproof.toml".into(),
            detail,
        })
        .collect();
    let mut layers_checked = false;
    let mut approved = Vec::new();
    if let Some(declared) = &structure.declared {
        layers_checked = declared.layout.is_some();
        let direction = crate::direction::problems(tree, parsers, declared)?;
        let approvals = crate::approvals::read(tree)?;
        let sorted = sort(&direction.forbidden, &approvals.entries);
        findings.extend(
            direction
                .problems
                .into_iter()
                .chain(sorted.forbidden.iter().map(|f| f.detail.clone()))
                .map(|detail| Finding {
                    check: "the layers import only what layers/table.toml allows".into(),
                    detail,
                }),
        );
        findings.extend(
            approvals
                .problems
                .into_iter()
                .chain(sorted.stale.iter().map(|entry| stale(entry)))
                .map(|detail| Finding {
                    check: "every approval matches a forbidden import".into(),
                    detail,
                }),
        );
        approved = sorted
            .approved
            .iter()
            .map(|(_, entry)| shown(entry))
            .collect();
        approved.sort();
        approved.dedup();
        let markers = crate::markers::problems(tree, parsers, declared)?;
        let heading = markers.heading();
        findings.extend(markers.found.into_iter().map(|detail| Finding {
            check: heading.clone().into(),
            detail,
        }));
        findings.extend(
            domain::upgrade::problems(
                declared.declaration.files.as_deref(),
                &declared.declaration.declined,
                VERSION,
            )
            .into_iter()
            .map(|detail| Finding {
                check: "the project's files are up to this version of Rotproof".into(),
                detail,
            }),
        );
        let guide = domain::project::guide(&declared.declaration.stack, declared.layout.as_ref());
        let found = exactly(tree, GUIDE)
            .ok()
            .and_then(|(path, _)| read_text(tree, &path).ok());
        if found.as_ref() != Some(&guide) {
            findings.push(Finding {
                check: "Rotproof's guide is up to date".into(),
                detail: format!(
                    "{}, run `rotproof init` after an upgrade of Rotproof, `rotproof create` otherwise: {GUIDE}",
                    if found.is_some() {
                        "out of date"
                    } else {
                        "missing"
                    }
                ),
            });
        }
    }
    findings.extend(records(tree, parsers, structure.areas, now)?);
    Ok(Report {
        findings,
        skipped: structure.skipped,
        layers_checked,
        approved,
    })
}

/// Every broken rule of the records at `now`, grouped by `areas`: `None` when the declaration cannot be read.
fn records(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    areas: Option<Vec<String>>,
    now: Time,
) -> io::Result<Vec<Finding>> {
    let mut found = Vec::new();
    let mut add = |check: &'static str, details: Vec<String>| {
        found.extend(details.into_iter().map(|detail| Finding {
            check: check.into(),
            detail,
        }));
    };
    // Every record names an area, so nothing below can be judged without them. The structure check names what is
    // wrong with the declaration; this says that the records were not checked because of it
    let areas = match areas {
        Some(areas) => areas,
        None => {
            add(
                "the bundle is seen",
                vec![format!(
                    "the records are not checked: they are grouped by the areas in {DECLARATION}, which cannot be read"
                )],
            );
            return Ok(found);
        }
    };
    add("the areas are distinct headings", area_problems(&areas));
    let bundle = Bundle::new(tree, areas);

    // The records of a project made before 0.3.0: one line that says how to move them, instead of one per record that
    // has no place, and nothing else, as no check below reads the old places
    let old: Vec<&str> = OLD
        .into_iter()
        .filter(|dir| tree.found(dir) == Some(true))
        .collect();
    if !old.is_empty() {
        add(
            "the records are in docs/work/",
            vec![format!(
                "{} hold the records of a project made before {}: run `rotproof init`, which moves them to {WORK}/",
                old.iter()
                    .map(|dir| format!("{dir}/"))
                    .collect::<Vec<_>>()
                    .join(" and "),
                domain::migrate::VERSION
            )],
        );
        return Ok(found);
    }

    // The floor: the directories and the root index exist, and the rules are found as a document. If a move
    // or a rename makes the scan come back empty, the checks below see nothing and pass. Each is found by its exact
    // name: `Rules.md` is found as `rules.md` on Windows, and missing on Linux and GitHub
    let missing: Vec<String> = floor()
        .into_iter()
        .filter_map(|rel| exactly(tree, rel.trim_end_matches('/')).err())
        .collect();
    if !missing.is_empty() {
        add("the bundle is seen", missing);
        return Ok(found);
    }

    let work = bundle.read_work()?;
    add(
        "every document in docs/work/ keeps the format",
        pairs(&work.problems),
    );
    let details: BTreeMap<String, String> = work
        .items
        .iter()
        .map(|(name, (_, sections))| (name.clone(), sections["Details"].clone()))
        .collect();
    add("every # Details has a link", pairs(&linkless(&details)));
    add(NOT_COME, work.not_come(now));
    // Every link a reader may follow, not only those in # Details: a link in the body of a closed spec broke unseen
    // when a heading it pointed at was renamed
    let mut documents = concepts(tree, DOCS)?;
    if let Ok((log, false)) = exactly(tree, "docs/log.md") {
        documents.insert("log.md".into(), read_text(tree, &log)?);
    }
    add(
        "every link in docs/ resolves",
        pairs(&unresolved(&documents, tree, parsers)),
    );

    let log_path = exactly(tree, "docs/log.md").and_then(|(path, is_dir)| {
        if !is_dir {
            Ok(path)
        } else {
            Err("docs/log.md is not a file".into())
        }
    });
    if let Ok(log_path) = log_path {
        let log = read_text(tree, &log_path)?;
        let names = file_names(tree, &in_docs("work"))?;
        let dangling = dangling_item_refs(&log, &names);
        add(
            "the log points only at real work items",
            dangling
                .into_iter()
                .map(|name| format!("no such item: docs/work/{name}"))
                .collect(),
        );
        add("the log keeps its structure", log_problems(&log));
        add(NOT_COME, log_days_to_come(&log, now));

        let refs = knowledge_refs(&log);
        let knowledge = bundle.read_knowledge()?;
        let mut documents = BTreeMap::new();
        for name in knowledge.documents.keys() {
            let path = in_docs(&format!("knowledge/{name}"));
            documents.insert(name.clone(), read_text(tree, &path)?);
        }
        add(
            "the log names every knowledge document as it is now",
            unlogged(&documents, &refs),
        );
        let names = file_names(tree, &in_docs("knowledge"))?;
        add(
            "the log points only at real knowledge documents",
            dangling_knowledge_refs(&refs, &names)
                .into_iter()
                .map(|name| format!("no such document: docs/knowledge/{name}"))
                .collect(),
        );
    } else if let Err(why) = log_path {
        add("the log keeps its structure", vec![why]);
    }
    let held = bundle
        .read_knowledge()?
        .documents
        .into_iter()
        .map(|(name, (doc, _))| (name, doc))
        .collect();
    add(
        "every knowledge document matches the code it follows",
        crate::follows::drift(tree, parsers, &held),
    );

    let mut out_of_place = misplaced(&concepts(tree, DOCS)?);
    out_of_place.extend(unread(tree, DOCS)?);
    add(
        "every document is a known type in its place",
        pairs(&out_of_place),
    );
    add(
        "every knowledge document keeps the format",
        pairs(&bundle.read_knowledge()?.problems),
    );
    add(
        "a parent closes after its children",
        work.closed_before_its_children(),
    );
    add(
        "the arrows keep their order",
        domain::arrows::problems(&work),
    );
    add(
        "every open record has a parent or something after it",
        domain::arrows::unbounded(&work),
    );
    add(
        "a milestone is open",
        work.without_an_open_milestone().into_iter().collect(),
    );
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
    add("every generated file is up to date", stale);
    let names = file_names(tree, "")?;
    add(
        "no spec sits at the repository root",
        root_specs(&names)
            .into_iter()
            .map(|name| format!("specs go in docs/work/: {name}"))
            .collect(),
    );
    Ok(found)
}

/// Every name in a directory: files, directories and the rest.
fn file_names(tree: &dyn Tree, dir: &str) -> io::Result<Vec<String>> {
    Ok(tree
        .entries(dir)?
        .into_iter()
        .map(|(name, _)| name)
        .collect())
}

fn pairs(problems: &BTreeMap<String, String>) -> Vec<String> {
    problems
        .iter()
        .map(|(name, why)| format!("{name}: {why}"))
        .collect()
}

/// Item -> what it holds, for the Details sections with no link a reader sees: Details point to the history.
pub fn linkless(details: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    details
        .iter()
        .filter(|(_, detail)| links(detail).is_empty())
        .map(|(name, detail)| (name.clone(), format!("no markdown link: {detail}")))
        .collect()
}

/// Document -> why, for the documents with a link that does not resolve, in the repository's `tree`. `documents` is
/// every document under `docs/` and the log, by its path in `docs/`.
///
/// A relative link resolves from the document's directory, and one starting with `/` from the bundle root (`docs/`).
pub fn unresolved(
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
        let reasons: Vec<String> = links(text)
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

/// Every document under the directory `docs`: path relative to it (with `/`) -> text.
pub fn concepts(tree: &dyn Tree, docs: &str) -> io::Result<Docs> {
    let mut out = Docs::new();
    for (path, full) in files(tree, docs)? {
        if is_document(file_name(&path), false) {
            out.insert(path, read_text(tree, &full)?);
        }
    }
    Ok(out)
}

/// Path -> why, for the files under the directory `docs` that Rotproof would not read (see [`unread_paths`]).
pub fn unread(tree: &dyn Tree, docs: &str) -> io::Result<BTreeMap<String, String>> {
    let paths: Vec<String> = files(tree, docs)?
        .into_iter()
        .map(|(path, _)| path)
        .collect();
    Ok(unread_paths(&paths))
}

/// Every file under `dir`: path relative to `dir` (with `/`) -> path from the root. Every file counts, hidden ones and
/// the ones `.gitignore` excludes too: the records are what the repository holds.
fn files(tree: &dyn Tree, dir: &str) -> io::Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    walk(tree, dir, "", &mut out)?;
    Ok(out)
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
    fn a_details_section_without_a_link_is_caught() {
        let details = map(&[
            ("linked.md", "[log](/log.md)"),
            ("no link (the old form).md", "docs/log.md「somewhere」"),
            (
                "in a comment only.md",
                "Nothing yet. <!-- [log](/log.md) -->",
            ),
        ]);
        assert_eq!(
            linkless(&details).keys().collect::<Vec<_>>(),
            ["in a comment only.md", "no link (the old form).md"]
        );
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
    fn the_files_rotproof_reads_pass() {
        let docs = docs_with(&[
            "index.md",
            "log.md",
            "work/index.md",
            "work/item.md",
            "knowledge/index.md",
            "assets/diagram.png",
        ]);
        assert_eq!(
            unread(&Disk::new(docs.path()), "").unwrap(),
            BTreeMap::new()
        );
    }

    #[test]
    fn a_file_rotproof_would_not_read_is_caught() {
        let bad = [
            // Reserved names where Rotproof neither writes nor reads them
            "extra/index.md",
            "work/deeper/index.md",
            // docs/done/ is no longer read: closed records stay where they are
            "done/index.md",
            // Nor docs/backlog/ and docs/specs/: their records are in docs/work/
            "backlog/index.md",
            "specs/index.md",
            "work/log.md",
            // Markdown that is not named .md
            "work/item.MD",
            "notes.Md",
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

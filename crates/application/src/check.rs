//! `rotproof check`: every rule of the structure and the records, run against one repository. The rules are listed
//! for a person in the help of `rotproof check` (`CHECK_HELP` in `cli.rs`) and in the README ("What `rotproof
//! check` checks"); each is judged in the module of `domain` that holds it, and this use case reads what it needs and
//! runs them.
//!
//! Each check has a floor: when the scan finds nothing at all, it fails instead of passing with nothing checked.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::io;

use crate::bundle::Bundle;

use crate::links::broken;
use crate::tree::{exactly, read_text};
use domain::approvals::{shown, sort, stale};
use domain::bundle::{DOCS, Docs, in_docs, is_document};
use domain::guide::GUIDE;
use domain::layers::{DECLARATION, Declared, INIT_OR_CREATE, area_problems};
use domain::markdown::links;
use domain::markers::KNOWLEDGE_HEADING;
use domain::paths::join;
use domain::records::{KNOWLEDGE, floor, log_days_to_come, log_problems, unread_paths};
use domain::schema::Time;
use domain::upgrade::VERSION;
use protocol::inbound::{Finding, Report};
use protocol::outbound::Parsers;
use protocol::outbound::Tree;

/// The check of the days written in the records
const NOT_COME: &str = "no day in the log is later than today";

/// Every broken rule in the repository `tree` holds, at `now`. An error is a file that could not be read at all.
pub fn check(tree: &dyn Tree, parsers: &dyn Parsers, now: Time) -> io::Result<Report> {
    let structure = crate::structure::problems(tree)?;
    let mut report = Report {
        skipped: structure.skipped,
        ..Report::default()
    };
    report.findings.extend(under(
        "the tree matches .config/rotproof.toml",
        structure.found,
    ));
    if let Some(declared) = &structure.declared {
        report.layers_checked = declared.layout.is_some();
        code(tree, parsers, declared, &mut report)?;
        report.findings.extend(rotproof_files(tree, declared));
    }
    let records = records(tree, parsers, structure.areas, now, &mut report.due)?;
    report.findings.extend(records);
    Ok(report)
}

/// Each of `details` as a finding printed under `check`.
fn under(
    check: impl Into<Cow<'static, str>>,
    details: impl IntoIterator<Item = String>,
) -> impl Iterator<Item = Finding> {
    let check = check.into();
    details.into_iter().map(move |detail| Finding {
        check: check.clone(),
        detail,
    })
}

/// Every broken rule of the code of `declared`, into `report`: the direction with its approvals, the markers, who
/// reads the environment, and the comments that switch the type checker off. The approvals and the comments that may
/// make the checker check less go in `report` too, as they are printed apart from what fails.
fn code(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    declared: &Declared,
    report: &mut Report,
) -> io::Result<()> {
    let direction = crate::direction::problems(tree, parsers, declared)?;
    let approvals = crate::approvals::read(tree)?;
    let sorted = sort(&direction.forbidden, &approvals.entries);
    report.findings.extend(under(
        "the layers import only what layers/table.toml allows",
        direction
            .problems
            .into_iter()
            .chain(sorted.forbidden.iter().map(|f| f.detail.clone())),
    ));
    report.findings.extend(under(
        "every approval matches a forbidden import",
        approvals
            .problems
            .into_iter()
            .chain(sorted.stale.iter().map(|entry| stale(entry))),
    ));
    report.approved = sorted
        .approved
        .iter()
        .map(|(_, entry)| shown(entry))
        .collect();
    report.approved.sort();
    report.approved.dedup();
    let comments = crate::markers::problems(tree, parsers, declared)?;
    let markers = comments.markers;
    report
        .findings
        .extend(under(markers.heading(), markers.found));
    report
        .findings
        .extend(under(KNOWLEDGE_HEADING, markers.knowledge));
    report.findings.extend(under(
        domain::environment::HEADING,
        crate::environment::problems(tree, parsers, declared)?,
    ));
    report
        .findings
        .extend(under(domain::suppressions::HEADING, comments.silenced));
    report.weakened = comments.weakened;
    Ok(())
}

/// Every broken rule of the files Rotproof writes into the project of `declared`: they are up to this version
/// (`files` in the declaration), and the guide is the one this version writes.
fn rotproof_files(tree: &dyn Tree, declared: &Declared) -> Vec<Finding> {
    let mut found: Vec<Finding> = under(
        "the project's files are up to this version of Rotproof",
        domain::upgrade::problems(
            declared.declaration.files.as_deref(),
            &declared.declaration.declined,
            VERSION,
        ),
    )
    .collect();
    let guide = domain::guide::guide(&declared.declaration.stack, declared.layout.as_ref());
    let written = exactly(tree, GUIDE)
        .ok()
        .and_then(|(path, _)| read_text(tree, &path).ok());
    if written.as_ref() != Some(&guide) {
        let state = if written.is_some() {
            "out of date"
        } else {
            "missing"
        };
        found.extend(under(
            "Rotproof's guide is up to date",
            [format!("{state}, {INIT_OR_CREATE}: {GUIDE}")],
        ));
    }
    found
}

/// Every broken rule of the records at `now`, grouped by `areas`: `None` when the declaration cannot be read. The
/// knowledge documents whose `stale_after` has come go in `due`, as warnings.
fn records(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    areas: Option<Vec<String>>,
    now: Time,
    due: &mut Vec<String>,
) -> io::Result<Vec<Finding>> {
    let mut found = Vec::new();
    let mut add = |check: &'static str, details: Vec<String>| found.extend(under(check, details));
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

    // The floor: the directory of knowledge and the root index exist, and the rules are found as a document. If a move
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

    // Every link a reader may follow in what Rotproof keeps: the knowledge documents and the log
    let mut documents = concepts(tree, DOCS)?;
    if let Ok((log, false)) = exactly(tree, "docs/log.md") {
        documents.insert("log.md".into(), read_text(tree, &log)?);
    }
    add(
        "every link in docs/knowledge/ and the log resolves",
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
        add("the log keeps its structure", log_problems(&log));
        add(NOT_COME, log_days_to_come(&log, now));
    } else if let Err(why) = log_path {
        add("the log keeps its structure", vec![why]);
    }
    let read = bundle.read_knowledge()?;
    let held = read
        .documents
        .into_iter()
        .map(|(name, (doc, _))| (name, doc))
        .collect();
    *due = domain::records::due(&held, now);
    add(
        "every knowledge document matches the code it follows",
        crate::follows::drift(tree, parsers, &held),
    );
    add(
        "every file in docs/knowledge/ is read",
        pairs(&unread(tree, DOCS)?),
    );
    add(
        "every knowledge document keeps the format",
        pairs(&read.problems),
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
    Ok(found)
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

/// Every knowledge document, directly in `knowledge/` of the bundle directory `docs`: its path relative to `docs` (with
/// `/`) -> text. The rest of `docs` is the project's, and is not read.
pub fn concepts(tree: &dyn Tree, docs: &str) -> io::Result<Docs> {
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
pub fn unread(tree: &dyn Tree, docs: &str) -> io::Result<BTreeMap<String, String>> {
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

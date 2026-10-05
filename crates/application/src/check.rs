//! `rotproof check`: every rule of the structure and the records, run against one repository.
//!
//! - **The tree matches `.config/rotproof.toml`**: the layers of the stack are present or declared absent, and no code
//!   sits outside them (`structure.rs`).
//! - **The layers import only what the table allows** (`direction.rs`): the imports of `python` and `typescript`, and
//!   the dependencies each crate of `rust` declares.
//! - **No comment holds a marker**, one of the words in `markers::MARKERS` (`markers.rs`): work left to do belongs in
//!   the backlog, where it is listed and closed.
//! - **Rotproof's guide is up to date**: `.rotproof/AGENTS.md` equals what `rotproof create` writes for the stack with
//!   this version of Rotproof (`project.rs`).
//! - **The backlog works as a backlog**: every document keeps the format, every link in `# Details` resolves, and every
//!   backlog item the log points to exists.
//! - **Every record has one declared area**: the areas in `.config/rotproof.toml` are distinct headings, and the one
//!   tag of every backlog item and spec is one of them.
//! - **An epic closes after its parts**: a part's `epic` names another spec, one level deep, and no closed epic has
//!   an open part.
//! - **`docs/` is one OKF bundle**: every document is a known type in the directory for its type, every file Rotproof
//!   generates (the index files and the rules) equals what `rotproof index` writes, and no spec sits at the root.
//! - **The log keeps the OKF log structure**: every second-level heading is a date, newest first, and the entries
//!   are a flat list of list items under those dates.
//!
//! Each check has a floor: when the scan finds nothing at all, it fails instead of passing with nothing checked.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::io;

use crate::bundle::Bundle;

use crate::links::broken;
use crate::tree::{exactly, read_text};
use domain::bundle::{DOCS, Docs, backlog, in_docs, is_document};
use domain::code::Parsers;
use domain::layers::{DECLARATION, area_problems};
use domain::project::GUIDE;
use domain::records::{
    dangling_backlog_refs, floor, knowledge_refs, log_problems, misplaced, root_specs, unlogged,
    unread_paths,
};
use domain::tree::Tree;
use utils::markdown::links;
use utils::paths::{file_name, join};

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
    /// Checks that did not run, and why
    pub skipped: Vec<String>,
    /// Whether the tree was checked against a stack's layers (not for `stack = "none"`)
    pub layers_checked: bool,
}

/// Every broken rule in the repository `tree` holds. An error is a file that could not be read at all.
pub fn check(tree: &dyn Tree, parsers: &dyn Parsers) -> io::Result<Report> {
    let structure = crate::structure::problems(tree)?;
    let mut findings: Vec<Finding> = structure
        .found
        .into_iter()
        .map(|detail| Finding {
            check: "the tree matches .config/rotproof.toml".into(),
            detail,
        })
        .collect();
    let skipped: Vec<String> = structure.skipped.into_iter().collect();
    let mut layers_checked = false;
    if let Some(declared) = &structure.declared {
        layers_checked = declared.layout.is_some();
        let direction = crate::direction::problems(tree, parsers, declared)?;
        findings.extend(direction.into_iter().map(|detail| Finding {
            check: "the layers import only what layers/table.toml allows".into(),
            detail,
        }));
        let markers = crate::markers::problems(tree, parsers, declared)?;
        let heading = markers.heading();
        findings.extend(markers.found.into_iter().map(|detail| Finding {
            check: heading.clone().into(),
            detail,
        }));
        let guide = domain::project::guide(&declared.declaration.stack, declared.layout.as_ref());
        let found = exactly(tree, GUIDE)
            .ok()
            .and_then(|(path, _)| read_text(tree, &path).ok());
        if found.as_ref() != Some(&guide) {
            findings.push(Finding {
                check: "Rotproof's guide is up to date".into(),
                detail: format!(
                    "{}, run `rotproof create`: {GUIDE}",
                    if found.is_some() {
                        "out of date"
                    } else {
                        "missing"
                    }
                ),
            });
        }
    }
    findings.extend(records(tree, parsers, structure.areas)?);
    Ok(Report {
        findings,
        skipped,
        layers_checked,
    })
}

/// Every broken rule of the records, grouped by `areas`: `None` when the declaration cannot be read.
fn records(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    areas: Option<Vec<String>>,
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

    // The floor: the directories and the root index exist, and the backlog rules are found as a document. If a move
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

    let docs = bundle.read_folder("backlog")?;
    let parsed = backlog(&docs, &bundle.areas);
    add(
        "every backlog document keeps the format",
        pairs(&parsed.problems),
    );
    let details: BTreeMap<String, String> = parsed
        .items
        .iter()
        .map(|(name, (_, sections))| (name.clone(), sections["Details"].clone()))
        .collect();
    add(
        "every link in # Details resolves",
        pairs(&unresolved(&details, tree, parsers)),
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
        let names = file_names(tree, &in_docs("backlog"))?;
        let dangling = dangling_backlog_refs(&log, &names);
        add(
            "the log points only at real backlog items",
            dangling
                .into_iter()
                .map(|name| format!("no such item: docs/backlog/{name}"))
                .collect(),
        );
        add("the log keeps its structure", log_problems(&log));

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
        let mut dangling: Vec<String> = refs
            .iter()
            .filter(|(name, _)| !names.contains(name))
            .map(|(name, _)| format!("no such document: docs/knowledge/{name}"))
            .collect();
        dangling.dedup();
        add("the log points only at real knowledge documents", dangling);
    } else if let Err(why) = log_path {
        add("the log keeps its structure", vec![why]);
    }

    let mut out_of_place = misplaced(&concepts(tree, DOCS)?);
    out_of_place.extend(unread(tree, DOCS)?);
    add(
        "every document is a known type in its place",
        pairs(&out_of_place),
    );
    let specs = bundle.read_specs()?;
    add("every spec keeps the format", pairs(&specs.problems));
    add(
        "every knowledge document keeps the format",
        pairs(&bundle.read_knowledge()?.problems),
    );
    add(
        "an epic closes after its parts",
        specs.closed_before_its_parts(),
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
            .map(|name| format!("specs go in docs/specs/: {name}"))
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

/// Item -> why, for the Details sections with no link or with a link that does not resolve, in the repository's
/// `tree`.
///
/// Details sit in a backlog item (`docs/backlog/<slug>.md`), so relative links resolve from there and links starting
/// with `/` from the bundle root (`docs/`).
pub fn unresolved(
    details: &BTreeMap<String, String>,
    tree: &dyn Tree,
    parsers: &dyn Parsers,
) -> BTreeMap<String, String> {
    let bundle_root = DOCS;
    let here = in_docs("backlog");
    let mut bad = BTreeMap::new();
    for (name, detail) in details {
        let found = links(detail);
        if found.is_empty() {
            bad.insert(name.clone(), format!("no markdown link: {detail}"));
            continue;
        }
        let reasons: Vec<String> = found
            .iter()
            .filter_map(|(text, target)| broken(tree, parsers, text, target, &here, bundle_root))
            .collect();
        if !reasons.is_empty() {
            bad.insert(
                name.clone(),
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
    use infrastructure::disk::Disk;
    use infrastructure::readers::Readers;

    fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn a_details_section_without_a_link_is_caught() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("docs/backlog")).unwrap();
        fs::write(root.path().join("docs/log.md"), "# Log\n").unwrap();
        let details = map(&[
            ("linked.md", "[log](/log.md)"),
            ("no link (the old form).md", "docs/log.md「somewhere」"),
            (
                "only the second link is broken.md",
                "[a](/log.md), [b](/no_such_file.md)",
            ),
        ]);
        let bad = unresolved(&details, &Disk::new(root.path()), &Readers);
        assert_eq!(
            bad.keys().collect::<Vec<_>>(),
            [
                "no link (the old form).md",
                "only the second link is broken.md"
            ]
        );
    }

    #[test]
    fn a_link_in_any_form_is_resolved() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("docs/backlog")).unwrap();
        fs::write(root.path().join("docs/log.md"), "# Log\n").unwrap();
        fs::write(root.path().join("docs/backlog/a (1).md"), "# A\n").unwrap();
        let good = "[log](/log.md)";
        let resolving = [
            "[log](/log.md \"the log\")",
            "[log](</log.md>)",
            "[a](<a (1).md>)",
            "[log][ref]\n\n[ref]: /log.md",
            "<a href=\"/log.md\">log</a>",
        ];
        for detail in resolving {
            let bad = unresolved(&map(&[("x.md", detail)]), &Disk::new(root.path()), &Readers);
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
                &map(&[("x.md", &detail)]),
                &Disk::new(root.path()),
                &Readers,
            );
            assert!(bad.contains_key("x.md"), "{form} passed");
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
            "backlog/index.md",
            "backlog/item.md",
            "specs/index.md",
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
            "specs/deeper/index.md",
            // docs/done/ is no longer read: closed specs stay in docs/specs/
            "done/index.md",
            "backlog/log.md",
            // Markdown that is not named .md
            "backlog/item.MD",
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

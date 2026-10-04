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
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use chrono::NaiveDate;
use percent_encoding::percent_decode_str;
use regex::Regex;
use sha2::{Digest, Sha256};

use crate::disk::Disk;

use crate::bundle::{Bundle, Docs, RESERVED, backlog};
use crate::frontmatter::split;
use crate::layers::{DECLARATION, area_problems, areas};
use crate::markdown::{broken, heading, links, visible};
use crate::project::GUIDE;
use crate::source::{exactly, read_source, relative_path};

/// Directory (relative to docs/, "" for the root) -> the document types allowed in it
const TYPES: [(&str, &[&str]); 4] = [
    ("", &["Guide"]),
    ("backlog", &["Backlog Item", "Guide"]),
    ("specs", &["Spec", "Guide"]),
    ("knowledge", &["Knowledge", "Guide"]),
];

// How the log points to a backlog item. Matched without `docs/`, so pointers written while the backlog was at the
// repository root (`backlog/<slug>.md`) still match an item by its slug. A pointer written with Windows separators
// (`docs\backlog\<slug>.md`) is a pointer too, and has to name an item that exists. So is a link that percent-encodes
// the slug (`backlog/%E6%97%A5.md`). A path without `.md` is not taken for a pointer: in prose, `backlog/` is also
// followed by words that name no file
static BACKLOG_REF: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"backlog[/\\]([\w.%-]+\.md)").unwrap());
// The `**Knowledge**` field of a log entry: its indentation and what follows the label
static KNOWLEDGE_FIELD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\s*)[*+-][ \t]+\*\*Knowledge\*\*:(.*)$").unwrap());
// A knowledge document named with its hash, written with `/` or `\`
static KNOWLEDGE_REF: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"knowledge[/\\]([\w.%-]+\.md)@([0-9a-f]{8})\b").unwrap());
// A list item at any depth, once its indentation is removed
static LIST_MARKER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[*+-](?:[ \t]|$)").unwrap());
// A file at the repository root with one of these names is taken for a spec
static ROOT_SPEC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(spec|仕様).*\.md$").unwrap());
static DATE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d{4}-\d{2}-\d{2}$").unwrap());
// A bullet list item, indented by up to 3 spaces as GFM allows
static LIST_ITEM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^ {0,3}[*+-](?:[ \t]|$)").unwrap());

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

/// Every broken rule in the repository at `root`. An error is a file that could not be read at all.
pub fn check(root: &Path) -> io::Result<Report> {
    let structure = crate::structure::problems(&Disk::new(root))?;
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
        let direction = crate::direction::problems(root, declared)?;
        findings.extend(direction.into_iter().map(|detail| Finding {
            check: "the layers import only what layers/table.toml allows".into(),
            detail,
        }));
        let markers = crate::markers::problems(root, declared)?;
        let heading = markers.heading();
        findings.extend(markers.found.into_iter().map(|detail| Finding {
            check: heading.clone().into(),
            detail,
        }));
        let guide = crate::project::guide(&declared.declaration.stack, declared.layout.as_ref());
        let found = exactly(root, GUIDE)
            .ok()
            .and_then(|path| read_source(&path).ok());
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
    findings.extend(records(root)?);
    Ok(Report {
        findings,
        skipped,
        layers_checked,
    })
}

/// Every broken rule of the records.
fn records(root: &Path) -> io::Result<Vec<Finding>> {
    let mut found = Vec::new();
    let mut add = |check: &'static str, details: Vec<String>| {
        found.extend(details.into_iter().map(|detail| Finding {
            check: check.into(),
            detail,
        }));
    };
    // Every record names an area, so nothing below can be judged without them. The structure check names what is
    // wrong with the declaration; this says that the records were not checked because of it
    let areas = match areas(&Disk::new(root))? {
        Ok(areas) => areas,
        Err(_) => {
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
    let bundle = Bundle::new(root, areas);

    // The floor: the directories and the root index exist, and the backlog rules are found as a document. If a move
    // or a rename makes the scan come back empty, the checks below see nothing and pass. Each is found by its exact
    // name: `Rules.md` is found as `rules.md` on Windows, and missing on Linux and GitHub
    let missing: Vec<String> = TYPES
        .iter()
        .map(|(folder, _)| format!("docs/{folder}"))
        .chain(
            [
                "docs/index.md",
                "docs/backlog/rules.md",
                "docs/specs/rules.md",
                "docs/knowledge/rules.md",
            ]
            .map(String::from),
        )
        .filter_map(|rel| exactly(root, rel.trim_end_matches('/')).err())
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
        pairs(&unresolved(&details, root)),
    );

    let log_path = exactly(root, "docs/log.md").and_then(|path| {
        if path.is_file() {
            Ok(path)
        } else {
            Err("docs/log.md is not a file".into())
        }
    });
    if let Ok(log_path) = log_path {
        let log = read_source(&log_path)?;
        let names = file_names(&bundle.docs.join("backlog"))?;
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
            let path = bundle.docs.join("knowledge").join(name);
            documents.insert(name.clone(), read_source(&path)?);
        }
        add(
            "the log names every knowledge document as it is now",
            unlogged(&documents, &refs),
        );
        let names = file_names(&bundle.docs.join("knowledge"))?;
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

    let mut out_of_place = misplaced(&concepts(&bundle.docs)?);
    out_of_place.extend(unread(&bundle.docs)?);
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
            let found = exactly(root, &relative_path(path, root)).ok();
            found.and_then(|path| read_source(&path).ok()).as_ref() != Some(text)
        })
        .map(|(path, _)| {
            format!(
                "out of date, run `rotproof index`: {}",
                relative_path(&path, root)
            )
        })
        .collect();
    add("every generated file is up to date", stale);
    let names = file_names(root)?;
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
fn file_names(dir: &Path) -> io::Result<Vec<String>> {
    fs::read_dir(dir)?
        .map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().into_owned()))
        .collect()
}

fn pairs(problems: &BTreeMap<String, String>) -> Vec<String> {
    problems
        .iter()
        .map(|(name, why)| format!("{name}: {why}"))
        .collect()
}

/// Item -> why, for the Details sections with no link or with a link that does not resolve. `root` is the root of the
/// repository.
///
/// Details sit in a backlog item (`docs/backlog/<slug>.md`), so relative links resolve from there and links starting
/// with `/` from the bundle root (`docs/`).
pub fn unresolved(details: &BTreeMap<String, String>, root: &Path) -> BTreeMap<String, String> {
    let bundle_root = root.join("docs");
    let here = bundle_root.join("backlog");
    let mut bad = BTreeMap::new();
    for (name, detail) in details {
        let found = links(detail);
        if found.is_empty() {
            bad.insert(name.clone(), format!("no markdown link: {detail}"));
            continue;
        }
        let reasons: Vec<String> = found
            .iter()
            .filter_map(|(text, target)| broken(text, target, &here, &bundle_root))
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

/// The hash the log names a knowledge document by: the first 8 hex digits of SHA-256 of its text, as `read_source`
/// reads it (every line ending as `\n`, so a checkout with CRLF has the same hash).
pub fn content_hash(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .take(4)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Every knowledge document the log names, as (file name, hash), in the `**Knowledge**` field of its entries: the label
/// line, and the lines under it that continue it (wrapped, not a list item of their own). A name is percent-decoded,
/// as a link may write it.
pub fn knowledge_refs(log: &str) -> Vec<(String, String)> {
    let mut text = String::new();
    let mut field: Option<usize> = None;
    for line in log.lines() {
        let indent = line.len() - line.trim_start().len();
        if let Some(caps) = KNOWLEDGE_FIELD.captures(line) {
            field = Some(caps[1].len());
            text.push_str(&caps[2]);
            text.push('\n');
            continue;
        }
        let continues = field.is_some_and(|label| {
            !line.trim().is_empty() && indent > label && !LIST_MARKER.is_match(line.trim_start())
        });
        if continues {
            text.push_str(line);
            text.push('\n');
        } else {
            field = None;
        }
    }
    KNOWLEDGE_REF
        .captures_iter(&text)
        .map(|caps| {
            (
                percent_decode_str(&caps[1])
                    .decode_utf8_lossy()
                    .into_owned(),
                caps[2].to_string(),
            )
        })
        .collect()
}

/// The knowledge documents (file name -> text) that no log entry names with their current hash, each with the line to
/// write.
pub fn unlogged(documents: &BTreeMap<String, String>, refs: &[(String, String)]) -> Vec<String> {
    documents
        .iter()
        .filter_map(|(name, text)| {
            let hash = content_hash(text);
            let named = refs.iter().any(|(n, h)| n == name && *h == hash);
            (!named).then(|| {
                format!(
                    "knowledge/{name}: no log entry names it as it is now. In the log entry of this change, write \
                     `* **Knowledge**: knowledge/{name}@{hash}`"
                )
            })
        })
        .collect()
}

/// The `docs/backlog/<slug>.md` the log points to that are not among `names` (the file names in `docs/backlog/`).
pub fn dangling_backlog_refs(log: &str, names: &[String]) -> Vec<String> {
    let mut dangling: Vec<String> = BACKLOG_REF
        .captures_iter(log)
        .map(|caps| {
            percent_decode_str(&caps[1])
                .decode_utf8_lossy()
                .into_owned()
        })
        .filter(|name| !names.contains(name))
        .collect();
    dangling.sort();
    dangling.dedup();
    dangling
}

/// What breaks the log structure, in reading order.
///
/// Every visible line below the title is a date heading or part of a list item under one. A new log holds only its
/// title and HTML comments, or nothing, and passes. Anything else visible is checked as an entry, so text alone, or
/// dates at another heading level, fails instead of passing with no date checked (the floor).
pub fn log_problems(text: &str) -> Vec<String> {
    let shown = visible(text);
    let mut found = Vec::new();
    let mut last: Option<NaiveDate> = None;
    let dates = shown
        .lines()
        .filter_map(heading)
        .filter(|(level, _)| *level == 2);
    for (_, heading) in dates {
        let day = DATE
            .is_match(heading)
            .then(|| NaiveDate::parse_from_str(heading, "%Y-%m-%d").ok())
            .flatten();
        let Some(day) = day else {
            found.push(format!("not a YYYY-MM-DD date heading: ## {heading}"));
            continue;
        };
        if let Some(previous) = last
            && day >= previous
        {
            found.push(format!(
                "not newest first: ## {heading} comes after ## {previous}"
            ));
        }
        last = Some(day);
    }
    let mut lines = shown
        .lines()
        .filter(|line| !line.trim().is_empty())
        .peekable();
    // The title: a first-level heading on the first line, unless it is a date (an entry one level up)
    lines.next_if(|line| {
        heading(line).is_some_and(|(level, title)| level == 1 && !DATE.is_match(title))
    });
    // OKF 0.2 (section 9): a flat list of entries grouped under the date headings. An entry is a list item; its
    // indented lines (wrapped text, nested items) belong to it. Anything else, a `### <task>` heading included, is not
    // an entry
    let (mut in_group, mut in_item) = (false, false);
    for line in lines {
        if let Some((level, _)) = heading(line) {
            in_item = false;
            if level == 2 {
                in_group = true;
            } else {
                found.push(format!(
                    "a heading other than ## YYYY-MM-DD in the log (entries are list items): {}",
                    line.trim()
                ));
            }
        } else if LIST_ITEM.is_match(line) {
            in_item = true;
            if !in_group {
                found.push(format!(
                    "an entry outside a ## YYYY-MM-DD group in the log: {}",
                    line.trim()
                ));
            }
        } else if !(in_item && line.starts_with([' ', '\t'])) {
            found.push(format!(
                "not a list entry under a date in the log: {}",
                line.trim()
            ));
        }
    }
    found
}

/// Every document under `docs/`: path relative to `docs/` (with `/`) -> text.
pub fn concepts(docs: &Path) -> io::Result<Docs> {
    let mut out = Docs::new();
    for (path, full) in files(docs)? {
        let name = file_name(&path);
        if name.ends_with(".md") && !RESERVED.contains(&name) {
            out.insert(path, read_source(&full)?);
        }
    }
    Ok(out)
}

/// Path -> why, for the files under `docs/` that a reader takes for part of the bundle and Rotproof would not read.
///
/// A reserved name (OKF 0.2, section 3.1) is read only where Rotproof writes or reads it: an `index.md` in a directory
/// that holds documents, and `log.md` at the root. Anywhere else, OKF says it follows the structure of an index or a
/// log, and nothing would check that. A markdown file whose extension is not `.md` in lowercase (`.MD`) is shown by
/// GitHub, but not read as a document, so a broken one would pass.
pub fn unread(docs: &Path) -> io::Result<BTreeMap<String, String>> {
    let mut bad = BTreeMap::new();
    for (path, _) in files(docs)? {
        let name = file_name(&path);
        let folder = path.rsplit_once('/').map_or("", |(folder, _)| folder);
        let read = match name {
            "index.md" => TYPES.iter().any(|(known, _)| *known == folder),
            "log.md" => folder.is_empty(),
            _ => true,
        };
        if !read {
            bad.insert(
                path,
                "a reserved name outside the places Rotproof writes and reads (index.md in docs/ and in each directory \
                 of documents, log.md in docs/)"
                    .into(),
            );
        } else if !name.ends_with(".md")
            && name
                .rsplit_once('.')
                .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("md"))
        {
            bad.insert(
                path,
                "a markdown file is named with .md, in lowercase".into(),
            );
        }
    }
    Ok(bad)
}

fn file_name(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, name)| name)
}

/// Every file under `dir`: path relative to `dir` (with `/`) -> full path.
fn files(dir: &Path) -> io::Result<Vec<(String, PathBuf)>> {
    let mut out = Vec::new();
    walk(dir, "", &mut out)?;
    Ok(out)
}

fn walk(dir: &Path, prefix: &str, out: &mut Vec<(String, PathBuf)>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = format!("{prefix}{name}");
        if entry.file_type()?.is_dir() {
            walk(&entry.path(), &format!("{path}/"), out)?;
        } else {
            out.push((path, entry.path()));
        }
    }
    Ok(())
}

/// Path -> why the document is not a known type in its directory.
pub fn misplaced(docs: &Docs) -> BTreeMap<String, String> {
    let mut bad = BTreeMap::new();
    for (path, text) in docs {
        let folder = path.rsplit_once('/').map_or("", |(folder, _)| folder);
        let kind = match split(text) {
            Ok((meta, _)) => match meta.get(&yaml_rust2::Yaml::String("type".into())) {
                Some(yaml_rust2::Yaml::String(kind)) => Some(kind.clone()),
                _ => None,
            },
            Err(why) => {
                bad.insert(path.clone(), why);
                continue;
            }
        };
        match TYPES.iter().find(|(name, _)| *name == folder) {
            None => {
                bad.insert(
                    path.clone(),
                    format!("no document belongs in docs/{folder}/"),
                );
            }
            Some((_, allowed)) if !kind.as_deref().is_some_and(|kind| allowed.contains(&kind)) => {
                let shown = if folder.is_empty() { "." } else { folder };
                // The type as written, quoted; a missing or non-text type said in words, not as Rust's `None`
                let kind = kind.map_or("no type".into(), |kind| format!("type {kind:?}"));
                bad.insert(
                    path.clone(),
                    format!(
                        "{kind} does not belong in docs/{shown} ({})",
                        allowed.join(", ")
                    ),
                );
            }
            Some(_) => {}
        }
    }
    bad
}

/// The names that look like a spec, among the files at the repository root.
pub fn root_specs(names: &[String]) -> Vec<String> {
    let mut found: Vec<String> = names
        .iter()
        .filter(|name| ROOT_SPEC.is_match(name))
        .cloned()
        .collect();
    found.sort();
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hash_is_the_first_8_hex_digits_of_sha_256() {
        // printf 'abc' | sha256sum
        assert_eq!(content_hash("abc"), "ba7816bf");
        assert_eq!(content_hash("line\n"), "c73b73af");
    }

    #[test]
    fn the_knowledge_field_is_read_with_its_wrapped_lines_only() {
        let log = "\
# Log

## 2026-10-03

* **A task**
  * **Changes**: knowledge/not-a-field.md@00000000 is in another field
  * **Knowledge**: knowledge/api.md@a3f9c1d2, knowledge\\model.md@0123abcd,
    knowledge/%E6%97%A5.md@ffffffff
  * **Lessons**: knowledge/after.md@11111111 is not in the field
* **Knowledge**: knowledge/top.md@22222222
- **Knowledge**:
  knowledge/next-line.md@33333333
";
        assert_eq!(
            knowledge_refs(log),
            [
                ("api.md", "a3f9c1d2"),
                ("model.md", "0123abcd"),
                ("日.md", "ffffffff"),
                ("top.md", "22222222"),
                ("next-line.md", "33333333"),
            ]
            .map(|(n, h)| (n.to_string(), h.to_string()))
        );
        // A hash that is not 8 hex digits names nothing
        assert_eq!(
            knowledge_refs("* **Knowledge**: knowledge/a.md@a3f9c1, knowledge/b.md@A3F9C1D2\n"),
            Vec::<(String, String)>::new()
        );
    }

    #[test]
    fn a_knowledge_document_passes_only_with_its_current_hash_in_the_log() {
        let documents = map(&[("api.md", "abc")]);
        let named = |hash: &str| vec![("api.md".to_string(), hash.to_string())];
        assert_eq!(
            unlogged(&documents, &named("ba7816bf")),
            Vec::<String>::new()
        );
        // Edited after its entry: the old hash names an older text
        let found = unlogged(&documents, &named("c73b73af"));
        assert_eq!(
            found,
            [
                "knowledge/api.md: no log entry names it as it is now. In the log entry of this change, write \
              `* **Knowledge**: knowledge/api.md@ba7816bf`"
            ]
        );
        // Never named
        assert_eq!(unlogged(&documents, &[]).len(), 1);
    }

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
        let bad = unresolved(&details, root.path());
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
            let bad = unresolved(&map(&[("x.md", detail)]), root.path());
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
            let bad = unresolved(&map(&[("x.md", &detail)]), root.path());
            assert!(bad.contains_key("x.md"), "{form} passed");
        }
    }

    #[test]
    fn a_dangling_log_pointer_is_caught() {
        let log = "## 2026-10-01\n\n### Something\n- **Open items**: docs/backlog/rules.md, \
                   docs/backlog/no-such-item.md, backlog/rules.md, docs\\backlog\\rules.md, \
                   docs\\backlog\\written-on-windows.md, [日](/backlog/%E6%97%A5.md), \
                   [月](/backlog/%E6%9C%88.md)\n";
        let names = ["rules.md", "index.md", "日.md"].map(String::from).to_vec();
        // A link to a slug outside ASCII is percent-encoded, and points at the item all the same
        assert_eq!(
            dangling_backlog_refs(log, &names),
            ["no-such-item.md", "written-on-windows.md", "月.md"]
        );
    }

    #[test]
    fn the_log_structure_is_checked() {
        let good = [
            (
                "entries",
                "# Log\n\n<!--\n## YYYY-MM-DD\n-->\n\n## 2026-10-02\n\n* **b**\n  * **Branch**: main\n    wrapped\n\n## 2026-10-01\n\n- a\n\n  more of a\n\n```\n## x\n```\n",
            ),
            // A new repository has no entries yet: the title and the format guide, or nothing at all
            (
                "only the title and the guide",
                "# Log\n\n<!--\n### <Task name>\n-->\n",
            ),
            ("empty", ""),
            // GitHub shows these as the same heading
            (
                "a date heading with spaces around it",
                "  ## 2026-10-02 \n\n## 2026-10-01 ##\n",
            ),
            ("an indented title", " # Log\n"),
        ];
        for (name, text) in good {
            assert_eq!(log_problems(text), Vec::<String>::new(), "{name}");
        }
        let bad = [
            ("task on the date line", "## 2026-10-01 a task\n"),
            ("not a real date", "## 2026-13-01\n"),
            ("oldest first", "## 2026-10-01\n\n## 2026-10-02\n"),
            ("same date twice", "## 2026-10-01\n\n## 2026-10-01\n"),
            ("digits of another script", "## ２０２６-10-01\n"),
            // The floor: text under the title means entries, and entries sit under a date
            ("text without a date", "# Log\n\nNo headings, just text.\n"),
            ("dates one level up", "# Log\n\n# 2026-10-02\n\n* a\n"),
            // OKF 0.2 (section 9): entries are a flat list under the dates, not sections
            (
                "a task heading",
                "## 2026-10-02\n\n### a task\n\n- **Branch**: main\n",
            ),
            (
                "a paragraph under a date",
                "## 2026-10-02\n\nDid something.\n",
            ),
            (
                "an entry before the first date",
                "# Log\n\n* early\n\n## 2026-10-02\n\n* a\n",
            ),
            ("dates one level down", "# Log\n\n### 2026-10-02\n"),
            ("a date for the title", "# 2026-10-02\n"),
            // GFM lets a heading be indented by up to 3 spaces
            (
                "an indented date heading out of order",
                "## 2026-10-01\n\n  ## 2026-10-02\n",
            ),
            (
                "an indented heading that is not a date",
                "## 2026-10-02\n\n   ## not a date\n",
            ),
        ];
        let passed: Vec<&str> = bad
            .iter()
            .filter(|(_, text)| log_problems(text).is_empty())
            .map(|(name, _)| *name)
            .collect();
        assert!(passed.is_empty(), "passed: {passed:?}");
    }

    const SPEC: &str = "---\ntype: Spec\ntitle: Something\ndescription: One sentence.\nstatus: stable\n---\n\n# Goals\n";

    #[test]
    fn a_known_type_in_its_place_passes() {
        let docs = map(&[
            ("specs/good.md", SPEC),
            ("backlog/rules.md", "---\ntype: Guide\n---\n"),
            ("guide.md", "---\ntype: Guide\n---\n"),
        ]);
        assert_eq!(misplaced(&docs), BTreeMap::new());
    }

    #[test]
    fn a_misplaced_document_is_caught() {
        let item = "---\ntype: Backlog Item\n---\n";
        let bad = map(&[
            ("specs/item.md", item),
            ("item.md", item),
            ("other/x.md", SPEC),
            ("backlog/plain.md", "# Just markdown\n"),
            ("backlog/no-type.md", "---\ntitle: x\n---\n"),
            ("specs/deeper/x.md", SPEC),
        ]);
        let found = misplaced(&bad);
        assert_eq!(
            found.keys().collect::<Vec<_>>(),
            bad.keys().collect::<Vec<_>>()
        );
        // The type is said as written, and a missing one in words
        assert_eq!(
            found["specs/item.md"],
            "type \"Backlog Item\" does not belong in docs/specs (Spec, Guide)"
        );
        assert_eq!(
            found["backlog/no-type.md"],
            "no type does not belong in docs/backlog (Backlog Item, Guide)"
        );
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
        assert_eq!(unread(docs.path()).unwrap(), BTreeMap::new());
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
        let found = unread(docs.path()).unwrap();
        assert_eq!(found.keys().collect::<Vec<_>>(), {
            let mut sorted = bad.to_vec();
            sorted.sort();
            sorted
        });
    }

    #[test]
    fn a_root_spec_is_caught() {
        let names: Vec<String> = [
            "README.md",
            "AGENTS.md",
            "genre_spec.md",
            "仕様書.md",
            "pyproject.toml",
            "SPEC.MD",
        ]
        .map(String::from)
        .into();
        assert_eq!(
            root_specs(&names),
            ["SPEC.MD", "genre_spec.md", "仕様書.md"]
        );
    }
}

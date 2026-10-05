//! The approvals: imports the layer table forbids that a person approved, kept in one file,
//! `.config/rotproof-approved.toml`.
//!
//! - **An entry is keyed by the file that imports and what it imports,** never by the line, so moving an import does
//!   not undo its approval, and importing something else is not covered.
//! - **An approved import passes, and is printed on every run.** An approval never hides.
//! - **An entry that matches no forbidden import fails.** The import moved away, was fixed, or became allowed: the
//!   approval goes with it, through `rotproof approve --prune`, instead of waiting for the next import to cover.
//!
//! Only `rotproof approve`, on a terminal, adds an entry; Claude Code is denied editing the file. The rules here take
//! the file's text and the forbidden imports `application` found, and give back what passes and what fails.

use std::collections::BTreeSet;

use chrono::DateTime;
use serde::Deserialize;
use toml_edit::{ArrayOfTables, DocumentMut, InlineTable, Item, Table, value};

use crate::direction::Forbidden;

/// The approvals file, from the root.
pub const APPROVALS: &str = ".config/rotproof-approved.toml";

/// What the approvals file says at its top, when `rotproof approve` starts it.
const HEADER: &str = "\
# Imports the layer table forbids, kept with a person's approval. `rotproof check` passes each one and prints it on
# every run, and fails on an entry that matches no forbidden import.
# Only `rotproof approve <file> <import>`, on a terminal, adds an entry; `rotproof approve --prune` removes the entries
# that match nothing. Claude Code is denied editing this file.
";

/// An import the project keeps for good.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Kept {
    /// The file that imports, from the root: a `.py` or TypeScript file, or a crate's `Cargo.toml`
    pub from: String,
    /// What it imports, as the check names it: a Python module, a TypeScript specifier, a crate's dependency
    pub import: String,
    pub reason: String,
    pub approved: Approved,
}

/// Who approved an entry and when, as `verified` in a backlog item.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Approved {
    pub by: String,
    /// A datetime with a time zone, as RFC 3339 writes it
    pub at: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    kept: Vec<Kept>,
}

/// The entries the approvals file `text` holds, or why it cannot be read.
pub fn parse(text: &str) -> Result<Vec<Kept>, String> {
    toml::from_str::<File>(text)
        .map(|file| file.kept)
        .map_err(|e| format!("{APPROVALS}: {}", e.message()))
}

/// What is wrong with the entries, each said once: a field left empty, a time that is not a datetime with a time
/// zone, and two entries for one import.
pub fn problems(entries: &[Kept]) -> Vec<String> {
    let mut found = Vec::new();
    let mut seen = BTreeSet::new();
    for entry in entries {
        let key = format!("{} in {}", entry.import, entry.from);
        for (field, text) in [
            ("from", &entry.from),
            ("import", &entry.import),
            ("reason", &entry.reason),
            ("approved.by", &entry.approved.by),
        ] {
            if text.trim().is_empty() {
                found.push(format!("{APPROVALS}: the entry for {key} has no {field}"));
            }
        }
        if DateTime::parse_from_rfc3339(&entry.approved.at).is_err() {
            found.push(format!(
                "{APPROVALS}: the entry for {key} has approved.at {:?}, not a datetime with a time zone \
                 (2026-10-06T09:00:00+09:00)",
                entry.approved.at
            ));
        }
        if !seen.insert((entry.from.as_str(), entry.import.as_str())) {
            found.push(format!("{APPROVALS}: two entries for {key}"));
        }
    }
    found
}

/// The forbidden imports `found` against the approvals `entries`.
#[derive(Debug, PartialEq, Eq)]
pub struct Sorted<'a> {
    /// Each forbidden import an entry approves, with the entry
    pub approved: Vec<(&'a Forbidden, &'a Kept)>,
    /// Each forbidden import no entry approves
    pub forbidden: Vec<&'a Forbidden>,
    /// Each entry that approves nothing found
    pub stale: Vec<&'a Kept>,
}

/// Sort `found` by the approvals `entries`: an entry approves every forbidden import of its import in its file, on any
/// line.
pub fn sort<'a>(found: &'a [Forbidden], entries: &'a [Kept]) -> Sorted<'a> {
    let entry_for = |f: &Forbidden| {
        entries
            .iter()
            .find(|entry| entry.from == f.file && entry.import == f.import)
    };
    let mut sorted = Sorted {
        approved: Vec::new(),
        forbidden: Vec::new(),
        stale: Vec::new(),
    };
    for f in found {
        match entry_for(f) {
            Some(entry) => sorted.approved.push((f, entry)),
            None => sorted.forbidden.push(f),
        }
    }
    sorted.stale = entries
        .iter()
        .filter(|entry| {
            !found
                .iter()
                .any(|f| entry.from == f.file && entry.import == f.import)
        })
        .collect();
    sorted
}

/// How an approved import is printed on every run.
pub fn shown(entry: &Kept) -> String {
    format!(
        "{} in {}: {} ({}, {})",
        entry.import, entry.from, entry.reason, entry.approved.by, entry.approved.at
    )
}

/// The finding for an entry that matches no forbidden import.
pub fn stale(entry: &Kept) -> String {
    format!(
        "{APPROVALS}: approved, but no such forbidden import: {} in {} (it moved away, was fixed, or became allowed). \
         Run `rotproof approve --prune`",
        entry.import, entry.from
    )
}

/// The approvals file `text` (`None` when it does not exist yet) with `entry` added at its end. The comments and the
/// entries already there are kept.
pub fn with_kept(text: Option<&str>, entry: &Kept) -> Result<String, String> {
    let mut document = text
        .unwrap_or(HEADER)
        .parse::<DocumentMut>()
        .map_err(|e| format!("{APPROVALS}: {e}"))?;
    let mut table = Table::new();
    table.insert("from", value(&entry.from));
    table.insert("import", value(&entry.import));
    table.insert("reason", value(&entry.reason));
    let mut approved = InlineTable::new();
    approved.insert("by", entry.approved.by.as_str().into());
    approved.insert("at", entry.approved.at.as_str().into());
    table.insert("approved", value(approved));
    match document.get_mut("kept") {
        Some(Item::ArrayOfTables(kept)) => {
            table.decor_mut().set_prefix("\n");
            kept.push(table);
        }
        Some(_) => return Err(format!("{APPROVALS}: kept is not a list of entries")),
        None => {
            // A file of comments only holds them after its last item: they go before the first entry, where they were
            let comments = trailing(&document);
            document.set_trailing("");
            table.decor_mut().set_prefix(format!("{comments}\n"));
            let mut kept = ArrayOfTables::new();
            kept.push(table);
            document.insert("kept", Item::ArrayOfTables(kept));
        }
    }
    Ok(document.to_string())
}

/// The approvals file `text` without the entries in `gone`. Every other entry is kept, and every comment: the ones
/// above a removed entry go above the next entry, or to the end of the file when none is left.
pub fn without(text: &str, gone: &[&Kept]) -> Result<String, String> {
    let mut document = text
        .parse::<DocumentMut>()
        .map_err(|e| format!("{APPROVALS}: {e}"))?;
    let Some(Item::ArrayOfTables(kept)) = document.get("kept") else {
        return Ok(document.to_string());
    };
    let mut left = ArrayOfTables::new();
    let mut carried = String::new();
    for table in kept.iter() {
        let field = |name: &str| table.get(name).and_then(|item| item.as_str());
        let prefix = table
            .decor()
            .prefix()
            .and_then(|p| p.as_str())
            .unwrap_or("")
            .to_string();
        if gone
            .iter()
            .any(|g| field("from") == Some(&g.from) && field("import") == Some(&g.import))
        {
            carried.push_str(prefix.trim_end_matches('\n'));
            if !carried.is_empty() {
                carried.push('\n');
            }
            continue;
        }
        let mut table = table.clone();
        if !carried.is_empty() {
            table
                .decor_mut()
                .set_prefix(format!("{carried}{}", prefix.trim_start_matches('\n')));
            carried.clear();
        }
        left.push(table);
    }
    if left.is_empty() {
        document.remove("kept");
        let end = trailing(&document);
        document.set_trailing(format!("{carried}{end}"));
    } else {
        document.insert("kept", Item::ArrayOfTables(left));
        if !carried.is_empty() {
            let end = trailing(&document);
            document.set_trailing(format!("{end}{carried}"));
        }
    }
    Ok(document.to_string())
}

/// What `document` holds after its last item: comments and blank lines.
fn trailing(document: &DocumentMut) -> String {
    document.trailing().as_str().unwrap_or("").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kept(from: &str, import: &str) -> Kept {
        Kept {
            from: from.into(),
            import: import.into(),
            reason: "the framework loads it".into(),
            approved: Approved {
                by: "someone".into(),
                at: "2026-10-06T09:00:00+09:00".into(),
            },
        }
    }

    fn forbidden(file: &str, line: usize, import: &str) -> Forbidden {
        Forbidden {
            file: file.into(),
            import: import.into(),
            detail: format!("{file}:{line}: imports {import}"),
        }
    }

    #[test]
    fn an_entry_written_reads_back_and_keeps_what_was_there() {
        let one = with_kept(None, &kept("domain/model.py", "infrastructure.db")).unwrap();
        assert!(one.starts_with(HEADER));
        assert_eq!(
            parse(&one).unwrap(),
            [kept("domain/model.py", "infrastructure.db")]
        );
        let two = with_kept(
            Some(&one.replace(HEADER, "# our own comment\n")),
            &kept("ui/atoms/x.py", "application"),
        )
        .unwrap();
        assert!(two.starts_with("# our own comment\n"), "{two}");
        assert_eq!(parse(&two).unwrap().len(), 2);
        assert!(problems(&parse(&two).unwrap()).is_empty());
    }

    #[test]
    fn an_entry_approves_its_import_in_its_file_on_any_line_and_nothing_else() {
        let found = [
            forbidden("domain/model.py", 3, "infrastructure.db"),
            forbidden("domain/model.py", 9, "infrastructure.db"),
            forbidden("domain/model.py", 4, "application.play"),
            forbidden("domain/other.py", 1, "infrastructure.db"),
        ];
        let entries = [
            kept("domain/model.py", "infrastructure.db"),
            kept("domain/gone.py", "infrastructure.db"),
        ];
        let sorted = sort(&found, &entries);
        assert_eq!(sorted.approved.len(), 2);
        assert_eq!(
            sorted
                .forbidden
                .iter()
                .map(|f| f.detail.as_str())
                .collect::<Vec<_>>(),
            [
                "domain/model.py:4: imports application.play",
                "domain/other.py:1: imports infrastructure.db"
            ]
        );
        assert_eq!(sorted.stale, [&entries[1]]);
    }

    #[test]
    fn a_broken_entry_is_named() {
        let mut empty = kept("domain/model.py", "infrastructure.db");
        empty.reason = " ".into();
        empty.approved.at = "2026-10-06".into();
        let found = problems(&[empty.clone(), empty]);
        assert_eq!(found.len(), 5, "{found:?}");
        assert!(found[0].ends_with("has no reason"));
        assert!(found[1].contains("not a datetime with a time zone"));
        assert!(found[4].contains("two entries"));
        // A field the file does not know fails, so a misspelled one is not dropped in silence
        assert!(
            parse("[[kept]]\nfrom = \"a\"\nimport = \"b\"\nreason = \"c\"\nbecause = \"d\"\n")
                .unwrap_err()
                .contains("because")
        );
    }

    #[test]
    fn pruning_removes_only_the_entries_named() {
        let text = with_kept(None, &kept("a.py", "x")).unwrap();
        let text = with_kept(Some(&text), &kept("b.py", "y")).unwrap();
        let gone = kept("a.py", "x");
        let left = without(&text, &[&gone]).unwrap();
        assert_eq!(parse(&left).unwrap(), [kept("b.py", "y")]);
        assert!(left.starts_with(HEADER));
        let none = without(&left, &[&kept("b.py", "y")]).unwrap();
        assert_eq!(none, HEADER);
        assert_eq!(parse(&none).unwrap(), []);
    }
}

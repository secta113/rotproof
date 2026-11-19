//! The baseline: the violations a project had when it froze them, kept in one file, `.config/rotproof-baseline.toml`,
//! which `rotproof baseline` writes once.
//!
//! - **An entry is keyed as an approval is,** by the file that imports and what it imports as `rotproof check` names it
//!   (the entry of Rotproof's table, for the I/O of the inner layers), never by the line. It has no reason and no name:
//!   nobody decided to keep it, it was there.
//! - **A baselined violation passes, and nothing is printed for it** but one line with how many entries remain. A new
//!   violation fails.
//! - **The baseline only shrinks.** An entry that matches nothing fails, and `rotproof baseline --prune` removes
//!   it. The file records how many entries it was written with (`count`), and `rotproof check` fails on more. That is
//!   a count that detects an increase, not a barrier, as `approved_max` is for approvals: a file written again larger
//!   shows in the diff.
//!
//! A forbidden import an approval covers is the approval's: the baseline is looked at for what the approvals left, so
//! an entry for it matches nothing.

use std::collections::BTreeSet;

use serde::Deserialize;
use toml_edit::{DocumentMut, value};

use crate::approvals::remove_entries;
use crate::direction::Forbidden;

/// The baseline file, from the root.
pub const BASELINE: &str = ".config/rotproof-baseline.toml";

/// The heading the entries that match nothing are printed under.
pub const HEADING: &str = "every baseline entry matches a forbidden import";

/// The heading the count is printed under.
pub const COUNT_HEADING: &str = "the baseline stays within its count";

/// What the baseline file says at its top.
const HEADER: &str = "\
# The violations the project had when `rotproof baseline` froze them: forbidden imports and uses of the I/O the inner
# layers do none of, each as `rotproof check` names it. `rotproof check` passes them, and fails on any other, on an
# entry that matches nothing, and on more entries than `count`, which is the number written. The file only shrinks:
# fix a violation and run `rotproof baseline --prune`, which removes the entries that match nothing and lowers `count`.
# Never write this file again and never raise `count` to pass the check.
";

/// A violation the project froze.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    /// The file that imports, from the root
    pub from: String,
    /// What it imports, as the check names it, as an approval's `import` is
    pub import: String,
}

/// What the baseline file holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Baseline {
    /// How many entries the file allows: the number it was written with, lowered by `rotproof baseline --prune`
    pub count: usize,
    pub entries: Vec<Entry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    count: Option<u32>,
    #[serde(default)]
    entry: Vec<Entry>,
}

/// The baseline the file `text` holds, or why it cannot be read.
pub fn parse(text: &str) -> Result<Baseline, String> {
    let file = toml::from_str::<File>(text).map_err(|e| format!("{BASELINE}: {}", e.message()))?;
    let count = file.count.ok_or_else(|| {
        format!(
            "{BASELINE}: count is missing: it is the number of entries the file was written with"
        )
    })?;
    Ok(Baseline {
        count: usize::try_from(count).unwrap_or(usize::MAX),
        entries: file.entry,
    })
}

/// What is wrong with the entries, each said once: a field left empty and two entries for one import.
pub fn problems(entries: &[Entry]) -> Vec<String> {
    let mut found = Vec::new();
    let mut seen = BTreeSet::new();
    for entry in entries {
        let key = format!("{} in {}", entry.import, entry.from);
        for (field, text) in [("from", &entry.from), ("import", &entry.import)] {
            if text.trim().is_empty() {
                found.push(format!("{BASELINE}: the entry for {key} has no {field}"));
            }
        }
        if !seen.insert((entry.from.as_str(), entry.import.as_str())) {
            found.push(format!("{BASELINE}: two entries for {key}"));
        }
    }
    found
}

/// The forbidden imports `found` against the baseline `entries`.
#[derive(Debug, PartialEq, Eq)]
pub struct Sorted<'a> {
    /// Each forbidden import an entry freezes
    pub baselined: Vec<&'a Forbidden>,
    /// Each forbidden import no entry freezes: a new violation
    pub forbidden: Vec<&'a Forbidden>,
    /// Each entry that freezes nothing found
    pub stale: Vec<&'a Entry>,
}

/// Sort `found`, what the approvals left forbidden, by the baseline `entries`: an entry freezes every forbidden import
/// of its import in its file, on any line.
pub fn sort<'a>(found: &[&'a Forbidden], entries: &'a [Entry]) -> Sorted<'a> {
    let mut sorted = Sorted {
        baselined: Vec::new(),
        forbidden: Vec::new(),
        stale: Vec::new(),
    };
    for f in found {
        let frozen = entries
            .iter()
            .any(|entry| entry.from == f.file && entry.import == f.import);
        if frozen {
            sorted.baselined.push(f);
        } else {
            sorted.forbidden.push(f);
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

/// The finding for an entry that matches no forbidden import.
pub fn stale(entry: &Entry) -> String {
    format!(
        "{BASELINE}: baselined, but no such forbidden import: {} in {} (it was fixed, moved, allowed or approved). Run \
         `rotproof baseline --prune`",
        entry.import, entry.from
    )
}

/// What `rotproof check` finds when the baseline holds more entries than the `count` it was written with. Only a person
/// who asked for it raises it, and the baseline is written once: the file is not written again, nor an entry added.
pub fn over_count(entries: usize, count: usize) -> Option<String> {
    (entries > count).then(|| {
        format!(
            "{BASELINE}: {entries} entries, and count is {count}, the number it was written with: the baseline only \
             shrinks. Remove the entries that were added; never raise count or write the baseline again to pass the \
             check"
        )
    })
}

/// The one line `rotproof check` prints on every run when the project has a baseline: how many entries remain.
pub fn remaining(entries: usize) -> String {
    match entries {
        0 => format!("baseline: no entry remains in {BASELINE}: remove the file"),
        1 => format!("baseline: 1 entry remains in {BASELINE}"),
        n => format!("baseline: {n} entries remain in {BASELINE}"),
    }
}

/// The text of a new baseline file with `found` frozen, each pair of a file and an import once, in the byte order of
/// the two, and `count` the number of them. `None` when there is nothing to freeze.
pub fn written(found: &[&Forbidden]) -> Option<(String, usize)> {
    let keys: BTreeSet<(&str, &str)> = found
        .iter()
        .map(|f| (f.file.as_str(), f.import.as_str()))
        .collect();
    if keys.is_empty() {
        return None;
    }
    let mut text = format!("{HEADER}\ncount = {}\n", keys.len());
    for (from, import) in &keys {
        text.push_str(&format!(
            "\n[[entry]]\nfrom = {}\nimport = {}\n",
            toml_edit::Value::from(*from),
            toml_edit::Value::from(*import)
        ));
    }
    Some((text, keys.len()))
}

/// The baseline file `text` without the entries in `gone`, and with `count` the number of entries left. Every other
/// entry is kept, and every comment: the ones above a removed entry go above the next entry, or to the end of the file
/// when none is left.
pub fn without(text: &str, gone: &[&Entry]) -> Result<String, String> {
    let mut document = text
        .parse::<DocumentMut>()
        .map_err(|e| format!("{BASELINE}: {e}"))?;
    let keys: Vec<(&str, &str)> = gone
        .iter()
        .map(|g| (g.from.as_str(), g.import.as_str()))
        .collect();
    remove_entries(&mut document, "entry", &keys);
    let left = document
        .get("entry")
        .and_then(|item| item.as_array_of_tables())
        .map_or(0, |entries| entries.len());
    let count = i64::try_from(left).unwrap_or(i64::MAX);
    match document.get_mut("count") {
        Some(item) => {
            let decor = item.as_value().map(|v| v.decor().clone());
            let mut counted = value(count);
            if let (Some(decor), Some(new)) = (decor, counted.as_value_mut()) {
                *new.decor_mut() = decor;
            }
            *item = counted;
        }
        None => return Err(format!("{BASELINE}: count is missing")),
    }
    Ok(document.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::direction::Rule;

    fn forbidden(file: &str, line: usize, import: &str) -> Forbidden {
        Forbidden {
            file: file.into(),
            import: import.into(),
            detail: format!("{file}:{line}: imports {import}"),
            rule: Rule::Direction,
        }
    }

    fn entry(from: &str, import: &str) -> Entry {
        Entry {
            from: from.into(),
            import: import.into(),
        }
    }

    #[test]
    fn a_baseline_written_reads_back_with_each_pair_once_in_order_and_its_count() {
        let found = [
            forbidden("domain/b.py", 3, "infrastructure.db"),
            forbidden("domain/a.py", 9, "infrastructure.db"),
            forbidden("domain/a.py", 1, "infrastructure.db"),
            forbidden("domain/a.py", 4, "application.play"),
        ];
        let found: Vec<&Forbidden> = found.iter().collect();
        let (text, count) = written(&found).unwrap();
        assert_eq!(count, 3);
        assert!(text.starts_with(HEADER), "{text}");
        let read = parse(&text).unwrap();
        assert_eq!(read.count, 3);
        assert_eq!(
            read.entries,
            [
                entry("domain/a.py", "application.play"),
                entry("domain/a.py", "infrastructure.db"),
                entry("domain/b.py", "infrastructure.db"),
            ]
        );
        assert!(problems(&read.entries).is_empty());
        assert_eq!(written(&[]), None);
    }

    #[test]
    fn an_entry_freezes_its_import_in_its_file_on_any_line_and_nothing_else() {
        let found = [
            forbidden("domain/model.py", 3, "infrastructure.db"),
            forbidden("domain/model.py", 9, "infrastructure.db"),
            forbidden("domain/model.py", 4, "application.play"),
            forbidden("domain/other.py", 1, "infrastructure.db"),
        ];
        let found: Vec<&Forbidden> = found.iter().collect();
        let entries = [
            entry("domain/model.py", "infrastructure.db"),
            entry("domain/gone.py", "infrastructure.db"),
        ];
        let sorted = sort(&found, &entries);
        assert_eq!(sorted.baselined.len(), 2);
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
    fn more_entries_than_the_count_is_a_finding_and_fewer_is_not() {
        assert_eq!(over_count(0, 0), None);
        assert_eq!(over_count(3, 3), None);
        assert_eq!(over_count(2, 3), None);
        let found = over_count(4, 3).unwrap();
        assert!(
            found.starts_with(".config/rotproof-baseline.toml: 4 entries, and count is 3,"),
            "{found}"
        );
    }

    #[test]
    fn the_line_says_how_many_remain_and_that_a_spent_file_is_removed() {
        assert_eq!(
            remaining(12),
            "baseline: 12 entries remain in .config/rotproof-baseline.toml"
        );
        assert_eq!(
            remaining(1),
            "baseline: 1 entry remains in .config/rotproof-baseline.toml"
        );
        assert!(remaining(0).ends_with("remove the file"));
    }

    #[test]
    fn pruning_removes_only_the_entries_named_and_lowers_the_count_to_what_is_left() {
        let found = [
            forbidden("a.py", 1, "x"),
            forbidden("b.py", 1, "y"),
            forbidden("c.py", 1, "z"),
        ];
        let found: Vec<&Forbidden> = found.iter().collect();
        let (text, _) = written(&found).unwrap();
        let left = without(&text, &[&entry("b.py", "y")]).unwrap();
        let read = parse(&left).unwrap();
        assert_eq!(read.count, 2);
        assert_eq!(read.entries, [entry("a.py", "x"), entry("c.py", "z")]);
        assert!(left.starts_with(HEADER));
        let none = without(&left, &[&entry("a.py", "x"), &entry("c.py", "z")]).unwrap();
        let read = parse(&none).unwrap();
        assert_eq!((read.count, read.entries.len()), (0, 0));
        assert!(none.starts_with(HEADER), "{none}");
        // An entry deleted by hand leaves the count above the entries, which pruning lowers though it removes none
        let by_hand = text.replace("\n[[entry]]\nfrom = \"b.py\"\nimport = \"y\"\n", "");
        assert_eq!(parse(&by_hand).unwrap().count, 3);
        assert_eq!(parse(&without(&by_hand, &[]).unwrap()).unwrap().count, 2);
    }

    #[test]
    fn a_broken_file_is_named() {
        assert!(
            parse("[[entry]]\nfrom = \"a\"\nimport = \"b\"\n")
                .unwrap_err()
                .contains("count is missing")
        );
        assert!(
            parse("count = 1\n[[entry]]\nfrom = \"a\"\nimport = \"b\"\nreason = \"c\"\n")
                .unwrap_err()
                .contains("reason")
        );
        assert!(parse("count = -1\n").is_err());
        let found = problems(&[entry(" ", "b"), entry("a", "b"), entry("a", "b")]);
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found[0].ends_with("has no from") && found[1].contains("two entries"));
    }
}

//! The rules of the records that `rotproof check` runs on what it has read: the files under `docs/knowledge/` Rotproof
//! would not read, the log's structure and its days that have not come, and the floor of the bundle.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use chrono::NaiveDate;
use regex::Regex;

use crate::markdown::{heading, visible};
use crate::paths::{file_name, parent};
use crate::schema::{Knowledge, Status, Time};

static DATE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d{4}-\d{2}-\d{2}$").unwrap());
// A bullet list item, indented by up to 3 spaces as GFM allows
static LIST_ITEM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^ {0,3}[*+-](?:[ \t]|$)").unwrap());

/// The date headings of the log, `text`, whose day has not come anywhere at `now` ([`latest_day`]): an entry is written
/// under the day it was written, read from the clock.
pub fn log_days_to_come(text: &str, now: Time) -> Vec<String> {
    let today = latest_day(now);
    let shown = now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    visible(text)
        .lines()
        .filter_map(heading)
        .filter(|(level, title)| *level == 2 && DATE.is_match(title))
        .filter_map(|(_, title)| NaiveDate::parse_from_str(title, "%Y-%m-%d").ok())
        .filter(|day| *day > today)
        .map(|day| {
            format!(
                "log.md: ## {day}, a day that has not come anywhere yet (now {shown}): write the entry under the day \
                 it was written, read from the clock"
            )
        })
        .collect()
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

/// Path -> why, for the files under `docs/knowledge/` (paths relative to `docs/`) that a reader takes for knowledge
/// and Rotproof would not read.
///
/// Rotproof reads the documents directly in `docs/knowledge/`, so a markdown file in a directory under it would pass
/// unchecked. A reserved name (OKF 0.2, section 3.1) is read only where Rotproof writes it, `index.md`; a `log.md`
/// there would follow the structure of a log, and nothing would check that. A markdown file whose extension is not
/// `.md` in lowercase (`.MD`) is shown by GitHub, but not read as a document, so a broken one would pass.
pub fn unread_paths(paths: &[String]) -> BTreeMap<String, String> {
    let mut bad = BTreeMap::new();
    for path in paths {
        let name = file_name(path);
        let markdown = name
            .rsplit_once('.')
            .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("md"));
        if parent(path) != KNOWLEDGE {
            if markdown {
                bad.insert(
                    path.clone(),
                    format!(
                        "in a directory under docs/{KNOWLEDGE}/, which Rotproof does not read: put a knowledge \
                         document directly in docs/{KNOWLEDGE}/"
                    ),
                );
            }
        } else if name == "log.md" {
            bad.insert(
                path.clone(),
                format!(
                    "a reserved name where Rotproof neither writes nor reads it (index.md in docs/{KNOWLEDGE}/, \
                     log.md in docs/)"
                ),
            );
        } else if markdown && !name.ends_with(".md") {
            bad.insert(
                path.clone(),
                "a markdown file is named with .md, in lowercase".into(),
            );
        }
    }
    bad
}

/// The directory of `docs/` that holds the knowledge documents.
pub const KNOWLEDGE: &str = "knowledge";

/// The directories and files `docs/` has to hold for the checks of the records to see anything (paths from the root):
/// if a move or a rename makes the scan come back empty, the checks see nothing and pass.
pub fn floor() -> Vec<String> {
    [
        format!("docs/{KNOWLEDGE}"),
        "docs/index.md".into(),
        format!("docs/{KNOWLEDGE}/rules.md"),
    ]
    .into()
}

/// The knowledge documents that hold, of `documents` (name -> document), whose `stale_after` has come on the day of
/// `now` in its own zone, the zone of the machine that runs the check, each with what to do. A warning, never a broken
/// rule: time alone turns no check from green to red, and a line on every run is enough for whoever reads the check to
/// look again. Unlike the log's days it is not compared with [`latest_day`]: that bound keeps a failure from depending
/// on where CI runs, and a warning that fails nothing needs no such bound.
pub fn due(documents: &BTreeMap<String, Knowledge>, now: Time) -> Vec<String> {
    let today = now.date_naive();
    documents
        .iter()
        .filter(|(_, doc)| doc.status == Status::Stable)
        .filter_map(|(name, doc)| Some((name, doc.stale_after?)))
        .filter(|(_, day)| *day <= today)
        .map(|(name, day)| {
            format!(
                "knowledge/{name}: stale_after {day} has come: measure again what it says of outside the code, then \
                 write the day in verified and the next day to look again in stale_after"
            )
        })
        .collect()
}

/// The latest day it is anywhere at `now`, the day in UTC+14. A day of the records is checked against it, not against
/// the day of `now` itself: before 09:00 in Tokyo, the day written there is still to come in UTC, where CI runs.
pub fn latest_day(now: Time) -> NaiveDate {
    (now.naive_utc() + chrono::Duration::hours(14)).date()
}

#[cfg(test)]
mod tests {
    use chrono::{FixedOffset, TimeZone};

    use super::*;

    #[test]
    fn the_latest_day_is_the_day_in_utc_plus_14() {
        let tokyo = FixedOffset::east_opt(9 * 3600).unwrap();
        // 08:00 in Tokyo is 23:00 of the day before in UTC, and already the same day in UTC+14
        let morning = tokyo.with_ymd_and_hms(2026, 10, 28, 8, 0, 0).unwrap();
        assert_eq!(
            latest_day(morning),
            NaiveDate::from_ymd_opt(2026, 10, 28).unwrap()
        );
        let evening = tokyo.with_ymd_and_hms(2026, 10, 28, 19, 0, 0).unwrap();
        assert_eq!(
            latest_day(evening),
            NaiveDate::from_ymd_opt(2026, 10, 29).unwrap()
        );
    }

    #[test]
    fn a_knowledge_document_is_due_from_its_stale_after_and_a_deprecated_one_never() {
        let doc = |stale_after: &str, status: &str| {
            let heading = if status == "deprecated" {
                "Resolution"
            } else {
                "Why"
            };
            let text = format!(
                "---\ntype: Knowledge\ntitle: T\ndescription: D.\ntags: [a]\nstatus: {status}\n{stale_after}---\n\n# {heading}\n\nBecause.\n"
            );
            match crate::schema::knowledge_doc(&text).unwrap() {
                crate::schema::KnowledgeDoc::Knowledge(doc, _) => doc,
                _ => panic!("not knowledge: {text}"),
            }
        };
        let documents: BTreeMap<String, Knowledge> = [
            ("past.md", doc("stale_after: 2026-10-27\n", "stable")),
            ("today.md", doc("stale_after: 2026-10-28\n", "stable")),
            ("tomorrow.md", doc("stale_after: 2026-10-29\n", "stable")),
            ("never set.md", doc("", "stable")),
            (
                "deprecated.md",
                doc("stale_after: 2026-01-01\n", "deprecated"),
            ),
        ]
        .into_iter()
        .map(|(name, doc)| (name.to_string(), doc))
        .collect();
        let tokyo = FixedOffset::east_opt(9 * 3600).unwrap();
        // 23:00 in Tokyo on 10-28 is already 10-29 in UTC+14, and the day where the check runs is what counts
        let late = tokyo.with_ymd_and_hms(2026, 10, 28, 23, 0, 0).unwrap();
        let found = due(&documents, late);
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found[0].starts_with("knowledge/past.md: stale_after 2026-10-27 has come"));
        assert!(found[1].starts_with("knowledge/today.md: stale_after 2026-10-28 has come"));
        // 08:00 in Tokyo on 10-28 is still 10-27 in UTC: the same instant is a day earlier where CI runs
        let morning = tokyo.with_ymd_and_hms(2026, 10, 28, 8, 0, 0).unwrap();
        assert_eq!(due(&documents, morning).len(), 2);
        let found = due(
            &documents,
            morning.with_timezone(&chrono::Utc).fixed_offset(),
        );
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].starts_with("knowledge/past.md"));
    }

    #[test]
    fn a_log_day_that_has_not_come_is_named() {
        let log = "# Log\n\n## 2026-10-29\n\n* **B**\n\n## 2026-10-28\n\n* **A**\n\n<!--\n## 2026-12-01\n-->\n\n### 2026-12-02\n";
        let tokyo = FixedOffset::east_opt(9 * 3600).unwrap();
        // 2026-10-29 comes in UTC+14 at 10:00 UTC on 10-28, 19:00 in Tokyo. A heading in a comment, or at another
        // level, is no date of the log
        let found = log_days_to_come(
            log,
            tokyo.with_ymd_and_hms(2026, 10, 28, 18, 59, 59).unwrap(),
        );
        assert_eq!(
            found,
            [
                "log.md: ## 2026-10-29, a day that has not come anywhere yet (now 2026-10-28T18:59:59+09:00): write \
              the entry under the day it was written, read from the clock"
            ]
        );
        let come = log_days_to_come(log, tokyo.with_ymd_and_hms(2026, 10, 28, 19, 0, 0).unwrap());
        assert_eq!(come, Vec::<String>::new());
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

    #[test]
    fn a_reserved_name_a_capital_extension_or_a_deeper_document_is_not_read() {
        let paths: Vec<String> = [
            "knowledge/index.md",
            "knowledge/a.md",
            "knowledge/diagram.png",
            "knowledge/assets/diagram.png",
            "knowledge/log.md",
            "knowledge/b.MD",
            "knowledge/deeper/c.md",
            "knowledge/deeper/index.md",
        ]
        .map(String::from)
        .into();
        let found = unread_paths(&paths);
        assert_eq!(
            found.keys().collect::<Vec<_>>(),
            [
                "knowledge/b.MD",
                "knowledge/deeper/c.md",
                "knowledge/deeper/index.md",
                "knowledge/log.md"
            ]
        );
    }

    #[test]
    fn the_floor_is_the_directory_of_knowledge_the_root_index_and_the_rules() {
        assert_eq!(
            floor(),
            ["docs/knowledge", "docs/index.md", "docs/knowledge/rules.md"]
        );
    }
}

//! The rules of the records that `rotproof check` runs on what it has read: the files under `docs/knowledge/` Rotproof
//! would not read, the log's structure, its days that have not come and the knowledge documents it names, the hash it
//! names one by, and the floor of the bundle.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use chrono::NaiveDate;
use percent_encoding::percent_decode_str;
use regex::Regex;
use sha2::{Digest, Sha256};

use crate::bundle::latest_day;
use crate::markdown::{heading, visible};
use crate::paths::{file_name, parent};
use crate::schema::Time;

// The `**Knowledge**` field of a log entry: its indentation and what follows the label
static KNOWLEDGE_FIELD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\s*)[*+-][ \t]+\*\*Knowledge\*\*:(.*)$").unwrap());
// A knowledge document named with its hash, written with `/` or `\`
static KNOWLEDGE_REF: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"knowledge[/\\]([\w.%-]+\.md)@([0-9a-f]{8})\b").unwrap());
// A list item at any depth, once its indentation is removed
static LIST_MARKER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[*+-](?:[ \t]|$)").unwrap());
static DATE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d{4}-\d{2}-\d{2}$").unwrap());
// A bullet list item, indented by up to 3 spaces as GFM allows
static LIST_ITEM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^ {0,3}[*+-](?:[ \t]|$)").unwrap());

/// The hash the log names a knowledge document by: the first 8 hex digits of SHA-256 of its text, as `read_text`
/// reads it (every line ending as `\n` and no byte order mark, so a checkout with CRLF has the same hash).
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

/// The knowledge documents the log names in `refs` (see [`knowledge_refs`]) that are not among `names` (the file names
/// in `docs/knowledge/`), each once.
pub fn dangling_knowledge_refs(refs: &[(String, String)], names: &[String]) -> Vec<String> {
    let mut dangling: Vec<String> = refs
        .iter()
        .map(|(name, _)| name.clone())
        .filter(|name| !names.contains(name))
        .collect();
    dangling.sort();
    dangling.dedup();
    dangling
}

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
    fn a_missing_knowledge_document_is_named_once() {
        // Named by entries that are not next to each other in the log, with another missing one between them
        let log = "## 2026-10-02\n\n* **a**\n  * **Knowledge**: knowledge/gone.md@12345678\n\
                   * **b**\n  * **Knowledge**: knowledge/other.md@12345678, knowledge/api.md@12345678\n\
                   * **c**\n  * **Knowledge**: knowledge/gone.md@abcdef01\n";
        let names = ["api.md", "rules.md"].map(String::from).to_vec();
        assert_eq!(
            dangling_knowledge_refs(&knowledge_refs(log), &names),
            ["gone.md", "other.md"]
        );
    }

    #[test]
    fn a_log_day_that_has_not_come_is_named() {
        use chrono::{FixedOffset, TimeZone};
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

//! An entry of the log, as `rotproof log` writes it: the form of its fields, which the guide gives, and the place it
//! goes in the text of the log, at the top under the heading of the day, which is made when it is not there. Every rule
//! is on text: the day is read from the clock by whoever calls, and the file is read and written by `application`.

use std::sync::LazyLock;

use chrono::NaiveDate;
use regex::Regex;

use crate::markdown::{heading, visible};
use crate::records::log_problems;

// `owner/repo#n`, as the guide writes the issue of a task
static ISSUE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+#[0-9]+$").unwrap());
static DATE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d{4}-\d{2}-\d{2}$").unwrap());

/// What one task left behind, as `rotproof log` is given it.
#[derive(Debug, Clone, Copy)]
pub struct Entry<'a> {
    pub title: &'a str,
    /// The task's issue, `owner/repo#n`, when it has one
    pub issue: Option<&'a str>,
    /// What went wrong, the cause and the workaround: at least one, and `None` when nothing did
    pub problems: &'a [String],
    /// What to do differently next time, and where it was recorded if it became a rule: at least one
    pub lessons: &'a [String],
}

/// `text` as the one line a field of an entry is: each line break, with the spaces around it, becomes one space, and
/// the ends are trimmed. `Err` when nothing is left, or when it opens an HTML comment, which would hide every line
/// below it from the check and from a reader. `what` names the field in the message.
pub fn one_line(what: &str, text: &str) -> Result<String, String> {
    let line = text
        .split(['\n', '\r'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if line.is_empty() {
        return Err(format!(
            "{what} is empty: say what it is, or None when there was nothing to say"
        ));
    }
    if line.contains("<!--") {
        return Err(format!(
            "{what} holds `<!--`, which opens an HTML comment and hides the rest of the log from every reader"
        ));
    }
    Ok(line)
}

/// The lines of the entry, in the form the guide gives:
///
/// ```markdown
/// * **<Task name>**
///   * **Issue**: owner/repo#n
///   * **Problems**: ...
///   * **Lessons**: ...
/// ```
///
/// A field given more than once is a list under its label. The issue line is left out when there is none.
pub fn entry(entry: &Entry) -> Result<Vec<String>, String> {
    let mut lines = vec![format!("* **{}**", one_line("--title", entry.title)?)];
    if let Some(issue) = entry.issue {
        let issue = issue.trim();
        if !ISSUE.is_match(issue) {
            return Err(format!(
                "--issue {issue:?} is not owner/repo#n (acme/app#12): the log names an issue so that it can be \
                 searched for"
            ));
        }
        lines.push(format!("  * **Issue**: {issue}"));
    }
    for (label, flag, values) in [
        ("Problems", "--problem", entry.problems),
        ("Lessons", "--lesson", entry.lessons),
    ] {
        let values: Vec<String> = values
            .iter()
            .map(|value| one_line(flag, value))
            .collect::<Result<_, _>>()?;
        match values.as_slice() {
            [] => {
                return Err(format!(
                    "{flag} is needed at least once: write None when there was nothing to say, so that a field left \
                     out is told from one with nothing in it"
                ));
            }
            [only] => lines.push(format!("  * **{label}**: {only}")),
            many => {
                lines.push(format!("  * **{label}**:"));
                lines.extend(many.iter().map(|value| format!("    * {value}")));
            }
        }
    }
    Ok(lines)
}

/// The day a line of the log is the heading of, when it is `## YYYY-MM-DD`.
fn day_of(line: &str) -> Option<NaiveDate> {
    match heading(line) {
        Some((2, title)) if DATE.is_match(title) => {
            NaiveDate::parse_from_str(title, "%Y-%m-%d").ok()
        }
        _ => None,
    }
}

/// The log `text` (as `rotproof check` reads it: every line ending `\n`) with `entry` at the top under the heading of
/// `day`, made when there is none: at the top of the day's entries when the heading is there, else before the first
/// heading of an older day, else at the end, so the days stay newest first. Blank lines separate the entries, and the
/// rest of the text is as it was. `Err`, with nothing to write, when the result would break a rule the log kept
/// (`rotproof check` finds the same), or a heading of a day sits where a reader does not see it (in a comment or a
/// code block), so that no place for the entry can be told.
pub fn inserted(text: &str, day: NaiveDate, entry: &[String]) -> Result<String, String> {
    let body = text.trim_end_matches('\n');
    let lines: Vec<&str> = if body.is_empty() {
        Vec::new()
    } else {
        body.split('\n').collect()
    };
    let days: Vec<(usize, NaiveDate)> = lines
        .iter()
        .enumerate()
        .filter_map(|(at, line)| day_of(line).map(|found| (at, found)))
        .collect();
    let shown = visible(text).lines().filter_map(day_of).count();
    if shown != days.len() {
        return Err(
            "a date heading of the log sits in an HTML comment or a code block, so where the entry goes cannot be \
             told: take it out of there, or write the entry by hand"
                .into(),
        );
    }
    let new: Vec<&str> = entry.iter().map(String::as_str).collect();
    let mut out: Vec<&str> = Vec::new();
    let heading_of_day = format!("## {day}");
    if let Some(&(at, _)) = days.iter().find(|(_, found)| *found == day) {
        // Under the heading of the day, before the entries that are there
        let rest: Vec<&str> = lines[at + 1..]
            .iter()
            .copied()
            .skip_while(|line| line.trim().is_empty())
            .collect();
        out.extend(&lines[..=at]);
        out.push("");
        out.extend(&new);
        if !rest.is_empty() {
            out.push("");
        }
        out.extend(rest);
    } else {
        // A new heading, where the days stay newest first
        let at = days
            .iter()
            .find(|(_, found)| *found < day)
            .map_or(lines.len(), |(at, _)| *at);
        let mut head = &lines[..at];
        while let Some((last, above)) = head.split_last() {
            if !last.trim().is_empty() {
                break;
            }
            head = above;
        }
        out.extend(head);
        if !head.is_empty() {
            out.push("");
        }
        out.push(&heading_of_day);
        out.push("");
        out.extend(&new);
        if at < lines.len() {
            out.push("");
            out.extend(&lines[at..]);
        }
    }
    let written = format!("{}\n", out.join("\n"));
    let before = log_problems(text);
    let broken: Vec<String> = log_problems(&written)
        .into_iter()
        .filter(|problem| !before.contains(problem))
        .collect();
    if !broken.is_empty() {
        return Err(format!(
            "the entry would break the log, so nothing is written: {}",
            broken.join("; ")
        ));
    }
    let first = entry.first().map_or("", String::as_str);
    if !visible(&written).lines().any(|line| line == first) {
        return Err(
            "the entry would sit where a reader does not see it (in an HTML comment or a code block of the log), so \
             nothing is written"
                .into(),
        );
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }

    fn strings(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn lines(title: &str) -> Vec<String> {
        strings(&[
            &format!("* **{title}**"),
            "  * **Problems**: None",
            "  * **Lessons**: L",
        ])
    }

    fn of(issue: Option<&str>, problems: &[&str], lessons: &[&str]) -> Result<Vec<String>, String> {
        entry(&Entry {
            title: "Task",
            issue,
            problems: &strings(problems),
            lessons: &strings(lessons),
        })
    }

    #[test]
    fn an_entry_has_the_fields_the_guide_gives_and_a_list_for_a_field_given_twice() {
        assert_eq!(
            of(Some("acme/app#12"), &["P"], &["L"]).unwrap(),
            [
                "* **Task**",
                "  * **Issue**: acme/app#12",
                "  * **Problems**: P",
                "  * **Lessons**: L"
            ]
        );
        assert_eq!(
            of(None, &["None"], &["a", "b"]).unwrap(),
            [
                "* **Task**",
                "  * **Problems**: None",
                "  * **Lessons**:",
                "    * a",
                "    * b"
            ]
        );
    }

    #[test]
    fn what_cannot_be_an_entry_is_refused_with_the_flag_to_change() {
        for (issue, problems, lessons, says) in [
            (None, vec![], vec!["L"], "--problem is needed"),
            (None, vec!["P"], vec![], "--lesson is needed"),
            (None, vec![""], vec!["L"], "--problem is empty"),
            (None, vec!["P"], vec!["  \n "], "--lesson is empty"),
            (None, vec!["a <!-- b"], vec!["L"], "--problem holds `<!--`"),
            (Some("12"), vec!["P"], vec!["L"], "--issue"),
            (Some("#12"), vec!["P"], vec!["L"], "--issue"),
            (Some("a/b#x"), vec!["P"], vec!["L"], "--issue"),
            (Some("a b/c#1"), vec!["P"], vec!["L"], "--issue"),
            (
                Some("https://github.com/a/b/issues/1"),
                vec!["P"],
                vec!["L"],
                "--issue",
            ),
        ] {
            let why = of(issue, &problems, &lessons).unwrap_err();
            assert!(why.contains(says), "{issue:?} {problems:?}: {why}");
        }
        let why = entry(&Entry {
            title: " ",
            issue: None,
            problems: &strings(&["P"]),
            lessons: &strings(&["L"]),
        })
        .unwrap_err();
        assert!(why.contains("--title is empty"), "{why}");
    }

    #[test]
    fn a_line_break_in_a_field_becomes_one_space() {
        assert_eq!(one_line("x", "a\nb").unwrap(), "a b");
        assert_eq!(one_line("x", "a  \r\n  b\n\nc ").unwrap(), "a b c");
        assert_eq!(one_line("x", "  inner  space ").unwrap(), "inner  space");
        // Never a second line of the entry, which would read as a list item or a heading of its own
        let entry = of(None, &["a\n## 2099-01-01\n* x"], &["L"]).unwrap();
        assert_eq!(entry[1], "  * **Problems**: a ## 2099-01-01 * x");
    }

    const LOG: &str = "# Log\n\n## 2026-11-15\n\n* **Old**\n  * **Problems**: None\n  * **Lessons**: L\n\n## 2026-11-10\n\n\
                       * **Older**\n  * **Problems**: None\n  * **Lessons**: L\n";

    #[test]
    fn the_entry_goes_to_the_top_of_the_days_entries_when_the_heading_is_there() {
        let written = inserted(LOG, day("2026-11-15"), &lines("New")).unwrap();
        assert_eq!(
            written,
            "# Log\n\n## 2026-11-15\n\n* **New**\n  * **Problems**: None\n  * **Lessons**: L\n\n* **Old**\n  * \
             **Problems**: None\n  * **Lessons**: L\n\n## 2026-11-10\n\n* **Older**\n  * **Problems**: None\n  * \
             **Lessons**: L\n"
        );
        assert!(log_problems(&written).is_empty());
    }

    #[test]
    fn the_heading_is_made_in_its_place_when_the_day_has_none() {
        // Newest first: above the older day, below a day that is later
        let at_top = inserted(LOG, day("2026-11-16"), &lines("New")).unwrap();
        assert!(
            at_top.starts_with("# Log\n\n## 2026-11-16\n\n* **New**\n  * **Problems**: None\n  * **Lessons**: L\n\n## 2026-11-15\n"),
            "{at_top}"
        );
        let between = inserted(LOG, day("2026-11-12"), &lines("New")).unwrap();
        let (above, below) = between.split_once("## 2026-11-12").unwrap();
        assert!(
            above.ends_with("**Lessons**: L\n\n") && below.ends_with("**Lessons**: L\n"),
            "{between}"
        );
        assert!(
            below.starts_with("\n\n* **New**") || below.starts_with("\n* **New**"),
            "{between}"
        );
        assert!(below.contains("## 2026-11-10"), "{between}");
        let at_end = inserted(LOG, day("2026-11-01"), &lines("New")).unwrap();
        assert!(
            at_end.ends_with(
                "## 2026-11-01\n\n* **New**\n  * **Problems**: None\n  * **Lessons**: L\n"
            ),
            "{at_end}"
        );
        for written in [&at_top, &between, &at_end] {
            assert!(log_problems(written).is_empty(), "{written}");
        }
        // A day later than today (found anywhere else, the check allows it) stays above
        let later = inserted(LOG, day("2026-11-12"), &lines("N")).unwrap();
        assert!(later.find("2026-11-15").unwrap() < later.find("2026-11-12").unwrap());
    }

    #[test]
    fn a_log_with_only_its_title_or_nothing_gets_the_day_and_the_entry() {
        for (text, want) in [
            ("# Log\n", "# Log\n\n## 2026-11-16\n\n"),
            ("# Log\n\n", "# Log\n\n## 2026-11-16\n\n"),
            ("# Log", "# Log\n\n## 2026-11-16\n\n"),
            ("", "## 2026-11-16\n\n"),
            (
                "# Log\n\n<!-- A note. -->\n",
                "# Log\n\n<!-- A note. -->\n\n## 2026-11-16\n\n",
            ),
        ] {
            let written = inserted(text, day("2026-11-16"), &lines("New")).unwrap();
            assert_eq!(
                written,
                format!("{want}* **New**\n  * **Problems**: None\n  * **Lessons**: L\n"),
                "{text:?}"
            );
            assert!(log_problems(&written).is_empty());
        }
    }

    #[test]
    fn a_day_with_a_heading_and_no_entry_is_filled_without_a_stray_blank_line() {
        let written =
            inserted("# Log\n\n## 2026-11-16\n", day("2026-11-16"), &lines("New")).unwrap();
        assert_eq!(
            written,
            "# Log\n\n## 2026-11-16\n\n* **New**\n  * **Problems**: None\n  * **Lessons**: L\n"
        );
    }

    #[test]
    fn a_log_that_breaks_a_rule_already_is_written_as_it_is_but_a_new_break_is_refused() {
        let unordered = "# Log\n\n## 2026-11-01\n\n* **A**\n\n## 2026-11-10\n\n* **B**\n";
        assert!(!log_problems(unordered).is_empty());
        let written = inserted(unordered, day("2026-11-10"), &lines("New")).unwrap();
        assert!(written.contains("* **New**"), "{written}");
        // A heading hidden from the reader leaves no place that can be told
        for hidden in [
            "# Log\n\n<!--\n## 2026-11-16\n-->\n",
            "# Log\n\n```\n## 2026-11-16\n```\n",
        ] {
            let why = inserted(hidden, day("2026-11-16"), &lines("New")).unwrap_err();
            assert!(why.contains("cannot be told"), "{hidden:?}: {why}");
        }
        // An unclosed comment above the place hides the entry
        let why = inserted("# Log\n\n<!-- open\n", day("2026-11-16"), &lines("New")).unwrap_err();
        assert!(why.contains("does not see it"), "{why}");
    }
}

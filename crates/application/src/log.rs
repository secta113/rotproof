//! `rotproof log`: an entry added to `docs/log.md`, read as the file was written and written back as it was.

use domain::bundle::in_docs;
use domain::log::{Entry, entry, inserted};
use domain::schema::Time;
use protocol::inbound::{LogEntry, Logged};
use protocol::outbound::{Tree, Writer};

use crate::tree::{exactly, read_written};

/// Add `new` to the log at the top, under the heading of the day `now` gives (the day on the machine's own clock, in
/// its own zone: `rotproof check` compares the days of the log with the latest day anywhere, so a day written in any
/// zone passes), through `out`, with the heading made when the log has none. The file keeps its line endings and its
/// byte order mark. Nothing is written when a field is not as the log has it, the log is not there, or the entry would
/// break a rule the log kept.
pub fn write(
    tree: &dyn Tree,
    out: &dyn Writer,
    now: Time,
    new: &LogEntry,
) -> Result<Logged, String> {
    let lines = entry(&Entry {
        title: &new.title,
        issue: new.issue.as_deref(),
        problems: &new.problems,
        lessons: &new.lessons,
    })?;
    let path = in_docs("log.md");
    let path = match exactly(tree, &path) {
        Ok((found, false)) => found,
        Ok((_, true)) => return Err(format!("{path} is not a file")),
        Err(_) => {
            return Err(format!(
                "{path} is not there (names are compared exactly): `rotproof create` makes it"
            ));
        }
    };
    let (text, was) = read_written(tree, &path).map_err(|e| format!("{path}: {e}"))?;
    let day = now.date_naive();
    let written = inserted(&text, day, &lines).map_err(|why| format!("{path}: {why}"))?;
    out.write(&path, &was.restore(&written))
        .map_err(|e| format!("{path}: {e}"))?;
    Ok(Logged {
        path,
        day: day.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::fake::Fake;
    use chrono::{FixedOffset, TimeZone};

    const LOG: &str = "docs/log.md";

    fn at(y: i32, m: u32, d: u32, h: u32, offset_hours: i32) -> Time {
        FixedOffset::east_opt(offset_hours * 3600)
            .unwrap()
            .with_ymd_and_hms(y, m, d, h, 30, 0)
            .unwrap()
    }

    fn entry_of(title: &str) -> LogEntry {
        LogEntry {
            title: title.into(),
            issue: Some("acme/app#4".into()),
            problems: vec!["None".into()],
            lessons: vec!["L".into()],
        }
    }

    #[test]
    fn the_entry_goes_under_the_day_the_clock_gives_in_its_own_zone() {
        let tree = Fake::new(&[(LOG, "# Log\n\n## 2026-11-15\n\n* **Old**\n")]);
        // 23:30 on the 16th in Tokyo is the 16th there, though it is the 16th at 14:30 in UTC and the 17th nowhere yet
        let logged = write(&tree, &tree, at(2026, 11, 16, 23, 9), &entry_of("New")).unwrap();
        assert_eq!(logged.path, LOG);
        assert_eq!(logged.day, "2026-11-16");
        assert_eq!(
            tree.text(LOG).unwrap(),
            "# Log\n\n## 2026-11-16\n\n* **New**\n  * **Issue**: acme/app#4\n  * **Problems**: None\n  * **Lessons**: L\n\n\
             ## 2026-11-15\n\n* **Old**\n"
        );
        // And in a zone behind UTC, the day it is there
        let west = Fake::new(&[(LOG, "# Log\n")]);
        let logged = write(&west, &west, at(2026, 11, 16, 1, -8), &entry_of("New")).unwrap();
        assert_eq!(logged.day, "2026-11-16");
    }

    #[test]
    fn a_second_entry_of_the_day_goes_above_the_first() {
        let tree = Fake::new(&[(LOG, "# Log\n")]);
        let now = at(2026, 11, 16, 10, 9);
        write(&tree, &tree, now, &entry_of("First")).unwrap();
        write(&tree, &tree, now, &entry_of("Second")).unwrap();
        let text = tree.text(LOG).unwrap();
        assert_eq!(text.matches("## 2026-11-16").count(), 1, "{text}");
        assert!(text.find("**Second**").unwrap() < text.find("**First**").unwrap());
        assert!(domain::records::log_problems(&text).is_empty(), "{text}");
    }

    #[test]
    fn the_line_endings_and_the_byte_order_mark_of_the_log_are_kept() {
        for (raw, crlf, bom) in [
            (
                "# Log\r\n\r\n## 2026-11-15\r\n\r\n* **Old**\r\n",
                true,
                false,
            ),
            ("\u{feff}# Log\n\n## 2026-11-15\n\n* **Old**\n", false, true),
            (
                "\u{feff}# Log\r\n\r\n## 2026-11-15\r\n\r\n* **Old**\r\n",
                true,
                true,
            ),
            ("# Log\n\n## 2026-11-15\n\n* **Old**\n", false, false),
        ] {
            let tree = Fake::new(&[(LOG, raw)]);
            write(&tree, &tree, at(2026, 11, 16, 10, 9), &entry_of("New")).unwrap();
            let text = tree.text(LOG).unwrap();
            assert_eq!(text.starts_with('\u{feff}'), bom, "{text:?}");
            assert_eq!(text.contains("\r\n"), crlf, "{text:?}");
            // Every line ending is the log's own: none is left as the other kind
            assert_eq!(text.replace("\r\n", "").contains('\n'), !crlf, "{text:?}");
            assert_eq!(text.contains('\r'), crlf, "{text:?}");
            assert!(text.contains("**New**") && text.contains("**Old**"));
        }
    }

    #[test]
    fn nothing_is_written_for_a_field_that_is_not_as_the_log_has_it_or_a_log_that_is_not_there() {
        let tree = Fake::new(&[(LOG, "# Log\n")]);
        let now = at(2026, 11, 16, 10, 9);
        for new in [
            LogEntry {
                problems: vec![],
                ..entry_of("T")
            },
            LogEntry {
                lessons: vec![],
                ..entry_of("T")
            },
            LogEntry {
                title: "".into(),
                ..entry_of("T")
            },
            LogEntry {
                issue: Some("4".into()),
                ..entry_of("T")
            },
        ] {
            assert!(write(&tree, &tree, now, &new).is_err(), "{new:?}");
            assert_eq!(tree.text(LOG).as_deref(), Some("# Log\n"));
        }
        let bare = Fake::new(&[("docs/knowledge/a.md", "")]);
        let why = write(&bare, &bare, now, &entry_of("T")).unwrap_err();
        assert!(why.contains("docs/log.md is not there"), "{why}");
        assert_eq!(bare.text(LOG), None);
        // A directory, or a name that only Windows reads as the log, is not the log
        let spelled = Fake::new(&[("docs/LOG.md", "# Log\n")]);
        assert!(write(&spelled, &spelled, now, &entry_of("T")).is_err());
        assert_eq!(spelled.text("docs/LOG.md").as_deref(), Some("# Log\n"));
    }
}

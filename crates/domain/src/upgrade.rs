//! Upgrading a project's files: what a later version of Rotproof adds to the files `rotproof create` writes once.
//!
//! - **The declaration records the version the project's files are up to,** in `files`. A declaration without it is up
//!   to 0.1.0, the first release. Only `rotproof init` writes it: added by `rotproof create`, as it adds the fields a
//!   declaration lacks, it would mark a project made by 0.1.0 as up to date and skip every update.
//! - **`rotproof check` fails while `files` is older than the running Rotproof,** whether or not an update applies, so
//!   every upgrade runs `rotproof init` once and the field never stays behind unseen.
//! - **An update has a name and belongs to the version that brings it.** It changes one file, can tell whether it is
//!   needed, applies the same way every time, does nothing the second time, and only adds: it never changes or removes
//!   what the project wrote. What it cannot do without a person, it says, and the project does by hand or declines by
//!   naming the update in `declined`.

use serde_json::{Map, Value};
use toml_edit::DocumentMut;

use crate::approvals::APPROVALS;
use crate::layers::{DECLARATION, DECLINED_COMMENT, FILES_COMMENT};

/// The version of Rotproof that runs.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The version the project's files are up to when the declaration has no `files`: the first release.
pub const FIRST: &str = "0.1.0";

/// A version as `major.minor.patch`, compared part by part.
pub fn version(text: &str) -> Option<Version> {
    let mut parts = text.split('.').map(|part| {
        // A part is digits only: no sign, no space, no pre-release
        (!part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
            .then(|| part.parse::<u64>().ok())
            .flatten()
    });
    let found = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(found)
}

/// An update of the project's files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Update {
    /// The rule that denies Claude Code editing the approvals file, in `.claude/settings.json`
    DenyApprovals,
}

/// Every update, oldest first.
pub const UPDATES: [Update; 1] = [Update::DenyApprovals];

/// What an update does to its file.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The file has it already: nothing to write
    Done,
    /// The file's new text
    Changed(String),
    /// It cannot be done without a person: what to do by hand
    ByHand(String),
}

/// The rule [`Update::DenyApprovals`] adds.
fn deny_rule() -> String {
    format!("Edit(/{APPROVALS})")
}

impl Update {
    /// The name a project declines it by, in `declined`
    pub fn name(self) -> &'static str {
        match self {
            Update::DenyApprovals => "claude-deny-approvals",
        }
    }

    /// The version that brings it
    pub fn version(self) -> &'static str {
        match self {
            Update::DenyApprovals => "0.2.0",
        }
    }

    /// The file it changes, from the root
    pub fn file(self) -> &'static str {
        match self {
            Update::DenyApprovals => ".claude/settings.json",
        }
    }

    /// What it does to its file, whose text is `text` (`None` when there is no file).
    pub fn apply(self, text: Option<&str>) -> Outcome {
        match self {
            Update::DenyApprovals => deny_approvals(text),
        }
    }

    /// What to write in the declaration to decline it.
    fn declining(self) -> String {
        format!(
            "or decline it: declined = [\"{}\"] in {DECLARATION}",
            self.name()
        )
    }
}

/// Add the rule that denies editing the approvals file to Claude Code's settings, `text`. The keys keep their order.
fn deny_approvals(text: Option<&str>) -> Outcome {
    let update = Update::DenyApprovals;
    let rule = deny_rule();
    let by_hand = |why: &str| {
        Outcome::ByHand(format!(
            "{}: {why}. Add \"{rule}\" to permissions.deny there, {}",
            update.file(),
            update.declining()
        ))
    };
    let Some(text) = text else {
        return by_hand("there is no such file");
    };
    let Ok(Value::Object(mut settings)) = serde_json::from_str::<Value>(text) else {
        return by_hand("it is not a JSON object Rotproof can read");
    };
    let permissions = settings
        .entry("permissions")
        .or_insert_with(|| Value::Object(Map::new()));
    let Value::Object(permissions) = permissions else {
        return by_hand("permissions is not an object");
    };
    let deny = permissions
        .entry("deny")
        .or_insert_with(|| Value::Array(Vec::new()));
    let Value::Array(deny) = deny else {
        return by_hand("permissions.deny is not a list");
    };
    if deny.iter().any(|r| r.as_str() == Some(rule.as_str())) {
        return Outcome::Done;
    }
    deny.push(Value::String(rule));
    let mut written = serde_json::to_string_pretty(&Value::Object(settings))
        .expect("a JSON value always serializes");
    written.push('\n');
    Outcome::Changed(written)
}

/// The updates a project whose files are up to `files` takes to reach `running`, without the `declined` ones, oldest
/// first. `Err` when a version cannot be read, or `files` is newer than `running`.
pub fn pending(files: &str, running: &str, declined: &[String]) -> Result<Vec<Update>, String> {
    let (from, to) = versions(files, running)?;
    Ok(UPDATES
        .iter()
        .copied()
        .filter(|update| {
            let at = version(update.version()).expect("every update names a version");
            from < at && at <= to && !declined.iter().any(|d| d == update.name())
        })
        .collect())
}

/// A version read as its three numbers.
type Version = (u64, u64, u64);

/// Both versions read, and `files` no newer than `running`.
fn versions(files: &str, running: &str) -> Result<(Version, Version), String> {
    let from = version(files).ok_or_else(|| {
        format!("{DECLARATION}: files is {files:?}, not a version such as \"0.2.0\"")
    })?;
    let to = version(running).expect("Rotproof's own version reads");
    if from > to {
        return Err(format!(
            "{DECLARATION}: files is {files}, newer than this Rotproof ({running}): install {files} or newer"
        ));
    }
    Ok((from, to))
}

/// What an upgrade reads from a declaration, before the rest of it is checked: a declaration written by an older
/// Rotproof may lack a field the running one requires, which the upgrade adds.
#[derive(Debug, PartialEq, Eq)]
pub struct Fields {
    pub stack: Option<String>,
    /// `None` when the declaration has no `files`
    pub files: Option<String>,
    pub declined: Vec<String>,
}

/// The fields of the declaration `text` an upgrade needs, or why they cannot be read.
pub fn fields(text: &str) -> Result<Fields, String> {
    let document = text
        .parse::<DocumentMut>()
        .map_err(|e| format!("{DECLARATION}: {}", e.message()))?;
    let string = |name: &str| -> Result<Option<String>, String> {
        match document.get(name) {
            None => Ok(None),
            Some(item) => item
                .as_str()
                .map(|s| Some(s.to_string()))
                .ok_or_else(|| format!("{DECLARATION}: {name} is not a string")),
        }
    };
    let declined = match document.get("declined") {
        None => Vec::new(),
        Some(item) => item
            .as_array()
            .and_then(|array| {
                array
                    .iter()
                    .map(|v| v.as_str().map(str::to_string))
                    .collect::<Option<Vec<_>>>()
            })
            .ok_or_else(|| format!("{DECLARATION}: declined is not a list of names"))?,
    };
    Ok(Fields {
        stack: string("stack")?,
        files: string("files")?,
        declined,
    })
}

/// The declaration `text` with `files` set to `running`, and `declined` added when it lacks one, each under its
/// comment. Every other comment and value is kept, and the lines end with `\r\n` when `crlf`, as the project wrote
/// them.
pub fn with_files(text: &str, running: &str, crlf: bool) -> Result<String, String> {
    let mut document = text
        .parse::<DocumentMut>()
        .map_err(|e| format!("{DECLARATION}: {}", e.message()))?;
    match document.get_mut("files") {
        Some(item) => {
            // The value changes, its comment and place stay
            let decor = item.as_value().map(|v| v.decor().clone());
            *item = toml_edit::value(running);
            if let (Some(decor), Some(value)) = (decor, item.as_value_mut()) {
                *value.decor_mut() = decor;
            }
        }
        None => {
            document.insert("files", toml_edit::value(running));
            document
                .key_mut("files")
                .expect("inserted just before")
                .leaf_decor_mut()
                .set_prefix(format!("\n{FILES_COMMENT}"));
        }
    }
    if !document.contains_key("declined") {
        document.insert("declined", toml_edit::value(toml_edit::Array::new()));
        document
            .key_mut("declined")
            .expect("inserted just before")
            .leaf_decor_mut()
            .set_prefix(format!("\n{DECLINED_COMMENT}"));
    }
    let text = document.to_string();
    Ok(if crlf {
        text.replace('\n', "\r\n")
    } else {
        text
    })
}

/// What `rotproof check` finds in the declaration's `files` and `declined`, against the running version.
pub fn problems(files: Option<&str>, declined: &[String], running: &str) -> Vec<String> {
    let mut found = Vec::new();
    for name in declined {
        if !UPDATES.iter().any(|update| update.name() == name) {
            found.push(format!(
                "{DECLARATION}: declined names {name:?}, which is no update (known: {})",
                UPDATES
                    .iter()
                    .map(|update| update.name())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }
    let files = files.unwrap_or(FIRST);
    match versions(files, running) {
        Err(why) => found.push(why),
        Ok((from, to)) if from < to => found.push(format!(
            "the project's files are up to {files}{}, and this Rotproof is {running}: run `rotproof init`, which \
             updates them",
            if files == FIRST { " (no files in the declaration)" } else { "" }
        )),
        Ok(_) => {}
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_is_three_numbers() {
        assert_eq!(version("0.2.0"), Some((0, 2, 0)));
        assert_eq!(version("10.0.12"), Some((10, 0, 12)));
        for bad in [
            "0.2",
            "0.2.0.1",
            "0.2.x",
            "v0.2.0",
            "0.2.0-rc1",
            " 0.2.0",
            "0..1",
            "+1.0.0",
        ] {
            assert_eq!(version(bad), None, "{bad}");
        }
        assert!(version("0.10.0") > version("0.9.9"));
    }

    #[test]
    fn every_update_names_a_version_and_a_distinct_name() {
        let mut names: Vec<&str> = UPDATES.iter().map(|u| u.name()).collect();
        assert!(UPDATES.iter().all(|u| version(u.version()).is_some()));
        names.sort();
        names.dedup();
        assert_eq!(names.len(), UPDATES.len());
        // Oldest first
        assert!(
            UPDATES
                .windows(2)
                .all(|w| version(w[0].version()) <= version(w[1].version()))
        );
    }

    #[test]
    fn the_updates_between_two_versions_apply_unless_declined() {
        assert_eq!(
            pending("0.1.0", "0.2.0", &[]),
            Ok(vec![Update::DenyApprovals])
        );
        assert_eq!(pending("0.2.0", "0.2.0", &[]), Ok(vec![]));
        assert_eq!(pending("0.1.0", "0.1.5", &[]), Ok(vec![]));
        assert_eq!(
            pending("0.1.0", "0.3.0", &["claude-deny-approvals".into()]),
            Ok(vec![])
        );
        assert!(
            pending("0.3.0", "0.2.0", &[])
                .unwrap_err()
                .contains("newer than this Rotproof")
        );
        assert!(
            pending("two", "0.2.0", &[])
                .unwrap_err()
                .contains("not a version")
        );
    }

    #[test]
    fn the_check_fails_while_the_files_are_behind() {
        assert_eq!(problems(Some("0.2.0"), &[], "0.2.0"), Vec::<String>::new());
        let behind = problems(None, &[], "0.2.0");
        assert_eq!(behind.len(), 1);
        assert!(
            behind[0].contains("up to 0.1.0 (no files in the declaration)"),
            "{behind:?}"
        );
        assert!(behind[0].contains("run `rotproof init`"));
        // Behind with no update between: it still fails, so the field never stays behind unseen
        assert_eq!(problems(Some("0.2.0"), &[], "0.2.1").len(), 1);
        let misspelled = problems(Some("0.2.0"), &["claude-deny-approval".into()], "0.2.0");
        assert!(
            misspelled[0].contains("which is no update"),
            "{misspelled:?}"
        );
    }

    #[test]
    fn the_deny_rule_is_added_once_and_only_added() {
        let settings =
            "{\n  \"hooks\": {\"Stop\": []},\n  \"permissions\": {\"allow\": [\"Bash(ls)\"]}\n}\n";
        let Outcome::Changed(once) = Update::DenyApprovals.apply(Some(settings)) else {
            panic!("the rule is added");
        };
        let read: Value = serde_json::from_str(&once).unwrap();
        assert_eq!(read["permissions"]["deny"][0], deny_rule());
        assert_eq!(read["permissions"]["allow"][0], "Bash(ls)");
        // The keys keep their order
        assert!(once.find("hooks") < once.find("permissions"), "{once}");
        assert_eq!(Update::DenyApprovals.apply(Some(&once)), Outcome::Done);
        // With no permissions at all
        let Outcome::Changed(added) = Update::DenyApprovals.apply(Some("{}")) else {
            panic!("the rule is added");
        };
        assert!(added.contains(&deny_rule()));
    }

    #[test]
    fn what_cannot_be_updated_is_said_with_how_to_decline_it() {
        for text in [
            None,
            Some("not json"),
            Some("[]"),
            Some("{\"permissions\": []}"),
            Some("{\"permissions\": {\"deny\": \"x\"}}"),
        ] {
            let Outcome::ByHand(why) = Update::DenyApprovals.apply(text) else {
                panic!("{text:?} needs a person");
            };
            assert!(
                why.contains("declined = [\"claude-deny-approvals\"]"),
                "{why}"
            );
        }
    }
}

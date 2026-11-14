//! Upgrading a project's files: what a later version of Rotproof adds to the files `rotproof create` writes once.
//!
//! - **The declaration records the version the project's files are up to,** in `files`. A declaration without it is up
//!   to 0.1.0, the first release. Only `rotproof init` writes it: added by `rotproof create`, as it adds the fields a
//!   declaration lacks, it would mark a project made by 0.1.0 as up to date and skip every update.
//! - **`rotproof check` fails while `files` is older than the running Rotproof,** whether or not an update applies, so
//!   every upgrade runs `rotproof init` once and the field never stays behind unseen.
//! - **An update has a name and belongs to the version that brings it.** It changes one file, can tell whether it is
//!   needed, applies the same way every time, does nothing the second time, and only adds, or takes out what Rotproof
//!   itself wrote and no longer keeps, matched exactly: it never changes or removes what the project wrote. What it
//!   cannot do without a person, it says, and the project does by hand or declines by naming the update in `declined`.

use serde_json::{Map, Value, json};

use crate::approvals::APPROVALS;
use crate::declaration::DECLARATION;
use crate::hook;

/// The version of Rotproof that runs.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The version the project's files are up to when the declaration has no `files`: the first release.
pub const FIRST: &str = "0.1.0";

/// A version as SemVer writes and orders it: `major.minor.patch`, then an optional pre-release after `-`
/// (`1.0.0-rc.1`, the form Cargo takes, which PyPI shows as `1.0.0rc1`). A pre-release comes before the release it
/// leads to, so a project whose files are up to `1.0.0-rc.1` is behind `1.0.0`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    core: (u64, u64, u64),
    /// The identifiers of the pre-release, empty for a release
    pre: Vec<Identifier>,
}

/// One dot-separated identifier of a pre-release: digits are compared as a number and come before any other.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Identifier {
    Number(u64),
    Text(String),
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        self.core
            .cmp(&other.core)
            .then_with(|| match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                // Identifier by identifier, and a shorter list that is a prefix of the other comes first, as SemVer
                // orders them
                (false, false) => self.pre.cmp(&other.pre),
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// The version `text` names, or `None` when it is not one: no `v`, no space, no build metadata (`+...`).
pub fn version(text: &str) -> Option<Version> {
    let (core, pre) = match text.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (text, None),
    };
    let mut parts = core.split('.').map(|part| {
        // A part is digits only: no sign, no space
        (!part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
            .then(|| part.parse::<u64>().ok())
            .flatten()
    });
    let core = (parts.next()??, parts.next()??, parts.next()??);
    if parts.next().is_some() {
        return None;
    }
    let pre = match pre {
        None => Vec::new(),
        Some(pre) => pre.split('.').map(identifier).collect::<Option<Vec<_>>>()?,
    };
    Some(Version { core, pre })
}

/// An identifier of a pre-release: ASCII letters, digits and `-`, not empty, and digits alone without a leading zero.
fn identifier(text: &str) -> Option<Identifier> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        return None;
    }
    if text.bytes().all(|b| b.is_ascii_digit()) {
        if text.len() > 1 && text.starts_with('0') {
            return None;
        }
        return text.parse().ok().map(Identifier::Number);
    }
    Some(Identifier::Text(text.to_string()))
}

/// An update of the project's files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Update {
    /// The rule that denies Claude Code editing the approvals file, in `.claude/settings.json`
    DenyApprovals,
    /// The hook beside the stop hook that says when `rotproof` is not on the `PATH`, in `.claude/settings.json`
    StopHookOnPath,
    /// The hook that tells the agent the time on each prompt and at the start of a session, in `.claude/settings.json`
    TimeHook,
    /// The stop hook taken out, with the hook beside it, from `.claude/settings.json`: 1.0 no longer has one
    RemoveStopHook,
    /// The time hook taken out of `.claude/settings.json`: 1.0 no longer has one
    RemoveTimeHook,
}

/// Every update, oldest first.
pub const UPDATES: [Update; 5] = [
    Update::DenyApprovals,
    Update::StopHookOnPath,
    Update::TimeHook,
    Update::RemoveStopHook,
    Update::RemoveTimeHook,
];

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
            Update::StopHookOnPath => "claude-stop-hook-on-path",
            Update::TimeHook => "claude-time-hook",
            Update::RemoveStopHook => "claude-remove-stop-hook",
            Update::RemoveTimeHook => "claude-remove-time-hook",
        }
    }

    /// The version that brings it
    pub fn version(self) -> &'static str {
        match self {
            Update::DenyApprovals => "0.2.0",
            Update::StopHookOnPath | Update::TimeHook => "0.4.0",
            Update::RemoveStopHook => "1.0.0-rc.1",
            Update::RemoveTimeHook => "1.0.0-rc.4",
        }
    }

    /// The file it changes, from the root
    pub fn file(self) -> &'static str {
        match self {
            Update::DenyApprovals
            | Update::StopHookOnPath
            | Update::TimeHook
            | Update::RemoveStopHook
            | Update::RemoveTimeHook => ".claude/settings.json",
        }
    }

    /// What it does to its file, whose text is `text` (`None` when there is no file).
    pub fn apply(self, text: Option<&str>) -> Outcome {
        match self {
            Update::DenyApprovals => deny_approvals(text),
            Update::StopHookOnPath => stop_hook_on_path(text),
            Update::TimeHook => time_hook(text),
            Update::RemoveStopHook => remove_stop_hook(text),
            Update::RemoveTimeHook => remove_time_hook(text),
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
    Outcome::Changed(written(settings))
}

/// Add the hook that says when `rotproof` is not on the `PATH` ([`hook::ON_PATH`]) to Claude Code's settings, `text`,
/// as a group of its own after the `Stop` hooks there. Needed only while a `Stop` hook runs the stop hook
/// ([`hook::COMMAND`]): a project that has no settings, or took the stop hook out, has nothing to guard.
fn stop_hook_on_path(text: Option<&str>) -> Outcome {
    let update = Update::StopHookOnPath;
    let Some(text) = text else {
        return Outcome::Done;
    };
    let Ok(Value::Object(mut settings)) = serde_json::from_str::<Value>(text) else {
        return Outcome::ByHand(format!(
            "{}: it is not a JSON object Rotproof can read. Add a Stop hook that runs {:?} beside {:?}, {}",
            update.file(),
            hook::ON_PATH,
            hook::COMMAND,
            update.declining()
        ));
    };
    let Some(Value::Array(stop)) = settings
        .get_mut("hooks")
        .and_then(|hooks| hooks.get_mut("Stop"))
    else {
        return Outcome::Done;
    };
    if !runs(stop, hook::COMMAND) || runs(stop, hook::ON_PATH) {
        return Outcome::Done;
    }
    stop.push(group(hook::ON_PATH));
    Outcome::Changed(written(settings))
}

/// Add the time hook ([`hook::TIME_COMMAND`]) to Claude Code's settings, `text`, as a group of its own on each of its
/// events whose hooks do not run it yet, after them. Needed only while a `Stop` hook runs the stop hook
/// ([`hook::COMMAND`]): a project that has no settings, or took Rotproof's hooks out, did not take them.
fn time_hook(text: Option<&str>) -> Outcome {
    let update = Update::TimeHook;
    let by_hand = |why: &str| {
        Outcome::ByHand(format!(
            "{}: {why}. Add a {} hook that runs {:?}, {}",
            update.file(),
            hook::TIME_EVENTS.join(" and a "),
            hook::TIME_COMMAND,
            update.declining()
        ))
    };
    let Some(text) = text else {
        return Outcome::Done;
    };
    let Ok(Value::Object(mut settings)) = serde_json::from_str::<Value>(text) else {
        return by_hand("it is not a JSON object Rotproof can read");
    };
    let Some(Value::Object(hooks)) = settings.get_mut("hooks") else {
        return Outcome::Done;
    };
    if !matches!(hooks.get("Stop"), Some(Value::Array(stop)) if runs(stop, hook::COMMAND)) {
        return Outcome::Done;
    }
    let mut changed = false;
    for event in hook::TIME_EVENTS {
        let groups = hooks
            .entry(event)
            .or_insert_with(|| Value::Array(Vec::new()));
        let Value::Array(groups) = groups else {
            return by_hand(&format!("hooks.{event} is not a list"));
        };
        if !runs(groups, hook::TIME_COMMAND) {
            groups.push(group(hook::TIME_COMMAND));
            changed = true;
        }
    }
    if changed {
        Outcome::Changed(written(settings))
    } else {
        Outcome::Done
    }
}

/// Take the stop hook ([`hook::COMMAND`]) and the hook beside it ([`hook::ON_PATH`]) out of Claude Code's settings,
/// `text`: each hook that runs one of them exactly, a group left with no hook, `Stop` when no group is left, and
/// `hooks` when no event is left. A hook the project changed (another command that runs `rotproof stop-hook`) is not
/// Rotproof's text, and stays: the command says, on every stop, that it does nothing now.
fn remove_stop_hook(text: Option<&str>) -> Outcome {
    let update = Update::RemoveStopHook;
    let Some(text) = text else {
        return Outcome::Done;
    };
    let Ok(Value::Object(mut settings)) = serde_json::from_str::<Value>(text) else {
        return Outcome::ByHand(format!(
            "{}: it is not a JSON object Rotproof can read. Remove the Stop hooks that run {:?} and {:?}, {}",
            update.file(),
            hook::COMMAND,
            hook::ON_PATH,
            update.declining()
        ));
    };
    let Some(Value::Object(hooks)) = settings.get_mut("hooks") else {
        return Outcome::Done;
    };
    let Some(Value::Array(stop)) = hooks.get_mut("Stop") else {
        return Outcome::Done;
    };
    let rotproofs =
        |hook: &Value| hook["command"] == hook::COMMAND || hook["command"] == hook::ON_PATH;
    if !stop.iter().any(|group| {
        group["hooks"]
            .as_array()
            .is_some_and(|hooks| hooks.iter().any(rotproofs))
    }) {
        return Outcome::Done;
    }
    stop.retain_mut(|group| match group.get_mut("hooks") {
        Some(Value::Array(hooks)) => {
            hooks.retain(|hook| !rotproofs(hook));
            !hooks.is_empty()
        }
        _ => true,
    });
    if stop.is_empty() {
        // Shifted out, so the events after it keep their order
        hooks.shift_remove("Stop");
    }
    // The settings of a project that ran no other hook end as `rotproof create` writes them now: with no `hooks`
    if hooks.is_empty() {
        settings.shift_remove("hooks");
    }
    Outcome::Changed(written(settings))
}

/// Take the time hook ([`hook::TIME_COMMAND`]) out of Claude Code's settings, `text`: each hook that runs it exactly
/// on `SessionStart` and `UserPromptSubmit`, a group left with no hook, an event left with no group, and `hooks` when
/// no event is left. A hook the project changed (another command that runs `rotproof time-hook`, such as a path to
/// the binary) is not Rotproof's text, and stays: the command says, at the start of every session, that it does
/// nothing now.
fn remove_time_hook(text: Option<&str>) -> Outcome {
    let update = Update::RemoveTimeHook;
    let Some(text) = text else {
        return Outcome::Done;
    };
    let Ok(Value::Object(mut settings)) = serde_json::from_str::<Value>(text) else {
        return Outcome::ByHand(format!(
            "{}: it is not a JSON object Rotproof can read. Remove the {} hooks that run {:?}, {}",
            update.file(),
            hook::TIME_EVENTS.join(" and "),
            hook::TIME_COMMAND,
            update.declining()
        ));
    };
    let Some(Value::Object(hooks)) = settings.get_mut("hooks") else {
        return Outcome::Done;
    };
    let mut changed = false;
    for event in hook::TIME_EVENTS {
        let Some(Value::Array(groups)) = hooks.get_mut(event) else {
            continue;
        };
        if !runs(groups, hook::TIME_COMMAND) {
            continue;
        }
        changed = true;
        groups.retain_mut(|group| match group.get_mut("hooks") {
            Some(Value::Array(hooks)) => {
                hooks.retain(|hook| hook["command"] != hook::TIME_COMMAND);
                !hooks.is_empty()
            }
            _ => true,
        });
        if groups.is_empty() {
            // Shifted out, so the events after it keep their order
            hooks.shift_remove(event);
        }
    }
    if !changed {
        return Outcome::Done;
    }
    if hooks.is_empty() {
        settings.shift_remove("hooks");
    }
    Outcome::Changed(written(settings))
}

/// Whether one of the hook groups `groups` of an event runs `command`.
fn runs(groups: &[Value], command: &str) -> bool {
    groups.iter().any(|group| {
        group["hooks"]
            .as_array()
            .is_some_and(|hooks| hooks.iter().any(|h| h["command"] == command))
    })
}

/// A hook group of its own that runs `command`, as `rotproof create` writes one.
fn group(command: &str) -> Value {
    json!({ "hooks": [{ "type": "command", "command": command }] })
}

/// The settings as the file's text, as `rotproof create` writes it: indented, keys in their order, ending in a newline.
fn written(settings: Map<String, Value>) -> String {
    let mut text = serde_json::to_string_pretty(&Value::Object(settings))
        .expect("a JSON value always serializes");
    text.push('\n');
    text
}

/// The updates a project whose files are up to `files` takes to reach `running`, without the `declined` ones, oldest
/// first. `Err` when a version cannot be read, or `files` is newer than `running`.
///
/// The hook beside the stop hook is not added when the stop hook is taken out in the same upgrade, and the time hook is
/// not added when it is taken out: each would be taken out with it, and a file Rotproof cannot read would ask a person
/// to add it and to remove it at once.
pub fn pending(files: &str, running: &str, declined: &[String]) -> Result<Vec<Update>, String> {
    let (from, to) = versions(files, running)?;
    let found: Vec<Update> = UPDATES
        .iter()
        .copied()
        .filter(|update| {
            let at = version(update.version()).expect("every update names a version");
            from < at && at <= to && !declined.iter().any(|d| d == update.name())
        })
        .collect();
    let stop_removed = found.contains(&Update::RemoveStopHook);
    let time_removed = found.contains(&Update::RemoveTimeHook);
    Ok(found
        .into_iter()
        .filter(|update| !(stop_removed && *update == Update::StopHookOnPath))
        .filter(|update| !(time_removed && *update == Update::TimeHook))
        .collect())
}

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
    let missing = files.is_none();
    let files = files.unwrap_or(FIRST);
    match versions(files, running) {
        Err(why) => found.push(why),
        Ok((from, to)) if from < to => found.push(format!(
            "the project's files are up to {files}{}, and this Rotproof is {running}: run `rotproof init`, which \
             updates them",
            if missing { " (no files in the declaration)" } else { "" }
        )),
        Ok(_) => {}
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_is_three_numbers_and_a_pre_release() {
        for good in [
            "0.2.0",
            "10.0.12",
            "1.0.0-rc.1",
            "1.0.0-alpha",
            "1.0.0-x-y.7.z",
        ] {
            assert!(version(good).is_some(), "{good}");
        }
        for bad in [
            "0.2",
            "0.2.0.1",
            "0.2.x",
            "v0.2.0",
            " 0.2.0",
            "0..1",
            "+1.0.0",
            "1.0.0-",
            "1.0.0-rc..1",
            "1.0.0-rc.01",
            "1.0.0-rc_1",
            "1.0.0+build",
            // The form PyPI shows: the declaration takes the version as Rotproof writes it, from Cargo
            "1.0.0rc1",
        ] {
            assert_eq!(version(bad), None, "{bad}");
        }
        assert!(version("0.10.0") > version("0.9.9"));
    }

    #[test]
    fn a_pre_release_comes_before_its_release_in_semver_order() {
        // SemVer 2.0.0, section 11, with the release Rotproof is going to
        let ordered = [
            "0.4.0",
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-alpha.beta",
            "1.0.0-beta",
            "1.0.0-beta.2",
            "1.0.0-beta.11",
            "1.0.0-rc.1",
            "1.0.0-rc.2",
            "1.0.0-rc.10",
            "1.0.0",
            "1.0.1-rc.1",
        ];
        for pair in ordered.windows(2) {
            assert!(version(pair[0]) < version(pair[1]), "{pair:?}");
        }
        assert_eq!(version("1.0.0-rc.1"), version("1.0.0-rc.1"));
    }

    #[test]
    fn a_pre_release_of_rotproof_upgrades_and_checks_a_project() {
        // A pre-release once panicked here, as its own version did not read
        assert!(pending("0.4.0", "1.0.0-rc.1", &[]).is_ok());
        assert_eq!(
            problems(Some("1.0.0-rc.1"), &[], "1.0.0-rc.1"),
            Vec::<String>::new()
        );
        assert!(problems(Some("0.4.0"), &[], "1.0.0-rc.1")[0].contains("run `rotproof init`"));
        assert!(problems(Some("1.0.0-rc.1"), &[], "1.0.0")[0].contains("run `rotproof init`"));
        assert!(problems(Some("1.0.0"), &[], "1.0.0-rc.1")[0].contains("newer than this Rotproof"));
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
        // The hook beside the stop hook is added only when the stop hook stays
        assert_eq!(
            pending("0.3.0", "0.4.0", &[]),
            Ok(vec![Update::StopHookOnPath, Update::TimeHook])
        );
        assert_eq!(
            pending("0.3.0", "1.0.0-rc.1", &[]),
            Ok(vec![Update::TimeHook, Update::RemoveStopHook])
        );
        assert_eq!(
            pending("0.3.0", "1.0.0-rc.1", &["claude-remove-stop-hook".into()]),
            Ok(vec![Update::StopHookOnPath, Update::TimeHook])
        );
        // The time hook is not added on the way to the version that takes it out, unless that update is declined
        assert_eq!(
            pending("0.3.0", "1.0.0-rc.4", &[]),
            Ok(vec![Update::RemoveStopHook, Update::RemoveTimeHook])
        );
        assert_eq!(
            pending("0.3.0", "1.0.0-rc.4", &["claude-remove-time-hook".into()]),
            Ok(vec![Update::TimeHook, Update::RemoveStopHook])
        );
        // A project that declined the update that added it still has a known name to decline
        assert_eq!(
            pending("0.3.0", "1.0.0-rc.4", &["claude-time-hook".into()]),
            Ok(vec![Update::RemoveStopHook, Update::RemoveTimeHook])
        );
        assert_eq!(
            pending("1.0.0-rc.3", "1.0.0-rc.4", &[]),
            Ok(vec![Update::RemoveTimeHook])
        );
        assert_eq!(pending("1.0.0-rc.4", "1.0.0", &[]), Ok(vec![]));
        assert!(
            problems(
                Some("1.0.0-rc.4"),
                &["claude-time-hook".into()],
                "1.0.0-rc.4"
            )
            .is_empty()
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
        // Written as the first release, it is not missing
        let written = problems(Some("0.1.0"), &[], "0.2.0");
        assert!(written[0].contains("up to 0.1.0, and"), "{written:?}");
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

    /// The settings `rotproof create` wrote before 0.4.0: the stop hook alone.
    const SETTINGS_0_2: &str = r#"{
  "permissions": {
    "deny": [
      "Edit(/.config/rotproof-approved.toml)"
    ]
  },
  "hooks": {
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "rotproof stop-hook"
          }
        ]
      }
    ]
  }
}
"#;

    /// Upgraded untouched, the settings are what a new project gets: every update of the file, in their order.
    #[test]
    fn untouched_settings_upgrade_to_what_a_new_project_gets() {
        let mut text = SETTINGS_0_2.to_string();
        for update in [
            Update::StopHookOnPath,
            Update::TimeHook,
            Update::RemoveStopHook,
            Update::RemoveTimeHook,
        ] {
            let Outcome::Changed(changed) = update.apply(Some(&text)) else {
                panic!("{} changes them", update.name());
            };
            text = changed;
        }
        assert_eq!(text, hook::SETTINGS[0].1);
        for update in UPDATES {
            assert_eq!(
                update.apply(Some(&text)),
                Outcome::Done,
                "{}",
                update.name()
            );
        }
    }

    /// The time hook is added once on each event, after what the project runs there, and only where a stop hook runs
    /// Rotproof.
    #[test]
    fn the_time_hook_is_added_once_on_each_event_and_only_beside_the_stop_hook() {
        let Outcome::Changed(added) = Update::TimeHook.apply(Some(SETTINGS_0_2)) else {
            panic!("the hook is added");
        };
        let read: Value = serde_json::from_str(&added).unwrap();
        for event in hook::TIME_EVENTS {
            assert_eq!(
                read["hooks"][event],
                json!([group(hook::TIME_COMMAND)]),
                "{event}"
            );
        }
        assert_eq!(Update::TimeHook.apply(Some(&added)), Outcome::Done);
        // A hook the project runs on the event stays first
        let theirs = SETTINGS_0_2.replace(
            "\"hooks\": {",
            "\"hooks\": {\n    \"UserPromptSubmit\": [{\"hooks\": [{\"type\": \"command\", \"command\": \"x\"}]}],",
        );
        let Outcome::Changed(kept) = Update::TimeHook.apply(Some(&theirs)) else {
            panic!("the hook is added");
        };
        let read: Value = serde_json::from_str(&kept).unwrap();
        assert_eq!(
            read["hooks"]["UserPromptSubmit"],
            json!([group("x"), group(hook::TIME_COMMAND)])
        );
        // Nothing to add to: no settings, no hooks, or a stop hook that is not Rotproof's
        for text in [
            None,
            Some("{}"),
            Some(
                "{\"hooks\": {\"Stop\": [{\"hooks\": [{\"type\": \"command\", \"command\": \"x\"}]}]}}",
            ),
        ] {
            assert_eq!(Update::TimeHook.apply(text), Outcome::Done, "{text:?}");
        }
        for text in [
            "not json",
            "{\"hooks\": {\"Stop\": [{\"hooks\": [{\"command\": \"rotproof stop-hook\"}]}], \"SessionStart\": {}}}",
        ] {
            let Outcome::ByHand(why) = Update::TimeHook.apply(Some(text)) else {
                panic!("{text} needs a person");
            };
            assert!(why.contains("declined = [\"claude-time-hook\"]"), "{why}");
        }
    }

    /// The update runs once, and only where a stop hook runs Rotproof.
    #[test]
    fn the_hook_beside_the_stop_hook_is_added_once_and_only_beside_it() {
        let Outcome::Changed(upgraded) = Update::StopHookOnPath.apply(Some(SETTINGS_0_2)) else {
            panic!("the hook is added");
        };
        let read: Value = serde_json::from_str(&upgraded).unwrap();
        assert_eq!(read["hooks"]["Stop"][1], group(hook::ON_PATH));
        assert_eq!(Update::StopHookOnPath.apply(Some(&upgraded)), Outcome::Done);
        // Nothing to guard: no settings, no hooks, or a stop hook that is not Rotproof's
        for text in [
            None,
            Some("{}"),
            Some(
                "{\"hooks\": {\"Stop\": [{\"hooks\": [{\"type\": \"command\", \"command\": \"x\"}]}]}}",
            ),
        ] {
            assert_eq!(
                Update::StopHookOnPath.apply(text),
                Outcome::Done,
                "{text:?}"
            );
        }
        // What the project added stays, in its order
        let theirs = SETTINGS_0_2.replace(
            "\"permissions\": {",
            "\"env\": {\"A\": \"1\"},\n  \"permissions\": {",
        );
        let Outcome::Changed(kept) = Update::StopHookOnPath.apply(Some(&theirs)) else {
            panic!("the hook is added");
        };
        assert!(
            kept.find("\"env\"") < kept.find("\"permissions\""),
            "{kept}"
        );
        let Outcome::ByHand(why) = Update::StopHookOnPath.apply(Some("not json")) else {
            panic!("needs a person");
        };
        assert!(
            why.contains("declined = [\"claude-stop-hook-on-path\"]"),
            "{why}"
        );
    }

    /// Only Rotproof's own hooks go, matched exactly; what the project runs on Stop, beside them or in their group,
    /// stays in its order.
    #[test]
    fn the_stop_hook_is_taken_out_and_only_it() {
        let Outcome::Changed(on_path) = Update::StopHookOnPath.apply(Some(SETTINGS_0_2)) else {
            panic!("the hook is added");
        };
        let Outcome::Changed(removed) = Update::RemoveStopHook.apply(Some(&on_path)) else {
            panic!("the hooks are taken out");
        };
        let read: Value = serde_json::from_str(&removed).unwrap();
        assert!(read["hooks"].get("Stop").is_none(), "{removed}");
        assert_eq!(Update::RemoveStopHook.apply(Some(&removed)), Outcome::Done);
        // The project's own hooks stay: a group of its own, one beside Rotproof's in a group, and a command that runs
        // rotproof stop-hook otherwise
        let theirs = r#"{"hooks": {"Stop": [
            {"hooks": [{"type": "command", "command": "x"}]},
            {"hooks": [{"type": "command", "command": "rotproof stop-hook"}, {"type": "command", "command": "y"}]},
            {"hooks": [{"type": "command", "command": "uv run rotproof stop-hook"}]}
        ], "SessionStart": []}}"#;
        let Outcome::Changed(kept) = Update::RemoveStopHook.apply(Some(theirs)) else {
            panic!("the hook is taken out");
        };
        let read: Value = serde_json::from_str(&kept).unwrap();
        assert_eq!(
            read["hooks"]["Stop"],
            json!([group("x"), group("y"), group("uv run rotproof stop-hook")])
        );
        // Stop taken out, the events after it keep their order
        let Outcome::Changed(alone) = Update::RemoveStopHook.apply(Some(
            r#"{"hooks": {"A": [], "Stop": [{"hooks": [{"command": "rotproof stop-hook"}]}], "B": [], "C": []}}"#,
        )) else {
            panic!("the hook is taken out");
        };
        let keys: Vec<String> = serde_json::from_str::<Value>(&alone).unwrap()["hooks"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        assert_eq!(keys, ["A", "B", "C"]);
        // No event left: `hooks` goes too, and what else the project wrote stays
        let Outcome::Changed(bare) = Update::RemoveStopHook.apply(Some(
            r#"{"hooks": {"Stop": [{"hooks": [{"command": "rotproof stop-hook"}]}]}, "env": {"A": "1"}}"#,
        )) else {
            panic!("the hook is taken out");
        };
        let read: Value = serde_json::from_str(&bare).unwrap();
        assert!(read.get("hooks").is_none(), "{bare}");
        assert_eq!(read["env"]["A"], "1");
        // Nothing to take out
        for text in [None, Some("{}"), Some("{\"hooks\": {\"Stop\": []}}")] {
            assert_eq!(
                Update::RemoveStopHook.apply(text),
                Outcome::Done,
                "{text:?}"
            );
        }
        let Outcome::ByHand(why) = Update::RemoveStopHook.apply(Some("not json")) else {
            panic!("needs a person");
        };
        assert!(
            why.contains("declined = [\"claude-remove-stop-hook\"]"),
            "{why}"
        );
    }

    /// What Rotproof wrote goes, matched exactly; what the project runs on the same events stays, in its order.
    #[test]
    fn the_time_hook_is_taken_out_and_only_it() {
        // What `rotproof create` wrote from 0.4.0 to 1.0.0-rc.3
        let written = r#"{
  "permissions": {
    "deny": [
      "Edit(/.config/rotproof-approved.toml)"
    ]
  },
  "hooks": {
    "SessionStart": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "rotproof time-hook"
          }
        ]
      }
    ],
    "UserPromptSubmit": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "rotproof time-hook"
          }
        ]
      }
    ]
  }
}
"#;
        let Outcome::Changed(removed) = Update::RemoveTimeHook.apply(Some(written)) else {
            panic!("the hooks are taken out");
        };
        // The file ends as a new project's: no hooks left, and the deny rule where it was
        assert_eq!(removed, hook::SETTINGS[0].1);
        assert_eq!(Update::RemoveTimeHook.apply(Some(&removed)), Outcome::Done);
        // The project's own hooks stay: a group of its own, one beside Rotproof's in a group, and a command that runs
        // rotproof time-hook otherwise (a path to the binary, as a project that builds Rotproof itself runs it)
        let theirs = r#"{"hooks": {"SessionStart": [
            {"hooks": [{"type": "command", "command": "x"}]},
            {"hooks": [{"type": "command", "command": "rotproof time-hook"}, {"type": "command", "command": "y"}]},
            {"hooks": [{"type": "command", "command": "\"$CLAUDE_PROJECT_DIR/rotproof/target/debug/rotproof\" time-hook"}]}
        ], "Stop": [{"hooks": [{"type": "command", "command": "z"}]}]}}"#;
        let Outcome::Changed(kept) = Update::RemoveTimeHook.apply(Some(theirs)) else {
            panic!("the hook is taken out");
        };
        let read: Value = serde_json::from_str(&kept).unwrap();
        assert_eq!(
            read["hooks"]["SessionStart"],
            json!([
                group("x"),
                group("y"),
                group("\"$CLAUDE_PROJECT_DIR/rotproof/target/debug/rotproof\" time-hook")
            ])
        );
        assert_eq!(read["hooks"]["Stop"], json!([group("z")]));
        // An event taken out leaves the events around it in their order, and `hooks` stays while one is left
        let Outcome::Changed(alone) = Update::RemoveTimeHook.apply(Some(
            r#"{"hooks": {"A": [], "SessionStart": [{"hooks": [{"command": "rotproof time-hook"}]}], "B": [], "UserPromptSubmit": [{"hooks": [{"command": "rotproof time-hook"}]}]}}"#,
        )) else {
            panic!("the hooks are taken out");
        };
        let keys: Vec<String> = serde_json::from_str::<Value>(&alone).unwrap()["hooks"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        assert_eq!(keys, ["A", "B"]);
        // Nothing to take out, and `hooks` the project left empty is not touched
        for text in [
            None,
            Some("{}"),
            Some("{\"hooks\": {}}"),
            Some("{\"hooks\": {\"SessionStart\": []}}"),
            Some("{\"hooks\": {\"SessionStart\": {}}}"),
        ] {
            assert_eq!(
                Update::RemoveTimeHook.apply(text),
                Outcome::Done,
                "{text:?}"
            );
        }
        let Outcome::ByHand(why) = Update::RemoveTimeHook.apply(Some("not json")) else {
            panic!("needs a person");
        };
        assert!(
            why.contains("declined = [\"claude-remove-time-hook\"]"),
            "{why}"
        );
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

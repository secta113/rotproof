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
//! - **What an update does to the settings file is in `settings.rs`, and how a version is read and ordered in
//!   `version.rs`.** This module says which updates a project takes, and tells a person what to do by hand when an edit
//!   cannot be made.

use crate::declaration::DECLARATION;
use crate::hook;
use crate::settings;
use crate::version::{Version, version};

/// The version the project's files are up to when the declaration has no `files`: the first release.
pub const FIRST: &str = "0.1.0";

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
        let edited = match self {
            Update::DenyApprovals => settings::deny_approvals(text),
            Update::StopHookOnPath => settings::stop_hook_on_path(text),
            Update::TimeHook => settings::time_hook(text),
            Update::RemoveStopHook => settings::remove_stop_hook(text),
            Update::RemoveTimeHook => settings::remove_time_hook(text),
        };
        match edited {
            Ok(None) => Outcome::Done,
            Ok(Some(changed)) => Outcome::Changed(changed),
            Err(why) => Outcome::ByHand(format!(
                "{}: {why}. {}, {}",
                self.file(),
                self.by_hand(),
                self.declining()
            )),
        }
    }

    /// What a person does when the update cannot be applied: the edit, as the settings then need it.
    fn by_hand(self) -> String {
        match self {
            Update::DenyApprovals => format!(
                "Add \"{}\" to permissions.deny there",
                settings::deny_rule()
            ),
            Update::StopHookOnPath => format!(
                "Add a Stop hook that runs {:?} beside {:?}",
                hook::ON_PATH,
                hook::COMMAND
            ),
            Update::TimeHook => format!(
                "Add a {} hook that runs {:?}",
                hook::TIME_EVENTS.join(" and a "),
                hook::TIME_COMMAND
            ),
            Update::RemoveStopHook => format!(
                "Remove the Stop hooks that run {:?} and {:?}",
                hook::COMMAND,
                hook::ON_PATH
            ),
            Update::RemoveTimeHook => format!(
                "Remove the {} hooks that run {:?}",
                hook::TIME_EVENTS.join(" and "),
                hook::TIME_COMMAND
            ),
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
    use crate::settings::{deny_rule, group};
    use serde_json::{Value, json};

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

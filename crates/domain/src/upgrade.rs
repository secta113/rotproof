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
    /// The stop hook taken out, with the hook beside it, from `.claude/settings.json`: 1.0 no longer has one
    RemoveStopHook,
    /// The time hook taken out of `.claude/settings.json`: 1.0 no longer has one
    RemoveTimeHook,
    /// The rule that denied Claude Code editing the approvals file taken out of `.claude/settings.json`: 2.0 lets an
    /// agent run `rotproof approve`, with a reason, and no longer writes the rule
    RemoveDenyApprovals,
}

/// Every update, oldest first.
pub const UPDATES: [Update; 3] = [
    Update::RemoveStopHook,
    Update::RemoveTimeHook,
    Update::RemoveDenyApprovals,
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
            Update::RemoveStopHook => "claude-remove-stop-hook",
            Update::RemoveTimeHook => "claude-remove-time-hook",
            Update::RemoveDenyApprovals => "claude-remove-deny-approvals",
        }
    }

    /// The version that brings it
    pub fn version(self) -> &'static str {
        match self {
            Update::RemoveStopHook => "1.0.0-rc.1",
            Update::RemoveTimeHook => "1.0.0-rc.4",
            Update::RemoveDenyApprovals => "2.0.0-rc.2",
        }
    }

    /// The file it changes, from the root
    pub fn file(self) -> &'static str {
        match self {
            Update::RemoveStopHook | Update::RemoveTimeHook | Update::RemoveDenyApprovals => {
                ".claude/settings.json"
            }
        }
    }

    /// What it does to its file, whose text is `text` (`None` when there is no file).
    pub fn apply(self, text: Option<&str>) -> Outcome {
        let edited = match self {
            Update::RemoveStopHook => settings::remove_stop_hook(text),
            Update::RemoveTimeHook => settings::remove_time_hook(text),
            Update::RemoveDenyApprovals => settings::remove_deny_approvals(text),
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
            Update::RemoveDenyApprovals => format!(
                "Remove \"{}\" from permissions.deny there",
                settings::deny_rule()
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

/// What a project may name in `declined`: every update, and the question whether its agent reads the guide
/// (`pointer.rs`), which belongs to no version and is asked until it is answered.
pub fn declinable() -> Vec<&'static str> {
    UPDATES
        .iter()
        .map(|update| update.name())
        .chain([crate::pointer::NAME])
        .collect()
}

/// What `rotproof check` finds in the declaration's `files` and `declined`, against the running version.
pub fn problems(files: Option<&str>, declined: &[String], running: &str) -> Vec<String> {
    let mut found = Vec::new();
    let known = declinable();
    for name in declined {
        if !known.contains(&name.as_str()) {
            found.push(format!(
                "{DECLARATION}: declined names {name:?}, which is nothing Rotproof offers (known: {})",
                known.join(", ")
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

/// Which command brings the guide `.rotproof/AGENTS.md` into line, when it is missing or is not what this version
/// writes. When `files` names the running version, the project's files are up to it and the guide changed under the
/// same version (a development build, or an edit by hand), so `rotproof create` writes it; otherwise the version moved
/// and `rotproof init`, which also sets `files` and says what to do by hand, is the upgrade. A missing `files` is the
/// first release's, and a `files` that cannot be read is another version.
pub fn guide_command(files: Option<&str>, running: &str) -> &'static str {
    match (version(files.unwrap_or(FIRST)), version(running)) {
        (Some(files), Some(running)) if files == running => "run `rotproof create`",
        _ => "run `rotproof init`",
    }
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
    fn the_guide_is_written_by_create_under_the_same_version_and_by_init_otherwise() {
        assert_eq!(
            guide_command(Some("1.0.0-rc.1"), "1.0.0-rc.1"),
            "run `rotproof create`"
        );
        for files in [
            Some("1.0.0-rc.1"),
            Some("1.0.1"),
            Some("not a version"),
            None,
        ] {
            assert_eq!(
                guide_command(files, "1.0.0"),
                "run `rotproof init`",
                "{files:?}"
            );
        }
        // Newer than the running one is no upgrade either: init says so
        assert_eq!(
            guide_command(Some("1.0.0"), "1.0.0-rc.1"),
            "run `rotproof init`"
        );
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
        // The rule that denied editing the approvals file is no longer added, from any version
        assert_eq!(pending("0.1.0", "0.2.0", &[]), Ok(vec![]));
        assert_eq!(pending("0.2.0", "0.2.0", &[]), Ok(vec![]));
        assert_eq!(pending("0.1.0", "0.1.5", &[]), Ok(vec![]));
        assert_eq!(pending("0.1.0", "0.3.0", &[]), Ok(vec![]));
        // Nothing adds a hook any more: from before the stop hook was taken out, both hooks go, in the order of the
        // versions, and the project that declined one keeps it
        assert_eq!(pending("0.3.0", "0.4.0", &[]), Ok(vec![]));
        assert_eq!(
            pending("0.3.0", "1.0.0-rc.1", &[]),
            Ok(vec![Update::RemoveStopHook])
        );
        assert_eq!(
            pending("0.3.0", "1.0.0-rc.1", &["claude-remove-stop-hook".into()]),
            Ok(vec![])
        );
        assert_eq!(
            pending("0.3.0", "1.0.0", &[]),
            Ok(vec![Update::RemoveStopHook, Update::RemoveTimeHook])
        );
        assert_eq!(
            pending("0.3.0", "1.0.0", &["claude-remove-time-hook".into()]),
            Ok(vec![Update::RemoveStopHook])
        );
        assert_eq!(
            pending("1.0.0-rc.3", "1.0.0-rc.4", &[]),
            Ok(vec![Update::RemoveTimeHook])
        );
        assert_eq!(pending("1.0.0-rc.4", "1.0.0", &[]), Ok(vec![]));
        // The rule goes from 2.0.0-rc.2, after the hooks when a project comes from before them, and a project may
        // keep it
        assert_eq!(pending("2.0.0-rc.1", "2.0.0-rc.1", &[]), Ok(vec![]));
        assert_eq!(
            pending("2.0.0-rc.1", "2.0.0", &[]),
            Ok(vec![Update::RemoveDenyApprovals])
        );
        assert_eq!(
            pending("1.0.0", "2.0.0-rc.2", &[]),
            Ok(vec![Update::RemoveDenyApprovals])
        );
        assert_eq!(
            pending("0.1.0", "2.0.0", &[]),
            Ok(vec![
                Update::RemoveStopHook,
                Update::RemoveTimeHook,
                Update::RemoveDenyApprovals
            ])
        );
        assert_eq!(
            pending("1.0.0", "2.0.0", &["claude-remove-deny-approvals".into()]),
            Ok(vec![])
        );
        // The names of the updates that added the hooks, until 1.0.0-rc.4, and the one that added the rule, until
        // 2.0.0-rc.1, are no names now
        for name in [
            "claude-time-hook",
            "claude-stop-hook-on-path",
            "claude-deny-approvals",
        ] {
            let found = problems(Some("1.0.0"), &[name.into()], "1.0.0");
            assert_eq!(found.len(), 1, "{found:?}");
            assert!(
                found[0].contains("which is nothing Rotproof offers"),
                "{found:?}"
            );
        }
        // The question whether the agent reads the guide is declined by its name, and is no update to apply
        assert_eq!(
            problems(Some("1.0.0"), &[crate::pointer::NAME.into()], "1.0.0"),
            Vec::<String>::new()
        );
        assert!(
            UPDATES
                .iter()
                .all(|update| update.name() != crate::pointer::NAME)
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
            misspelled[0].contains("which is nothing Rotproof offers"),
            "{misspelled:?}"
        );
    }

    #[test]
    fn the_deny_rule_is_taken_out_once_and_only_it() {
        let rule = deny_rule();
        let settings = format!(
            r#"{{"hooks": {{"Stop": []}}, "permissions": {{"allow": ["Bash(ls)"], "deny": ["{rule}"]}}}}"#
        );
        let Outcome::Changed(once) = Update::RemoveDenyApprovals.apply(Some(&settings)) else {
            panic!("the rule is taken out");
        };
        let read: Value = serde_json::from_str(&once).unwrap();
        assert!(read["permissions"].get("deny").is_none(), "{once}");
        assert_eq!(read["permissions"]["allow"][0], "Bash(ls)");
        // The keys keep their order
        assert!(once.find("hooks") < once.find("permissions"), "{once}");
        assert_eq!(
            Update::RemoveDenyApprovals.apply(Some(&once)),
            Outcome::Done
        );
        // What the project denies itself stays
        let theirs = format!(r#"{{"permissions": {{"deny": ["Read(x)", "{rule}"]}}}}"#);
        let Outcome::Changed(kept) = Update::RemoveDenyApprovals.apply(Some(&theirs)) else {
            panic!("the rule is taken out");
        };
        let read: Value = serde_json::from_str(&kept).unwrap();
        assert_eq!(read["permissions"]["deny"], json!(["Read(x)"]));
        // No settings, none of Rotproof's: nothing to take out
        for text in [None, Some("{}")] {
            assert_eq!(Update::RemoveDenyApprovals.apply(text), Outcome::Done);
        }
    }

    /// The settings `rotproof create` wrote from 1.0.0-rc.4 to 2.0.0-rc.1: the rule that denied editing the approvals
    /// file, and no hook.
    const DENY_ONLY: &str = "{\n  \"permissions\": {\n    \"deny\": [\n      \"Edit(/.config/rotproof-approved.toml)\"\n    ]\n  }\n}\n";

    /// The settings `rotproof create` wrote from 0.4.0 to 1.0.0-rc.3: the stop hook and the hook beside it, and the
    /// time hook on both of its events, as the updates of those versions wrote them.
    fn settings_0_4() -> String {
        let mut settings: Value = serde_json::from_str(DENY_ONLY).unwrap();
        settings["hooks"] = json!({
            "Stop": [group(hook::COMMAND), group(hook::ON_PATH)],
            "SessionStart": [group(hook::TIME_COMMAND)],
            "UserPromptSubmit": [group(hook::TIME_COMMAND)],
        });
        serde_json::to_string_pretty(&settings).unwrap()
            + "
"
    }

    /// Upgraded untouched, the settings end as nothing is left in them, which a new project has no file for: every
    /// update of the file, in their order.
    #[test]
    fn untouched_settings_upgrade_to_what_a_new_project_gets() {
        let mut text = settings_0_4();
        for update in UPDATES {
            let Outcome::Changed(changed) = update.apply(Some(&text)) else {
                panic!("{} changes them", update.name());
            };
            text = changed;
        }
        assert_eq!(text, "{}\n");
        for update in UPDATES {
            assert_eq!(
                update.apply(Some(&text)),
                Outcome::Done,
                "{}",
                update.name()
            );
        }
    }

    /// Only Rotproof's own hooks go, matched exactly; what the project runs on Stop, beside them or in their group,
    /// stays in its order.
    #[test]
    fn the_stop_hook_is_taken_out_and_only_it() {
        let Outcome::Changed(removed) = Update::RemoveStopHook.apply(Some(&settings_0_4())) else {
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
        // The file ends as `rotproof create` wrote it from 1.0.0-rc.4: no hooks left, and the deny rule where it was
        assert_eq!(removed, DENY_ONLY);
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
        for text in [Some("not json"), Some("[]"), Some("\"x\"")] {
            let Outcome::ByHand(why) = Update::RemoveDenyApprovals.apply(text) else {
                panic!("{text:?} needs a person");
            };
            assert!(
                why.contains("declined = [\"claude-remove-deny-approvals\"]"),
                "{why}"
            );
            assert!(
                why.contains("Remove \"Edit(/.config/rotproof-approved.toml)\""),
                "{why}"
            );
        }
        // A shape that holds no rule of Rotproof's has nothing to take out, whatever else it holds
        for text in [
            "{\"permissions\": []}",
            "{\"permissions\": {\"deny\": \"x\"}}",
        ] {
            assert_eq!(Update::RemoveDenyApprovals.apply(Some(text)), Outcome::Done);
        }
    }
}

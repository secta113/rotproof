//! The edits an update makes to Claude Code's settings, `.claude/settings.json`, as a JSON object. The settings
//! `rotproof create` writes and the commands of the hooks Rotproof wrote are in `hook.rs`; which update asks for which
//! edit, and what a person is told when one cannot be made, is in `upgrade.rs`.
//!
//! - **An edit only takes out what Rotproof wrote and no longer keeps (the hooks, the rule that denied editing the
//!   approvals file),** matched exactly, and keeps every other key and value of the project's, and the order of the
//!   keys.
//! - **An edit says what it came to and nothing about the update that asked for it.** It gives the settings' new text,
//!   `None` when they have it already, or why a person has to make the edit.

use serde_json::{Map, Value, json};

use crate::approvals::APPROVALS;
use crate::hook;

/// What an edit came to: the new text of the settings, `None` when they need no change, or, as `Err`, why only a
/// person can make it.
pub type Edited = Result<Option<String>, String>;

/// The rule that denied Claude Code editing the approvals file, which `rotproof create` wrote until 2.0.0-rc.1.
pub fn deny_rule() -> String {
    format!("Edit(/{APPROVALS})")
}

/// Take the rule that denied editing the approvals file ([`deny_rule`]) out of Claude Code's settings, `text`: each
/// copy of it in `permissions.deny`, `deny` when no rule is left, and `permissions` when no key is left. A rule the
/// project wrote itself (another text) stays, and so does everything else, in its order.
pub fn remove_deny_approvals(text: Option<&str>) -> Edited {
    let Some(text) = text else {
        return Ok(None);
    };
    let mut settings = object(text)?;
    let rule = deny_rule();
    let Some(Value::Object(permissions)) = settings.get_mut("permissions") else {
        return Ok(None);
    };
    let Some(Value::Array(deny)) = permissions.get_mut("deny") else {
        return Ok(None);
    };
    let before = deny.len();
    deny.retain(|r| r.as_str() != Some(rule.as_str()));
    if deny.len() == before {
        return Ok(None);
    }
    if deny.is_empty() {
        permissions.shift_remove("deny");
    }
    if permissions.is_empty() {
        settings.shift_remove("permissions");
    }
    Ok(Some(written(settings)))
}

/// Take the stop hook ([`hook::COMMAND`]) and the hook beside it ([`hook::ON_PATH`]) out of Claude Code's settings,
/// `text`: each hook that runs one of them exactly, a group left with no hook, `Stop` when no group is left, and
/// `hooks` when no event is left. A hook the project changed (another command that runs `rotproof stop-hook`) is not
/// Rotproof's text, and stays: the command fails with exit 1 on every stop, saying it was removed.
pub fn remove_stop_hook(text: Option<&str>) -> Edited {
    remove(text, &["Stop"], &[hook::COMMAND, hook::ON_PATH])
}

/// Take the time hook ([`hook::TIME_COMMAND`]) out of Claude Code's settings, `text`: each hook that runs it exactly
/// on `SessionStart` and `UserPromptSubmit`, a group left with no hook, an event left with no group, and `hooks` when
/// no event is left. A hook the project changed (another command that runs `rotproof time-hook`, such as a path to
/// the binary) is not Rotproof's text, and stays: the command fails with exit 1 at the start of every session and on
/// every prompt, saying it was removed.
pub fn remove_time_hook(text: Option<&str>) -> Edited {
    remove(text, &hook::TIME_EVENTS, &[hook::TIME_COMMAND])
}

/// The settings `text` without the hooks that run one of `commands` exactly on one of `events`, or `None` when none
/// does. A project that has no settings has nothing to take out.
fn remove(text: Option<&str>, events: &[&str], commands: &[&str]) -> Edited {
    let Some(text) = text else {
        return Ok(None);
    };
    let mut settings = object(text)?;
    Ok(take_out(&mut settings, events, commands).then(|| written(settings)))
}

/// Take the hooks that run one of `commands` exactly out of `events` in `settings`: a group left with no hook goes, an
/// event left with no group goes, and `hooks` goes when no event is left. Whether anything was taken out.
fn take_out(settings: &mut Map<String, Value>, events: &[&str], commands: &[&str]) -> bool {
    let Some(Value::Object(hooks)) = settings.get_mut("hooks") else {
        return false;
    };
    let mut changed = false;
    for event in events {
        let Some(Value::Array(groups)) = hooks.get_mut(*event) else {
            continue;
        };
        if !runs(groups, commands) {
            continue;
        }
        changed = true;
        groups.retain_mut(|group| match group.get_mut("hooks") {
            Some(Value::Array(inside)) => {
                inside.retain(|candidate| !is_one_of(candidate, commands));
                !inside.is_empty()
            }
            _ => true,
        });
        if groups.is_empty() {
            // Shifted out, so the events after it keep their order
            hooks.shift_remove(*event);
        }
    }
    // The settings of a project that ran no other hook end as `rotproof create` writes them now: with no `hooks`
    if changed && hooks.is_empty() {
        settings.shift_remove("hooks");
    }
    changed
}

/// The settings `text` as a JSON object, or why a person has to edit them.
fn object(text: &str) -> Result<Map<String, Value>, String> {
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(settings)) => Ok(settings),
        _ => Err("it is not a JSON object Rotproof can read".to_string()),
    }
}

/// Whether one of the hook groups `groups` of an event runs one of `commands`.
fn runs(groups: &[Value], commands: &[&str]) -> bool {
    groups.iter().any(|group| {
        group["hooks"]
            .as_array()
            .is_some_and(|hooks| hooks.iter().any(|h| is_one_of(h, commands)))
    })
}

/// Whether the hook `candidate` runs one of `commands`, exactly.
fn is_one_of(candidate: &Value, commands: &[&str]) -> bool {
    commands
        .iter()
        .any(|command| candidate["command"] == *command)
}

/// A hook group of its own that runs `command`, as `rotproof create` writes one.
pub fn group(command: &str) -> Value {
    json!({ "hooks": [{ "type": "command", "command": command }] })
}

/// The settings as the file's text, as `rotproof create` writes it: indented, keys in their order, ending in a newline.
fn written(settings: Map<String, Value>) -> String {
    let mut text = serde_json::to_string_pretty(&Value::Object(settings))
        .expect("a JSON value always serializes");
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every edit reads the settings the same way, and says why when it cannot.
    #[test]
    fn settings_that_are_no_object_are_left_to_a_person_by_every_edit() {
        let edits: [fn(Option<&str>) -> Edited; 3] =
            [remove_deny_approvals, remove_stop_hook, remove_time_hook];
        for edit in edits {
            for text in ["not json", "[]", "\"x\""] {
                assert_eq!(
                    edit(Some(text)),
                    Err("it is not a JSON object Rotproof can read".to_string()),
                    "{text}"
                );
            }
        }
    }

    /// A project with no settings has nothing to take out of.
    #[test]
    fn no_settings_have_nothing_to_take_out() {
        for edit in [remove_deny_approvals, remove_stop_hook, remove_time_hook] {
            assert_eq!(edit(None), Ok(None));
        }
    }

    /// The rule goes alone and exactly: what else the project denies, allows or runs stays in its order, and a key left
    /// empty by the rule going goes with it, while one the project left empty is not touched.
    #[test]
    fn the_deny_rule_is_taken_out_and_only_it() {
        let rule = deny_rule();
        // What `rotproof create` wrote until 2.0.0-rc.1: the rule alone, which leaves the file empty
        let written = format!(
            "{{\n  \"permissions\": {{\n    \"deny\": [\n      \"{rule}\"\n    ]\n  }}\n}}\n"
        );
        assert_eq!(
            remove_deny_approvals(Some(&written)),
            Ok(Some("{}\n".to_string()))
        );
        assert_eq!(remove_deny_approvals(Some("{}\n")), Ok(None));
        let mixed = format!(
            r#"{{"env": {{"A": "1"}}, "permissions": {{"allow": ["Bash(ls)"], "deny": ["Read(x)", "{rule}", "{rule}"]}}, "hooks": {{}}}}"#
        );
        let Ok(Some(kept)) = remove_deny_approvals(Some(&mixed)) else {
            panic!("the rule is taken out");
        };
        let read: Value = serde_json::from_str(&kept).unwrap();
        assert_eq!(read["permissions"]["deny"], json!(["Read(x)"]));
        assert_eq!(read["permissions"]["allow"], json!(["Bash(ls)"]));
        let keys: Vec<&String> = read.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["env", "permissions", "hooks"]);
        // Only the rule in the list goes: a `deny` left with nothing else goes, `permissions` stays while a key is left
        let only = format!(r#"{{"permissions": {{"allow": ["a"], "deny": ["{rule}"]}}}}"#);
        let Ok(Some(without)) = remove_deny_approvals(Some(&only)) else {
            panic!("the rule is taken out");
        };
        let read: Value = serde_json::from_str(&without).unwrap();
        assert_eq!(read, json!({"permissions": {"allow": ["a"]}}));
        // Nothing of Rotproof's to take out: another text, no list, no permissions
        for text in [
            r#"{"permissions": {"deny": ["Edit(/other.toml)"]}}"#,
            r#"{"permissions": {"deny": []}}"#,
            r#"{"permissions": {"deny": "x"}}"#,
            r#"{"permissions": []}"#,
            r#"{"hooks": {}}"#,
        ] {
            assert_eq!(remove_deny_approvals(Some(text)), Ok(None), "{text}");
        }
    }

    /// The hooks that go are those that run a command exactly, on the events asked for and no other.
    #[test]
    fn only_the_events_asked_for_lose_their_hooks() {
        let text = r#"{"hooks": {"A": [{"hooks": [{"command": "x"}]}], "B": [{"hooks": [{"command": "x"}]}]}}"#;
        let mut settings = object(text).unwrap();
        assert!(take_out(&mut settings, &["A"], &["x"]));
        assert_eq!(
            Value::Object(settings.clone())["hooks"],
            json!({"B": [{"hooks": [{"command": "x"}]}]})
        );
        assert!(!take_out(&mut settings, &["A"], &["x"]));
        assert!(take_out(&mut settings, &["B"], &["y", "x"]));
        assert!(settings.is_empty(), "{settings:?}");
    }
}

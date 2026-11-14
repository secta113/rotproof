//! The edits an update makes to Claude Code's settings, `.claude/settings.json`, as a JSON object. The settings
//! `rotproof create` writes and the commands of the hooks Rotproof wrote are in `hook.rs`; which update asks for which
//! edit, and what a person is told when one cannot be made, is in `upgrade.rs`.
//!
//! - **An edit only adds the rule Rotproof writes, or takes out the hooks Rotproof wrote,** matched exactly, and keeps
//!   every other key and value of the project's, and the order of the keys.
//! - **An edit says what it came to and nothing about the update that asked for it.** It gives the settings' new text,
//!   `None` when they have it already, or why a person has to make the edit.

use serde_json::{Map, Value, json};

use crate::approvals::APPROVALS;
use crate::hook;

/// What an edit came to: the new text of the settings, `None` when they need no change, or, as `Err`, why only a
/// person can make it.
pub type Edited = Result<Option<String>, String>;

/// The rule that denies Claude Code editing the approvals file.
pub fn deny_rule() -> String {
    format!("Edit(/{APPROVALS})")
}

/// Add the rule that denies editing the approvals file to Claude Code's settings, `text`. The keys keep their order.
pub fn deny_approvals(text: Option<&str>) -> Edited {
    let rule = deny_rule();
    let Some(text) = text else {
        return Err("there is no such file".to_string());
    };
    let mut settings = object(text)?;
    let permissions = settings
        .entry("permissions")
        .or_insert_with(|| Value::Object(Map::new()));
    let Value::Object(permissions) = permissions else {
        return Err("permissions is not an object".to_string());
    };
    let deny = permissions
        .entry("deny")
        .or_insert_with(|| Value::Array(Vec::new()));
    let Value::Array(deny) = deny else {
        return Err("permissions.deny is not a list".to_string());
    };
    if deny.iter().any(|r| r.as_str() == Some(rule.as_str())) {
        return Ok(None);
    }
    deny.push(Value::String(rule));
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
            [deny_approvals, remove_stop_hook, remove_time_hook];
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

    /// A project with no settings has nothing to add a hook to or take out of, and the rule is the one edit that
    /// says so.
    #[test]
    fn no_settings_are_said_by_the_rule_alone() {
        assert_eq!(
            deny_approvals(None),
            Err("there is no such file".to_string())
        );
        for edit in [remove_stop_hook, remove_time_hook] {
            assert_eq!(edit(None), Ok(None));
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

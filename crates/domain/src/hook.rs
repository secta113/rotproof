//! `rotproof stop-hook`: the hook Claude Code runs when the agent stops (its `Stop` hook). It sends the agent back once
//! when its last message leaves something open and nothing in `docs/` changed.
//!
//! - An agent's findings are written somewhere before they are lost: in its report ("not checked", "out of scope").
//!   The hook reads that report when the agent stops, and when it holds one of [`PHRASES`] while `docs/` has no
//!   change (`git status --porcelain -- docs`), it asks the agent to record the finding or to say where it already is.
//! - The agent decides what the phrase meant; the hook only makes the moment. It answers with `additionalContext`,
//!   which Claude Code shows as feedback rather than an error.
//! - It sends the agent back at most once per stop: while the agent is already continuing because of a stop hook
//!   (`stop_hook_active`), it lets the agent stop, so a phrase quoted in the answer cannot loop.
//! - The project is the nearest directory, from where the hook runs upwards, that holds `.config/rotproof.toml`.
//!   Outside one, the hook lets the agent stop and says nothing: it may be configured for every repository.
//!
//! An input from another hook, or a `git` that fails, is an error: the agent shows it, and stops.

use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Value, json};

/// The event of the hook this answers, in `hook_event_name`
const EVENT: &str = "Stop";
/// The field of the input that holds the agent's last message
const MESSAGE: &str = "last_assistant_message";

/// The settings Claude Code runs the project with, and where `rotproof create` writes them, once: the project's file
/// from then on. They run the hook, for which `rotproof` has to be on the `PATH` the agent runs hooks with, and deny
/// editing the approvals file (`approvals.rs`): Claude Code then refuses its edit tools and its shell's file commands
/// and redirects on it, with no prompt, so only a person adds an approval, through `rotproof approve`.
pub const SETTINGS: [(&str, &str); 1] = [(
    ".claude/settings.json",
    r#"{
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
"#,
)];

// A line that points at the records: the word spec, specs, backlog or knowledge standing alone in ASCII (so `spec に`,
// `docs/specs/x.md` and `Backlog` count, and `specific` or `inspect` do not). Matched against the line in lower case
static RECORDS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|[^a-z0-9_])(?:specs?|backlog|knowledge)(?:[^a-z0-9_]|$)").unwrap()
});

/// What in a report leaves something open, matched in any case. Words common in plain prose ("later") are left out:
/// a hook that fires on every message is answered without reading.
pub const PHRASES: [&str; 15] = [
    "未確認",
    "後で",
    "あとで",
    "別途",
    "対象外",
    "未決",
    "見送",
    "未着手",
    "保留",
    "not checked",
    "not verified",
    "unverified",
    "out of scope",
    "follow-up",
    "todo",
];

/// The changes a project's version control shows: the port the hook asks whether anything was recorded.
pub trait Changes {
    /// Whether anything in `dir` (from the root) is changed, staged or new.
    fn changed(&self, dir: &str) -> Result<bool, String>;
}

/// The directory whose changes say that a finding was recorded, from the root
pub const RECORDS_DIR: &str = "docs";

/// What the hook reads in `input` (the JSON the agent writes on stdin): the phrases of [`PHRASES`] the agent's last
/// message leaves open, in the order of the list. None while the agent is already continuing because of a stop hook.
/// Only when some are found does the hook ask whether [`RECORDS_DIR`] changed.
pub fn open_in(input: &str) -> Result<Vec<&'static str>, String> {
    let input: Value =
        serde_json::from_str(input).map_err(|e| format!("the hook input is not JSON: {e}"))?;
    let event = input
        .get("hook_event_name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if event != EVENT {
        return Err(format!(
            "the hook input is from {event:?}: run `rotproof stop-hook` as Claude Code's Stop hook"
        ));
    }
    if input.get("stop_hook_active").and_then(Value::as_bool) == Some(true) {
        return Ok(Vec::new());
    }
    match input.get(MESSAGE) {
        Some(Value::String(message)) => Ok(open_phrases(message)),
        // A turn that ended without text
        Some(Value::Null) => Ok(Vec::new()),
        _ => Err(format!("the {event} hook input has no {MESSAGE}")),
    }
}

/// What the hook prints to send the agent back, for the phrases `found` while nothing in [`RECORDS_DIR`] changed.
pub fn send_back(found: &[&str]) -> String {
    let quoted: Vec<String> = found.iter().map(|phrase| format!("\"{phrase}\"")).collect();
    let text = format!(
        "Your last message says {} and nothing in docs/ changed. If it leaves a finding open, record it now: an \
         item in docs/backlog/ (docs/backlog/rules.md), or the spec it belongs to. If it is already recorded, or is not \
         a finding, say where or why in one line, then stop.",
        quoted.join(", ")
    );
    let answer = json!({
        "hookSpecificOutput": {
            "hookEventName": EVENT,
            "additionalContext": text,
        }
    });
    answer.to_string()
}

/// The phrases of [`PHRASES`] that `message` holds, in the order of the list. A line that points at the records is
/// not read: what it leaves open is recorded where it points.
fn open_phrases(message: &str) -> Vec<&'static str> {
    let message: String = message
        .to_lowercase()
        .lines()
        .filter(|line| !RECORDS.is_match(line))
        .collect::<Vec<_>>()
        .join("\n");
    PHRASES
        .into_iter()
        .filter(|phrase| message.contains(phrase))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_settings_are_json_and_deny_editing_the_approvals_file() {
        let (_, text) = SETTINGS[0];
        let read: Value = serde_json::from_str(text).unwrap();
        // A rule's leading `/` is the directory that holds `.claude/`: the project's root, where the file is
        assert_eq!(
            read["permissions"]["deny"],
            json!([format!("Edit(/{})", crate::approvals::APPROVALS)])
        );
        assert_eq!(
            read["hooks"]["Stop"][0]["hooks"][0]["command"],
            "rotproof stop-hook"
        );
    }

    fn input(message: &str, active: bool) -> String {
        json!({
            "hook_event_name": "Stop",
            "stop_hook_active": active,
            "last_assistant_message": message,
        })
        .to_string()
    }

    #[test]
    fn a_phrase_found_is_named_in_what_sends_the_agent_back() {
        let found = open_in(&input("Done. The Windows path is not checked.", false)).unwrap();
        assert_eq!(found, ["not checked"]);
        let out: Value = serde_json::from_str(&send_back(&found)).unwrap();
        let context = out["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap();
        assert!(context.contains("\"not checked\""), "{context}");
        assert!(context.contains("nothing in docs/ changed"), "{context}");
        assert_eq!(out["hookSpecificOutput"]["hookEventName"], "Stop");
    }

    #[test]
    fn a_todo_left_in_a_report_is_found() {
        assert_eq!(open_phrases("Done. TODO: the Windows path"), ["todo"]);
    }

    #[test]
    fn every_phrase_is_found_in_any_case() {
        for phrase in PHRASES {
            let upper = phrase.to_uppercase();
            assert_eq!(open_phrases(&format!("x {upper} y")), [phrase], "{phrase}");
        }
    }

    #[test]
    fn a_line_that_points_at_the_records_is_not_read() {
        for line in [
            "未決の問いは spec に書いた",
            "未決の問いはspecに書いた",
            "Backlog: the PATH is not checked",
            "- [x.md](docs/specs/x.md): 未確認",
            "閉じた [y.md](/backlog/y.md) は未着手のまま",
        ] {
            assert_eq!(open_phrases(line), Vec::<&str>::new(), "{line}");
        }
        // Only that line: the next one is read
        assert_eq!(open_phrases("spec に書いた\nLinux は未確認"), ["未確認"]);
        // A word that only contains spec is not a pointer
        for line in ["the specific path is not checked", "inspect: not checked"] {
            assert_eq!(open_phrases(line), ["not checked"], "{line}");
        }
    }

    #[test]
    fn nothing_is_open_without_a_phrase_or_once_already_sent_back() {
        assert_eq!(open_in(&input("All done.", false)), Ok(Vec::new()));
        assert_eq!(open_in(&input("未確認のまま", true)), Ok(Vec::new()));
        let null = json!({"hook_event_name": "Stop", "stop_hook_active": false, "last_assistant_message": null});
        assert_eq!(open_in(&null.to_string()), Ok(Vec::new()));
    }

    #[test]
    fn an_input_from_another_hook_fails() {
        assert!(open_in("not json").is_err());
        assert!(open_in("{}").is_err());
        let other = json!({"hook_event_name": "PreToolUse", "last_assistant_message": "未確認"});
        assert!(open_in(&other.to_string()).is_err());
        // Gemini CLI's event is no longer answered
        let gemini = json!({"hook_event_name": "AfterAgent", "prompt_response": "未確認"});
        assert!(open_in(&gemini.to_string()).is_err());
        let empty = json!({"hook_event_name": "Stop"});
        assert!(open_in(&empty.to_string()).is_err());
    }
}

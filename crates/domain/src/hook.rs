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
//! - The hook runs the first `rotproof` on the `PATH` the agent runs hooks with. Not there, a second hook beside it
//!   ([`ON_PATH`]) says so; another version there than the project's files are up to, the hook says so itself
//!   ([`other_version`]). Both are shown to the person and keep no agent from stopping.
//!
//! An input from another hook, or a `git` that fails, is an error: the agent shows it, and stops.

use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Value, json};

use crate::layers::DECLARATION;

/// The event of the hook this answers, in `hook_event_name`
const EVENT: &str = "Stop";
/// The field of the input that holds the agent's last message
const MESSAGE: &str = "last_assistant_message";

/// The command of the hook.
pub const COMMAND: &str = "rotproof stop-hook";

/// The command of a second hook, beside it, that says so when `rotproof` is not on the `PATH` the agent runs hooks
/// with. The hook itself then fails with the shell's "command not found", which Claude Code may show only in its debug
/// log, and the agent stops unwatched. This one runs in the shell, so it needs no Rotproof, and answers with a
/// `systemMessage`, which Claude Code shows to the person without keeping the agent from stopping. Its text has no `'`,
/// as it is echoed between single quotes.
pub const ON_PATH: &str = "command -v rotproof >/dev/null 2>&1 || echo '{\"systemMessage\": \"rotproof is not on \
     the PATH the agent runs hooks with, so the stop hook of Rotproof did not run. Start the agent with the Rotproof \
     the project pins on the PATH: for a Python project, with its venv active.\"}'";

/// The settings Claude Code runs the project with, and where `rotproof create` writes them, once: the project's file
/// from then on. They run the hook, for which `rotproof` has to be on the `PATH` the agent runs hooks with, with
/// [`ON_PATH`] beside it, and the time hook (`time_hook.rs`) on each of its events, and deny editing the approvals file
/// (`approvals.rs`): Claude Code then refuses its edit tools and its shell's file commands and redirects on it, with
/// no prompt, so only a person adds an approval, through `rotproof approve`. The text is as the updates that add
/// [`ON_PATH`] and the time hook write it (`upgrade.rs`), so a project upgraded with an untouched file ends with the
/// same settings as a new one.
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
      },
      {
        "hooks": [
          {
            "type": "command",
            "command": "command -v rotproof >/dev/null 2>&1 || echo '{\"systemMessage\": \"rotproof is not on the PATH the agent runs hooks with, so the stop hook of Rotproof did not run. Start the agent with the Rotproof the project pins on the PATH: for a Python project, with its venv active.\"}'"
          }
        ]
      }
    ],
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
"#,
)];

/// What the hook says when the Rotproof that runs it, `running`, is not the one the project's files are up to,
/// `files` (the declaration's `files`, which `rotproof init` sets to the version the project upgraded to). The hook
/// runs the first `rotproof` on the `PATH`, which may be another install than the one the project pins. Shown to the
/// person; it does not keep the agent from stopping.
pub fn other_version(running: &str, files: &str) -> Option<String> {
    (running != files).then(|| {
        format!(
            "rotproof stop-hook ran Rotproof {running}, but this project's files are up to {files} ({DECLARATION}). \
             The hook runs the first rotproof on the PATH the agent runs hooks with: put the Rotproof the project \
             pins first on that PATH (for a Python project, start the agent with its venv active), or run `rotproof \
             init` if the project was not upgraded yet."
        )
    })
}

// A line that points at the records: the word spec, specs, backlog or knowledge, or the words work item, standing
// alone in ASCII (so `spec に`, `docs/specs/x.md`, `Backlog` and `work items` count, and `specific` or `inspect` do
// not), or a path through `work/` (`docs/work/x.md`, but not `network/`). The word work alone does not count: it is in
// too many lines that point at nothing. Matched against the line in lower case
static RECORDS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?:^|[^a-z0-9_])(?:specs?|backlog|knowledge|work[ -]items?)(?:[^a-z0-9_]|$)|(?:^|[^a-z0-9_])work[/\\]",
    )
    .unwrap()
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

/// What the hook prints: the agent sent back for the phrases `send_back` (found while nothing in [`RECORDS_DIR`]
/// changed), and `warning` shown to the person. `None` when there is neither: the agent stops, with nothing printed.
pub fn answer(send_back: &[&str], warning: Option<&str>) -> Option<String> {
    let mut answer = serde_json::Map::new();
    if !send_back.is_empty() {
        let quoted: Vec<String> = send_back
            .iter()
            .map(|phrase| format!("\"{phrase}\""))
            .collect();
        let text = format!(
            "Your last message says {} and nothing in docs/ changed. If it leaves a finding open, record it now: a \
             draft work item in docs/work/ (docs/work/rules.md), or the spec it belongs to. If it is already recorded, or is not \
             a finding, say where or why in one line, then stop.",
            quoted.join(", ")
        );
        answer.insert(
            "hookSpecificOutput".into(),
            json!({ "hookEventName": EVENT, "additionalContext": text }),
        );
    }
    if let Some(warning) = warning {
        answer.insert("systemMessage".into(), json!(warning));
    }
    (!answer.is_empty()).then(|| Value::Object(answer).to_string())
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
        assert_eq!(read["hooks"]["Stop"][0]["hooks"][0]["command"], COMMAND);
        assert_eq!(read["hooks"]["Stop"][1]["hooks"][0]["command"], ON_PATH);
        for event in crate::time_hook::EVENTS {
            assert_eq!(
                read["hooks"][event][0]["hooks"][0]["command"],
                crate::time_hook::COMMAND
            );
        }
    }

    /// The second hook is a shell command whose output, when rotproof is not on the PATH, is the JSON Claude Code
    /// reads: the message is echoed between single quotes, so it must hold none.
    #[test]
    fn the_hook_beside_it_echoes_a_system_message() {
        let (test, echoed) = ON_PATH.split_once(" || echo ").unwrap();
        assert_eq!(test, "command -v rotproof >/dev/null 2>&1");
        let inner = echoed
            .strip_prefix('\'')
            .unwrap()
            .strip_suffix('\'')
            .unwrap();
        assert!(!inner.contains('\''), "{inner}");
        let read: Value = serde_json::from_str(inner).unwrap();
        assert!(
            read["systemMessage"]
                .as_str()
                .unwrap()
                .starts_with("rotproof is not on the PATH")
        );
    }

    #[test]
    fn another_version_is_said_and_the_same_is_not() {
        assert_eq!(other_version("0.4.0", "0.4.0"), None);
        let said = other_version("0.3.0", "0.4.0").unwrap();
        assert!(
            said.starts_with(
                "rotproof stop-hook ran Rotproof 0.3.0, but this project's files are up to 0.4.0"
            ),
            "{said}"
        );
    }

    #[test]
    fn the_answer_holds_what_there_is_and_nothing_without_either() {
        assert_eq!(answer(&[], None), None);
        let warned: Value = serde_json::from_str(&answer(&[], Some("other")).unwrap()).unwrap();
        assert_eq!(warned, json!({ "systemMessage": "other" }));
        let both: Value = serde_json::from_str(&answer(&["todo"], Some("other")).unwrap()).unwrap();
        assert_eq!(both["systemMessage"], "other");
        assert_eq!(both["hookSpecificOutput"]["hookEventName"], "Stop");
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
        let out: Value = serde_json::from_str(&answer(&found, None).unwrap()).unwrap();
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
            "閉じた [z.md](/work/z.md) は未着手のまま",
            "- docs\\work\\z.md: 未確認",
            "Work items: the PATH is not checked",
            "the work-item says it is not checked",
        ] {
            assert_eq!(open_phrases(line), Vec::<&str>::new(), "{line}");
        }
        // Only that line: the next one is read
        assert_eq!(open_phrases("spec に書いた\nLinux は未確認"), ["未確認"]);
        // A word that only contains spec, or work alone, is not a pointer
        for line in [
            "the specific path is not checked",
            "inspect: not checked",
            "the work is not checked",
            "network/ is not checked",
            "workitem is not checked",
        ] {
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

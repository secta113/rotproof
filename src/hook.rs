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

/// The settings that run the hook, and where `rotproof create` writes them, once: the project's file from then on.
/// `rotproof` has to be on the `PATH` the agent runs hooks with.
pub const SETTINGS: [(&str, &str); 1] = [(
    ".claude/settings.json",
    r#"{
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
/// a hook that fires on every message is answered without reading. So is "todo" for now: while the marker check is
/// being built and talked about, it names that check more often than it leaves work open, and the word in a comment
/// in the code fails the marker check anyway.
pub const PHRASES: [&str; 14] = [
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
];

/// The changes a project's version control shows: the port the hook asks whether anything was recorded.
pub trait Changes {
    /// Whether anything in `dir` (from the root) is changed, staged or new.
    fn changed(&self, dir: &str) -> Result<bool, String>;
}

/// What the hook prints for `input` (the JSON the agent writes on stdin). `changes` is the project's, or `None` outside
/// a project. `None` lets the agent stop, with nothing printed.
pub fn run(input: &str, changes: Option<&dyn Changes>) -> Result<Option<String>, String> {
    decide(input, || match changes {
        Some(changes) => changes.changed("docs"),
        // Outside a project there is nothing to record into
        None => Ok(true),
    })
}

/// The decision, with `recorded` saying whether `docs/` changed. It is asked only when a phrase is found.
fn decide(
    input: &str,
    recorded: impl FnOnce() -> Result<bool, String>,
) -> Result<Option<String>, String> {
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
        return Ok(None);
    }
    let message = match input.get(MESSAGE) {
        Some(Value::String(message)) => message.as_str(),
        // A turn that ended without text
        Some(Value::Null) => "",
        _ => return Err(format!("the {event} hook input has no {MESSAGE}")),
    };
    let found = open_phrases(message);
    if found.is_empty() || recorded()? {
        return Ok(None);
    }
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
    Ok(Some(answer.to_string()))
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

    /// Changes in memory: whether each directory changed, and every directory asked about
    struct Fake(bool, std::cell::RefCell<Vec<String>>);

    impl Changes for Fake {
        fn changed(&self, dir: &str) -> Result<bool, String> {
            self.1.borrow_mut().push(dir.to_string());
            Ok(self.0)
        }
    }

    #[test]
    fn the_project_is_asked_about_docs_and_outside_one_nothing_is_asked() {
        let unchanged = Fake(false, Default::default());
        let said = run(&input("未確認のまま", false), Some(&unchanged)).unwrap();
        assert!(said.is_some_and(|out| out.contains("nothing in docs/ changed")));
        assert_eq!(*unchanged.1.borrow(), ["docs"]);
        let changed = Fake(true, Default::default());
        assert_eq!(run(&input("未確認のまま", false), Some(&changed)), Ok(None));
        assert_eq!(run(&input("未確認のまま", false), None), Ok(None));
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
    fn a_phrase_with_no_change_in_docs_sends_the_agent_back() {
        let out = decide(
            &input("Done. The Windows path is not checked.", false),
            || Ok(false),
        )
        .unwrap()
        .expect("sent back");
        let out: Value = serde_json::from_str(&out).unwrap();
        let context = out["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap();
        assert!(context.contains("\"not checked\""), "{context}");
        assert_eq!(out["hookSpecificOutput"]["hookEventName"], "Stop");
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
    fn the_agent_stops_when_docs_changed_or_no_phrase_or_already_sent_back() {
        assert_eq!(decide(&input("未確認のまま", false), || Ok(true)), Ok(None));
        assert_eq!(
            decide(&input("All done.", false), || panic!("not asked")),
            Ok(None)
        );
        assert_eq!(
            decide(&input("未確認のまま", true), || panic!("not asked")),
            Ok(None)
        );
        let null = json!({"hook_event_name": "Stop", "stop_hook_active": false, "last_assistant_message": null});
        assert_eq!(decide(&null.to_string(), || panic!("not asked")), Ok(None));
    }

    #[test]
    fn an_input_from_another_hook_fails() {
        assert!(decide("not json", || Ok(false)).is_err());
        assert!(decide("{}", || Ok(false)).is_err());
        let other = json!({"hook_event_name": "PreToolUse", "last_assistant_message": "未確認"});
        assert!(decide(&other.to_string(), || Ok(false)).is_err());
        // Gemini CLI's event is no longer answered
        let gemini = json!({"hook_event_name": "AfterAgent", "prompt_response": "未確認"});
        assert!(decide(&gemini.to_string(), || Ok(false)).is_err());
        let empty = json!({"hook_event_name": "Stop"});
        assert!(decide(&empty.to_string(), || Ok(false)).is_err());
    }

    #[test]
    fn a_failing_git_is_an_error() {
        assert!(decide(&input("未決", false), || Err("git".into())).is_err());
    }
}

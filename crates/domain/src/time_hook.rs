//! `rotproof time-hook`: the hook Claude Code runs when a session starts and when the person sends a prompt (its
//! `SessionStart` and `UserPromptSubmit` hooks). It tells the agent the time now.
//!
//! - An agent has no clock of its own. It writes a day or a time into the records (`filed`, `verified`, the log's date
//!   headings) from what it was last told, and a guess runs ahead of the clock. `rotproof check` fails on one that has
//!   not come; this hook gives the agent the time before it writes one.
//! - It answers with `additionalContext`, which Claude Code adds to what the agent reads, read again on every prompt
//!   and at the start of every session, a resumed or a compacted one too. Within a long turn the time goes on, so the
//!   answer says to read the clock for a later one.
//!
//! An input from another hook is an error: Claude Code shows it, and the prompt goes on.

use serde_json::{Value, json};

use crate::schema::Time;

/// The command of the hook.
pub const COMMAND: &str = "rotproof time-hook";

/// The events Claude Code runs the hook on, in `hook_event_name`.
pub const EVENTS: [&str; 2] = ["SessionStart", "UserPromptSubmit"];

/// What the hook prints for `input` (the JSON the agent writes on stdin) at `now`.
pub fn answer(input: &str, now: Time) -> Result<String, String> {
    let input: Value =
        serde_json::from_str(input).map_err(|e| format!("the hook input is not JSON: {e}"))?;
    let event = input
        .get("hook_event_name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !EVENTS.contains(&event) {
        return Err(format!(
            "the hook input is from {event:?}: run `{COMMAND}` as Claude Code's {} hook",
            EVENTS.join(" or ")
        ));
    }
    let text = format!(
        "The time now is {} ({COMMAND}). Write a day or a time into docs/ (filed, verified, the log's date headings) \
         from this, or from the clock for a later one, never from a guess: rotproof check fails on one that has not \
         come.",
        now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    );
    Ok(
        json!({ "hookSpecificOutput": { "hookEventName": event, "additionalContext": text } })
            .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use chrono::{FixedOffset, TimeZone};

    use super::*;

    fn now() -> Time {
        FixedOffset::east_opt(9 * 3600)
            .unwrap()
            .with_ymd_and_hms(2026, 10, 28, 9, 41, 7)
            .unwrap()
    }

    #[test]
    fn the_time_is_given_to_the_event_that_asked() {
        for event in EVENTS {
            let input = json!({ "hook_event_name": event, "prompt": "go on" }).to_string();
            let out: Value = serde_json::from_str(&answer(&input, now()).unwrap()).unwrap();
            assert_eq!(out["hookSpecificOutput"]["hookEventName"], event);
            let context = out["hookSpecificOutput"]["additionalContext"]
                .as_str()
                .unwrap();
            assert!(
                context
                    .starts_with("The time now is 2026-10-28T09:41:07+09:00 (rotproof time-hook)."),
                "{context}"
            );
        }
    }

    #[test]
    fn an_input_from_another_hook_fails() {
        assert!(answer("not json", now()).is_err());
        assert!(answer("{}", now()).is_err());
        let stop = json!({ "hook_event_name": "Stop" }).to_string();
        let why = answer(&stop, now()).unwrap_err();
        assert!(why.contains("SessionStart or UserPromptSubmit"), "{why}");
    }
}

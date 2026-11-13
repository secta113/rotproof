//! The settings Claude Code runs a project with, as `rotproof create` writes them, and the two hooks Rotproof wrote
//! into them before.
//!
//! - **The stop hook, before 1.0.** `rotproof stop-hook` read the agent's last message when it stopped and sent it
//!   back once when the message held a phrase that leaves something open ("not checked") while nothing in `docs/`
//!   changed. It caught only what the agent had already said, which the person reads in that message, and fired on
//!   every phrase quoted as a topic; it went with the records of work it asked the agent to write.
//! - **The time hook, until 1.0.0-rc.3.** `rotproof time-hook` told the agent the time on `SessionStart` and
//!   `UserPromptSubmit`. It was written when `rotproof check` failed on a time that had not come and an agent wrote a
//!   guess a few minutes ahead of the clock. Records hold a day now, and Claude Code gives the agent the day itself, so
//!   the hook has nothing left to tell.
//!
//! Rotproof no longer writes either, and `rotproof init` takes them out of a project's settings (`upgrade.rs`). The
//! commands stay, hidden, to say so to a project whose settings still run them.

/// The command of the stop hook Rotproof wrote before 1.0.
pub const COMMAND: &str = "rotproof stop-hook";

/// The command of the hook Rotproof wrote beside the stop hook in 0.4.0, which said so when `rotproof` was not on the
/// `PATH` the agent runs hooks with.
pub const ON_PATH: &str = "command -v rotproof >/dev/null 2>&1 || echo '{\"systemMessage\": \"rotproof is not on \
     the PATH the agent runs hooks with, so the stop hook of Rotproof did not run. Start the agent with the Rotproof \
     the project pins on the PATH: for a Python project, with its venv active.\"}'";

/// The command of the time hook Rotproof wrote from 0.4.0 until 1.0.0-rc.3.
pub const TIME_COMMAND: &str = "rotproof time-hook";

/// The events Claude Code ran the time hook on, in `hook_event_name`.
pub const TIME_EVENTS: [&str; 2] = ["SessionStart", "UserPromptSubmit"];

/// The settings Claude Code runs the project with, and where `rotproof create` writes them, once: the project's file
/// from then on. They deny editing the approvals file (`approvals.rs`): Claude Code then refuses its edit tools and
/// its shell's file commands and redirects on it, with no prompt, so only a person adds an approval, through
/// `rotproof approve`. The text is as the updates write it (`upgrade.rs`), so a project upgraded with an untouched
/// file ends with the same settings as a new one.
pub const SETTINGS: [(&str, &str); 1] = [(
    ".claude/settings.json",
    r#"{
  "permissions": {
    "deny": [
      "Edit(/.config/rotproof-approved.toml)"
    ]
  }
}
"#,
)];

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    #[test]
    fn the_settings_are_json_and_deny_editing_the_approvals_file_and_run_no_hook() {
        let (_, text) = SETTINGS[0];
        let read: Value = serde_json::from_str(text).unwrap();
        // A rule's leading `/` is the directory that holds `.claude/`: the project's root, where the file is
        assert_eq!(
            read["permissions"]["deny"],
            json!([format!("Edit(/{})", crate::approvals::APPROVALS)])
        );
        assert!(read.get("hooks").is_none());
    }
}

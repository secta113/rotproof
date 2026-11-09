//! The settings Claude Code runs a project with, as `rotproof create` writes them, and the stop hook Rotproof wrote
//! into them before 1.0.
//!
//! Before 1.0, `rotproof stop-hook` read the agent's last message when it stopped and sent it back once when the
//! message held a phrase that leaves something open ("not checked") while nothing in `docs/` changed. It caught only
//! what the agent had already said, which the person reads in that message, and fired on every phrase quoted as a
//! topic; it went with the records of work it asked the agent to write. Rotproof no longer writes it, and
//! `rotproof init` takes it out of a project's settings (`upgrade.rs`). The command stays, hidden, to say so to a
//! project whose settings still run it.

/// The command of the stop hook Rotproof wrote before 1.0.
pub const COMMAND: &str = "rotproof stop-hook";

/// The command of the hook Rotproof wrote beside the stop hook in 0.4.0, which said so when `rotproof` was not on the
/// `PATH` the agent runs hooks with.
pub const ON_PATH: &str = "command -v rotproof >/dev/null 2>&1 || echo '{\"systemMessage\": \"rotproof is not on \
     the PATH the agent runs hooks with, so the stop hook of Rotproof did not run. Start the agent with the Rotproof \
     the project pins on the PATH: for a Python project, with its venv active.\"}'";

/// The settings Claude Code runs the project with, and where `rotproof create` writes them, once: the project's file
/// from then on. They run the time hook (`time_hook.rs`) on each of its events, for which `rotproof` has to be on the
/// `PATH` the agent runs hooks with, and deny editing the approvals file (`approvals.rs`): Claude Code then refuses
/// its edit tools and its shell's file commands and redirects on it, with no prompt, so only a person adds an
/// approval, through `rotproof approve`. The text is as the updates write it (`upgrade.rs`), so a project upgraded with
/// an untouched file ends with the same settings as a new one.
pub const SETTINGS: [(&str, &str); 1] = [(
    ".claude/settings.json",
    r#"{
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
"#,
)];

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

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
        assert!(read["hooks"].get("Stop").is_none());
        for event in crate::time_hook::EVENTS {
            assert_eq!(
                read["hooks"][event][0]["hooks"][0]["command"],
                crate::time_hook::COMMAND
            );
        }
    }
}

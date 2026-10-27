//! `rotproof stop-hook`: the agent sent back when its last message leaves something open and nothing was recorded.

use domain::hook::{RECORDS_DIR, answer, open_in, other_version};
use domain::upgrade::VERSION;
use protocol::outbound::Changes;

/// What the hook prints for `input` (the JSON the agent writes on stdin). `changes` is the project's, or `None` outside
/// a project; `files` is the version the project's files are up to, when its declaration says it. `None` lets the
/// agent stop, with nothing printed.
pub fn run(
    input: &str,
    changes: Option<&dyn Changes>,
    files: Option<&str>,
) -> Result<Option<String>, String> {
    let found = open_in(input)?;
    let warning = files.and_then(|files| other_version(VERSION, files));
    let recorded = match changes {
        // git runs only when a phrase was found
        Some(changes) if !found.is_empty() => changes.changed(RECORDS_DIR)?,
        // Nothing to record, or outside a project nothing to record into
        _ => true,
    };
    let send_back = if recorded { &[][..] } else { &found[..] };
    Ok(answer(send_back, warning.as_deref()))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use serde_json::json;

    use super::*;

    /// Changes in memory: whether each directory changed, or that asking fails; and every directory asked about
    struct Fake(Result<bool, String>, RefCell<Vec<String>>);

    impl Fake {
        fn new(changed: Result<bool, String>) -> Self {
            Fake(changed, RefCell::default())
        }
    }

    impl Changes for Fake {
        fn changed(&self, dir: &str) -> Result<bool, String> {
            self.1.borrow_mut().push(dir.to_string());
            self.0.clone()
        }
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
    fn the_project_is_asked_about_docs_and_outside_one_nothing_is_asked() {
        let unchanged = Fake::new(Ok(false));
        let said = run(&input("未確認のまま", false), Some(&unchanged), None).unwrap();
        assert!(said.is_some_and(|out| out.contains("nothing in docs/ changed")));
        assert_eq!(*unchanged.1.borrow(), ["docs"]);
        let changed = Fake::new(Ok(true));
        assert_eq!(
            run(&input("未確認のまま", false), Some(&changed), None),
            Ok(None)
        );
        assert_eq!(run(&input("未確認のまま", false), None, None), Ok(None));
    }

    #[test]
    fn the_project_is_asked_only_when_a_phrase_is_found() {
        for message in [input("All done.", false), input("未確認のまま", true)] {
            let asked = Fake::new(Ok(false));
            assert_eq!(run(&message, Some(&asked), None), Ok(None));
            assert!(asked.1.borrow().is_empty(), "{message}");
        }
        let asked = Fake::new(Ok(false));
        assert!(run("not json", Some(&asked), None).is_err());
        assert!(asked.1.borrow().is_empty());
    }

    /// Another version than the project's files are up to is said to the person on every stop, with or without a
    /// phrase, and asks git nothing more.
    #[test]
    fn another_version_is_said_beside_what_sends_the_agent_back() {
        let asked = Fake::new(Ok(false));
        let said = run(&input("All done.", false), Some(&asked), Some("0.0.1")).unwrap();
        let said: serde_json::Value = serde_json::from_str(&said.unwrap()).unwrap();
        assert!(
            said["systemMessage"]
                .as_str()
                .unwrap()
                .contains("this project's files are up to 0.0.1")
        );
        assert!(said.get("hookSpecificOutput").is_none());
        assert!(asked.1.borrow().is_empty());
        let both = run(&input("未確認のまま", false), Some(&asked), Some("0.0.1"))
            .unwrap()
            .unwrap();
        assert!(both.contains("systemMessage") && both.contains("nothing in docs/ changed"));
        // The running version says nothing
        assert_eq!(
            run(&input("All done.", false), Some(&asked), Some(VERSION)),
            Ok(None)
        );
    }

    #[test]
    fn a_failing_git_is_an_error() {
        let failing = Fake::new(Err("git".into()));
        assert!(run(&input("未決", false), Some(&failing), None).is_err());
    }
}

//! `rotproof stop-hook`: the agent sent back when its last message leaves something open and nothing was recorded.

use domain::hook::{Changes, RECORDS_DIR, open_in, send_back};

/// What the hook prints for `input` (the JSON the agent writes on stdin). `changes` is the project's, or `None` outside
/// a project. `None` lets the agent stop, with nothing printed.
pub fn run(input: &str, changes: Option<&dyn Changes>) -> Result<Option<String>, String> {
    let found = open_in(input)?;
    if found.is_empty() {
        return Ok(None);
    }
    let recorded = match changes {
        Some(changes) => changes.changed(RECORDS_DIR)?,
        // Outside a project there is nothing to record into
        None => true,
    };
    Ok((!recorded).then(|| send_back(&found)))
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
        let said = run(&input("未確認のまま", false), Some(&unchanged)).unwrap();
        assert!(said.is_some_and(|out| out.contains("nothing in docs/ changed")));
        assert_eq!(*unchanged.1.borrow(), ["docs"]);
        let changed = Fake::new(Ok(true));
        assert_eq!(run(&input("未確認のまま", false), Some(&changed)), Ok(None));
        assert_eq!(run(&input("未確認のまま", false), None), Ok(None));
    }

    #[test]
    fn the_project_is_asked_only_when_a_phrase_is_found() {
        for message in [input("All done.", false), input("未確認のまま", true)] {
            let asked = Fake::new(Ok(false));
            assert_eq!(run(&message, Some(&asked)), Ok(None));
            assert!(asked.1.borrow().is_empty(), "{message}");
        }
        let asked = Fake::new(Ok(false));
        assert!(run("not json", Some(&asked)).is_err());
        assert!(asked.1.borrow().is_empty());
    }

    #[test]
    fn a_failing_git_is_an_error() {
        let failing = Fake::new(Err("git".into()));
        assert!(run(&input("未決", false), Some(&failing)).is_err());
    }
}

//! One update of `domain::upgrade` applied to its file: the file read as the project wrote it, the text the update
//! gives written back in the same line endings and with the same byte order mark, or what the update leaves to a
//! person.

use crate::tree::{exactly, read_written};
use domain::tree::Written;
use domain::upgrade::Update;
use protocol::inbound::Upgraded;
use protocol::outbound::{Tree, Writer};

/// Apply one update to its file, or return what it leaves to a person.
pub(super) fn apply(
    tree: &dyn Tree,
    out: &dyn Writer,
    update: Update,
    upgraded: &mut Upgraded,
) -> Result<Option<String>, String> {
    let path = update.file();
    // Read with how it was written, so the text written back keeps the file's line endings and byte order mark
    let (text, how) = match exactly(tree, path) {
        Ok((found, false)) => {
            let (text, how) = read_written(tree, &found).map_err(|e| format!("{path}: {e}"))?;
            (Some(text), how)
        }
        _ => (None, Written::default()),
    };
    match update.apply(text.as_deref()) {
        domain::upgrade::Outcome::Done => {}
        domain::upgrade::Outcome::Changed(written) => {
            out.write(path, &how.restore(&written))
                .map_err(|e| format!("{path}: {e}"))?;
            upgraded.applied.push((update.name(), path));
        }
        domain::upgrade::Outcome::ByHand(why) => return Ok(Some(why)),
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::init::init;
    use crate::init::tests::{SETTINGS, made_by_first};
    use crate::tree::fake::Fake;
    use domain::declaration::{DECLARATION, fields as read_fields};
    use protocol::inbound::Initialized;

    /// The settings `rotproof create` wrote from 1.0.0-rc.4 to 2.0.0-rc.1: the rule that denied editing the approvals
    /// file, and nothing else.
    const DENY_ONLY: &str = "{\n  \"permissions\": {\n    \"deny\": [\n      \"Edit(/.config/rotproof-approved.toml)\"\n    ]\n  }\n}\n";

    /// A project made by 2.0.0-rc.1, whose files are up to it, with `settings` as its `.claude/settings.json`.
    fn made_by_rc_1(settings: &str) -> Fake {
        let tree = Fake::default();
        tree.write(DECLARATION, "stack = \"none\"\nfiles = \"2.0.0-rc.1\"\n")
            .unwrap();
        tree.write(SETTINGS, settings).unwrap();
        tree
    }

    #[test]
    fn a_project_with_no_settings_has_nothing_to_take_out_and_is_given_none() {
        let tree = made_by_first();
        tree.remove(SETTINGS).unwrap();
        let Ok(Initialized::Upgraded(upgraded)) = init(&tree, &tree, None, None, "x", "2.0.0-rc.2")
        else {
            panic!("upgraded");
        };
        // The updates ask no person for a file that is not there, and the refresh writes none: `rotproof create` has no
        // settings of its own to write now
        assert!(upgraded.by_hand.is_empty(), "{:?}", upgraded.by_hand);
        assert!(upgraded.applied.is_empty(), "{:?}", upgraded.applied);
        assert!(upgraded.files_set);
        assert_eq!(tree.text(SETTINGS), None);
        assert!(!upgraded.made.written.iter().any(|p| p == SETTINGS));
    }

    #[test]
    fn the_deny_rule_is_taken_out_on_the_way_to_rc_2_and_a_project_that_declined_keeps_it() {
        let tree = made_by_rc_1(DENY_ONLY);
        let Ok(Initialized::Upgraded(upgraded)) = init(&tree, &tree, None, None, "x", "2.0.0-rc.2")
        else {
            panic!("upgraded");
        };
        assert_eq!(
            upgraded.applied,
            [("claude-remove-deny-approvals", SETTINGS)]
        );
        assert!(upgraded.by_hand.is_empty(), "{:?}", upgraded.by_hand);
        assert!(upgraded.files_set);
        assert_eq!(tree.text(SETTINGS).as_deref(), Some("{}\n"));
        // Said once: the next run finds nothing to take out
        let Ok(Initialized::Upgraded(again)) = init(&tree, &tree, None, None, "x", "2.0.0-rc.2")
        else {
            panic!("upgraded");
        };
        assert!(again.applied.is_empty(), "{:?}", again.applied);
        // A project that declined it keeps the rule
        let tree = made_by_rc_1(DENY_ONLY);
        let text = tree.text(DECLARATION).unwrap();
        tree.write(
            DECLARATION,
            &format!("{text}declined = [\"claude-remove-deny-approvals\"]\n"),
        )
        .unwrap();
        let Ok(Initialized::Upgraded(kept)) = init(&tree, &tree, None, None, "x", "2.0.0-rc.2")
        else {
            panic!("upgraded");
        };
        assert!(kept.applied.is_empty() && kept.files_set);
        assert_eq!(tree.text(SETTINGS).as_deref(), Some(DENY_ONLY));
        // What the project denies itself and allows stays, in its order
        let theirs = "{\n  \"permissions\": {\n    \"allow\": [\"Bash(ls)\"],\n    \"deny\": [\n      \"Read(x)\",\n      \"Edit(/.config/rotproof-approved.toml)\"\n    ]\n  }\n}\n";
        let tree = made_by_rc_1(theirs);
        let Ok(Initialized::Upgraded(_)) = init(&tree, &tree, None, None, "x", "2.0.0-rc.2") else {
            panic!("upgraded");
        };
        assert_eq!(
            tree.text(SETTINGS).as_deref(),
            Some(
                "{\n  \"permissions\": {\n    \"allow\": [\n      \"Bash(ls)\"\n    ],\n    \"deny\": [\n      \"Read(x)\"\n    ]\n  }\n}\n"
            )
        );
    }

    /// An update writes its file back as the project wrote it, as `files` is set: the settings came back all LF and
    /// without the byte order mark.
    #[test]
    fn an_update_keeps_the_line_endings_and_the_byte_order_mark_of_its_file() {
        for (crlf, bom) in [(true, true), (true, false), (false, true), (false, false)] {
            let text = if crlf {
                DENY_ONLY.replace('\n', "\r\n")
            } else {
                DENY_ONLY.to_string()
            };
            let raw = if bom { format!("\u{feff}{text}") } else { text };
            let tree = made_by_rc_1(&raw);
            let Ok(Initialized::Upgraded(upgraded)) =
                init(&tree, &tree, None, None, "x", "2.0.0-rc.2")
            else {
                panic!("upgraded");
            };
            assert_eq!(
                upgraded.applied,
                [("claude-remove-deny-approvals", SETTINGS)]
            );
            let now = tree.text(SETTINGS).unwrap();
            let said = format!("crlf {crlf}, bom {bom}: {now:?}");
            assert_eq!(now.starts_with('\u{feff}'), bom, "{said}");
            if crlf {
                assert!(!now.replace("\r\n", "").contains('\n'), "{said}");
            } else {
                assert!(!now.contains('\r'), "{said}");
            }
            assert!(now.contains("{}"), "{said}");
            assert!(!now.contains("Edit("), "{said}");
        }
    }

    /// The settings `rotproof create` wrote from 0.4.0 to 1.0.0-rc.3, which run the time hook on both events.
    const SETTINGS_RC3: &str = r#"{
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
"#;

    /// Upgraded to 1.0.0-rc.4, the settings of rc.3 lose the time hook and end as a new project's, in their line
    /// endings, and the update that took them out is named.
    #[test]
    fn the_time_hook_is_taken_out_on_the_way_to_rc_4() {
        let tree = Fake::default();
        tree.write(DECLARATION, "stack = \"none\"\nfiles = \"1.0.0-rc.3\"\n")
            .unwrap();
        tree.write(SETTINGS, &SETTINGS_RC3.replace('\n', "\r\n"))
            .unwrap();
        let Ok(Initialized::Upgraded(upgraded)) = init(&tree, &tree, None, None, "x", "1.0.0-rc.4")
        else {
            panic!("upgraded");
        };
        assert_eq!(
            upgraded.applied,
            [("claude-remove-time-hook", SETTINGS)],
            "{:?}",
            upgraded.applied
        );
        assert!(upgraded.by_hand.is_empty(), "{:?}", upgraded.by_hand);
        assert_eq!(tree.text(SETTINGS), Some(DENY_ONLY.replace('\n', "\r\n")));
        // Said once: the next run finds nothing to take out
        let Ok(Initialized::Upgraded(again)) = init(&tree, &tree, None, None, "x", "1.0.0-rc.4")
        else {
            panic!("upgraded");
        };
        assert!(again.applied.is_empty(), "{:?}", again.applied);
        // A project that declined it keeps the hook
        let tree = Fake::default();
        tree.write(
            DECLARATION,
            "stack = \"none\"\nfiles = \"1.0.0-rc.3\"\ndeclined = [\"claude-remove-time-hook\"]\n",
        )
        .unwrap();
        tree.write(SETTINGS, SETTINGS_RC3).unwrap();
        let Ok(Initialized::Upgraded(kept)) = init(&tree, &tree, None, None, "x", "1.0.0-rc.4")
        else {
            panic!("upgraded");
        };
        assert!(kept.applied.is_empty(), "{:?}", kept.applied);
        assert_eq!(tree.text(SETTINGS).as_deref(), Some(SETTINGS_RC3));
    }

    /// From 0.3.0 the time hook is not added to be taken out in the same run: the settings end as a new project's.
    #[test]
    fn the_time_hook_is_not_added_on_the_way_from_0_3_to_rc_4() {
        let tree = made_by_first();
        let old = "{\n  \"permissions\": {\n    \"deny\": [\n      \"Edit(/.config/rotproof-approved.toml)\"\n    ]\n  \
                   },\n  \"hooks\": {\n    \"Stop\": [\n      {\n        \"hooks\": [\n          {\n            \
                   \"type\": \"command\",\n            \"command\": \"rotproof stop-hook\"\n          }\n        ]\n      \
                   }\n    ]\n  }\n}\n";
        tree.write(SETTINGS, old).unwrap();
        let Ok(Initialized::Upgraded(upgraded)) = init(&tree, &tree, None, None, "x", "1.0.0-rc.4")
        else {
            panic!("upgraded");
        };
        // The time hook is neither added nor taken out: the stop hook alone is there to go
        assert_eq!(
            upgraded.applied,
            [("claude-remove-stop-hook", SETTINGS)],
            "{:?}",
            upgraded.applied
        );
        assert_eq!(tree.text(SETTINGS).as_deref(), Some(DENY_ONLY));
    }

    /// Upgraded to 1.0, the settings 0.4.0 wrote lose the stop hook and end as a new project's, in their line endings.
    #[test]
    fn the_stop_hook_is_taken_out_on_the_way_to_1_0() {
        let tree = made_by_first();
        let new = SETTINGS_RC3;
        let stop = "\"hooks\": {\n    \"Stop\": [\n      {\n        \"hooks\": [\n          {\n            \
                    \"type\": \"command\",\n            \"command\": \"rotproof stop-hook\"\n          }\n        ]\n      \
                    }\n    ],\n";
        let old = new.replacen("\"hooks\": {\n", stop, 1);
        assert_ne!(old, new);
        tree.write(SETTINGS, &old.replace('\n', "\r\n")).unwrap();
        let Ok(Initialized::Upgraded(upgraded)) = init(&tree, &tree, None, None, "x", "1.0.0-rc.1")
        else {
            panic!("upgraded");
        };
        assert!(
            upgraded
                .applied
                .contains(&("claude-remove-stop-hook", SETTINGS)),
            "{:?}",
            upgraded.applied
        );
        assert!(upgraded.by_hand.is_empty(), "{:?}", upgraded.by_hand);
        assert_eq!(tree.text(SETTINGS), Some(new.replace('\n', "\r\n")));
    }

    #[test]
    fn an_update_that_needs_a_person_leaves_the_files_behind_until_it_is_declined() {
        let tree = made_by_rc_1("not json");
        let Ok(Initialized::Upgraded(upgraded)) = init(&tree, &tree, None, None, "x", "2.0.0-rc.2")
        else {
            panic!("upgraded");
        };
        assert_eq!(upgraded.by_hand.len(), 1, "{:?}", upgraded.by_hand);
        assert!(!upgraded.files_set);
        assert_eq!(tree.text(SETTINGS).as_deref(), Some("not json"));
        assert_eq!(
            read_fields(&tree.text(DECLARATION).unwrap())
                .unwrap()
                .files
                .as_deref(),
            Some("2.0.0-rc.1")
        );

        // Declined, it is neither applied nor asked for again, and the files reach the running version
        let text = tree.text(DECLARATION).unwrap();
        tree.write(
            DECLARATION,
            &format!("{text}declined = [\"claude-remove-deny-approvals\"]\n"),
        )
        .unwrap();
        let Ok(Initialized::Upgraded(upgraded)) = init(&tree, &tree, None, None, "x", "2.0.0-rc.2")
        else {
            panic!("upgraded");
        };
        assert!(upgraded.by_hand.is_empty() && upgraded.applied.is_empty());
        assert!(upgraded.files_set);
        assert_eq!(tree.text(SETTINGS).as_deref(), Some("not json"));
    }
}

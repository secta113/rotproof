//! `rotproof init`: write a project's declaration, `.config/rotproof.toml`, or, when it exists, bring the project's
//! files up to the running version of Rotproof.
//!
//! - **Without a declaration,** it writes only the declaration, so the project declares the layers it does not have
//!   before `rotproof create` makes anything. The declaration is the project's from then on.
//! - **With one,** it is the whole upgrade (`upgrade.rs` in `domain`): it applies every update of the project's files
//!   between the declaration's `files` and the running version that the project did not decline, does what
//!   `rotproof create` does after an upgrade without making a layer, and sets `files` to the running version. What an
//!   update cannot do without a person it says, and then `files` stays where it was, so `rotproof check` keeps
//!   failing until it is done or declined. An update whose file is missing asks no person when the refresh then
//!   writes that file with what the update adds, as it writes `.claude/settings.json`. It adds `declined`, empty,
//!   when the declaration lacks it, names every field it adds, and never changes the declaration's other values.
//!   It never moves or rewrites a record: from 1.0, Rotproof reads no records of work, wherever they are.

use crate::create::refresh;
use crate::tree::{exactly, read_text, read_written};
use domain::layers::{
    DECLARATION, ONION, PORTS_MOVED, crosses_onion, declaration_text, known_stacks, layout, retired,
};
use domain::tree::Written;
use domain::upgrade::{FIRST, Update, fields, pending, with_files};
use protocol::inbound::{Initialized, Upgraded};
use protocol::outbound::{Tree, Writer};

/// Write the declaration for `stack` into `tree` through `out`, or upgrade the project there to `running`. `name` is
/// the project's, for the files it writes. `Err` when no declaration exists and the stack is unknown or not named, the
/// stack named differs from the declared one, or a file cannot be read or written.
pub fn init(
    tree: &dyn Tree,
    out: &dyn Writer,
    stack: Option<&str>,
    name: &str,
    running: &str,
) -> Result<Initialized, String> {
    if tree.found(DECLARATION).is_some() {
        return upgrade(tree, out, stack, name, running).map(Initialized::Upgraded);
    }
    let Some(stack) = stack else {
        return Err(format!(
            "{DECLARATION} does not exist: name the stack with --stack ({})",
            known_stacks().join(", ")
        ));
    };
    if !known_stacks().contains(&stack) {
        return Err(format!(
            "unknown stack {stack:?} (known: {})",
            known_stacks().join(", ")
        ));
    }
    out.write(DECLARATION, &declaration_text(stack))
        .map_err(|e| format!("{DECLARATION}: {e}"))?;
    Ok(Initialized::Declared(DECLARATION))
}

/// Bring the project's files up to `running`.
fn upgrade(
    tree: &dyn Tree,
    out: &dyn Writer,
    stack: Option<&str>,
    name: &str,
    running: &str,
) -> Result<Upgraded, String> {
    let (path, _) = exactly(tree, DECLARATION)?;
    let text = read_text(tree, &path).map_err(|e| format!("{DECLARATION}: {e}"))?;
    let declared = fields(&text)?;
    if let (Some(named), Some(declared)) = (stack, &declared.stack)
        && named != declared
    {
        return Err(format!(
            "{DECLARATION} declares stack = \"{declared}\", and rotproof init never changes a stack: run it without \
             --stack to upgrade the project's files"
        ));
    }
    let from = declared.files.clone().unwrap_or_else(|| FIRST.to_string());
    let mut upgraded = Upgraded {
        from: from.clone(),
        ..Upgraded::default()
    };
    let mut asked = Vec::new();
    for update in pending(&from, running, &declared.declined)? {
        if let Some(why) = apply(tree, out, update, &mut upgraded)? {
            asked.push((update, why));
        }
    }
    if crosses_onion(&from, running) {
        upgraded.moves = onion_moves(tree, declared.stack.as_deref());
    }
    upgraded.made = refresh(tree, out, name)?;
    // An update that asked a person for a file the refresh then wrote with what the update adds asks no more
    for (update, why) in asked {
        if !written_done(tree, update, &upgraded.made.written)? {
            upgraded.by_hand.push(why);
        }
    }
    if upgraded.by_hand.is_empty() && declared.files.as_deref() != Some(running) {
        // Read again: the refresh may have added a field the declaration lacked
        let (text, was) = read_written(tree, &path).map_err(|e| format!("{DECLARATION}: {e}"))?;
        let (written, added) = with_files(&text, running)?;
        let written = was.restore(&written);
        out.write(DECLARATION, &written)
            .map_err(|e| format!("{DECLARATION}: {e}"))?;
        upgraded.files_set = true;
        // Said where the refresh's fields are said, in the same words
        upgraded.made.added.extend(added);
    }
    Ok(upgraded)
}

/// How to move the code of a project of `stack` onto the onion table: each retired layer that is there, with where its
/// code goes, and what moved inside the layers that stay. Nothing for a repository of records only.
fn onion_moves(tree: &dyn Tree, stack: Option<&str>) -> Vec<String> {
    let Some(layout) = stack.and_then(layout) else {
        return Vec::new();
    };
    let mut moves: Vec<String> = retired(&layout)
        .into_iter()
        .filter(|(_, path, _)| tree.found(path) == Some(true))
        .map(|(name, path, goes)| {
            format!(
                "{path}/ holds the layer {name} of the table before {ONION}, which is gone: {goes}"
            )
        })
        .collect();
    moves.push(format!("in the layers that stay, {PORTS_MOVED}"));
    moves
}

/// Apply one update to its file, or return what it leaves to a person.
fn apply(
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

/// Whether the refresh wrote the file of an update that asked a person, as it writes `.claude/settings.json` when
/// there is none, and the file it wrote has what the update adds. The update is asked again of the written file
/// rather than trusted by the path alone, so a file the refresh writes without what the update adds still needs a
/// person.
fn written_done(tree: &dyn Tree, update: Update, written: &[String]) -> Result<bool, String> {
    let path = update.file();
    if !written.iter().any(|w| w == path) {
        return Ok(false);
    }
    let text = read_text(tree, path).map_err(|e| format!("{path}: {e}"))?;
    Ok(update.apply(Some(&text)) == domain::upgrade::Outcome::Done)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::fake::Fake;
    use domain::upgrade::fields as read_fields;

    const SETTINGS: &str = ".claude/settings.json";

    /// A project as `rotproof create` of 0.1.0 left it: no files in the declaration, settings without the deny rule
    fn made_by_first() -> Fake {
        let tree = Fake::default();
        assert!(matches!(
            init(&tree, &tree, Some("none"), "x", "0.1.0"),
            Ok(Initialized::Declared(_))
        ));
        let text = tree.text(DECLARATION).unwrap();
        let first = text
            .split("\n# The version of Rotproof")
            .next()
            .unwrap()
            .to_string();
        tree.write(DECLARATION, &format!("{first}\n")).unwrap();
        tree.write(SETTINGS, "{\n  \"hooks\": {}\n}\n").unwrap();
        tree
    }

    #[test]
    fn the_declaration_is_written_once_with_the_running_version() {
        let tree = Fake::default();
        assert!(matches!(
            init(&tree, &tree, Some("python"), "x", "0.2.0"),
            Ok(Initialized::Declared(DECLARATION))
        ));
        assert_eq!(tree.text(DECLARATION), Some(declaration_text("python")));
        assert!(
            init(
                &Fake::default(),
                &Fake::default(),
                Some("cobol"),
                "x",
                "0.2.0"
            )
            .unwrap_err()
            .starts_with("unknown stack")
        );
        assert!(
            init(&Fake::default(), &Fake::default(), None, "x", "0.2.0")
                .unwrap_err()
                .contains("--stack")
        );
        // Run again with another stack: it never changes the stack
        assert!(
            init(&tree, &tree, Some("rust"), "x", "0.2.0")
                .unwrap_err()
                .contains("never changes a stack")
        );
    }

    #[test]
    fn a_project_made_by_the_first_release_is_upgraded_and_a_second_run_changes_nothing() {
        let tree = made_by_first();
        let Ok(Initialized::Upgraded(upgraded)) = init(&tree, &tree, None, "x", "0.2.0") else {
            panic!("upgraded");
        };
        assert_eq!(upgraded.from, "0.1.0");
        assert_eq!(upgraded.applied, [("claude-deny-approvals", SETTINGS)]);
        assert!(upgraded.by_hand.is_empty());
        assert!(upgraded.files_set);
        assert!(
            tree.text(SETTINGS)
                .unwrap()
                .contains("Edit(/.config/rotproof-approved.toml)")
        );
        let declared = read_fields(&tree.text(DECLARATION).unwrap()).unwrap();
        assert_eq!(declared.files.as_deref(), Some("0.2.0"));
        assert_eq!(declared.stack.as_deref(), Some("none"));
        // The refresh wrote what create writes after an upgrade
        assert!(tree.text(".rotproof/AGENTS.md").is_some());

        let before: Vec<Option<String>> = [DECLARATION, SETTINGS]
            .iter()
            .map(|p| tree.text(p))
            .collect();
        let Ok(Initialized::Upgraded(again)) = init(&tree, &tree, Some("none"), "x", "0.2.0")
        else {
            panic!("upgraded");
        };
        assert!(again.applied.is_empty() && !again.files_set && again.made.written.is_empty());
        let after: Vec<Option<String>> = [DECLARATION, SETTINGS]
            .iter()
            .map(|p| tree.text(p))
            .collect();
        assert_eq!(before, after);
    }

    #[test]
    fn an_update_asks_no_person_for_a_file_the_refresh_writes_with_what_it_adds() {
        let tree = made_by_first();
        tree.remove(SETTINGS).unwrap();
        let Ok(Initialized::Upgraded(upgraded)) = init(&tree, &tree, None, "x", "0.2.0") else {
            panic!("upgraded");
        };
        // The refresh wrote the settings, with the rule, and the update asks for nothing more
        assert!(upgraded.made.written.iter().any(|p| p == SETTINGS));
        assert!(
            tree.text(SETTINGS)
                .unwrap()
                .contains("Edit(/.config/rotproof-approved.toml)")
        );
        assert!(upgraded.by_hand.is_empty(), "{:?}", upgraded.by_hand);
        assert!(upgraded.applied.is_empty());
        assert!(upgraded.files_set);

        // Asked again of the file, not trusted by its path: a written file without the rule still needs a person
        let written = [SETTINGS.to_string()];
        assert_eq!(
            written_done(&tree, Update::DenyApprovals, &written),
            Ok(true)
        );
        // Not written by the refresh: the request stays, whatever the file holds
        assert_eq!(written_done(&tree, Update::DenyApprovals, &[]), Ok(false));
        tree.write(SETTINGS, "{}\n").unwrap();
        assert_eq!(
            written_done(&tree, Update::DenyApprovals, &written),
            Ok(false)
        );
    }

    #[test]
    fn files_is_set_in_the_line_endings_and_with_the_byte_order_mark_the_project_wrote() {
        for (crlf, bom) in [(true, true), (true, false), (false, true), (false, false)] {
            let tree = made_by_first();
            let lf = tree.text(DECLARATION).unwrap();
            let text = if crlf {
                lf.replace('\n', "\r\n")
            } else {
                lf.clone()
            };
            let raw = if bom { format!("\u{feff}{text}") } else { text };
            tree.write(DECLARATION, &raw).unwrap();
            let Ok(Initialized::Upgraded(upgraded)) = init(&tree, &tree, None, "x", "0.2.0") else {
                panic!("upgraded");
            };
            assert!(upgraded.files_set);
            let now = tree.text(DECLARATION).unwrap();
            let said = format!("crlf {crlf}, bom {bom}: {now:?}");
            assert!(now.starts_with(&raw), "{said}");
            assert_eq!(now.starts_with('\u{feff}'), bom, "{said}");
            // Every line ends as the others did, the lines added too
            if crlf {
                assert!(!now.replace("\r\n", "").contains('\n'), "{said}");
            } else {
                assert!(!now.contains('\r'), "{said}");
            }
            let read = domain::tree::with_lf(&now);
            assert_eq!(read_fields(&read).unwrap().files.as_deref(), Some("0.2.0"));
        }
    }

    /// An update writes its file back as the project wrote it, as `files` is set: the settings came back all LF and
    /// without the byte order mark.
    #[test]
    fn an_update_keeps_the_line_endings_and_the_byte_order_mark_of_its_file() {
        for (crlf, bom) in [(true, true), (true, false), (false, true), (false, false)] {
            let tree = made_by_first();
            let lf = tree.text(SETTINGS).unwrap();
            let text = if crlf { lf.replace('\n', "\r\n") } else { lf };
            let raw = if bom { format!("\u{feff}{text}") } else { text };
            tree.write(SETTINGS, &raw).unwrap();
            let Ok(Initialized::Upgraded(upgraded)) = init(&tree, &tree, None, "x", "0.2.0") else {
                panic!("upgraded");
            };
            assert_eq!(upgraded.applied, [("claude-deny-approvals", SETTINGS)]);
            let now = tree.text(SETTINGS).unwrap();
            let said = format!("crlf {crlf}, bom {bom}: {now:?}");
            assert_eq!(now.starts_with('\u{feff}'), bom, "{said}");
            if crlf {
                assert!(!now.replace("\r\n", "").contains('\n'), "{said}");
            } else {
                assert!(!now.contains('\r'), "{said}");
            }
            assert!(
                now.contains("Edit(/.config/rotproof-approved.toml)"),
                "{said}"
            );
        }
    }

    /// Upgraded to 1.0, the settings 0.4.0 wrote lose the stop hook and end as a new project's, in their line endings.
    #[test]
    fn the_stop_hook_is_taken_out_on_the_way_to_1_0() {
        let tree = made_by_first();
        let new = domain::hook::SETTINGS[0].1;
        let stop = "\"hooks\": {\n    \"Stop\": [\n      {\n        \"hooks\": [\n          {\n            \
                    \"type\": \"command\",\n            \"command\": \"rotproof stop-hook\"\n          }\n        ]\n      \
                    }\n    ],\n";
        let old = new.replacen("\"hooks\": {\n", stop, 1);
        assert_ne!(old, new);
        tree.write(SETTINGS, &old.replace('\n', "\r\n")).unwrap();
        let Ok(Initialized::Upgraded(upgraded)) = init(&tree, &tree, None, "x", "1.0.0-rc.1")
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

    /// A project of records as 0.2.0 laid it out: specs in docs/specs/, backlog items in docs/backlog/
    fn made_by_0_2() -> Fake {
        let tree = Fake::default();
        assert!(matches!(
            init(&tree, &tree, Some("none"), "x", "0.2.0"),
            Ok(Initialized::Declared(_))
        ));
        // Up to 0.2.0 whatever this Rotproof is: the declaration is written with its version
        let written = tree.text(DECLARATION).unwrap();
        let running = format!("files = \"{}\"", domain::upgrade::VERSION);
        assert!(written.contains(&running), "{written}");
        let declaration = written
            .replace(&running, "files = \"0.2.0\"")
            .replace("areas = []", "areas = [\"a\"]");
        tree.write(DECLARATION, &declaration).unwrap();
        let item = "---\ntype: Backlog Item\ntitle: X\ndescription: D.\ntags: [a]\nstatus: stable\nfiled: 2026-10-01\n\
                    verified: {by: human:a, at: 2026-10-01T10:00:00+09:00}\ndeadline_kind: until\ndeadline: until \
                    0.3.0\n---\n\n# Trigger\n\nT.\n\n# State\n\nS.\n\n# Details\n\n[spec](/specs/design.md)\n";
        for (path, text) in [
            (
                "docs/log.md",
                "# Log\n\n## 2026-10-01\n\n* [x](/backlog/item.md)\n",
            ),
            ("docs/index.md", "old"),
            ("docs/backlog/item.md", item),
            ("docs/backlog/index.md", "old"),
            ("docs/backlog/rules.md", "old"),
            (
                "docs/specs/design.md",
                "---\ntype: Spec\ntitle: D\ndescription: D.\ntags: [a]\nstatus: stable\n---\n\n# Goals\n\nG.\n",
            ),
            ("docs/specs/rules.md", "old"),
            ("docs/knowledge/rules.md", "old"),
        ] {
            tree.write(path, text).unwrap();
        }
        tree
    }

    /// The records of a project made before 0.3.0 stay where they are, as they were: Rotproof no longer moves them,
    /// and the upgrade asks no person about them.
    #[test]
    fn the_records_of_an_old_layout_stay_as_they_are() {
        let tree = made_by_0_2();
        let before: Vec<Option<String>> = [
            "docs/backlog/item.md",
            "docs/specs/design.md",
            "docs/log.md",
        ]
        .iter()
        .map(|p| tree.text(p))
        .collect();
        let Ok(Initialized::Upgraded(upgraded)) = init(&tree, &tree, None, "x", "1.0.0-rc.1")
        else {
            panic!("upgraded");
        };
        assert!(upgraded.by_hand.is_empty(), "{:?}", upgraded.by_hand);
        assert!(upgraded.files_set);
        let after: Vec<Option<String>> = [
            "docs/backlog/item.md",
            "docs/specs/design.md",
            "docs/log.md",
        ]
        .iter()
        .map(|p| tree.text(p))
        .collect();
        assert_eq!(before, after);
        assert_eq!(tree.found("docs/work/item.md"), None);
    }

    #[test]
    fn crossing_the_onion_table_says_how_to_move_the_layers_once_and_moves_nothing() {
        let tree = Fake::default();
        assert!(matches!(
            init(&tree, &tree, Some("python"), "x", "0.3.0"),
            Ok(Initialized::Declared(_))
        ));
        // Up to 0.3.0 whatever this Rotproof is: the declaration is written with its version
        let written = tree.text(DECLARATION).unwrap();
        let running = format!("files = \"{}\"", domain::upgrade::VERSION);
        assert!(written.contains(&running), "{written}");
        tree.write(DECLARATION, &written.replace(&running, "files = \"0.3.0\""))
            .unwrap();
        tree.write("handler/__init__.py", "").unwrap();
        tree.write("utils/text.py", "").unwrap();
        let Ok(Initialized::Upgraded(upgraded)) = init(&tree, &tree, None, "x", "0.4.0") else {
            panic!("upgraded");
        };
        assert_eq!(upgraded.moves.len(), 3, "{:#?}", upgraded.moves);
        assert!(
            upgraded.moves[0]
                .starts_with("handler/ holds the layer handler of the table before 0.4.0")
        );
        assert!(upgraded.moves[1].starts_with("utils/ holds the layer utils"));
        assert!(upgraded.moves[2].contains("protocol.outbound"));
        // It keeps nothing behind, and moves nothing
        assert!(upgraded.files_set && upgraded.by_hand.is_empty());
        assert_eq!(tree.text("handler/__init__.py").as_deref(), Some(""));
        // Said once: the next run does not cross 0.4.0
        let Ok(Initialized::Upgraded(again)) = init(&tree, &tree, None, "x", "0.4.0") else {
            panic!("upgraded");
        };
        assert!(again.moves.is_empty(), "{:#?}", again.moves);

        // A repository of records only has no layers to move
        let records = Fake::default();
        assert!(init(&records, &records, Some("none"), "x", "0.3.0").is_ok());
        let written = records.text(DECLARATION).unwrap();
        records
            .write(DECLARATION, &written.replace(&running, "files = \"0.3.0\""))
            .unwrap();
        let Ok(Initialized::Upgraded(none)) = init(&records, &records, None, "x", "0.4.0") else {
            panic!("upgraded");
        };
        assert!(none.moves.is_empty());
    }

    #[test]
    fn an_update_that_needs_a_person_leaves_the_files_behind_until_it_is_declined() {
        let tree = made_by_first();
        tree.write(SETTINGS, "not json").unwrap();
        let Ok(Initialized::Upgraded(upgraded)) = init(&tree, &tree, None, "x", "0.2.0") else {
            panic!("upgraded");
        };
        assert_eq!(upgraded.by_hand.len(), 1, "{:?}", upgraded.by_hand);
        assert!(!upgraded.files_set);
        assert_eq!(tree.text(SETTINGS).as_deref(), Some("not json"));
        assert_eq!(
            read_fields(&tree.text(DECLARATION).unwrap()).unwrap().files,
            None
        );

        // Declined, it is neither applied nor asked for again, and the files reach the running version
        let text = tree.text(DECLARATION).unwrap();
        tree.write(
            DECLARATION,
            &format!("{text}declined = [\"claude-deny-approvals\"]\n"),
        )
        .unwrap();
        let Ok(Initialized::Upgraded(upgraded)) = init(&tree, &tree, None, "x", "0.2.0") else {
            panic!("upgraded");
        };
        assert!(upgraded.by_hand.is_empty() && upgraded.applied.is_empty());
        assert!(upgraded.files_set);
        assert_eq!(tree.text(SETTINGS).as_deref(), Some("not json"));
    }
}

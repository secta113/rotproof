//! `rotproof init`: write a project's declaration, `.config/rotproof.toml`, or, when it exists, bring the project's
//! files up to the running version of Rotproof.
//!
//! - **Without a declaration,** it writes only the declaration, so the project declares the layers it does not have
//!   before `rotproof create` makes anything. The declaration is the project's from then on.
//! - **With one,** it is the whole upgrade (`upgrade.rs` in `domain`): it applies every update of the project's files
//!   between the declaration's `files` and the running version that the project did not decline, does what
//!   `rotproof create` does after an upgrade without making a layer, and sets `files` to the running version. What an
//!   update cannot do without a person it says, and then `files` stays where it was, so `rotproof check` keeps
//!   failing until it is done or declined. It never changes the declaration's other values.
//! - **Crossing 0.3.0,** it first moves the records from `docs/backlog/` and `docs/specs/` into `docs/work/`
//!   (`migrate.rs` in `domain`), which cannot be declined: Rotproof from 0.3.0 reads no other place.

use std::collections::BTreeMap;

use crate::create::{Made, refresh};
use crate::tree::{exactly, read_text};
use domain::bundle::DOCS;
use domain::layers::{DECLARATION, declaration_text, known_stacks};
use domain::migrate;
use domain::tree::{Tree, Writer};
use domain::upgrade::{FIRST, Update, fields, pending, with_files};

/// What `rotproof init` did.
#[derive(Debug)]
pub enum Initialized {
    /// The declaration was written, at this path
    Declared(&'static str),
    Upgraded(Upgraded),
}

/// What an upgrade of the project's files did.
#[derive(Debug, Default)]
pub struct Upgraded {
    /// The version the project's files were up to
    pub from: String,
    /// Each update applied, with the file it changed
    pub applied: Vec<(&'static str, &'static str)>,
    /// How many records moved into `docs/work/`
    pub moved: usize,
    /// What each update that could not be applied leaves to a person
    pub by_hand: Vec<String>,
    /// What `rotproof create` would have done after the upgrade, without a layer
    pub made: Made,
    /// Whether `files` was set to the running version
    pub files_set: bool,
}

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
    // Before the refresh, which writes docs/work/ and reads the records there for the areas a declaration lacks
    if migrate::due(&from, running) {
        move_records(tree, out, &mut upgraded)?;
    }
    for update in pending(&from, running, &declared.declined)? {
        apply(tree, out, update, &mut upgraded)?;
    }
    upgraded.made = refresh(tree, out, name)?;
    if upgraded.by_hand.is_empty() && declared.files.as_deref() != Some(running) {
        // Read again: the refresh may have added a field the declaration lacked
        let text = read_text(tree, &path).map_err(|e| format!("{DECLARATION}: {e}"))?;
        let raw = tree
            .read(&path)
            .map_err(|e| format!("{DECLARATION}: {e}"))?;
        let written = with_files(&text, running, raw.contains("\r\n"))?;
        out.write(DECLARATION, &written)
            .map_err(|e| format!("{DECLARATION}: {e}"))?;
        upgraded.files_set = true;
    }
    Ok(upgraded)
}

/// Move the records of a project made before 0.3.0 into `docs/work/` (`migrate.rs` in `domain`): every file under
/// `docs/` read, the move planned whole, then written. A move that cannot be planned changes nothing.
fn move_records(tree: &dyn Tree, out: &dyn Writer, upgraded: &mut Upgraded) -> Result<(), String> {
    if tree.found(DOCS) != Some(true) {
        return Ok(());
    }
    let mut files = BTreeMap::new();
    let mut dirs = vec![DOCS.to_string()];
    while let Some(dir) = dirs.pop() {
        for (name, is_dir) in tree.entries(&dir).map_err(|e| format!("{dir}: {e}"))? {
            let path = format!("{dir}/{name}");
            if is_dir {
                dirs.push(path);
            } else {
                // Only markdown is read: any other file is placed by name, or stops the move
                let text = if name.ends_with(".md") {
                    read_text(tree, &path).map_err(|e| format!("{path}: {e}"))?
                } else {
                    String::new()
                };
                files.insert(path, text);
            }
        }
    }
    let plan = migrate::plan(&files)?;
    for (path, text) in &plan.writes {
        out.write(path, text).map_err(|e| format!("{path}: {e}"))?;
    }
    for path in &plan.removes {
        out.remove(path).map_err(|e| format!("{path}: {e}"))?;
    }
    for dir in &plan.dirs {
        out.remove_dir(dir).map_err(|e| format!("{dir}: {e}"))?;
    }
    upgraded.moved = plan.moved;
    upgraded.by_hand.extend(plan.by_hand);
    Ok(())
}

/// Apply one update to its file, or say what it leaves to a person.
fn apply(
    tree: &dyn Tree,
    out: &dyn Writer,
    update: Update,
    upgraded: &mut Upgraded,
) -> Result<(), String> {
    let path = update.file();
    let text = match exactly(tree, path) {
        Ok((found, false)) => Some(read_text(tree, &found).map_err(|e| format!("{path}: {e}"))?),
        _ => None,
    };
    match update.apply(text.as_deref()) {
        domain::upgrade::Outcome::Done => {}
        domain::upgrade::Outcome::Changed(written) => {
            out.write(path, &written)
                .map_err(|e| format!("{path}: {e}"))?;
            upgraded.applied.push((update.name(), path));
        }
        domain::upgrade::Outcome::ByHand(why) => upgraded.by_hand.push(why),
    }
    Ok(())
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

    /// A project of records as 0.2.0 laid it out: specs in docs/specs/, backlog items in docs/backlog/
    fn made_by_0_2() -> Fake {
        let tree = Fake::default();
        assert!(matches!(
            init(&tree, &tree, Some("none"), "x", "0.2.0"),
            Ok(Initialized::Declared(_))
        ));
        let declaration = tree
            .text(DECLARATION)
            .unwrap()
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

    #[test]
    fn the_upgrade_to_0_3_moves_the_records_once() {
        let tree = made_by_0_2();
        let Ok(Initialized::Upgraded(upgraded)) = init(&tree, &tree, None, "x", "0.3.0") else {
            panic!("upgraded");
        };
        assert_eq!(upgraded.moved, 2);
        for gone in [
            "docs/backlog",
            "docs/specs",
            "docs/backlog/item.md",
            "docs/specs/rules.md",
        ] {
            assert_eq!(tree.found(gone), None, "{gone} is still there");
        }
        let item = tree.text("docs/work/item.md").unwrap();
        assert!(
            item.contains("type: Work Item\n") && item.contains("[spec](/work/design.md)"),
            "{item}"
        );
        assert!(tree.text("docs/work/design.md").is_some());
        assert_eq!(
            tree.text("docs/log.md").unwrap(),
            "# Log\n\n## 2026-10-01\n\n* [x](/work/item.md)\n"
        );
        // The refresh wrote docs/work/ after the move: its rules, its index, and a milestone for a person to write
        for path in [
            "docs/work/rules.md",
            "docs/work/index.md",
            "docs/work/next-milestone.md",
        ] {
            assert!(tree.text(path).is_some(), "{path}");
        }
        // The deadline of the open item and the draft it became are left to a person, so files stays where it was
        assert_eq!(upgraded.by_hand.len(), 2, "{:?}", upgraded.by_hand);
        assert!(!upgraded.files_set);
        assert_eq!(
            read_fields(&tree.text(DECLARATION).unwrap())
                .unwrap()
                .files
                .as_deref(),
            Some("0.2.0")
        );

        // Run again: nothing more moves, and files reaches the running version. What is left to a person, the check
        // names from then on, record by record
        let Ok(Initialized::Upgraded(again)) = init(&tree, &tree, None, "x", "0.3.0") else {
            panic!("upgraded");
        };
        assert_eq!(again.moved, 0);
        assert!(
            again.by_hand.is_empty() && again.files_set,
            "{:?}",
            again.by_hand
        );
    }

    #[test]
    fn the_move_waits_for_the_version_and_stops_whole() {
        // Not crossing 0.3.0: nothing moves
        let tree = made_by_0_2();
        let Ok(Initialized::Upgraded(upgraded)) = init(&tree, &tree, None, "x", "0.2.0") else {
            panic!("upgraded");
        };
        assert_eq!(upgraded.moved, 0);
        assert!(tree.text("docs/backlog/item.md").is_some());

        // A name in both directories: nothing changes at all
        let tree = made_by_0_2();
        tree.write("docs/specs/item.md", "---\ntype: Spec\n---\n")
            .unwrap();
        let before = tree.text("docs/backlog/item.md");
        let why = init(&tree, &tree, None, "x", "0.3.0").unwrap_err();
        assert!(
            why.contains("docs/backlog/item.md and docs/specs/item.md have the same name"),
            "{why}"
        );
        assert_eq!(tree.text("docs/backlog/item.md"), before);
        assert_eq!(tree.found("docs/work"), None);
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

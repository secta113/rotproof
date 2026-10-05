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

use crate::create::{Made, refresh};
use crate::tree::{exactly, read_text};
use domain::layers::{DECLARATION, declaration_text, known_stacks};
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

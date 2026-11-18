//! `rotproof init`: write a project's declaration, `.config/rotproof.toml`, or, when it exists, bring the project's
//! files up to the running version of Rotproof.
//!
//! - **Without a declaration,** it writes only the declaration, so the project declares the layers it does not have
//!   before `rotproof create` makes anything. The declaration is the project's from then on.
//! - **With one,** it is the whole upgrade (`upgrade.rs` in `domain`): it applies every update of the project's files
//!   between the declaration's `files` and the running version that the project did not decline, does what
//!   `rotproof create` does after an upgrade without making a layer, and sets `files` to the running version. What an
//!   update cannot do without a person it says, and then `files` stays where it was, so `rotproof check` keeps
//!   failing until it is done or declined. An update whose file is missing has nothing to do. It adds `declined`,
//!   empty, when the declaration lacks it, names every field it adds, and never changes the declaration's other values.
//!   It never moves or rewrites a record: from 1.0, Rotproof reads no records of work, wherever they are.
//! - **With units,** an upgrade also drafts, once, the knowledge document of the units (`draft`), which needs the
//!   parsers to read the imports between them.
//! - **The parts:** `updates` applies one update to its file, `crossing` says what a person moves when an upgrade
//!   crosses a table that changed what the layers are, and `draft` writes the document of the units.

mod crossing;
mod draft;
mod updates;

pub use draft::draft;

use crate::create::refresh;
use crate::layers::read;
use crate::project::map_problems;
use crate::tree::{exactly, read_written};
use domain::declaration::{
    DECLARATION, Declared, declaration_text, fields, renamed_ui, with_files,
};
use domain::layers::{chosen_ui, known_stacks};
use domain::upgrade::{FIRST, pending};
use protocol::inbound::{Initialized, Upgraded};
use protocol::outbound::{Tree, Writer};

/// Write the declaration for `stack` and `ui` into `tree` through `out`, or upgrade the project there to `running`.
/// `name` is the project's, for the files it writes. `Err` when no declaration exists and the stack is unknown or not
/// named, or the UI is unknown for it or not named where the stack offers one; when the stack or the UI named differs
/// from the declared one; or when a file cannot be read or written.
pub fn init(
    tree: &dyn Tree,
    out: &dyn Writer,
    stack: Option<&str>,
    ui: Option<&str>,
    name: &str,
    running: &str,
) -> Result<Initialized, String> {
    if tree.found(DECLARATION).is_some() {
        return upgrade(tree, out, stack, ui, name, running)
            .map(|upgraded| Initialized::Upgraded(Box::new(upgraded)));
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
    let ui = chosen_ui(stack, ui)?;
    out.write(DECLARATION, &declaration_text(stack, &ui))
        .map_err(|e| format!("{DECLARATION}: {e}"))?;
    Ok(Initialized::Declared(DECLARATION))
}

/// Bring the project's files up to `running`.
fn upgrade(
    tree: &dyn Tree,
    out: &dyn Writer,
    stack: Option<&str>,
    ui: Option<&str>,
    name: &str,
    running: &str,
) -> Result<Upgraded, String> {
    let (path, _) = exactly(tree, DECLARATION)?;
    let (text, was) = read_written(tree, &path).map_err(|e| format!("{DECLARATION}: {e}"))?;
    // A declaration written before the field was renamed is read as renamed, and written so once nothing refuses
    let renamed = renamed_ui(&text);
    let declared = fields(renamed.as_deref().unwrap_or(&text))?;
    if let (Some(named), Some(declared)) = (stack, &declared.stack)
        && named != declared
    {
        return Err(format!(
            "{DECLARATION} declares stack = \"{declared}\", and rotproof init never changes a stack: run it without \
             --stack to upgrade the project's files"
        ));
    }
    if let Some(named) = ui
        && declared.ui.as_deref() != Some(named)
    {
        return Err(match &declared.ui {
            Some(declared) => format!(
                "{DECLARATION} declares ui_design = \"{declared}\", and rotproof init never changes it: edit the \
                 declaration to change the UI, and run it without --ui-design to upgrade the project's files"
            ),
            None => format!(
                "{DECLARATION} has no ui_design yet: run rotproof init without --ui-design, which adds it from the \
                 layers the tree holds, then edit it"
            ),
        });
    }
    let from = declared.files.clone().unwrap_or_else(|| FIRST.to_string());
    let mut upgraded = Upgraded {
        from: from.clone(),
        ..Upgraded::default()
    };
    if let Some(renamed) = renamed {
        out.write(DECLARATION, &was.restore(&renamed))
            .map_err(|e| format!("{DECLARATION}: {e}"))?;
        upgraded.renamed.push("ui to ui_design".to_string());
    }
    for update in pending(&from, running, &declared.declined)? {
        if let Some(why) = updates::apply(tree, out, update, &mut upgraded)? {
            upgraded.by_hand.push(why);
        }
    }
    upgraded.moves = crossing::moves(tree, &declared, &from, running);
    upgraded.made = refresh(tree, out, name)?;
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

/// Put what is out of date in the map of the project's own `AGENTS.md` into `upgraded`, as `rotproof check` warns of
/// it: an upgrade says nothing of the file, which is the project's, so a map left behind when a layer was declared
/// absent and made later, or a unit added, would be told of nowhere else when the project is up to date. Nothing for a
/// declaration that cannot be read.
pub fn note_map(tree: &dyn Tree, upgraded: &mut Upgraded) {
    let Ok(Ok(declaration)) = read(tree) else {
        return;
    };
    if let Ok(declared) = Declared::new(declaration) {
        upgraded.stale_map = map_problems(tree, &declared);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::fake::Fake;
    use domain::declaration::fields as read_fields;

    pub(super) const SETTINGS: &str = ".claude/settings.json";

    /// A project as `rotproof create` of 0.1.0 left it: no files in the declaration, settings without the deny rule
    pub(super) fn made_by_first() -> Fake {
        let tree = Fake::default();
        assert!(matches!(
            init(&tree, &tree, Some("none"), None, "x", "0.1.0"),
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
            init(&tree, &tree, Some("python"), Some("atomic"), "x", "0.2.0"),
            Ok(Initialized::Declared(DECLARATION))
        ));
        assert_eq!(
            tree.text(DECLARATION),
            Some(declaration_text("python", "atomic"))
        );
        assert!(
            init(
                &Fake::default(),
                &Fake::default(),
                Some("cobol"),
                None,
                "x",
                "0.2.0"
            )
            .unwrap_err()
            .starts_with("unknown stack")
        );
        assert!(
            init(&Fake::default(), &Fake::default(), None, None, "x", "0.2.0")
                .unwrap_err()
                .contains("--stack")
        );
        // Run again with another stack: it never changes the stack
        assert!(
            init(&tree, &tree, Some("rust"), None, "x", "0.2.0")
                .unwrap_err()
                .contains("never changes a stack")
        );
        // Nor the UI; the one declared may be named
        assert!(
            init(&tree, &tree, None, Some("none"), "x", "0.2.0")
                .unwrap_err()
                .contains("declares ui_design = \"atomic\"")
        );
        let named = init(&tree, &tree, None, Some("atomic"), "x", "0.2.0");
        assert!(named.err().is_none_or(|why| !why.contains("ui")));
    }

    #[test]
    fn the_ui_is_named_where_the_stack_offers_one() {
        let new = |stack: &str, ui: Option<&str>| {
            let tree = Fake::default();
            init(&tree, &tree, Some(stack), ui, "x", "0.2.0")
                .map(|_| tree.text(DECLARATION).unwrap())
        };
        // A project with screens and one without start from the same command: no default makes the layers of either
        let why = new("python", None).unwrap_err();
        assert!(
            why.contains("--ui-design (atomic, rendered, none)"),
            "{why}"
        );
        assert!(
            new("typescript", Some("none"))
                .unwrap()
                .contains("\nui_design = \"none\"\n")
        );
        assert!(new("python", Some("fsd")).unwrap_err().starts_with(
            "unknown UI \"fsd\" for the python stack (known: atomic, rendered, none)"
        ));
        // Only Python offers the UI whose views are templates
        assert!(
            new("python", Some("rendered"))
                .unwrap()
                .contains("\nui_design = \"rendered\"\n")
        );
        assert!(new("typescript", Some("rendered")).is_err());
        // Rust offers none alone, so it needs no --ui and takes no other
        assert!(
            new("rust", None)
                .unwrap()
                .contains("\nui_design = \"none\"\n")
        );
        assert!(new("rust", Some("atomic")).is_err());
        // A repository of records only has no UI
        assert!(!new("none", None).unwrap().contains("ui ="));
        assert!(new("none", Some("none")).unwrap_err().contains("no UI"));
    }

    #[test]
    fn a_project_made_by_the_first_release_is_upgraded_and_a_second_run_changes_nothing() {
        let tree = made_by_first();
        let Ok(Initialized::Upgraded(upgraded)) = init(&tree, &tree, None, None, "x", "0.2.0")
        else {
            panic!("upgraded");
        };
        assert_eq!(upgraded.from, "0.1.0");
        // The rule that denied editing the approvals file is not added: nothing is to be applied up to 0.2.0
        assert!(upgraded.applied.is_empty(), "{:?}", upgraded.applied);
        assert!(upgraded.by_hand.is_empty());
        assert!(upgraded.files_set);
        assert_eq!(
            tree.text(SETTINGS).as_deref(),
            Some("{\n  \"hooks\": {}\n}\n")
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
        let Ok(Initialized::Upgraded(again)) = init(&tree, &tree, Some("none"), None, "x", "0.2.0")
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
            let Ok(Initialized::Upgraded(upgraded)) = init(&tree, &tree, None, None, "x", "0.2.0")
            else {
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

    /// A project of records as 0.2.0 laid it out: specs in docs/specs/, backlog items in docs/backlog/
    fn made_by_0_2() -> Fake {
        let tree = Fake::default();
        assert!(matches!(
            init(&tree, &tree, Some("none"), None, "x", "0.2.0"),
            Ok(Initialized::Declared(_))
        ));
        // Up to 0.2.0 whatever this Rotproof is: the declaration is written with its version
        let written = tree.text(DECLARATION).unwrap();
        let running = format!("files = \"{}\"", domain::version::VERSION);
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
        let Ok(Initialized::Upgraded(upgraded)) = init(&tree, &tree, None, None, "x", "1.0.0-rc.1")
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
}

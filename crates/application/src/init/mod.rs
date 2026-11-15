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
//! - **Across a table that changed what the layers are,** what a person is told to move is in `crossing`.

mod crossing;

use crate::bundle::Bundle;
use crate::create::refresh;
use crate::direction::problems as direction;
use crate::follows::current;
use crate::layers::read;
use crate::project::map_problems;
use crate::tree::{exactly, read_text, read_written};
use domain::declaration::{DECLARATION, Declared, declaration_text, fields, with_files};
use domain::layers::{chosen_ui, known_stacks};
use domain::tree::Written;
use domain::units_doc::{self, DOCUMENT, FILE, KEY};
use domain::upgrade::{FIRST, Update, pending};
use protocol::inbound::{Initialized, Upgraded};
use protocol::outbound::{Parsers, Tree, Writer};

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
    if let Some(named) = ui
        && declared.ui.as_deref() != Some(named)
    {
        return Err(match &declared.ui {
            Some(declared) => format!(
                "{DECLARATION} declares ui = \"{declared}\", and rotproof init never changes it: edit the declaration \
                 to change the UI, and run it without --ui to upgrade the project's files"
            ),
            None => format!(
                "{DECLARATION} has no ui yet: run rotproof init without --ui, which adds it from the layers the tree \
                 holds, then edit it"
            ),
        });
    }
    let from = declared.files.clone().unwrap_or_else(|| FIRST.to_string());
    let mut upgraded = Upgraded {
        from: from.clone(),
        ..Upgraded::default()
    };
    for update in pending(&from, running, &declared.declined)? {
        if let Some(why) = apply(tree, out, update, &mut upgraded)? {
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

/// Draft the knowledge document of the units into `upgraded`, once: the project declares units, no document of
/// `docs/knowledge/` follows them (a deprecated one counts, as the person deprecated it on purpose) and none is named
/// `units.md` in any case, so a person's picture is never written over and a second run changes nothing. The draft is
/// tagged with the first of the declared areas and pinned at the declaration's units as they are now; its edges are the
/// imports the direction check reads between the units. The index of `docs/knowledge/` is written again with it, so
/// that `rotproof check` passes right after. Nothing for a project that declares no unit, or whose declaration cannot
/// be read (the other checks say why); with no area to tag it with, or with units that cannot be followed, nothing is
/// written and `not_drafted` says why.
pub fn draft(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    out: &dyn Writer,
    upgraded: &mut Upgraded,
) -> Result<(), String> {
    let Ok(declaration) = read(tree).map_err(|e| e.to_string())? else {
        return Ok(());
    };
    let areas = declaration.areas.clone();
    let Ok(declared) = Declared::new(declaration) else {
        return Ok(());
    };
    let units: Vec<_> = declared
        .units
        .iter()
        .filter(|unit| unit.declared)
        .cloned()
        .collect();
    if units.is_empty() || declared.layout.is_none() || tree.found(FILE).is_some() {
        return Ok(());
    }
    let folder = Bundle::new(tree, areas.clone())
        .read_knowledge()
        .map_err(|e| e.to_string())?;
    let named = |name: &String| name.eq_ignore_ascii_case(DOCUMENT);
    let followed = folder
        .documents
        .values()
        .any(|(doc, _)| doc.follows.iter().any(|(key, _)| key == KEY));
    if followed
        || folder.documents.keys().any(named)
        || folder.guides.keys().any(named)
        || folder.problems.keys().any(named)
    {
        return Ok(());
    }
    let Some(area) = areas.first() else {
        upgraded.not_drafted = Some(format!(
            "{FILE} is not drafted: {DECLARATION} declares no areas, and a knowledge document is tagged with one: add \
             an area to areas, then run `rotproof init` again"
        ));
        return Ok(());
    };
    let Some(hash) = current(tree, parsers, KEY)? else {
        upgraded.not_drafted = Some(format!(
            "{FILE} is not drafted: {DECLARATION} has no `[[unit]]` table for a document to follow: write the units as \
             tables"
        ));
        return Ok(());
    };
    let found = direction(tree, parsers, &declared).map_err(|e| e.to_string())?;
    out.write(FILE, &units_doc::draft(&units, &found.links, area, &hash))
        .map_err(|e| format!("{FILE}: {e}"))?;
    // The document is in the index now
    upgraded
        .made
        .written
        .extend(crate::index::index(tree, out)?.written);
    upgraded.drafted = Some(FILE.to_string());
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::fake::Fake;
    use domain::declaration::fields as read_fields;
    use infrastructure::outbound::readers::Readers;

    const SETTINGS: &str = ".claude/settings.json";

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

    /// A project as `rotproof create` of 0.1.0 left it: no files in the declaration, settings without the deny rule
    fn made_by_first() -> Fake {
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
                .contains("declares ui = \"atomic\"")
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
        assert!(why.contains("--ui (atomic, none)"), "{why}");
        assert!(
            new("typescript", Some("none"))
                .unwrap()
                .contains("\nui = \"none\"\n")
        );
        assert!(
            new("python", Some("fsd"))
                .unwrap_err()
                .starts_with("unknown UI \"fsd\" for the python stack (known: atomic, none)")
        );
        // Rust offers none alone, so it needs no --ui and takes no other
        assert!(new("rust", None).unwrap().contains("\nui = \"none\"\n"));
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

    /// The declaration of a Python project of three units, each in a directory of its name: `core` (shared),
    /// `billing`, and the app `cli`, with `areas` as given (a list in TOML) and, when `files` is up to the running
    /// version, nothing to upgrade.
    fn units_declaration(areas: &str) -> String {
        format!(
            "stack = \"python\"\nareas = {areas}\nui = \"none\"\nfiles = \"{}\"\ndeclined = []\n\n\
             [[unit]]\nname = \"core\"\npath = \"core\"\nshared = true\n\n\
             [[unit]]\nname = \"billing\"\npath = \"billing\"\n\n\
             [[unit]]\nname = \"cli\"\npath = \"cli\"\nentry = [\"cli/main.py\"]\n",
            domain::version::VERSION
        )
    }

    /// The code of the units: `billing` imports `core`, and `cli` wires `billing` in its composition.
    const UNIT_CODE: [(&str, &str); 4] = [
        ("core/domain/score.py", ""),
        (
            "billing/application/play.py",
            "from core.domain import score\n",
        ),
        (
            "cli/composition/wire.py",
            "from billing.application import play\n",
        ),
        ("cli/main.py", ""),
    ];

    fn units_project(areas: &str) -> Fake {
        let tree = Fake::new(&UNIT_CODE);
        tree.write(DECLARATION, &units_declaration(areas)).unwrap();
        tree
    }

    /// `rotproof init` on `tree` as the command runs it: the upgrade, then the draft.
    fn upgrade_and_draft(tree: &Fake) -> Upgraded {
        let Ok(Initialized::Upgraded(mut upgraded)) =
            init(tree, tree, None, None, "x", domain::version::VERSION)
        else {
            panic!("upgraded");
        };
        draft(tree, &Readers, tree, &mut upgraded).unwrap();
        *upgraded
    }

    #[test]
    fn the_units_are_drafted_once_with_the_imports_between_them_and_a_second_run_writes_nothing() {
        let tree = units_project("[\"design\", \"ops\"]");
        let upgraded = upgrade_and_draft(&tree);
        assert_eq!(upgraded.drafted.as_deref(), Some(FILE));
        assert_eq!(upgraded.not_drafted, None);
        let text = tree.text(FILE).unwrap();
        // Tagged with the first area, pinned at the units as the declaration has them now, the edges the imports found
        assert!(text.contains("\ntags: [\"design\"]\n"), "{text}");
        let hash = current(&tree, &Readers, KEY).unwrap().unwrap();
        assert!(
            text.contains(&format!("follows:\n  {KEY}: \"{hash}\"\n")),
            "{text}"
        );
        for edge in ["billing --> core", "cli --> billing"] {
            assert!(text.contains(&format!("\n    {edge}\n")), "{text}");
        }
        assert!(!text.contains("cli --> core"), "{text}");
        for node in [
            "core([\"core (shared)\"])",
            "billing[\"billing\"]",
            "cli[\"cli (app)\"]",
        ] {
            assert!(text.contains(&format!("\n    {node}\n")), "{text}");
        }
        // The index of the knowledge documents is written with it, and lists it
        let index = tree.text("docs/knowledge/index.md").unwrap();
        assert!(index.contains("units.md"), "{index}");
        assert!(
            upgraded
                .made
                .written
                .iter()
                .any(|p| p == "docs/knowledge/index.md"),
            "{:?}",
            upgraded.made.written
        );

        // A second run writes nothing, and a document a person rewrote is left as it is
        let before = tree.text(FILE).unwrap();
        let again = upgrade_and_draft(&tree);
        assert_eq!((again.drafted, again.not_drafted), (None, None));
        assert!(again.made.written.is_empty(), "{:?}", again.made.written);
        assert_eq!(tree.text(FILE).unwrap(), before);
        let mine = text.replace("_Not written yet", "Written, and not");
        tree.write(FILE, &mine).unwrap();
        let again = upgrade_and_draft(&tree);
        assert_eq!(again.drafted, None);
        assert_eq!(tree.text(FILE).unwrap(), mine);
    }

    #[test]
    fn the_draft_keeps_the_rules_of_the_records_it_is_checked_by() {
        use chrono::DateTime;
        let tree = units_project("[\"design\"]");
        upgrade_and_draft(&tree);
        let now = DateTime::parse_from_rfc3339("2026-11-15T12:00:00+09:00").unwrap();
        let report = crate::check::check(&tree, &Readers, now).unwrap();
        // The layers are not made in this tree, which the structure check says; of the document, not a word
        let of_it: Vec<&str> = report
            .findings
            .iter()
            .map(|f| f.detail.as_str())
            .filter(|detail| detail.contains("units.md") || detail.contains("index.md"))
            .collect();
        assert!(of_it.is_empty(), "{of_it:#?}");
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    }

    #[test]
    fn a_document_that_follows_the_units_is_not_written_beside() {
        for status in ["stable", "deprecated"] {
            let tree = units_project("[\"design\"]");
            let closed = if status == "deprecated" {
                "\n# Resolution\n\nReplaced by the code.\n"
            } else {
                ""
            };
            tree.write(
                "docs/knowledge/picture.md",
                &format!(
                    "---\ntype: Knowledge\ntitle: The picture\ndescription: Why the units are as they are.\n\
                     tags: [design]\nstatus: {status}\nfollows:\n  {KEY}: \"0a1b2c3d\"\n---\n{closed}\n# Why\n\nThey are.\n"
                ),
            )
            .unwrap();
            let upgraded = upgrade_and_draft(&tree);
            assert_eq!(
                (upgraded.drafted, upgraded.not_drafted),
                (None, None),
                "{status}"
            );
            assert_eq!(tree.text(FILE), None, "{status}");
        }
        // A document that follows something else does not stand in for it
        let tree = units_project("[\"design\"]");
        tree.write(
            "docs/knowledge/other.md",
            "---\ntype: Knowledge\ntitle: Other\ndescription: Another.\ntags: [design]\nstatus: stable\n\
             follows:\n  core/domain/score.py: \"0a1b2c3d\"\n---\n\n# Why\n\nIt is.\n",
        )
        .unwrap();
        assert_eq!(upgrade_and_draft(&tree).drafted.as_deref(), Some(FILE));
    }

    #[test]
    fn a_document_of_that_name_is_never_written_over_in_any_case() {
        for name in ["units.md", "Units.md"] {
            let tree = units_project("[\"design\"]");
            let path = format!("docs/knowledge/{name}");
            // Not a document that follows the units, nor a valid one: it is a person's file
            tree.write(&path, "not a document\n").unwrap();
            let upgraded = upgrade_and_draft(&tree);
            assert_eq!(upgraded.drafted, None, "{name}");
            assert_eq!(tree.text(&path).as_deref(), Some("not a document\n"));
            if name != "units.md" {
                // A file of the same name in another case would not check out on every system
                assert_eq!(tree.text(FILE), None, "{name}");
            }
        }
    }

    #[test]
    fn with_no_area_nothing_is_written_and_the_reason_is_said() {
        let tree = units_project("[]");
        let upgraded = upgrade_and_draft(&tree);
        assert_eq!(upgraded.drafted, None);
        assert_eq!(tree.text(FILE), None);
        let why = upgraded.not_drafted.unwrap();
        assert!(
            why.contains("declares no areas") && why.contains("add an area"),
            "{why}"
        );
        // Said again on the next run, until there is an area; then it is drafted
        assert!(upgrade_and_draft(&tree).not_drafted.is_some());
        let text = tree.text(DECLARATION).unwrap();
        tree.write(
            DECLARATION,
            &text.replace("areas = []", "areas = [\"design\"]"),
        )
        .unwrap();
        let upgraded = upgrade_and_draft(&tree);
        assert_eq!(upgraded.drafted.as_deref(), Some(FILE));
        assert_eq!(upgraded.not_drafted, None);
    }

    #[test]
    fn units_written_inline_cannot_be_followed_and_that_is_said() {
        let tree = units_project("[\"design\"]");
        tree.write(
            DECLARATION,
            &format!(
                "stack = \"python\"\nareas = [\"design\"]\nui = \"none\"\nfiles = \"{}\"\ndeclined = []\n\
                 unit = [{{ name = \"core\", path = \"core\" }}]\n",
                domain::version::VERSION
            ),
        )
        .unwrap();
        let upgraded = upgrade_and_draft(&tree);
        assert_eq!(upgraded.drafted, None);
        assert_eq!(tree.text(FILE), None);
        assert!(
            upgraded
                .not_drafted
                .unwrap()
                .contains("no `[[unit]]` table"),
        );
    }

    #[test]
    fn a_project_that_declares_no_unit_has_no_draft_and_no_word_about_it() {
        let tree = Fake::new(&UNIT_CODE);
        tree.write(
            DECLARATION,
            &format!(
                "stack = \"python\"\nareas = [\"design\"]\nui = \"none\"\nfiles = \"{}\"\ndeclined = []\n",
                domain::version::VERSION
            ),
        )
        .unwrap();
        let upgraded = upgrade_and_draft(&tree);
        assert_eq!((upgraded.drafted, upgraded.not_drafted), (None, None));
        assert_eq!(tree.text(FILE), None);
        // Nor one whose declaration cannot be read: the checks say why
        tree.write(DECLARATION, "stack = ").unwrap();
        let mut upgraded = Upgraded::default();
        draft(&tree, &Readers, &tree, &mut upgraded).unwrap();
        assert!(upgraded.drafted.is_none() && upgraded.not_drafted.is_none());
    }
}

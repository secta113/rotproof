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

use crate::create::{refresh, ui_in_tree};
use crate::tree::{exactly, read_text, read_written};
use domain::layers::{
    DECLARATION, NO_UI, ONION, PORTS_MOVED, PRESENTATION, PRESENTATION_ADDED, UI_NONE, UI_SPLIT,
    crosses, declaration_text, known_stacks, layout, retired, uis,
};
use domain::tree::Written;
use domain::upgrade::{FIRST, Fields, Update, fields, pending, with_files};
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
        return upgrade(tree, out, stack, ui, name, running).map(Initialized::Upgraded);
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

/// The UI a new declaration for `stack` gets from `--ui`. Named, it is one the stack offers; not named, it is
/// [`NO_UI`] where the stack offers nothing else, and asked for where it does: a project with screens and one without
/// both start from the same command, and a default would make the layers of one of them. A repository of records only
/// has no UI.
fn chosen_ui(stack: &str, ui: Option<&str>) -> Result<String, String> {
    let Some(offered) = uis(stack) else {
        if let Some(ui) = ui {
            return Err(format!(
                "stack {stack:?} has no layers, so it has no UI: run it without --ui {ui}"
            ));
        }
        return Ok(String::new());
    };
    match ui {
        Some(ui) if offered.iter().any(|u| u == ui) => Ok(ui.to_string()),
        Some(ui) => Err(format!(
            "unknown UI {ui:?} for the {stack} stack (known: {})",
            offered.join(", ")
        )),
        None if offered == [NO_UI] => Ok(NO_UI.to_string()),
        None => Err(format!(
            "name the UI with --ui ({}): \"none\" for a project with no screens",
            offered.join(", ")
        )),
    }
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
    let mut asked = Vec::new();
    for update in pending(&from, running, &declared.declined)? {
        if let Some(why) = apply(tree, out, update, &mut upgraded)? {
            asked.push((update, why));
        }
    }
    if crosses(ONION, &from, running) {
        upgraded.moves = onion_moves(tree, declared.stack.as_deref());
    }
    if crosses(PRESENTATION, &from, running) {
        upgraded.moves.extend(presentation_moves(tree, &declared));
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
    // Only where the layers live is read, which no UI changes
    let Some(layout) = stack.and_then(|stack| layout(stack, NO_UI)) else {
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

/// What a project does on the table that splits the UI, by the UI it declares, or the one the refresh is about to add
/// from the tree: with `ui = "atomic"`, make the new layer `presentation` and move what a page holds and decides out
/// of `ui`; with `ui = "none"`, take `ui` out of absent, where it says nothing any more. Nothing for a repository of
/// records only.
fn presentation_moves(tree: &dyn Tree, declared: &Fields) -> Vec<String> {
    let Some(stack) = declared.stack.as_deref() else {
        return Vec::new();
    };
    let ui = declared
        .ui
        .clone()
        .unwrap_or_else(|| ui_in_tree(tree, stack));
    let Some(layout) = layout(stack, &ui) else {
        return Vec::new();
    };
    let path = |name: &str| layout.layer.path.replace("{name}", name);
    let has = |name: &str| !layout.without.iter().any(|w| w == name);
    let mut moves = Vec::new();
    if has("presentation")
        && !declared.absent.iter().any(|a| a == "presentation")
        && tree.found(&path("presentation")).is_none()
    {
        moves.push(PRESENTATION_ADDED.to_string());
    }
    if has("ui") && tree.found(&path("ui")) == Some(true) {
        moves.push(format!("{}/: {UI_SPLIT}", path("ui")));
    }
    if !has("ui") && declared.absent.iter().any(|a| a == "ui") {
        moves.push(UI_NONE.to_string());
    }
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
    fn an_update_asks_no_person_for_a_file_the_refresh_writes_with_what_it_adds() {
        let tree = made_by_first();
        tree.remove(SETTINGS).unwrap();
        let Ok(Initialized::Upgraded(upgraded)) = init(&tree, &tree, None, None, "x", "0.2.0")
        else {
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
            let tree = made_by_first();
            let lf = tree.text(SETTINGS).unwrap();
            let text = if crlf { lf.replace('\n', "\r\n") } else { lf };
            let raw = if bom { format!("\u{feff}{text}") } else { text };
            tree.write(SETTINGS, &raw).unwrap();
            let Ok(Initialized::Upgraded(upgraded)) = init(&tree, &tree, None, None, "x", "0.2.0")
            else {
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
        let new = domain::hook::SETTINGS[0].1;
        assert_eq!(tree.text(SETTINGS), Some(new.replace('\n', "\r\n")));
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
        // Neither `claude-time-hook` nor `claude-remove-time-hook` has anything to do
        assert_eq!(
            upgraded.applied,
            [("claude-remove-stop-hook", SETTINGS)],
            "{:?}",
            upgraded.applied
        );
        assert_eq!(
            tree.text(SETTINGS).as_deref(),
            Some(domain::hook::SETTINGS[0].1)
        );
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
    fn crossing_the_onion_table_says_how_to_move_the_layers_once_and_moves_nothing() {
        let tree = Fake::default();
        assert!(matches!(
            init(&tree, &tree, Some("python"), Some("atomic"), "x", "0.3.0"),
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
        let Ok(Initialized::Upgraded(upgraded)) = init(&tree, &tree, None, None, "x", "0.4.0")
        else {
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
        let Ok(Initialized::Upgraded(again)) = init(&tree, &tree, None, None, "x", "0.4.0") else {
            panic!("upgraded");
        };
        assert!(again.moves.is_empty(), "{:#?}", again.moves);

        // A repository of records only has no layers to move
        let records = Fake::default();
        assert!(init(&records, &records, Some("none"), None, "x", "0.3.0").is_ok());
        let written = records.text(DECLARATION).unwrap();
        records
            .write(DECLARATION, &written.replace(&running, "files = \"0.3.0\""))
            .unwrap();
        let Ok(Initialized::Upgraded(none)) = init(&records, &records, None, None, "x", "0.4.0")
        else {
            panic!("upgraded");
        };
        assert!(none.moves.is_empty());
    }

    #[test]
    fn crossing_the_split_of_the_ui_says_what_to_declare_and_what_moves_once() {
        // A project of rc.2, which had no ui in its declaration, with `absent` and the files `planted`, upgraded to the
        // table that splits the UI: what it is told, and the ui the refresh added
        let moves = |stack: &str, absent: &str, planted: &[&str]| {
            let tree = Fake::default();
            let areas = if stack == "none" { "" } else { "areas = []\n" };
            tree.write(
                DECLARATION,
                &format!(
                    "stack = \"{stack}\"\n{areas}absent = [{absent}]\nfiles = \"1.0.0-rc.2\"\n"
                )
                .replace("absent = []\n", ""),
            )
            .unwrap();
            for path in planted {
                tree.write(path, "").unwrap();
            }
            let Ok(Initialized::Upgraded(upgraded)) =
                init(&tree, &tree, None, None, "x", PRESENTATION)
            else {
                panic!("upgraded");
            };
            assert!(upgraded.files_set && upgraded.by_hand.is_empty());
            // Said once: the next run does not cross it
            let Ok(Initialized::Upgraded(again)) =
                init(&tree, &tree, None, None, "x", PRESENTATION)
            else {
                panic!("upgraded");
            };
            assert!(again.moves.is_empty(), "{:#?}", again.moves);
            // It moves nothing and makes no layer
            assert_eq!(tree.found("presentation"), None);
            let declared = fields(&tree.text(DECLARATION).unwrap()).unwrap();
            (upgraded.moves, declared.ui)
        };
        let atomic = Some("atomic".to_string());
        let none = Some("none".to_string());
        // A project with screens keeps them: presentation is to be made, and the state of its pages moves there
        let (python, ui) = moves("python", "", &["ui/pages/__init__.py"]);
        assert_eq!(ui, atomic);
        assert_eq!(python.len(), 2, "{python:#?}");
        assert_eq!(python[0], PRESENTATION_ADDED);
        assert!(python[1].starts_with("ui/: ui imports neither application nor domain"));
        let (typescript, ui) = moves("typescript", "", &["src/ui/index.ts"]);
        assert_eq!(ui, atomic);
        assert!(typescript[1].starts_with("src/ui/: "), "{typescript:#?}");
        // presentation declared absent already is not asked for
        let (python, _) = moves("python", "\"presentation\"", &["ui/pages/__init__.py"]);
        assert_eq!(python.len(), 1, "{python:#?}");
        // One that declared ui absent has no screens: ui = "none" says so, and the entry in absent is to go
        assert_eq!(
            moves("python", "\"ui\"", &[]),
            (vec![UI_NONE.to_string()], none.clone())
        );
        // One without ui says nothing more
        assert_eq!(moves("typescript", "", &[]), (Vec::new(), none.clone()));
        assert_eq!(moves("rust", "", &[]), (Vec::new(), none));
        assert_eq!(moves("none", "", &[]), (Vec::new(), None));
    }

    #[test]
    fn an_update_that_needs_a_person_leaves_the_files_behind_until_it_is_declined() {
        let tree = made_by_first();
        tree.write(SETTINGS, "not json").unwrap();
        let Ok(Initialized::Upgraded(upgraded)) = init(&tree, &tree, None, None, "x", "0.2.0")
        else {
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
        let Ok(Initialized::Upgraded(upgraded)) = init(&tree, &tree, None, None, "x", "0.2.0")
        else {
            panic!("upgraded");
        };
        assert!(upgraded.by_hand.is_empty() && upgraded.applied.is_empty());
        assert!(upgraded.files_set);
        assert_eq!(tree.text(SETTINGS).as_deref(), Some("not json"));
    }
}

//! What `rotproof init` tells a project that an upgrade crosses a table that changed what the layers are
//! (`domain::crossing` names the versions and the words): where the code of each retired layer goes, and what the
//! screens move out of `ui`; for a project that has a `docs/work/`, what that directory is now; and, for a Python
//! project, what locking it asks. It reads the tree to say which of it applies, and moves nothing.

use crate::create::ui_in_tree;
use domain::crossing::{
    LOCKED, LOCKED_MOVES, LOCKED_REQUIREMENTS, ONION, PORTS_MOVED, PRESENTATION,
    PRESENTATION_ADDED, UI_NONE, UI_SPLIT, WORK_LEFT, WORK_STOPPED, crosses, old_paths, retired,
};
use domain::declaration::Fields;
use domain::layers::{NO_UI, layout};
use protocol::outbound::Tree;

/// What a person is told to move for the tables an upgrade from `from` to `running` crosses, in the order the tables
/// came. Nothing when it crosses none.
pub(super) fn moves(tree: &dyn Tree, declared: &Fields, from: &str, running: &str) -> Vec<String> {
    let mut moves = Vec::new();
    if crosses(ONION, from, running) {
        moves = onion_moves(tree, declared.stack.as_deref());
    }
    if crosses(WORK_STOPPED, from, running) && tree.found("docs/work") == Some(true) {
        moves.push(format!("docs/work/: {WORK_LEFT}"));
    }
    if crosses(PRESENTATION, from, running) {
        moves.extend(presentation_moves(tree, declared));
    }
    if crosses(LOCKED, from, running) {
        moves.extend(locked_moves(tree, declared.stack.as_deref()));
    }
    moves
}

/// What a Python project is told on becoming locked: to list its dependencies, lock them and keep one `.venv`, and,
/// when `requirements-dev.txt` is there, to move the pin of Rotproof out of it. Nothing for another stack.
fn locked_moves(tree: &dyn Tree, stack: Option<&str>) -> Vec<String> {
    if stack != Some("python") {
        return Vec::new();
    }
    let mut moves = vec![LOCKED_MOVES.to_string()];
    if tree.found("requirements-dev.txt") == Some(false) {
        moves.push(format!("requirements-dev.txt: {LOCKED_REQUIREMENTS}"));
    }
    moves
}

/// How to move the code of a project of `stack` onto the onion table: each retired layer that is there, with where its
/// code goes, and what moved inside the layers that stay. Nothing for a repository of records only.
fn onion_moves(tree: &dyn Tree, stack: Option<&str>) -> Vec<String> {
    // Only where the layers live is read, which no UI changes
    let Some(layout) = stack.and_then(|stack| layout(stack, NO_UI)) else {
        return Vec::new();
    };
    let found: Vec<_> = retired(&layout)
        .into_iter()
        .filter(|(_, path, _)| tree.found(path) == Some(true))
        .collect();
    let mut moves: Vec<String> = found
        .iter()
        .map(|(name, path, goes)| {
            format!(
                "{path}/ holds the layer {name} of the table before {ONION}, which is gone: {goes}"
            )
        })
        .collect();
    moves.push(format!("in the layers that stay, {PORTS_MOVED}"));
    // The code moved by hand leaves its old paths in the files no check reads
    if !found.is_empty() {
        let paths: Vec<String> = found.into_iter().map(|(_, path, _)| path).collect();
        moves.push(old_paths(&paths));
    }
    moves
}

/// What a project does on the table that splits the UI, by the UI it declares, or the one the refresh is about to add
/// from the tree: with `ui_design = "atomic"`, make the new layer `presentation` and move what a page holds and decides
/// out of `ui`; with `ui_design = "none"`, take `ui` out of absent, where it says nothing any more. Nothing for a
/// repository of records only.
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
    let path = |name: &str| layout.layer_path(name);
    let has = |name: &str| !layout.leaves_out(name);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::init::init;
    use crate::tree::fake::Fake;
    use domain::declaration::{DECLARATION, fields};
    use protocol::inbound::Initialized;
    use protocol::outbound::Writer;

    #[test]
    fn crossing_the_onion_table_says_how_to_move_the_layers_once_and_moves_nothing() {
        let tree = Fake::default();
        assert!(matches!(
            init(&tree, &tree, Some("python"), Some("atomic"), "x", "0.3.0"),
            Ok(Initialized::Declared(_))
        ));
        // Up to 0.3.0 whatever this Rotproof is: the declaration is written with its version
        let written = tree.text(DECLARATION).unwrap();
        let running = format!("files = \"{}\"", domain::version::VERSION);
        assert!(written.contains(&running), "{written}");
        tree.write(DECLARATION, &written.replace(&running, "files = \"0.3.0\""))
            .unwrap();
        tree.write("handler/__init__.py", "").unwrap();
        tree.write("utils/text.py", "").unwrap();
        let Ok(Initialized::Upgraded(upgraded)) = init(&tree, &tree, None, None, "x", "0.4.0")
        else {
            panic!("upgraded");
        };
        assert_eq!(upgraded.moves.len(), 4, "{:#?}", upgraded.moves);
        assert!(
            upgraded.moves[0]
                .starts_with("handler/ holds the layer handler of the table before 0.4.0")
        );
        assert!(upgraded.moves[1].starts_with("utils/ holds the layer utils"));
        assert!(upgraded.moves[2].contains("protocol.outbound"));
        // The last line names the directories it found, and says to search the files no check reads for them
        assert_eq!(
            upgraded.moves[3],
            old_paths(&["handler".to_string(), "utils".to_string()])
        );
        assert!(
            upgraded.moves[3].contains("(handler/ and utils/)")
                && upgraded.moves[3].contains("documents, scripts, configuration, comments"),
            "{}",
            upgraded.moves[3]
        );
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
    fn crossing_the_onion_table_names_only_the_retired_directories_it_found() {
        let moves = |planted: &[&str]| {
            let tree = Fake::default();
            tree.write(
                DECLARATION,
                "stack = \"rust\"\nareas = []\nui_design = \"none\"\nfiles = \"0.3.0\"\n",
            )
            .unwrap();
            for path in planted {
                tree.write(path, "").unwrap();
            }
            let Ok(Initialized::Upgraded(upgraded)) = init(&tree, &tree, None, None, "x", "0.4.0")
            else {
                panic!("upgraded");
            };
            upgraded.moves
        };
        // One directory: it alone is named, with its path from the root
        let one = moves(&["crates/utils/src/lib.rs"]);
        assert_eq!(one.len(), 3, "{one:#?}");
        assert!(one[2].contains("(crates/utils/)"), "{}", one[2]);
        assert!(!one[2].contains("handler"), "{}", one[2]);
        // None: the layers that stay move, and there is no old path to search for
        let none = moves(&[]);
        assert_eq!(none.len(), 1, "{none:#?}");
        assert!(none[0].contains("protocol.outbound"));
    }

    #[test]
    fn crossing_the_end_of_work_records_says_what_docs_work_is_now_and_moves_nothing() {
        let upgrade = |files: &str, running: &str, planted: &[&str]| {
            let tree = Fake::default();
            tree.write(
                DECLARATION,
                &format!("stack = \"none\"\nfiles = \"{files}\"\n"),
            )
            .unwrap();
            for path in planted {
                tree.write(path, "kept\n").unwrap();
            }
            let Ok(Initialized::Upgraded(upgraded)) = init(&tree, &tree, None, None, "x", running)
            else {
                panic!("upgraded");
            };
            assert!(upgraded.by_hand.is_empty());
            // Nothing in docs/work/ is moved, rewritten or taken out
            for path in planted {
                assert_eq!(tree.text(path).as_deref(), Some("kept\n"), "{path}");
            }
            upgraded.moves
        };
        let planted = [
            "docs/work/index.md",
            "docs/work/a.md",
            "docs/work/closed.md",
        ];
        // A project of before 1.0 with docs/work/ is told what it is now, in one line
        let said = upgrade("0.4.0", WORK_STOPPED, &planted);
        assert_eq!(said, [format!("docs/work/: {WORK_LEFT}")]);
        for word in ["index.md, closed.md or rules.md", "docs/index.md", "issues"] {
            assert!(WORK_LEFT.contains(word), "{word}");
        }
        // One without it has nothing to be told, and one already at the version was told then
        assert!(upgrade("0.4.0", WORK_STOPPED, &["docs/log.md"]).is_empty());
        assert!(upgrade(WORK_STOPPED, "1.0.0-rc.3", &planted).is_empty());
        assert!(upgrade(WORK_STOPPED, WORK_STOPPED, &planted).is_empty());
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
        // One that declared ui absent has no screens: ui_design = "none" says so, and the entry in absent is to go
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
    fn crossing_the_lock_of_python_says_what_the_project_does_and_edits_no_file() {
        let moves = |stack: &str, planted: &[&str], running: &str| {
            let tree = Fake::default();
            let areas = if stack == "none" {
                ""
            } else {
                "areas = []\nui_design = \"none\"\n"
            };
            tree.write(
                DECLARATION,
                &format!("stack = \"{stack}\"\n{areas}files = \"2.0.0-rc.1\"\n"),
            )
            .unwrap();
            for path in planted {
                tree.write(path, "pinned\n").unwrap();
            }
            let Ok(Initialized::Upgraded(upgraded)) = init(&tree, &tree, None, None, "x", running)
            else {
                panic!("upgraded");
            };
            for path in planted {
                assert_eq!(tree.text(path).as_deref(), Some("pinned\n"), "{path}");
            }
            upgraded.moves
        };
        // A Python project with the requirements file of before is told of it, after the rule for the project
        let said = moves("python", &["requirements-dev.txt"], LOCKED);
        assert_eq!(said.len(), 2, "{said:#?}");
        assert_eq!(said[0], LOCKED_MOVES);
        assert_eq!(
            said[1],
            format!("requirements-dev.txt: {LOCKED_REQUIREMENTS}")
        );
        // One without it is told the first only, and another stack nothing
        assert_eq!(moves("python", &[], LOCKED), [LOCKED_MOVES]);
        assert!(moves("rust", &[], LOCKED).is_empty());
        assert!(moves("none", &[], LOCKED).is_empty());
        // Not before the version it comes with
        assert!(moves("python", &["requirements-dev.txt"], "2.0.0-rc.1").is_empty());
    }
}

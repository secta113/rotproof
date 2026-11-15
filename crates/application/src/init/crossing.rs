//! What `rotproof init` tells a project that an upgrade crosses a table that changed what the layers are
//! (`domain::crossing` names the versions and the words): where the code of each retired layer goes, and what the
//! screens move out of `ui`. It reads the tree to say which of it applies, and moves nothing.

use crate::create::ui_in_tree;
use domain::crossing::{
    ONION, PORTS_MOVED, PRESENTATION, PRESENTATION_ADDED, UI_NONE, UI_SPLIT, crosses, retired,
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
    if crosses(PRESENTATION, from, running) {
        moves.extend(presentation_moves(tree, declared));
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
}

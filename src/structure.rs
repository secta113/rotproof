//! The structure check: the tree agrees with `.config/rotproof.toml`, either way.
//!
//! - Every layer of the stack's layout is present or declared absent, and no layer declared absent is present.
//! - No code sits outside the layers, except in the stack's paths that are not layers (`tests/`) and the paths the
//!   project lists in `unchecked`. A path in `unchecked` exists and holds no layer, so a layer cannot be switched off by
//!   listing it.
//! - `ui` holds only its levels: code in it beside them fails, except the files the layout makes for `ui` itself. A
//!   part there would have no place in the order of levels, and an import through it (`atoms → ui/helpers.py →
//!   domain`) would pass every direct check of the direction.
//!
//! The floor: at least one layer is present. A project with every layer declared absent would check nothing. A
//! repository that keeps records only declares `stack = "none"`, and the check says that it skipped the layers.

use std::collections::BTreeSet;
use std::io;

use crate::application::layers::declaration;
use crate::application::tree::{code_files, exactly};
use crate::domain::layers::{DECLARATION, Declared, Layout, MISSING, Place};
use crate::domain::tree::Tree;
use utils::source::within;

/// What the structure check found.
#[derive(Debug, Default)]
pub struct Structure {
    /// Every way the tree differs from its declaration
    pub found: Vec<String>,
    /// What was not checked, and why. Printed on every run, so a skipped check is never silent
    pub skipped: Option<String>,
    /// The declaration with its layout, when it can be read and fits, for the checks that follow
    pub declared: Option<Declared>,
}

/// Every way `tree` differs from its declaration. An error is a file or directory that could not be read.
pub fn problems(tree: &dyn Tree) -> io::Result<Structure> {
    let failed = |found: Vec<String>| {
        Ok(Structure {
            found,
            skipped: None,
            declared: None,
        })
    };
    let declared = match declaration(tree)? {
        None => return failed(vec![MISSING.into()]),
        Some(Err(why)) => return failed(vec![format!("{DECLARATION}: {why}")]),
        Some(Ok(declaration)) => match Declared::new(declaration) {
            Ok(declared) => declared,
            Err(found) => return failed(found),
        },
    };
    let Some(layout) = &declared.layout else {
        return Ok(Structure {
            found: Vec::new(),
            skipped: Some(format!(
                "the layers are not checked: {DECLARATION} declares stack = \"none\" (records only)"
            )),
            declared: Some(declared),
        });
    };
    let mut found = Vec::new();
    // By the exact name: `Domain/` is `domain/` on Windows, and a directory of its own on Linux and GitHub
    let present = |path: &str| exactly(tree, path).is_ok();

    let mut any = false;
    for place in &declared.places {
        // A level is judged only when its layer is there and not declared absent: otherwise the layer's own finding
        // says it all
        if let Some(parent) = &place.parent {
            let parent = declared.places.iter().find(|p| &p.name == parent).unwrap();
            if declared.is_absent(parent) || !present(&parent.path) {
                continue;
            }
        }
        match (declared.is_absent(place), present(&place.path)) {
            (true, true) => found.push(format!(
                "{} is declared absent, but {}/ exists: remove one or the other",
                place.name, place.path
            )),
            (false, false) => found.push(match exactly(tree, &place.path) {
                // A directory in another case: `rotproof create` would write into it on Windows, and fix nothing
                Err(why) if why.contains(" is there") => format!("{} is {why}", place.name),
                _ => format!(
                    "{} is missing: {}/ (run `rotproof create`, or declare it in absent in {DECLARATION})",
                    place.name, place.path
                ),
            }),
            (false, true) => any = true,
            (true, false) => {}
        }
    }
    if !any {
        found.push(format!(
            "no layer is present: the {} layout has {}",
            declared.declaration.stack,
            declared
                .places
                .iter()
                .filter(|p| p.parent.is_none())
                .map(|p| format!("{}/", p.path))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    let mut skipped: Vec<&str> = layout.not_layers.iter().map(String::as_str).collect();
    for path in &declared.declaration.unchecked {
        let path = path.trim_end_matches('/');
        if path.contains('\\') {
            found.push(format!(
                "unchecked lists {path:?}: separate a path with /, which every platform reads"
            ));
        } else if path.starts_with('/')
            || path.contains(':')
            || path.split('/').any(|part| matches!(part, "" | "." | ".."))
        {
            // `./scripts` would never equal the paths Rotproof compares it with, and would switch nothing off silently
            found.push(format!(
                "unchecked lists {path:?}: write a path from the root, such as scripts or src/generated"
            ));
        } else if let Some(place) = declared
            .places
            .iter()
            .find(|p| within(&p.path, path) || within(path, &p.path))
        {
            found.push(format!(
                "unchecked lists {path}, which holds or sits in the layer {} ({}/): a layer cannot be switched off",
                place.name, place.path
            ));
        } else if !present(path) {
            found.push(format!(
                "unchecked lists {path}, which does not exist: remove it"
            ));
        } else {
            skipped.push(path);
        }
    }
    // A file a starter puts outside the layers is its layer's while that layer is there; without the layer, it is code
    // outside the layers like any other
    for (path, layer) in &layout.belongs {
        if declared
            .places
            .iter()
            .any(|p| &p.name == layer && !declared.is_absent(p) && present(&p.path))
        {
            skipped.push(path);
        }
    }
    let places: Vec<&str> = declared.places.iter().map(|p| p.path.as_str()).collect();
    let goes = layout
        .where_code_goes
        .as_ref()
        .map_or(",".to_string(), |goes| format!(": {goes};"));
    for outside in outside(tree, layout, &places, &skipped)? {
        found.push(format!(
            "code outside the layers: {outside} (move it into a layer{goes} or list it in unchecked in {DECLARATION})"
        ));
    }
    for layer in declared.places.iter().filter(|p| p.parent.is_none()) {
        let levels: Vec<&Place> = declared
            .places
            .iter()
            .filter(|p| p.parent.as_ref() == Some(&layer.name))
            .collect();
        if levels.is_empty() || declared.is_absent(layer) || !present(&layer.path) {
            continue;
        }
        for beside in beside_levels(tree, layout, layer, &levels)? {
            found.push(format!(
                "code in {} outside its levels: {beside} (move it into a level or a layer: {})",
                layer.name,
                where_ui_parts_go(&declared.places)
            ));
        }
    }
    Ok(Structure {
        found,
        skipped: None,
        declared: Some(declared),
    })
}

/// The entries of the layout's scope that hold code outside `places` and `skipped`: a file, or the directory directly
/// in the scope that holds it. Files the project's `.gitignore` files exclude, and hidden ones, are not looked at: see
/// [`code_files`].
fn outside(
    tree: &dyn Tree,
    layout: &Layout,
    places: &[&str],
    skipped: &[&str],
) -> io::Result<BTreeSet<String>> {
    let scope = &layout.scope;
    let mut found = BTreeSet::new();
    for path in code_files(tree, scope, |name| layout.is_code(name))? {
        if places.iter().chain(skipped).any(|p| within(&path, p)) {
            continue;
        }
        found.insert(first_entry(&path, scope));
    }
    Ok(found)
}

/// The entry directly in `dir` that holds `path`: the file itself, or the directory it sits in. Both are from the
/// root, and `dir` is "" for the root.
fn first_entry(path: &str, dir: &str) -> String {
    let inside = if dir.is_empty() {
        path
    } else {
        &path[dir.len() + 1..]
    };
    let first = inside.split('/').next().unwrap_or(inside);
    if dir.is_empty() {
        first.to_string()
    } else {
        format!("{dir}/{first}")
    }
}

/// The code in a layer with levels (`ui`) that sits beside its levels, as entries directly in the layer: the layer's
/// own files (`ui/__init__.py`) are not counted. Every level counts as a level here, absent or not: an absent level
/// that exists has a finding of its own.
fn beside_levels(
    tree: &dyn Tree,
    layout: &Layout,
    layer: &Place,
    levels: &[&Place],
) -> io::Result<BTreeSet<String>> {
    let mut found = BTreeSet::new();
    for path in code_files(tree, &layer.path, |name| layout.is_code(name))? {
        if layer.files.iter().any(|(own, _)| *own == path)
            || levels.iter().any(|level| within(&path, &level.path))
        {
            continue;
        }
        found.insert(first_entry(&path, &layer.path));
    }
    Ok(found)
}

/// Where each kind of part shared by the whole UI goes, for the message on code beside the levels. Few readers know
/// that Atomic Design counts parts that render nothing as atoms, so the message says it.
fn where_ui_parts_go(places: &[Place]) -> String {
    let path = |name: &str| {
        &places
            .iter()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("every layout with ui has {name}: a test reads them"))
            .path
    };
    format!(
        "a part that knows no project concept, visible or not (a design value, one behaviour, a provider of a theme), \
         goes in {}/; one that knows domain types, such as a provider of the signed-in user, in {}/ or above; one \
         that calls a use case in {}/; one that knows no UI framework in {}/",
        path("ui.atoms"),
        path("ui.organisms"),
        path("ui.pages"),
        path("utils"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::tree::fake::Fake;

    #[test]
    fn the_tree_is_read_through_the_port_alone() {
        // No file system: a declaration, two layers, one in another case, and code outside the layers
        let tree = Fake::new(&[
            (
                ".config/rotproof.toml",
                "stack = \"python\"\nareas = [\"a\"]\nabsent = [\"ui\", \"handler\", \"application\"]\n",
            ),
            ("domain/__init__.py", ""),
            ("Infrastructure/__init__.py", ""),
            ("utils/__init__.py", ""),
            ("scripts/run.py", ""),
        ]);
        let found = problems(&tree).unwrap().found;
        assert_eq!(
            found,
            [
                "infrastructure is missing: infrastructure (Infrastructure is there: names are compared exactly, as \
                 Linux and GitHub compare them)",
                "code outside the layers: Infrastructure (move it into a layer, or list it in unchecked in \
                 .config/rotproof.toml)",
                "code outside the layers: scripts (move it into a layer, or list it in unchecked in \
                 .config/rotproof.toml)",
            ]
        );
    }
}

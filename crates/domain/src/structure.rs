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
//!
//! The rules judge what the tree shows ([`Seen`]): whether each path [`named`] gives is there by its exact name, and
//! the code files of each directory [`listed`] gives.

use std::collections::{BTreeMap, BTreeSet};

use crate::layers::{DECLARATION, Declared, Layout, Place};
use utils::source::within;

/// What the structure check reads from the tree.
#[derive(Debug, Default)]
pub struct Seen {
    /// Each path [`named`] gives: `Ok` when an entry has that exact name, otherwise what to say
    pub named: BTreeMap<String, Result<(), String>>,
    /// The code files (by the layout's [`Layout::is_code`]) in each directory [`listed`] gives, from the root
    pub code: BTreeMap<String, Vec<String>>,
}

impl Seen {
    /// Whether an entry has the exact name `path`. By the exact name: `Domain/` is `domain/` on Windows, and a
    /// directory of its own on Linux and GitHub
    fn present(&self, path: &str) -> bool {
        self.named(path).is_ok()
    }

    fn named(&self, path: &str) -> &Result<(), String> {
        self.named
            .get(path)
            .unwrap_or_else(|| panic!("{path} is judged, so named gives it"))
    }

    fn code(&self, dir: &str) -> &[String] {
        self.code
            .get(dir)
            .unwrap_or_else(|| panic!("{dir} is judged, so listed gives it"))
    }
}

/// Every path the rules ask about by its exact name: each layer's, and each in `unchecked`.
pub fn named(declared: &Declared) -> Vec<String> {
    declared
        .places
        .iter()
        .map(|p| p.path.clone())
        .chain(unchecked(declared).map(String::from))
        .collect()
}

/// The paths in `unchecked`, as compared: without a `/` at the end.
fn unchecked(declared: &Declared) -> impl Iterator<Item = &str> {
    declared
        .declaration
        .unchecked
        .iter()
        .map(|path| path.trim_end_matches('/'))
}

/// Every directory whose code files the rules read, by what `seen` says of the paths [`named`] gives: the layout's
/// scope, then each present layer that has levels.
pub fn listed(declared: &Declared, layout: &Layout, seen: &Seen) -> Vec<String> {
    let mut dirs = vec![layout.scope.clone()];
    dirs.extend(
        with_levels(declared, seen)
            .into_iter()
            .map(|(layer, _)| layer.path.clone()),
    );
    dirs
}

/// Each layer with levels (`ui`) that is there and not declared absent, with its levels.
fn with_levels<'a>(declared: &'a Declared, seen: &Seen) -> Vec<(&'a Place, Vec<&'a Place>)> {
    let mut found = Vec::new();
    for layer in declared.places.iter().filter(|p| p.parent.is_none()) {
        let levels: Vec<&Place> = declared
            .places
            .iter()
            .filter(|p| p.parent.as_ref() == Some(&layer.name))
            .collect();
        if levels.is_empty() || declared.is_absent(layer) || !seen.present(&layer.path) {
            continue;
        }
        found.push((layer, levels));
    }
    found
}

/// Every way the tree, as `seen`, differs from the declaration of a project with layers.
pub fn problems(declared: &Declared, layout: &Layout, seen: &Seen) -> Vec<String> {
    let mut found = Vec::new();
    let mut any = false;
    for place in &declared.places {
        // A level is judged only when its layer is there and not declared absent: otherwise the layer's own finding
        // says it all
        if let Some(parent) = &place.parent {
            let parent = declared.places.iter().find(|p| &p.name == parent).unwrap();
            if declared.is_absent(parent) || !seen.present(&parent.path) {
                continue;
            }
        }
        match (declared.is_absent(place), seen.present(&place.path)) {
            (true, true) => found.push(format!(
                "{} is declared absent, but {}/ exists: remove one or the other",
                place.name, place.path
            )),
            (false, false) => found.push(match seen.named(&place.path) {
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
    for path in unchecked(declared) {
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
        } else if !seen.present(path) {
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
            .any(|p| &p.name == layer && !declared.is_absent(p) && seen.present(&p.path))
        {
            skipped.push(path);
        }
    }
    let places: Vec<&str> = declared.places.iter().map(|p| p.path.as_str()).collect();
    let goes = layout
        .where_code_goes
        .as_ref()
        .map_or(",".to_string(), |goes| format!(": {goes};"));
    for outside in outside(seen.code(&layout.scope), &layout.scope, &places, &skipped) {
        found.push(format!(
            "code outside the layers: {outside} (move it into a layer{goes} or list it in unchecked in {DECLARATION})"
        ));
    }
    for (layer, levels) in with_levels(declared, seen) {
        for beside in beside_levels(seen.code(&layer.path), layer, &levels) {
            found.push(format!(
                "code in {} outside its levels: {beside} (move it into a level or a layer: {})",
                layer.name,
                where_ui_parts_go(&declared.places)
            ));
        }
    }
    found
}

/// The entries of the layout's `scope` that hold code outside `places` and `skipped`, of the code files `code` in it:
/// a file, or the directory directly in the scope that holds it.
fn outside(code: &[String], scope: &str, places: &[&str], skipped: &[&str]) -> BTreeSet<String> {
    code.iter()
        .filter(|path| !places.iter().chain(skipped).any(|p| within(path, p)))
        .map(|path| first_entry(path, scope))
        .collect()
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

/// The code in a layer with levels (`ui`) that sits beside its levels, of the code files `code` in it, as entries
/// directly in the layer: the layer's own files (`ui/__init__.py`) are not counted. Every level counts as a level
/// here, absent or not: an absent level that exists has a finding of its own.
fn beside_levels(code: &[String], layer: &Place, levels: &[&Place]) -> BTreeSet<String> {
    code.iter()
        .filter(|path| {
            !(layer.files.iter().any(|(own, _)| own == *path)
                || levels.iter().any(|level| within(path, &level.path)))
        })
        .map(|path| first_entry(path, &layer.path))
        .collect()
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
    use crate::layers::parse_declaration;

    fn declared(text: &str) -> Declared {
        Declared::new(parse_declaration(text).unwrap()).unwrap()
    }

    #[test]
    fn ui_is_listed_only_while_it_is_there_and_not_absent() {
        let with_ui = declared("stack = \"python\"\nareas = []\nunchecked = [\"scripts/\"]\n");
        let layout = with_ui.layout.as_ref().unwrap();
        assert!(named(&with_ui).contains(&"ui/atoms".to_string()));
        assert!(named(&with_ui).contains(&"scripts".to_string()));
        let mut seen = Seen::default();
        for path in named(&with_ui) {
            seen.named.insert(path, Ok(()));
        }
        assert_eq!(listed(&with_ui, layout, &seen), ["", "ui"]);
        seen.named.insert("ui".into(), Err("missing: ui".into()));
        assert_eq!(listed(&with_ui, layout, &seen), [""]);
        let without_ui = declared("stack = \"python\"\nareas = []\nabsent = [\"ui\"]\n");
        assert_eq!(
            listed(
                &without_ui,
                without_ui.layout.as_ref().unwrap(),
                &Seen::default()
            ),
            [""]
        );
    }

    #[test]
    fn code_beside_the_levels_is_named_by_its_entry_in_the_layer() {
        let python = declared("stack = \"python\"\nareas = []\n");
        let layout = python.layout.as_ref().unwrap();
        let mut seen = Seen::default();
        for path in named(&python) {
            seen.named.insert(path, Ok(()));
        }
        seen.code.insert("".into(), Vec::new());
        seen.code.insert(
            "ui".into(),
            [
                "ui/__init__.py",
                "ui/atoms/a.py",
                "ui/helpers/x.py",
                "ui/y.py",
            ]
            .map(String::from)
            .into(),
        );
        let found = problems(&python, layout, &seen);
        assert_eq!(found.len(), 2, "{found:#?}");
        assert!(found[0].starts_with("code in ui outside its levels: ui/helpers (move it"));
        assert!(found[1].starts_with("code in ui outside its levels: ui/y.py (move it"));
    }
}

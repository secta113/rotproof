//! The structure check: the tree agrees with `.config/rotproof.toml`, either way.
//!
//! - Every layer of the stack's layout is present or declared absent, and no layer declared absent is present.
//!   `absent` names no layer the declared UI leaves out.
//! - No code sits outside the layers, except in the stack's paths that are not layers (`tests/`), the entry file and
//!   the paths the project lists in `unchecked`. Where code is looked for is the layout's `scope`, and its `also_code`
//!   (the root package's `src/` in Rust). A path in `unchecked` exists and holds no layer, so a layer cannot be
//!   switched off by listing it.
//! - A project that declares units is read unit by unit, and every finding about a layer, a view or code outside the
//!   layers names its unit (`domain of unit billing is missing: ...`); a project that declares none is one unit, and
//!   nothing names it. Each declared unit's path and entry files are there, each unit holds a layer, and the code of
//!   a unit is what is in its path outside its layers, the unit's own entry files, the paths that are not layers, what
//!   `unchecked` keeps and the paths of the units inside it (those are judged by their own). Code of the layout's
//!   scope that is in no unit's path fails as code outside the layers, so that a file moved out of every unit does not
//!   escape. A unit's own directory
//!   holds the code files the layout allows there (a Python package's `__init__.py`), unless its path is the root.
//!   A path in `unchecked` does not hold a unit, so a unit cannot be switched off either.
//! - Only when units are declared, a package's manifest (the layout's `manifest`) sits at the root of a unit or
//!   below it, at the root of the project, in a layer (a Rust layer is a crate), or in a path `unchecked` keeps; one
//!   anywhere else belongs to no unit, which says that a package of the project has none declared.
//! - A layer with levels (`ui`) holds only its levels: code in it beside them fails, except the files the layout
//!   makes for the layer itself. A part there would have no place among the levels, and an import through it
//!   (`atoms → ui/helpers.py → domain`) would pass every direct check of the direction.
//! - A layer the layout says holds no view (`presentation` in TypeScript) holds no file with the extension of a view
//!   (`.tsx`, `.jsx`), whatever its level: the extension is all that tells one, as the check reads imports, not what
//!   a file draws.
//!
//! The floor: at least one layer is present. A project with every layer declared absent would check nothing. A
//! repository that keeps records only declares `stack = "none"`, and the check says that it skipped the layers.
//!
//! The rules judge what the tree shows ([`Seen`]): whether each path [`named`] gives is there by its exact name, and
//! the code files of each directory [`listed`] gives.

use std::collections::{BTreeMap, BTreeSet};

use crate::crossing::{ONION, retired};
use crate::declaration::{DECLARATION, Declared};
use crate::layers::{Layout, Place};
use crate::paths::{Miswritten, file_name, inside, join, miswritten, parent, within};
use crate::units::Unit;

/// What the structure check reads from the tree.
#[derive(Debug, Default)]
pub struct Seen {
    /// Each path [`named`] gives: `Ok` when an entry has that exact name, otherwise what to say
    pub named: BTreeMap<String, Result<(), String>>,
    /// The code files (by the layout's [`Layout::is_code`]) in each directory [`listed`] gives, from the root
    pub code: BTreeMap<String, Vec<String>>,
    /// Every manifest (by the layout's [`Layout::is_manifest`]) in the tree, from the root: read only when the project
    /// declares units
    pub manifests: Vec<String>,
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

/// Every path the rules ask about by its exact name: each layer's, each in `unchecked`, and the path and the entry
/// files of each declared unit.
pub fn named(declared: &Declared) -> Vec<String> {
    let declared_units = declared.units.iter().filter(|unit| unit.declared);
    declared
        .places
        .iter()
        .map(|p| p.path.clone())
        .chain(declared.unchecked().map(String::from))
        .chain(declared_units.clone().map(|unit| unit.path.clone()))
        .chain(declared_units.flat_map(|unit| unit.entries.clone()))
        .collect()
}

/// Whether the project declares units, which the rules read unit by unit.
pub fn declares_units(declared: &Declared) -> bool {
    declared.units.iter().any(|unit| unit.declared)
}

/// Every directory whose code files the rules read, by what `seen` says of the paths [`named`] gives: the path of
/// each unit (the layout's scope for the implicit one, then the directories it also looks in), the layout's scope and
/// the directories it also looks in when the project declares units, whose code in no unit's path fails, then each
/// present layer that has levels.
pub fn listed(declared: &Declared, layout: &Layout, seen: &Seen) -> Vec<String> {
    let mut dirs: Vec<String> = Vec::new();
    for unit in &declared.units {
        dirs.push(unit.path.clone());
        if !unit.declared {
            dirs.extend(layout.also_code.iter().map(|also| also.scope.clone()));
        }
    }
    if declares_units(declared) {
        for scope in scopes(layout) {
            if !dirs.iter().any(|dir| dir == scope) {
                dirs.push(scope.to_string());
            }
        }
    }
    dirs.extend(
        with_levels(declared, seen)
            .into_iter()
            .map(|(layer, _)| layer.path.clone()),
    );
    dirs
}

/// The directories the layout looks in for code: its scope, then those it also looks in.
fn scopes(layout: &Layout) -> impl Iterator<Item = &str> {
    std::iter::once(layout.scope.as_str())
        .chain(layout.also_code.iter().map(|also| also.scope.as_str()))
}

/// Each layer with levels that is there and not declared absent, with its levels.
fn with_levels<'a>(declared: &'a Declared, seen: &Seen) -> Vec<(&'a Place, Vec<&'a Place>)> {
    let mut found = Vec::new();
    for layer in declared.places.iter().filter(|p| p.is_layer()) {
        let levels: Vec<&Place> = declared
            .places
            .iter()
            .filter(|p| p.is_level_of(layer))
            .collect();
        if levels.is_empty() || declared.is_absent(layer) || !seen.present(&layer.path) {
            continue;
        }
        found.push((layer, levels));
    }
    found
}

/// Every way the tree, as `seen`, differs from the declaration of a project with layers, in the order they are
/// printed: each place there or declared absent, how `unchecked` is written, code outside the layers, views in a layer
/// that holds none, code beside the levels of a layer, and the manifests that belong to no unit.
pub fn problems(declared: &Declared, layout: &Layout, seen: &Seen) -> Vec<String> {
    let mut found = presence(declared, layout, seen);
    let (unchecked, kept) = unchecked_paths(declared, seen);
    found.extend(unchecked);
    found.extend(outside_layers(declared, layout, seen, &kept));
    found.extend(views(declared, layout, seen));
    found.extend(outside_levels(declared, seen));
    found.extend(manifests(declared, seen));
    found
}

/// What a message adds to name the unit, as " of unit billing": nothing for the implicit unit, which no message names.
fn of_unit(name: &str) -> String {
    if name.is_empty() {
        String::new()
    } else {
        format!(" of unit {name}")
    }
}

/// Whether each unit is there, its entry files are, and each of its places is there or declared absent, and the floor
/// of each unit: some layer is present. A level is judged only while its layer is there and not declared absent. The
/// names `absent` holds that the declared UI leaves out come first. A unit that is not there says it all, and its
/// layers are not judged.
fn presence(declared: &Declared, layout: &Layout, seen: &Seen) -> Vec<String> {
    let mut found: Vec<String> = declared
        .left_out
        .iter()
        .map(|(unit, name)| {
            if unit.is_empty() {
                format!(
                    "absent names {name:?}, which ui = {:?} leaves out already: take it out of absent in {DECLARATION}",
                    layout.ui
                )
            } else {
                format!(
                    "absent of unit {unit} names {name:?}, which ui = {:?} leaves out already: take it out of its \
                     absent in {DECLARATION}",
                    layout.ui
                )
            }
        })
        .collect();
    for unit in &declared.units {
        let missing = unit.declared && !unit.path.is_empty() && !seen.present(&unit.path);
        if missing {
            found.push(match seen.named(&unit.path) {
                // A directory in another case, as for a layer
                Err(why) if why.contains(" is there") => format!("unit {} is {why}", unit.name),
                _ => format!(
                    "unit {} is missing: {}/ (make it with rotproof create, or take the unit out of {DECLARATION})",
                    unit.name, unit.path
                ),
            });
        }
        found.extend(missing_entries(unit, seen));
        if !missing {
            found.extend(places_of(declared, unit, seen));
        }
    }
    found
}

/// The entry files a declared unit names that are not there. The implicit unit's entry is optional, as it was before.
fn missing_entries(unit: &Unit, seen: &Seen) -> Vec<String> {
    let entries = unit.entries.iter().filter(|_| unit.declared);
    entries
        .filter(|entry| !seen.present(entry))
        .map(|entry| {
            // A file in another case: the parenthesis of what `exactly` says
            let note = match seen.named(entry) {
                Err(why) if why.contains(" is there") => why
                    .split_once(" (")
                    .map(|(_, rest)| format!(" ({rest}"))
                    .unwrap_or_default(),
                _ => String::new(),
            };
            format!("the entry {entry} of unit {} is missing{note}", unit.name)
        })
        .collect()
}

/// Whether each place of `unit` is there or declared absent, and its floor: some layer is present.
fn places_of(declared: &Declared, unit: &Unit, seen: &Seen) -> Vec<String> {
    let of = of_unit(&unit.name);
    let places: Vec<&Place> = declared
        .places
        .iter()
        .filter(|p| p.unit.name == unit.name)
        .collect();
    let mut found = Vec::new();
    let mut any = false;
    for place in &places {
        // A level is judged only when its layer is there and not declared absent: otherwise the layer's own finding
        // says it all
        if let Some(parent) = &place.parent {
            let parent = places
                .iter()
                .find(|p| p.is_layer() && &p.name == parent)
                .unwrap();
            if declared.is_absent(parent) || !seen.present(&parent.path) {
                continue;
            }
        }
        match (declared.is_absent(place), seen.present(&place.path)) {
            (true, true) => found.push(format!(
                "{}{of} is declared absent, but {}/ exists: remove one or the other",
                place.name, place.path
            )),
            (false, false) => found.push(match seen.named(&place.path) {
                // A directory in another case: `rotproof create` would write into it on Windows, and fix nothing
                Err(why) if why.contains(" is there") => format!("{}{of} is {why}", place.name),
                _ if unit.declared => format!(
                    "{}{of} is missing: {}/ (run `rotproof create`, or name it in the absent of the unit in \
                     {DECLARATION})",
                    place.name, place.path
                ),
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
            "no layer is present{}: the {} layout has {}",
            if unit.declared {
                format!(" in unit {}", unit.name)
            } else {
                String::new()
            },
            declared.declaration.stack,
            places
                .iter()
                .filter(|p| p.is_layer())
                .map(|p| format!("{}/", p.path))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    found
}

/// How `unchecked` is written, one finding for each path that is not fit to be read as it is, and the paths that are:
/// the ones that switch the check of code off.
fn unchecked_paths<'a>(declared: &'a Declared, seen: &Seen) -> (Vec<String>, Vec<&'a str>) {
    let mut found = Vec::new();
    let mut kept = Vec::new();
    for path in declared.unchecked() {
        if let Some(why) = miswritten(path) {
            found.push(match why {
                Miswritten::Backslash => format!(
                    "unchecked lists {path:?}: separate a path with /, which every platform reads"
                ),
                // `./scripts` would never equal the paths Rotproof compares it with, and would switch nothing off
                // silently
                Miswritten::NotFromRoot => format!(
                    "unchecked lists {path:?}: write a path from the root, such as scripts or src/generated"
                ),
            });
        } else if let Some(unit) = declared
            .units
            .iter()
            .find(|unit| unit.declared && within(&unit.path, path))
        {
            // What sits in a unit's path outside its layers is the code `unchecked` is for; the unit itself is not
            found.push(format!(
                "unchecked lists {path}, which holds the unit {} ({}/): a unit cannot be switched off",
                unit.name, unit.path
            ));
        } else if let Some(place) = declared.overlapped(path) {
            found.push(format!(
                "unchecked lists {path}, which holds or sits in the layer {} ({}/): a layer cannot be switched off",
                place.name, place.path
            ));
        } else if !seen.present(path) {
            found.push(format!(
                "unchecked lists {path}, which does not exist: remove it"
            ));
        } else {
            kept.push(path);
        }
    }
    (found, kept)
}

/// The code outside the layers, one finding for each entry of each unit's path (of the layout's scope, and of each
/// directory it also looks in, for the implicit unit) that holds some. Not counted: the paths that are not layers
/// (`tests/`), the unit's own entry files, `unchecked_paths`, the ones `unchecked` keeps, the paths of the units inside
/// this one, which are judged by their own, and the files the layout lets a unit's own directory hold (a Python
/// package's `__init__.py`), which a unit at the root has none of.
fn outside_layers(
    declared: &Declared,
    layout: &Layout,
    seen: &Seen,
    unchecked: &[&str],
) -> Vec<String> {
    let mut found = Vec::new();
    let retired = retired(layout);
    for unit in &declared.units {
        let of = of_unit(&unit.name);
        let places: Vec<&str> = declared
            .places
            .iter()
            .filter(|p| p.unit.name == unit.name)
            .map(|p| p.path.as_str())
            .collect();
        let mut skipped: Vec<String> = layout.not_layers_in(&unit.path);
        skipped.extend(unchecked.iter().map(|path| path.to_string()));
        // The entry file is no layer, and code all the same: the direction check reads it
        skipped.extend(unit.entries.iter().cloned());
        skipped.extend(
            declared
                .units
                .iter()
                .filter(|other| other.path != unit.path && inside(&other.path, &unit.path))
                .map(|other| other.path.clone()),
        );
        let skipped: Vec<&str> = skipped.iter().map(String::as_str).collect();
        // Where code goes is said in the layout's paths, which are the project's own only for the implicit unit
        let goes = match (&layout.where_code_goes, unit.declared) {
            (Some(goes), false) => format!(": {goes};"),
            _ => ",".to_string(),
        };
        let mut scopes = vec![unit.path.as_str()];
        if !unit.declared {
            scopes.extend(layout.also_code.iter().map(|also| also.scope.as_str()));
        }
        let own = unit.declared && !unit.path.is_empty();
        for scope in scopes {
            let code: Vec<String> = seen
                .code(scope)
                .iter()
                .filter(|path| {
                    !(own && parent(path) == unit.path && layout.is_unit_file(file_name(path)))
                })
                .cloned()
                .collect();
            for outside in outside(&code, scope, &places, &skipped) {
                // A layer of the table before 0.4.0 says where its code goes, on every run until it is moved
                if let Some((name, _, where_it_goes)) = retired
                    .iter()
                    .find(|(_, path, _)| layout.moved(path, &unit.path) == outside)
                {
                    found.push(format!(
                        "code outside the layers{of}: {outside} (the layer {name} of the table before {ONION}, which \
                         is gone: {where_it_goes})"
                    ));
                    continue;
                }
                found.push(format!(
                    "code outside the layers{of}: {outside} (move it into a layer{goes} or list it in unchecked in \
                     {DECLARATION})"
                ));
            }
        }
    }
    found.extend(outside_units(declared, layout, seen, unchecked));
    found
}

/// The code of the layout's scope (and of the directories it also looks in) that sits in no unit's path, one finding
/// for each entry of the scope that holds some, when the project declares units: moving a file out of every unit would
/// otherwise escape every check. Not counted: a unit's entry files, the paths that are not layers (in the scope, and
/// in the root of each unit, where a package keeps its `tests/`) and what `unchecked` keeps. A unit at the root holds
/// all of it.
fn outside_units(
    declared: &Declared,
    layout: &Layout,
    seen: &Seen,
    unchecked: &[&str],
) -> Vec<String> {
    if !declares_units(declared) || declared.units.iter().any(|unit| unit.path.is_empty()) {
        return Vec::new();
    }
    let paths: Vec<&str> = declared
        .units
        .iter()
        .map(|unit| unit.path.as_str())
        .collect();
    let mut skipped: Vec<String> = layout.not_layers_in(&layout.scope);
    for unit in &declared.units {
        skipped.extend(layout.not_layers_in(&unit.root));
        skipped.extend(unit.entries.iter().cloned());
    }
    skipped.extend(unchecked.iter().map(|path| path.to_string()));
    let skipped: Vec<&str> = skipped.iter().map(String::as_str).collect();
    let mut found = Vec::new();
    for scope in scopes(layout) {
        for outside in outside(seen.code(scope), scope, &paths, &skipped) {
            found.push(format!(
                "code outside the layers: {outside} (it is in no unit: move it into a layer of one, or list it in \
                 unchecked in {DECLARATION})"
            ));
        }
    }
    found
}

/// The code in a layer with levels that sits beside them, one finding for each entry of the layer that holds some.
fn outside_levels(declared: &Declared, seen: &Seen) -> Vec<String> {
    let mut found = Vec::new();
    for (layer, levels) in with_levels(declared, seen) {
        // Where a part of the UI goes needs saying; elsewhere the levels say it by their names
        let goes = if layer.name == "ui" {
            where_ui_parts_go(&declared.places, &layer.unit.name)
        } else {
            let paths: Vec<String> = levels.iter().map(|l| format!("{}/", l.path)).collect();
            paths.join(" or ")
        };
        for beside in beside_levels(seen.code(&layer.path), layer, &levels) {
            found.push(format!(
                "code in {}{} outside its levels: {beside} (move it into a level or a layer: {goes})",
                layer.name,
                of_unit(&layer.unit.name),
            ));
        }
    }
    found
}

/// The files of a view in a layer that holds none, one finding each. A layer the project does not have, or declares
/// absent, is not judged: its own finding says it all.
fn views(declared: &Declared, layout: &Layout, seen: &Seen) -> Vec<String> {
    let Some(no_view) = &layout.no_view else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for unit in &declared.units {
        let of = of_unit(&unit.name);
        for name in &no_view.layers {
            let Some(layer) = declared
                .places
                .iter()
                .find(|p| &p.name == name && p.unit.name == unit.name)
            else {
                continue;
            };
            if declared.is_absent(layer) {
                continue;
            }
            let mut files: Vec<&String> = seen
                .code(&unit.path)
                .iter()
                .filter(|path| within(path, &layer.path) && no_view.holds(path))
                .collect();
            files.sort();
            for path in files {
                found.push(format!(
                    "a view in {name}{of}: {path} ({name} holds no view, so no .{} file sits there: {})",
                    no_view.extensions.join(" or ."),
                    no_view.goes
                ));
            }
        }
    }
    found
}

/// The manifests of the tree that belong to no unit, one finding each, when the project declares units: a package of
/// the project that no unit says it is. A manifest belongs to the unit whose root holds its directory, and is the
/// project's own at the root of the project (the workspace's), in a layer (a Rust layer is a crate of its own) and in
/// a path `unchecked` keeps.
fn manifests(declared: &Declared, seen: &Seen) -> Vec<String> {
    if !declares_units(declared) {
        return Vec::new();
    }
    seen.manifests
        .iter()
        .filter(|path| {
            let dir = parent(path);
            !(dir.is_empty()
                || declared
                    .units
                    .iter()
                    .any(|unit| unit.declared && inside(dir, &unit.root))
                || declared.places.iter().any(|p| within(dir, &p.path))
                || declared.is_unchecked(path))
        })
        .map(|path| {
            let dir = parent(path);
            format!(
                "{path} is a manifest outside every unit's root: declare the unit whose root is {dir} (root = \
                 \"{dir}\"), or list {dir} in unchecked in {DECLARATION}"
            )
        })
        .collect()
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
    join(dir, first)
}

/// The code in a layer with levels that sits beside its levels, of the code files `code` in it, as entries
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

/// Where each kind of part shared by the whole UI goes in the unit `unit`, for the message on code beside the levels.
/// Few readers know that Atomic Design counts parts that render nothing as atoms, so the message says it.
fn where_ui_parts_go(places: &[Place], unit: &str) -> String {
    let path = |name: &str| {
        &places
            .iter()
            .find(|p| p.name == name && p.unit.name == unit)
            .unwrap_or_else(|| panic!("every layout with ui has {name}: a test reads them"))
            .path
    };
    format!(
        "a part that knows no project concept, visible or not (a design value, one behaviour, a provider of a theme), \
         goes in {}/; one that shows the values of a screen, such as a provider of the signed-in user, in {}/ or \
         above; one that uses a model in {}/; what a screen holds and decides, which draws nothing, in {}/, and the \
         plain data it hands the view in {}/; anything else in the layer whose concept it speaks, or it comes from a \
         library",
        path("ui.atoms"),
        path("ui.organisms"),
        path("ui.pages"),
        path("presentation.models"),
        path("presentation.display"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declaration::parse_declaration;
    use crate::layers::layout;

    fn declared(text: &str) -> Declared {
        Declared::new(parse_declaration(text).unwrap()).unwrap()
    }

    #[test]
    fn a_layer_with_levels_is_listed_only_while_it_is_there_and_not_absent() {
        let with_ui = declared(
            "stack = \"python\"\nareas = []\nui = \"atomic\"\nunchecked = [\"scripts/\"]\n",
        );
        let layout = with_ui.layout.as_ref().unwrap();
        assert!(named(&with_ui).contains(&"ui/atoms".to_string()));
        assert!(named(&with_ui).contains(&"scripts".to_string()));
        let mut seen = Seen::default();
        for path in named(&with_ui) {
            seen.named.insert(path, Ok(()));
        }
        assert_eq!(
            listed(&with_ui, layout, &seen),
            ["", "ui", "presentation", "infrastructure", "protocol"]
        );
        seen.named.insert("ui".into(), Err("missing: ui".into()));
        assert_eq!(
            listed(&with_ui, layout, &seen),
            ["", "presentation", "infrastructure", "protocol"]
        );
        let without_ui =
            declared("stack = \"python\"\nareas = []\nui = \"atomic\"\nabsent = [\"ui\"]\n");
        let mut seen = Seen::default();
        for path in named(&without_ui) {
            seen.named.insert(path, Ok(()));
        }
        assert_eq!(
            listed(&without_ui, without_ui.layout.as_ref().unwrap(), &seen),
            ["", "presentation", "infrastructure", "protocol"]
        );
        // With no UI, neither is named at all
        let no_ui = declared("stack = \"python\"\nareas = []\nui = \"none\"\n");
        assert!(
            !named(&no_ui)
                .iter()
                .any(|p| p.starts_with("ui") || p.starts_with("presentation"))
        );
    }

    #[test]
    fn unchecked_is_read_path_by_path_and_keeps_the_ones_that_fit() {
        let declared = declared(
            "stack = \"python\"\nareas = []\nui = \"atomic\"\nunchecked = [\"a\\\\b\", \"./scripts\", \"domain/x\", \
             \"gone/\", \"scripts/\"]\n",
        );
        let mut seen = Seen::default();
        for path in named(&declared) {
            seen.named.insert(path, Ok(()));
        }
        seen.named
            .insert("gone".into(), Err("missing: gone".into()));
        let (found, kept) = unchecked_paths(&declared, &seen);
        assert_eq!(
            found,
            [
                "unchecked lists \"a\\\\b\": separate a path with /, which every platform reads",
                "unchecked lists \"./scripts\": write a path from the root, such as scripts or src/generated",
                "unchecked lists domain/x, which holds or sits in the layer domain (domain/): a layer cannot be \
                 switched off",
                "unchecked lists gone, which does not exist: remove it",
            ]
        );
        assert_eq!(kept, ["scripts"]);
    }

    #[test]
    fn absent_naming_a_layer_the_ui_leaves_out_fails() {
        let left = declared("stack = \"python\"\nareas = []\nui = \"none\"\nabsent = [\"ui\"]\n");
        let layout = left.layout.as_ref().unwrap();
        let mut seen = Seen::default();
        for path in named(&left) {
            seen.named.insert(path, Ok(()));
        }
        for dir in listed(&left, layout, &seen) {
            seen.code.insert(dir, Vec::new());
        }
        let found = problems(&left, layout, &seen);
        assert_eq!(
            found[0],
            "absent names \"ui\", which ui = \"none\" leaves out already: take it out of absent in \
             .config/rotproof.toml"
        );
    }

    #[test]
    fn a_view_in_a_layer_that_holds_none_is_named_file_by_file() {
        let seen_with = |declared: &Declared, files: &[&str]| {
            let mut seen = Seen::default();
            for path in named(declared) {
                seen.named.insert(path, Ok(()));
            }
            seen.code
                .insert("src".into(), files.iter().map(|f| f.to_string()).collect());
            seen
        };
        let files = [
            "src/presentation/models/b.tsx",
            "src/presentation/models/a.JSX",
            "src/presentation/models/a.ts",
            "src/presentation/index.ts",
            "src/ui/pages/home.tsx",
            "src/presentationx/y.tsx",
        ];
        let ts = declared("stack = \"typescript\"\nareas = []\nui = \"atomic\"\n");
        let layout = ts.layout.as_ref().unwrap();
        let found = views(&ts, layout, &seen_with(&ts, &files));
        assert_eq!(found.len(), 2, "{found:#?}");
        assert!(found[0].starts_with(
            "a view in presentation: src/presentation/models/a.JSX (presentation holds no view, so no .tsx or .jsx \
             file sits there: "
        ));
        assert!(found[1].contains("src/presentation/models/b.tsx"));
        // Declared absent, the layer's own finding says it all; with no UI it is not a layer
        let absent = declared(
            "stack = \"typescript\"\nareas = []\nui = \"atomic\"\nabsent = [\"presentation\"]\n",
        );
        let layout = absent.layout.as_ref().unwrap();
        assert!(views(&absent, layout, &seen_with(&absent, &files)).is_empty());
        let none = declared("stack = \"typescript\"\nareas = []\nui = \"none\"\n");
        let layout = none.layout.as_ref().unwrap();
        assert!(views(&none, layout, &seen_with(&none, &files)).is_empty());
        // A stack that tells a view by no extension judges nothing
        let python = declared("stack = \"python\"\nareas = []\nui = \"atomic\"\n");
        let layout = python.layout.as_ref().unwrap();
        assert!(layout.no_view.is_none());
        assert!(views(&python, layout, &seen_with(&python, &files)).is_empty());
    }

    #[test]
    fn code_beside_the_levels_is_named_by_its_entry_in_the_layer() {
        let python = declared("stack = \"python\"\nareas = []\nui = \"atomic\"\n");
        let layout = python.layout.as_ref().unwrap();
        let mut seen = Seen::default();
        for path in named(&python) {
            seen.named.insert(path, Ok(()));
        }
        // The entry file is code outside the layers that passes
        seen.code.insert("".into(), vec!["__main__.py".into()]);
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
        seen.code.insert(
            "infrastructure".into(),
            [
                "infrastructure/__init__.py",
                "infrastructure/inbound/cli.py",
                "infrastructure/shared.py",
            ]
            .map(String::from)
            .into(),
        );
        seen.code.insert("protocol".into(), Vec::new());
        seen.code.insert("presentation".into(), Vec::new());
        let found = problems(&python, layout, &seen);
        assert_eq!(found.len(), 3, "{found:#?}");
        assert!(found[0].starts_with("code in ui outside its levels: ui/helpers (move it"));
        assert!(found[1].starts_with("code in ui outside its levels: ui/y.py (move it"));
        assert_eq!(
            found[2],
            "code in infrastructure outside its levels: infrastructure/shared.py (move it into a level or a \
             layer: infrastructure/inbound/ or infrastructure/outbound/)"
        );
    }

    /// What a tree of `files` shows, as the application reads it: a path is there when a file is, or sits in it, and
    /// the code of a directory is the files in it that the layout calls code
    fn seen_of(declared: &Declared, files: &[&str]) -> Seen {
        let layout = declared.layout.as_ref().unwrap();
        let mut seen = Seen::default();
        for path in named(declared) {
            let there = path.is_empty() || files.iter().any(|f| within(f, &path));
            seen.named.insert(
                path.clone(),
                if there {
                    Ok(())
                } else {
                    Err(format!("missing: {path}"))
                },
            );
        }
        for dir in listed(declared, layout, &seen) {
            let code = files
                .iter()
                .filter(|f| inside(f, &dir) && layout.is_code_in(&dir, file_name(f)))
                .map(|f| f.to_string())
                .collect();
            seen.code.insert(dir, code);
        }
        seen.manifests = files
            .iter()
            .filter(|f| layout.is_manifest(file_name(f)))
            .map(|f| f.to_string())
            .collect();
        seen
    }

    /// The `__init__.py` of each layer and level of a Python unit at `path` with `ui = "none"`, but for `lacks`.
    fn python_unit(path: &str, lacks: &[&str]) -> Vec<String> {
        [
            "composition",
            "infrastructure",
            "infrastructure/inbound",
            "infrastructure/outbound",
            "application",
            "protocol",
            "protocol/inbound",
            "protocol/outbound",
            "domain",
        ]
        .into_iter()
        .filter(|layer| !lacks.iter().any(|l| layer.starts_with(l)))
        .map(|layer| join(path, &format!("{layer}/__init__.py")))
        .collect()
    }

    /// A modular monolith: a shared unit with a manifest of its own, a unit that is not shared, and an app whose
    /// entry file is outside its path.
    const MONOLITH: &str = "stack = \"python\"\nareas = []\nui = \"none\"\n\n[[unit]]\nname = \"core\"\n\
        path = \"packages/core/core\"\nroot = \"packages/core\"\nshared = true\n\n[[unit]]\nname = \"billing\"\n\
        path = \"modules/billing\"\n\n[[unit]]\nname = \"app\"\npath = \"modules/app\"\nentry = [\"bin/run.py\"]\n\
        absent = [\"domain\", \"application\", \"protocol\"]\n";

    /// The tree of [`MONOLITH`] with `extra` files added and `without` taken out.
    fn monolith(extra: &[&str], without: &[&str]) -> Vec<String> {
        let mut files = python_unit("packages/core/core", &[]);
        files.extend(python_unit("modules/billing", &[]));
        files.extend(python_unit(
            "modules/app",
            &["domain", "application", "protocol"],
        ));
        files.extend(
            [
                "pyproject.toml",
                "packages/core/pyproject.toml",
                "packages/core/core/__init__.py",
                "modules/billing/__init__.py",
                "bin/run.py",
            ]
            .map(String::from),
        );
        files.retain(|f| !without.contains(&f.as_str()));
        files.extend(extra.iter().map(|f| f.to_string()));
        files
    }

    fn judge(declaration: &str, files: &[String]) -> Vec<String> {
        let declared = declared(declaration);
        let files: Vec<&str> = files.iter().map(String::as_str).collect();
        let seen = seen_of(&declared, &files);
        problems(&declared, declared.layout.as_ref().unwrap(), &seen)
    }

    #[test]
    fn a_modular_monolith_whose_units_hold_their_layers_passes() {
        // The shared unit's own `__init__.py` and its manifest, the unit that has none of either, and an app whose
        // entry file sits outside its path, with the manifest of the project at the root
        let found = judge(MONOLITH, &monolith(&[], &[]));
        assert!(found.is_empty(), "{found:#?}");
    }

    #[test]
    fn a_unit_is_there_with_its_entry_and_its_layers_and_what_is_missing_names_its_unit() {
        let found = judge(
            MONOLITH,
            &monolith(
                &[],
                &[
                    "modules/billing/domain/__init__.py",
                    "modules/app/composition/__init__.py",
                    "bin/run.py",
                ],
            ),
        );
        assert_eq!(
            found,
            [
                "domain of unit billing is missing: modules/billing/domain/ (run `rotproof create`, or name it in \
                 the absent of the unit in .config/rotproof.toml)",
                "the entry bin/run.py of unit app is missing",
                "composition of unit app is missing: modules/app/composition/ (run `rotproof create`, or name it \
                 in the absent of the unit in .config/rotproof.toml)",
            ]
        );
        // A unit that is not there says it all: its layers are not judged, and no floor is added
        let files: Vec<String> = monolith(&[], &[])
            .into_iter()
            .filter(|f| !f.starts_with("modules/billing"))
            .collect();
        assert_eq!(
            judge(MONOLITH, &files),
            [
                "unit billing is missing: modules/billing/ (make it with rotproof create, or take the unit out of \
                 .config/rotproof.toml)"
            ]
        );
    }

    #[test]
    fn a_layer_in_the_absent_of_a_unit_is_not_missing_there_and_the_floor_is_that_of_each_unit() {
        let found = judge(
            MONOLITH,
            &monolith(&["modules/app/domain/__init__.py"], &[]),
        );
        assert_eq!(
            found,
            [
                "domain of unit app is declared absent, but modules/app/domain/ exists: remove one or the other"
            ]
        );
        // A unit with no layer at all has none, as the floor says of each unit and not of the project
        let files: Vec<String> = monolith(&[], &[])
            .into_iter()
            .filter(|f| !f.starts_with("modules/billing/") || f.ends_with("billing/__init__.py"))
            .collect();
        let found = judge(MONOLITH, &files);
        assert_eq!(found.iter().filter(|f| f.contains("no layer")).count(), 1);
        assert!(
            found.iter().any(|f| f.starts_with(
                "no layer is present in unit billing: the python layout has modules/billing/"
            )),
            "{found:#?}"
        );
    }

    #[test]
    fn the_levels_of_a_unit_are_judged_by_the_layer_of_that_unit() {
        // `ui` is in the first unit and absent in the second: the levels of the second are not found missing because
        // the first has the layer
        let declaration = "stack = \"python\"\nareas = []\nui = \"atomic\"\n\n[[unit]]\nname = \"a\"\npath = \"a\"\n\n\
            [[unit]]\nname = \"b\"\npath = \"b\"\nabsent = [\"ui\"]\n";
        let layers = [
            "composition",
            "infrastructure/inbound",
            "infrastructure/outbound",
            "application",
            "protocol/inbound",
            "protocol/outbound",
            "domain",
            "presentation/models",
            "presentation/display",
        ];
        let mut files: Vec<String> = Vec::new();
        for unit in ["a", "b"] {
            for layer in layers {
                files.push(format!("{unit}/{layer}/__init__.py"));
                if let Some((outer, _)) = layer.split_once('/') {
                    files.push(format!("{unit}/{outer}/__init__.py"));
                }
            }
        }
        let ui = [
            "ui",
            "ui/pages",
            "ui/templates",
            "ui/organisms",
            "ui/molecules",
            "ui/atoms",
        ];
        files.extend(ui.map(|dir| format!("a/{dir}/__init__.py")));
        assert_eq!(judge(declaration, &files), Vec::<String>::new());
        // And what a unit lacks is its own: the second without `ui.atoms`, which it names in no absent
        let declaration = declaration.replace("absent = [\"ui\"]", "absent = [\"ui.molecules\"]");
        files.extend(ui[..4].iter().map(|dir| format!("b/{dir}/__init__.py")));
        assert_eq!(
            judge(&declaration, &files),
            [
                "ui.atoms of unit b is missing: b/ui/atoms/ (run `rotproof create`, or name it in the absent of the \
                 unit in .config/rotproof.toml)"
            ]
        );
        // A layer that is not there in the second unit does not make its levels missing, though the first has them
        let without: Vec<String> = files
            .iter()
            .filter(|f| !f.starts_with("b/ui/"))
            .cloned()
            .collect();
        assert_eq!(
            judge(
                &declaration.replace("absent = [\"ui.molecules\"]", ""),
                &without
            ),
            [
                "ui of unit b is missing: b/ui/ (run `rotproof create`, or name it in the absent of the unit in .config/rotproof.toml)"
            ]
        );
    }

    #[test]
    fn the_names_a_unit_lists_that_the_ui_leaves_out_are_said_with_their_unit() {
        let declaration = "stack = \"python\"\nareas = []\nui = \"none\"\n\n[[unit]]\nname = \"a\"\npath = \"a\"\n\
            absent = [\"ui\"]\n\n[[unit]]\nname = \"b\"\npath = \"b\"\nabsent = [\"ui\"]\n";
        let mut files = python_unit("a", &[]);
        files.extend(python_unit("b", &[]));
        assert_eq!(
            judge(declaration, &files),
            [
                "absent of unit a names \"ui\", which ui = \"none\" leaves out already: take it out of its absent \
                 in .config/rotproof.toml",
                "absent of unit b names \"ui\", which ui = \"none\" leaves out already: take it out of its absent \
                 in .config/rotproof.toml",
            ]
        );
    }

    #[test]
    fn code_outside_the_layers_is_found_in_each_unit_and_stops_at_the_units_inside_it() {
        // The unit around, at the root, holds a unit inside it: what sits in the inner one is the inner one's to say
        let declaration = "stack = \"python\"\nareas = []\nui = \"none\"\n\n[[unit]]\nname = \"outer\"\npath = \".\"\n\n\
            [[unit]]\nname = \"inner\"\npath = \"lib/inner\"\n";
        let mut files = python_unit("", &[]);
        files.extend(python_unit("lib/inner", &[]));
        files.extend(["lib/inner/stray.py", "lib/other.py", "tests/test_a.py"].map(String::from));
        assert_eq!(
            judge(declaration, &files),
            [
                "code outside the layers of unit outer: lib (move it into a layer, or list it in unchecked in \
                 .config/rotproof.toml)",
                "code outside the layers of unit inner: lib/inner/stray.py (move it into a layer, or list it in \
                 unchecked in .config/rotproof.toml)",
            ]
        );
        // `unchecked` keeps what it names, and the inner unit's layers are no stray code of the outer one
        let kept = declaration.replace(
            "ui = \"none\"\n",
            "ui = \"none\"\nunchecked = [\"lib/inner/stray.py\", \"lib/other.py\"]\n",
        );
        assert_eq!(judge(&kept, &files), Vec::<String>::new());
    }

    #[test]
    fn a_unit_holds_its_own_init_unless_its_path_is_the_root_and_its_entry_is_no_stray_code() {
        let declaration = "stack = \"python\"\nareas = []\nui = \"none\"\n\n[[unit]]\nname = \"a\"\npath = \"a\"\n\
            entry = [\"a/main.py\"]\n";
        let mut files = python_unit("a", &[]);
        files.extend(["a/__init__.py", "a/main.py"].map(String::from));
        assert_eq!(judge(declaration, &files), Vec::<String>::new());
        // Only the name the layout says, and only directly in the unit
        files.extend(["a/__main__.py", "a/domain/stray.py"].map(String::from));
        assert_eq!(
            judge(declaration, &files),
            [
                "code outside the layers of unit a: a/__main__.py (move it into a layer, or list it in unchecked \
                 in .config/rotproof.toml)"
            ]
        );
        // A unit at the root has no such file: its `__init__.py` would make the whole project one package
        let at_root = "stack = \"python\"\nareas = []\nui = \"none\"\n\n[[unit]]\nname = \"a\"\npath = \".\"\n";
        let mut files = python_unit("", &[]);
        files.push("__init__.py".to_string());
        let found = judge(at_root, &files);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert!(found[0].starts_with("code outside the layers of unit a: __init__.py"));
    }

    #[test]
    fn a_unit_of_typescript_holds_no_index_of_its_own() {
        let declaration = "stack = \"typescript\"\nareas = []\nui = \"none\"\n\n[[unit]]\nname = \"billing\"\n\
            path = \"packages/billing/src\"\nroot = \"packages/billing\"\n";
        let mut files: Vec<String> = [
            "composition",
            "infrastructure",
            "infrastructure/inbound",
            "infrastructure/outbound",
            "application",
            "protocol",
            "protocol/inbound",
            "protocol/outbound",
            "domain",
        ]
        .iter()
        .map(|layer| format!("packages/billing/src/{layer}/index.ts"))
        .collect();
        files.push("packages/billing/package.json".to_string());
        assert_eq!(judge(declaration, &files), Vec::<String>::new());
        files.push("packages/billing/src/index.ts".to_string());
        assert_eq!(
            judge(declaration, &files),
            [
                "code outside the layers of unit billing: packages/billing/src/index.ts (move it into a layer, or \
                 list it in unchecked in .config/rotproof.toml)"
            ]
        );
    }

    #[test]
    fn unchecked_does_not_switch_a_unit_off_but_keeps_the_code_in_its_path_outside_the_layers() {
        let files = monolith(&["modules/billing/helpers.py"], &[]);
        let listed = |path: &str| {
            MONOLITH.replace(
                "ui = \"none\"\n",
                &format!("ui = \"none\"\nunchecked = [{path:?}]\n"),
            )
        };
        assert_eq!(
            judge(&listed("modules/billing/helpers.py"), &files),
            Vec::<String>::new()
        );
        let found = judge(&listed("modules/billing"), &files);
        assert_eq!(
            found,
            [
                "unchecked lists modules/billing, which holds the unit billing (modules/billing/): a unit cannot \
                 be switched off",
                "code outside the layers of unit billing: modules/billing/helpers.py (move it into a layer, or list \
                 it in unchecked in .config/rotproof.toml)",
            ]
        );
        // A path above a unit holds it too
        let found = judge(&listed("modules"), &files);
        assert!(
            found[0].starts_with(
                "unchecked lists modules, which holds the unit billing (modules/billing/)"
            ),
            "{found:#?}"
        );
    }

    #[test]
    fn a_manifest_belongs_to_a_unit_by_its_root_and_one_elsewhere_is_a_package_no_unit_says_it_is()
    {
        // The workspace's at the root, the shared unit's at its root, below a root, in a layer, and under `unchecked`
        let files = monolith(
            &[
                "packages/core/docs/pyproject.toml",
                "modules/billing/domain/pyproject.toml",
                "tools/pyproject.toml",
            ],
            &[],
        );
        assert_eq!(
            judge(MONOLITH, &files),
            [
                "tools/pyproject.toml is a manifest outside every unit's root: declare the unit whose root is \
                 tools (root = \"tools\"), or list tools in unchecked in .config/rotproof.toml"
            ]
        );
        let kept = MONOLITH.replace(
            "ui = \"none\"\n",
            "ui = \"none\"\nunchecked = [\"tools\"]\n",
        );
        assert_eq!(judge(&kept, &files), Vec::<String>::new());
        // A project that declares no unit has no such rule: a manifest anywhere is no finding
        let one = "stack = \"python\"\nareas = []\nui = \"none\"\n";
        let mut files = python_unit("", &[]);
        files.push("tools/pyproject.toml".to_string());
        assert_eq!(judge(one, &files), Vec::<String>::new());
    }

    #[test]
    fn code_in_no_unit_fails_as_code_outside_the_layers_unless_it_is_kept() {
        let stray = [
            "helper.py",
            "scripts/a.py",
            "tests/t.py",
            "packages/core/tests/t.py",
        ];
        let files = monolith(&stray, &[]);
        let said = |what: &str| {
            format!(
                "code outside the layers: {what} (it is in no unit: move it into a layer of one, or list it in \
                 unchecked in .config/rotproof.toml)"
            )
        };
        // The entry file of the app (`bin/run.py`), the paths that are no layer, in the scope and in a unit's root
        // (`tests/`), and the units' own code pass
        assert_eq!(
            judge(MONOLITH, &files),
            [said("helper.py"), said("scripts")]
        );
        let kept = MONOLITH.replace(
            "ui = \"none\"\n",
            "ui = \"none\"\nunchecked = [\"helper.py\", \"scripts\"]\n",
        );
        assert_eq!(judge(&kept, &files), Vec::<String>::new());
        // A unit at the root holds every path, so none is in no unit
        let at_root = "stack = \"python\"\nareas = []\nui = \"none\"\n\n[[unit]]\nname = \"a\"\npath = \".\"\n";
        let mut files = python_unit("", &[]);
        files.push("helper.py".to_string());
        assert_eq!(
            judge(at_root, &files),
            [
                "code outside the layers of unit a: helper.py (move it into a layer, or list it in unchecked in \
              .config/rotproof.toml)"
            ]
        );
    }

    #[test]
    fn code_outside_every_unit_is_found_in_the_scope_of_typescript_and_the_root_package_of_rust() {
        let declaration = "stack = \"typescript\"\nareas = []\nui = \"none\"\n\n[[unit]]\nname = \"billing\"\n\
            path = \"packages/billing/src\"\nroot = \"packages/billing\"\n";
        let mut files: Vec<String> = [
            "composition",
            "infrastructure",
            "infrastructure/inbound",
            "infrastructure/outbound",
            "application",
            "protocol",
            "protocol/inbound",
            "protocol/outbound",
            "domain",
        ]
        .iter()
        .map(|layer| format!("packages/billing/src/{layer}/index.ts"))
        .collect();
        // `src/` of the repository is the scope, and no unit is in it
        files.push("src/stray.ts".to_string());
        assert_eq!(
            judge(declaration, &files),
            [
                "code outside the layers: src/stray.ts (it is in no unit: move it into a layer of one, or list it in \
                 unchecked in .config/rotproof.toml)"
            ]
        );
        // Rust: the layers are crates of a unit, and the root package's `src/` holds the entry of an app only
        let declaration = "stack = \"rust\"\nareas = []\nui = \"none\"\n\n[[unit]]\nname = \"billing\"\n\
            path = \"crates/billing\"\nentry = [\"src/main.rs\"]\n";
        let mut files: Vec<String> = [
            "composition",
            "infrastructure",
            "application",
            "protocol",
            "domain",
        ]
        .iter()
        .map(|layer| format!("crates/billing/{layer}/Cargo.toml"))
        .collect();
        files.extend(
            [
                "crates/billing/infrastructure/src/inbound/mod.rs",
                "crates/billing/infrastructure/src/outbound/mod.rs",
                "crates/billing/protocol/src/inbound/mod.rs",
                "crates/billing/protocol/src/outbound/mod.rs",
                "Cargo.toml",
                "src/main.rs",
            ]
            .map(String::from),
        );
        assert_eq!(judge(declaration, &files), Vec::<String>::new());
        files.extend(["src/pricing.rs", "crates/helper/Cargo.toml"].map(String::from));
        assert_eq!(
            judge(declaration, &files),
            [
                "code outside the layers: crates/helper (it is in no unit: move it into a layer of one, or list it in \
                 unchecked in .config/rotproof.toml)",
                "code outside the layers: src/pricing.rs (it is in no unit: move it into a layer of one, or list it \
                 in unchecked in .config/rotproof.toml)",
                // A crate in no unit is a package no unit says it is, too
                "crates/helper/Cargo.toml is a manifest outside every unit's root: declare the unit whose root is \
                 crates/helper (root = \"crates/helper\"), or list crates/helper in unchecked in \
                 .config/rotproof.toml",
            ]
        );
    }

    #[test]
    fn the_manifest_of_a_stack_is_the_one_its_toolchain_reads() {
        for (stack, manifest) in [
            ("python", "pyproject.toml"),
            ("typescript", "package.json"),
            ("rust", "Cargo.toml"),
        ] {
            let layout = layout(stack, "none").unwrap();
            assert!(layout.is_manifest(manifest), "{stack}");
            assert!(
                layout.is_manifest(&manifest.to_ascii_uppercase()),
                "{stack}"
            );
            // Only Python has a file of its own for a unit's directory
            assert_eq!(layout.unit_files.is_empty(), stack != "python", "{stack}");
        }
    }
}

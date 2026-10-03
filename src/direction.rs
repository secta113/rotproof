//! The direction check: every layer imports only what the table (`layers/table.toml`) allows.
//!
//! - An import from layer A of layer B fails unless B is in A's `imports`. Inside `ui`, a level may import the levels
//!   below it and the layers in its own `imports`, and not `ui` beside its levels. A place may always import itself,
//!   and a layer the levels inside it.
//! - Only direct imports are judged. What the table allows is closed under chaining (a test keeps it so), so a chain
//!   of allowed imports never reaches what its first module may not import.
//! - An import of a module in no layer present (the standard library, a third-party package, a layer declared absent)
//!   is not judged. Imports built at run time (`importlib.import_module`, `__import__`) are not seen.
//!
//! For `python`, a module is named by its dotted name (`python.rs`), and sits in the place its parts start with. For
//! `typescript`, an import is resolved to a path (`typescript.rs`), and sits in the place that path starts with. For
//! `rust`, the check says that it did not run.

use std::io;
use std::path::Path;

use crate::layers::{Declared, Entry, Layout, Place, Table, listed, table};
use crate::python::module_parts;
use crate::source::{code_files, read_code};

/// What the direction check found.
#[derive(Debug, Default)]
pub struct Direction {
    /// Every import a layer may not make, and every file that could not be read
    pub found: Vec<String>,
    /// What was not checked, and why
    pub skipped: Option<String>,
}

/// Every import in the layers of `declared` that the table does not allow. An error is a directory that could not be
/// walked.
pub fn problems(root: &Path, declared: &Declared) -> io::Result<Direction> {
    let Some(layout) = &declared.layout else {
        // A repository of records only: the structure check already says that the layers were not checked
        return Ok(Direction::default());
    };
    // Only the places that are there: an import of a layer declared absent names a module the project does not have
    let places: Vec<&Place> = declared
        .places
        .iter()
        .filter(|p| !declared.is_absent(p) && crate::source::exactly(root, &p.path).is_ok())
        .collect();
    match declared.declaration.stack.as_str() {
        "python" => python(root, layout, &places),
        "typescript" => typescript(root, layout, &places),
        stack => Ok(Direction {
            found: Vec::new(),
            skipped: Some(format!(
                "the direction of imports is not checked: Rotproof does not read the imports of a {stack} project yet"
            )),
        }),
    }
}

/// The finding for an import from `from` that lands in `to`, or `None` when the table allows it.
fn judged(table: &Table, at: &str, what: &str, from: &Place, to: &Place) -> Option<String> {
    (!allowed(table, from, to)).then(|| {
        format!(
            "{at}: imports {what}, {}; {}",
            described(from, to),
            what_it_may_import(table, from)
        )
    })
}

/// The direction check of a TypeScript project: every import of a source file in a layer, resolved to a path
/// (`typescript.rs`), judged by the place that path sits in.
fn typescript(root: &Path, layout: &Layout, places: &[&Place]) -> io::Result<Direction> {
    let table = table();
    let aliases = crate::typescript::aliases(root)?;
    let mut found: Vec<String> = aliases
        .problems
        .iter()
        .map(|why| format!("{why}; imports through it are not checked"))
        .collect();
    let parts = |path: &str| -> Vec<String> { path.split('/').map(String::from).collect() };
    // Every source file of the layers with the place it sits in, and each file a starter puts outside the layers with
    // the layer it belongs to, while that layer is there
    let mut files: Vec<(String, &Place)> = Vec::new();
    for layer in places.iter().filter(|p| p.parent.is_none()) {
        for path in code_files(root, &layer.path, |name| layout.is_code(name))? {
            if let Some(from) = place_of(&parts(&path), places) {
                files.push((path, from));
            }
        }
    }
    for (path, layer) in &layout.belongs {
        let place = places.iter().find(|p| &p.name == layer);
        if let Some(place) = place
            && crate::source::exactly(root, path).is_ok_and(|full| full.is_file())
        {
            files.push((path.clone(), place));
        }
    }
    for (path, from) in files {
        if !crate::typescript::is_source(&path) {
            continue;
        }
        let Some(source) = read_code(root, &path, "imports", &mut found)? else {
            continue;
        };
        let read = crate::typescript::read(&source, &path);
        if let Some((line, why)) = read.error {
            found.push(format!(
                "{path}:{line}: cannot be read as TypeScript ({why}), so the imports after it may be misread"
            ));
        }
        for (line, specifier) in read.imports {
            let Some(target) = aliases.resolve(root, &path, &specifier) else {
                continue;
            };
            let Some(to) = place_of(&parts(&target), places) else {
                continue;
            };
            found.extend(judged(
                &table,
                &format!("{path}:{line}"),
                &format!("{specifier} ({target})"),
                from,
                to,
            ));
        }
    }
    Ok(Direction {
        found,
        skipped: None,
    })
}

/// The direction check of a Python project: every import of a `.py` file in a layer, as the module it names.
fn python(root: &Path, layout: &Layout, places: &[&Place]) -> io::Result<Direction> {
    let table = table();
    let mut found = Vec::new();
    for layer in places.iter().filter(|p| p.parent.is_none()) {
        for path in code_files(root, &layer.path, |name| layout.is_code(name))? {
            let Some(from) = place_of(&module_parts(&path), places) else {
                continue;
            };
            let Some(source) = read_code(root, &path, "imports", &mut found)? else {
                continue;
            };
            let read = crate::python::read(&source, &path);
            if let Some((line, why)) = read.error {
                found.push(format!(
                    "{path}:{line}: cannot be read as Python ({why}), so the imports after it are not checked"
                ));
            }
            for (line, module) in read.imports {
                let Some(to) = place_of(&module, places) else {
                    continue;
                };
                found.extend(judged(
                    &table,
                    &format!("{path}:{line}"),
                    &module.join("."),
                    from,
                    to,
                ));
            }
        }
    }
    Ok(Direction {
        found,
        skipped: None,
    })
}

/// The place a module sits in: the one whose path is the longest prefix of the module's parts.
fn place_of<'a>(module: &[String], places: &[&'a Place]) -> Option<&'a Place> {
    places
        .iter()
        .copied()
        .filter(|place| {
            let parts: Vec<&str> = place.path.split('/').collect();
            module.len() >= parts.len() && parts.iter().zip(module).all(|(a, b)| a == b)
        })
        .max_by_key(|place| place.path.len())
}

/// The table's entry for a place: a layer by its name, a level of `ui` by the part after the dot.
fn entry<'t>(table: &'t Table, place: &Place) -> &'t Entry {
    let (list, name) = match &place.parent {
        None => (&table.layers, place.name.as_str()),
        Some(_) => (
            &table.levels,
            place.name.rsplit('.').next().unwrap_or(&place.name),
        ),
    };
    list.iter()
        .find(|e| e.name == name)
        .unwrap_or_else(|| panic!("every place is in the table: {}", place.name))
}

/// The layer a place sits in: itself, or the layer of a level.
fn layer_name(place: &Place) -> &str {
    place.parent.as_deref().unwrap_or(&place.name)
}

/// Whether the place `from` may import the place `to`.
fn allowed(table: &Table, from: &Place, to: &Place) -> bool {
    if from.name == to.name {
        return true;
    }
    let imports = &entry(table, from).imports;
    match &from.parent {
        // A layer may import itself, the levels inside it included, and the layers in its imports with their levels
        None => layer_name(to) == from.name || imports.iter().any(|i| i == layer_name(to)),
        // A level may import the levels below it, and the layers in its imports; not its layer beside the levels
        Some(parent) => {
            if layer_name(to) != parent {
                return imports.iter().any(|i| i == layer_name(to));
            }
            if to.parent.is_none() {
                return false;
            }
            let rank = |place: &Place| {
                table
                    .levels
                    .iter()
                    .position(|l| entry(table, place).name == l.name)
            };
            rank(to) > rank(from)
        }
    }
}

/// The place an import lands in, for the message. A level that imports its own layer imports what sits beside the
/// levels, which has no place in their order.
fn described(from: &Place, to: &Place) -> String {
    if to.parent.is_none() && from.parent.as_deref() == Some(to.name.as_str()) {
        return format!("in {} outside its levels", to.name);
    }
    format!("in {}", to.name)
}

/// What a place may import, for the message.
fn what_it_may_import(table: &Table, place: &Place) -> String {
    let entry = entry(table, place);
    let below = place.parent.is_some()
        && table
            .levels
            .last()
            .is_some_and(|last| last.name != entry.name);
    match (below, entry.imports.is_empty()) {
        (false, true) => format!("{} imports no other layer", place.name),
        (false, false) => format!("{} may import {}", place.name, listed(&entry.imports)),
        (true, true) => format!("{} may import the levels below it", place.name),
        (true, false) => format!(
            "{} may import the levels below it and {}",
            place.name,
            listed(&entry.imports)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layers::layout;

    fn python_places() -> Vec<Place> {
        layout("python").unwrap().places(&table())
    }

    fn parts(dotted: &str) -> Vec<String> {
        dotted.split('.').map(String::from).collect()
    }

    #[test]
    fn what_the_table_allows_is_closed_under_chaining() {
        // Only direct imports are judged, so a chain of allowed imports must never reach what its first may not
        let table = table();
        let places = python_places();
        let mut chains = 0;
        for a in &places {
            for b in &places {
                for c in &places {
                    if allowed(&table, a, b) && allowed(&table, b, c) {
                        chains += 1;
                        assert!(
                            allowed(&table, a, c),
                            "{} may import {}, which may import {}, but {} may not import {}",
                            a.name,
                            b.name,
                            c.name,
                            a.name,
                            c.name
                        );
                    }
                }
            }
        }
        assert!(chains > places.len(), "{chains} chains");
    }

    #[test]
    fn a_module_sits_in_the_longest_place_it_starts_with() {
        let places = python_places();
        let places: Vec<&Place> = places.iter().collect();
        let name = |dotted: &str| place_of(&parts(dotted), &places).map(|p| p.name.clone());
        assert_eq!(name("ui.atoms.button"), Some("ui.atoms".into()));
        assert_eq!(name("ui.atoms"), Some("ui.atoms".into()));
        assert_eq!(name("ui.helpers"), Some("ui".into()));
        assert_eq!(name("ui"), Some("ui".into()));
        assert_eq!(name("domain.song"), Some("domain".into()));
        // Whole names only, and not the standard library
        assert_eq!(name("domains.song"), None);
        assert_eq!(name("ui_kit"), None);
        assert_eq!(name("os.path"), None);
    }

    #[test]
    fn the_message_says_what_a_place_may_import() {
        let table = table();
        let places = python_places();
        let say = |name: &str| {
            what_it_may_import(&table, places.iter().find(|p| p.name == name).unwrap())
        };
        assert_eq!(say("utils"), "utils imports no other layer");
        assert_eq!(say("domain"), "domain may import `utils`");
        assert_eq!(say("ui.atoms"), "ui.atoms may import `utils`");
        assert_eq!(
            say("ui.molecules"),
            "ui.molecules may import the levels below it and `utils`"
        );
    }
}

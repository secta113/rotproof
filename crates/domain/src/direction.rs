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
//! The layout's `language` says how the code is read. For `python`, a module is named by its dotted name
//! (`python.rs`), and sits in the place its parts start with. For `typescript`, an import is resolved to a path
//! (`typescript.rs`), and sits in the place that path starts with. For `rust`, a crate's code file is its
//! `Cargo.toml`, and each dependency it declares (`cargo.rs`) is resolved to the path it names, directly or through
//! its workspace, as the operating system resolves it (on Windows, another case, dots and spaces at the end and short
//! names too), and sits in the place it lands in. Cargo compiles a crate only against the crates it declares, so the
//! declarations are its imports.
//!
//! The rules here judge an import once it is resolved to a place; `application` reads the code and resolves it.

use crate::layers::{Place, listed};

/// The finding for an import from `from` that lands in `to`, or `None` when the table allows it. `what` says what
/// was imported, as "imports x" or "depends on x".
pub fn judged(at: &str, what: &str, from: &Place, to: &Place) -> Option<String> {
    (!allowed(from, to)).then(|| {
        format!(
            "{at}: {what}, {}; {}",
            described(from, to),
            what_it_may_import(from)
        )
    })
}

/// The directory of a path from the root ("" for the root).
pub fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

/// The path of `name` in `dir`, both from the root.
pub fn in_dir(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

/// The directories Cargo looks in for the workspace of a manifest in `dir` that does not name one: `dir`, then each
/// directory above it, up to the root ("").
pub fn workspace_dirs(dir: &str) -> Vec<String> {
    let mut dirs = vec![dir.to_string()];
    let mut at = dir;
    while !at.is_empty() {
        at = parent(at);
        dirs.push(at.to_string());
    }
    dirs
}

/// The place a module sits in: the one whose path is the longest prefix of the module's parts.
pub fn place_of<'a>(module: &[String], places: &[&'a Place]) -> Option<&'a Place> {
    places
        .iter()
        .copied()
        .filter(|place| {
            let parts: Vec<&str> = place.path.split('/').collect();
            module.len() >= parts.len() && parts.iter().zip(module).all(|(a, b)| a == b)
        })
        .max_by_key(|place| place.path.len())
}

/// The layer a place sits in: itself, or the layer of a level.
fn layer_name(place: &Place) -> &str {
    place.parent.as_deref().unwrap_or(&place.name)
}

/// Whether the place `from` may import the place `to`.
fn allowed(from: &Place, to: &Place) -> bool {
    if from.name == to.name {
        return true;
    }
    let imports = &from.imports;
    match &from.parent {
        // A layer may import itself, the levels inside it included, and the layers in its imports with their levels
        None => layer_name(to) == from.name || imports.iter().any(|i| i == layer_name(to)),
        // A level may import the levels below it, and the layers in its imports; not its layer beside the levels
        Some(parent) => {
            if layer_name(to) != parent {
                return imports.iter().any(|i| i == layer_name(to));
            }
            from.below.contains(&to.name)
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
fn what_it_may_import(place: &Place) -> String {
    match (place.below.is_empty(), place.imports.is_empty()) {
        (true, true) => format!("{} imports no other layer", place.name),
        (true, false) => format!("{} may import {}", place.name, listed(&place.imports)),
        (false, true) => format!("{} may import the levels below it", place.name),
        (false, false) => format!(
            "{} may import the levels below it and {}",
            place.name,
            listed(&place.imports)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layers::{layout, table};

    fn python_places() -> Vec<Place> {
        layout("python").unwrap().places(&table())
    }

    fn parts(dotted: &str) -> Vec<String> {
        dotted.split('.').map(String::from).collect()
    }

    #[test]
    fn what_the_table_allows_is_closed_under_chaining() {
        // Only direct imports are judged, so a chain of allowed imports must never reach what its first may not
        let places = python_places();
        let mut chains = 0;
        for a in &places {
            for b in &places {
                for c in &places {
                    if allowed(a, b) && allowed(b, c) {
                        chains += 1;
                        assert!(
                            allowed(a, c),
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
        let places = python_places();
        let say = |name: &str| what_it_may_import(places.iter().find(|p| p.name == name).unwrap());
        assert_eq!(say("utils"), "utils imports no other layer");
        assert_eq!(say("domain"), "domain may import `utils`");
        assert_eq!(say("ui.atoms"), "ui.atoms may import `utils`");
        assert_eq!(
            say("ui.molecules"),
            "ui.molecules may import the levels below it and `utils`"
        );
    }

    #[test]
    fn cargo_looks_for_a_workspace_from_the_crate_up_to_the_root() {
        assert_eq!(
            workspace_dirs("crates/domain"),
            ["crates/domain", "crates", ""]
        );
        assert_eq!(workspace_dirs(""), [""]);
    }
}

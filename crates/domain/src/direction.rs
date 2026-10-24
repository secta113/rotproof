//! The direction check: every layer imports only what the table (`layers/table.toml`) allows.
//!
//! - An import from layer A of layer B fails unless B is in A's `imports`. An entry of `imports` names a layer, which
//!   takes in its levels, or one level of another layer (`protocol.inbound`). A level may import what its own
//!   `imports` name, and the levels below it in an ordered layer (`ui`); not the levels beside it otherwise, nor its
//!   layer beside the levels. A place may always import itself, and a layer the levels inside it.
//! - Only direct imports are judged. What the table allows is closed under chaining (a test keeps it so), so a chain
//!   of allowed imports never reaches what its first module may not import.
//! - An import of a module in no layer present (the standard library, a third-party package, a layer declared absent)
//!   is not judged. Imports built at run time (`importlib.import_module`, `__import__`) are not seen.
//! - The entry file, which is no layer, is judged as a place of its own that may import what the table's `entry` lists.
//!   Nothing may import it.
//!
//! The layout's `language` says how the code is read. For `python`, a module is named by its dotted name
//! (`python.rs`), and its parts joined with `/` are its path. For `typescript`, an import is resolved to a path
//! (`typescript.rs`). For `rust`, a crate's code file is its `Cargo.toml`, and each dependency it declares
//! (`cargo.rs`) is resolved to the path it names, directly or through its workspace; Cargo compiles a crate only
//! against the crates it declares, so the declarations are its imports. The levels inside a crate are its modules,
//! judged from its code (`modules.rs`), and the entry file through the manifest of its package. In every language,
//! the path sits in the place it lands in as the operating system resolves it (on Windows, another case, dots and
//! spaces at the end and short names too), so a spelling that builds on the machine that runs the check is judged
//! there.
//!
//! The rules here judge an import once it is resolved to a place; `application` reads the code and resolves it.

use crate::layers::{Place, importable};
use utils::paths::{parent, within};

/// An import the table forbids: the file that imports, what it imports as the check names it (a Python dotted name,
/// `m.n` for `from m import n`; a TypeScript specifier; a crate's dependency), and the finding. The file and the
/// import are what an approval is keyed by (`approvals.rs`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Forbidden {
    pub file: String,
    pub import: String,
    pub detail: String,
}

/// The import of `import` by `file`, on `line`, that lands in `to`, when the table forbids it from `from`. `what`
/// says what was imported, as "imports x" or "depends on x".
pub fn judged(
    file: &str,
    line: usize,
    import: &str,
    what: &str,
    from: &Place,
    to: &Place,
) -> Option<Forbidden> {
    (!allowed(from, to)).then(|| Forbidden {
        file: file.to_string(),
        import: import.to_string(),
        detail: format!(
            "{file}:{line}: {what}, {}; {}",
            described(from, to),
            what_it_may_import(from)
        ),
    })
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

/// The place a path from the root sits in: the longest place it is in, by whole names. A Python module is placed by
/// its parts joined with `/`, as the path of its package.
pub fn place_of<'a>(path: &str, places: &[&'a Place]) -> Option<&'a Place> {
    places
        .iter()
        .copied()
        .filter(|place| within(path, &place.path))
        .max_by_key(|place| place.path.len())
}

/// The layer a place sits in: itself, or the layer of a level.
fn layer_name(place: &Place) -> &str {
    place.parent.as_deref().unwrap_or(&place.name)
}

/// Whether an entry of a place's imports takes in the place `to`: a layer with its levels, or one level.
fn covers(import: &str, to: &Place) -> bool {
    import == to.name || import == layer_name(to)
}

/// Whether the place `from` may import the place `to`.
fn allowed(from: &Place, to: &Place) -> bool {
    if from.name == to.name {
        return true;
    }
    let imported = |to: &Place| from.imports.iter().any(|i| covers(i, to));
    match &from.parent {
        // A layer may import itself, the levels inside it included, and what its imports name
        None => layer_name(to) == from.name || imported(to),
        // A level may import the levels below it, and what its imports name; not its layer beside the levels
        Some(parent) => {
            if layer_name(to) != parent {
                return imported(to);
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
    let importable = importable(place);
    if importable.is_empty() {
        format!("{} imports no other layer", place.name)
    } else {
        format!("{} may import {importable}", place.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layers::{layout, table};

    fn python_places() -> Vec<Place> {
        layout("python").unwrap().places(&table())
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
    fn levels_beside_each_other_never_import_each_other_and_an_import_may_name_one_level() {
        let table: crate::layers::Table = toml::from_str(
            r#"
            [entry]
            imports = ["edge"]
            role = "x"

            [[layer]]
            name = "edge"
            imports = ["port"]
            role = "x"
            [[layer.level]]
            name = "in"
            imports = ["port.in"]
            role = "x"
            [[layer.level]]
            name = "out"
            imports = ["port.out", "core"]
            role = "x"

            [[layer]]
            name = "port"
            imports = ["core"]
            role = "x"
            [[layer.level]]
            name = "in"
            imports = []
            role = "x"
            [[layer.level]]
            name = "out"
            imports = ["core"]
            role = "x"

            [[layer]]
            name = "core"
            imports = []
            role = "x"
            "#,
        )
        .unwrap();
        let places = layout("python").unwrap().places(&table);
        let place = |name: &str| places.iter().find(|p| p.name == name).unwrap();
        let may = |from: &str, to: &str| allowed(place(from), place(to));
        assert_eq!(place("port.out").path, "port/out");
        assert!(place("port.out").below.is_empty());
        // One level named takes in that level alone
        assert!(may("edge.in", "port.in"));
        assert!(!may("edge.in", "port.out") && !may("edge.in", "port") && !may("edge.in", "core"));
        // A layer named takes in its levels
        assert!(may("edge", "port.in") && may("edge", "port.out"));
        // Levels beside each other, in either direction, and the layer beside its levels
        assert!(!may("edge.in", "edge.out") && !may("edge.out", "edge.in"));
        assert!(!may("port.in", "port.out") && !may("port.out", "port"));
        assert!(may("edge.out", "port.out") && may("edge.out", "core"));
        assert!(may("port", "port.in"));
        assert_eq!(
            what_it_may_import(place("edge.in")),
            "edge.in may import `port.in`"
        );
    }

    #[test]
    fn a_module_sits_in_the_longest_place_it_starts_with() {
        let places = python_places();
        let places: Vec<&Place> = places.iter().collect();
        let name =
            |dotted: &str| place_of(&dotted.replace('.', "/"), &places).map(|p| p.name.clone());
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
        assert_eq!(say("domain"), "domain imports no other layer");
        assert_eq!(
            say("protocol.outbound"),
            "protocol.outbound may import `domain`"
        );
        assert_eq!(
            say("infrastructure.inbound"),
            "infrastructure.inbound may import `protocol.inbound`"
        );
        assert_eq!(say("ui.atoms"), "ui.atoms imports no other layer");
        assert_eq!(
            say("ui.organisms"),
            "ui.organisms may import the levels below it, and `domain`"
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

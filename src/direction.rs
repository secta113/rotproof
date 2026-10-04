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

use std::collections::BTreeMap;
use std::io;

use crate::code::{Manifest, Origin, Parsers, is_source, module_parts};
use crate::layers::{Declared, Language, Layout, Place, listed};
use crate::tree::{Tree, code_files, exactly, read_code};

/// Every import in the layers of `declared` that the table does not allow, and every file that could not be read. An
/// error is a directory that could not be walked.
pub fn problems(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    declared: &Declared,
) -> io::Result<Vec<String>> {
    let Some(layout) = &declared.layout else {
        // A repository of records only: the structure check already says that the layers were not checked
        return Ok(Vec::new());
    };
    // Only the places that are there: an import of a layer declared absent names a module the project does not have
    let places: Vec<&Place> = declared
        .places
        .iter()
        .filter(|p| !declared.is_absent(p) && exactly(tree, &p.path).is_ok())
        .collect();
    match layout.language {
        Language::Python => python(tree, parsers, layout, &places),
        Language::TypeScript => typescript(tree, parsers, layout, &places),
        Language::Rust => rust(tree, parsers, layout, &places),
    }
}

/// The finding for an import from `from` that lands in `to`, or `None` when the table allows it. `what` says what
/// was imported, as "imports x" or "depends on x".
fn judged(at: &str, what: &str, from: &Place, to: &Place) -> Option<String> {
    (!allowed(from, to)).then(|| {
        format!(
            "{at}: {what}, {}; {}",
            described(from, to),
            what_it_may_import(from)
        )
    })
}

/// The direction check of a Rust project: every dependency the manifest of a crate in a layer declares, judged by the
/// place the path it comes from sits in.
fn rust(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    layout: &Layout,
    places: &[&Place],
) -> io::Result<Vec<String>> {
    let mut found = Vec::new();
    let mut manifests = Manifests::new(parsers);
    let parts = |path: &str| -> Vec<String> { path.split('/').map(String::from).collect() };
    for layer in places.iter().filter(|p| p.parent.is_none()) {
        for path in code_files(tree, &layer.path, |name| layout.is_code(name))? {
            let Some(from) = place_of(&parts(&path), places) else {
                continue;
            };
            let Some(manifest) = manifests.read(tree, &path, &mut found)? else {
                continue;
            };
            let dir = parent(&path);
            for dependency in &manifest.dependencies {
                let at = format!("{path}:{}", dependency.line);
                let name = &dependency.name;
                let (target, through) = match &dependency.origin {
                    Origin::Path(rel) => (tree.landed(dir, rel), ""),
                    Origin::Workspace => {
                        let Some((workspace, entry)) =
                            manifests.workspace(tree, dir, &manifest, &mut found)?
                        else {
                            let looked = match &manifest.workspace {
                                Some(rel) => format!(
                                    "{rel}, which [package] workspace names, is no workspace"
                                ),
                                None => {
                                    format!("no Cargo.toml from {dir} up to the root declares one")
                                }
                            };
                            found.push(format!(
                                "{at}: {name} comes from the workspace, and {looked}, so it is not checked"
                            ));
                            continue;
                        };
                        let Some(rel) = entry.workspace_dependencies.get(name) else {
                            found.push(format!(
                                "{at}: {name} comes from the workspace, and {} has no {name} in \
                                 [workspace.dependencies], so it is not checked",
                                in_dir(&workspace, "Cargo.toml")
                            ));
                            continue;
                        };
                        // One from a registry or git names no place
                        let Some(rel) = rel else {
                            continue;
                        };
                        (tree.landed(&workspace, rel), ", from the workspace")
                    }
                };
                let Some(target) = target else {
                    continue;
                };
                let Some(to) = place_of(&parts(&target), places) else {
                    continue;
                };
                found.extend(judged(
                    &at,
                    &format!("depends on {name} ({target}{through})"),
                    from,
                    to,
                ));
            }
        }
    }
    Ok(found)
}

/// Every manifest read by the Rust check, by its path from the root, so a workspace is read once for all its members
/// and a manifest that cannot be read is said once.
struct Manifests<'p> {
    parsers: &'p dyn Parsers,
    read: BTreeMap<String, Option<Manifest>>,
}

impl<'p> Manifests<'p> {
    fn new(parsers: &'p dyn Parsers) -> Self {
        Manifests {
            parsers,
            read: BTreeMap::new(),
        }
    }

    /// The manifest at `path`, from the root: `None`, with a finding, when it cannot be read as TOML or UTF-8.
    fn read(
        &mut self,
        tree: &dyn Tree,
        path: &str,
        found: &mut Vec<String>,
    ) -> io::Result<Option<Manifest>> {
        if let Some(manifest) = self.read.get(path) {
            return Ok(manifest.clone());
        }
        let manifest = match read_code(tree, path, "dependencies", found)? {
            None => None,
            Some(source) => match self.parsers.manifest(&source) {
                Ok(manifest) => Some(manifest),
                Err((line, why)) => {
                    found.push(format!(
                        "{path}:{line}: cannot be read as TOML ({why}), so its dependencies are not checked"
                    ));
                    None
                }
            },
        };
        self.read.insert(path.to_string(), manifest.clone());
        Ok(manifest)
    }

    /// The workspace of the manifest in `dir`, as Cargo finds it: the directory its `[package] workspace` names, or
    /// the first directory from `dir` up to the root whose `Cargo.toml` declares `[workspace]`. Its directory from the
    /// root and its manifest, or `None` when there is none in the project.
    fn workspace(
        &mut self,
        tree: &dyn Tree,
        dir: &str,
        manifest: &Manifest,
        found: &mut Vec<String>,
    ) -> io::Result<Option<(String, Manifest)>> {
        let mut at = match &manifest.workspace {
            Some(rel) => match tree.landed(dir, rel) {
                Some(workspace) => workspace,
                None => return Ok(None),
            },
            None => dir.to_string(),
        };
        loop {
            let path = in_dir(&at, "Cargo.toml");
            if exactly(tree, &path).is_ok_and(|(_, is_dir)| !is_dir)
                && let Some(candidate) = self.read(tree, &path, found)?
                && candidate.is_workspace
            {
                return Ok(Some((at, candidate)));
            }
            // `[package] workspace` names the one directory to look in
            if manifest.workspace.is_some() || at.is_empty() {
                return Ok(None);
            }
            at = parent(&at).to_string();
        }
    }
}

/// The directory of a path from the root ("" for the root).
fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

/// The path of `name` in `dir`, both from the root.
fn in_dir(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

/// The direction check of a TypeScript project: every import of a source file in a layer, resolved to a path
/// (`typescript.rs`), judged by the place that path sits in.
fn typescript(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    layout: &Layout,
    places: &[&Place],
) -> io::Result<Vec<String>> {
    let aliases = parsers.typescript_aliases(tree)?;
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
        for path in code_files(tree, &layer.path, |name| layout.is_code(name))? {
            if let Some(from) = place_of(&parts(&path), places) {
                files.push((path, from));
            }
        }
    }
    for (path, layer) in &layout.belongs {
        let place = places.iter().find(|p| &p.name == layer);
        if let Some(place) = place
            && exactly(tree, path).is_ok_and(|(_, is_dir)| !is_dir)
        {
            files.push((path.clone(), place));
        }
    }
    for (path, from) in files {
        if !is_source(&path) {
            continue;
        }
        let Some(source) = read_code(tree, &path, "imports", &mut found)? else {
            continue;
        };
        let read = parsers.typescript(&source, &path);
        if let Some((line, why)) = read.error {
            found.push(format!(
                "{path}:{line}: cannot be read as TypeScript ({why}), so the imports after it may be misread"
            ));
        }
        for (line, specifier) in read.imports {
            let Some(target) = aliases.resolve(tree, &path, &specifier) else {
                continue;
            };
            let Some(to) = place_of(&parts(&target), places) else {
                continue;
            };
            found.extend(judged(
                &format!("{path}:{line}"),
                &format!("imports {specifier} ({target})"),
                from,
                to,
            ));
        }
    }
    Ok(found)
}

/// The direction check of a Python project: every import of a `.py` file in a layer, as the module it names.
fn python(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    layout: &Layout,
    places: &[&Place],
) -> io::Result<Vec<String>> {
    let mut found = Vec::new();
    for layer in places.iter().filter(|p| p.parent.is_none()) {
        for path in code_files(tree, &layer.path, |name| layout.is_code(name))? {
            let Some(from) = place_of(&module_parts(&path), places) else {
                continue;
            };
            let Some(source) = read_code(tree, &path, "imports", &mut found)? else {
                continue;
            };
            let read = parsers.python(&source, &path);
            if let Some((line, why)) = read.error {
                found.push(format!(
                    "{path}:{line}: cannot be read as Python ({why}), so the imports after it may be misread"
                ));
            }
            for (line, module) in read.imports {
                let Some(to) = place_of(&module, places) else {
                    continue;
                };
                found.extend(judged(
                    &format!("{path}:{line}"),
                    &format!("imports {}", module.join(".")),
                    from,
                    to,
                ));
            }
        }
    }
    Ok(found)
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
}

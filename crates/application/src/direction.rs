//! The direction check, read from the tree: each language's code read through the ports, every import resolved to a
//! place, and judged by the rules in `domain`.

use std::collections::BTreeMap;
use std::io;

use crate::code::lands;
use crate::tree::{code_files, exactly, read_code};
use domain::code::{Manifest, Origin, Parsers, is_source, module_parts};
use domain::direction::{judged, place_of, workspace_dirs};
use domain::layers::{Declared, Language, Layout, Place};
use domain::tree::Tree;
use utils::paths::{join, parent};

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
    for layer in places.iter().filter(|p| p.parent.is_none()) {
        for path in code_files(tree, &layer.path, |name| layout.is_code(name))? {
            let Some(from) = place_of(&path, places) else {
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
                                join(&workspace, "Cargo.toml")
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
                let Some(to) = place_of(&target, places) else {
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
        let dirs = match &manifest.workspace {
            // `[package] workspace` names the one directory to look in
            Some(rel) => tree.landed(dir, rel).into_iter().collect(),
            None => workspace_dirs(dir),
        };
        for at in dirs {
            let path = join(&at, "Cargo.toml");
            if exactly(tree, &path).is_ok_and(|(_, is_dir)| !is_dir)
                && let Some(candidate) = self.read(tree, &path, found)?
                && candidate.is_workspace
            {
                return Ok(Some((at, candidate)));
            }
        }
        Ok(None)
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
    // Every source file of the layers with the place it sits in, and each file a starter puts outside the layers with
    // the layer it belongs to, while that layer is there
    let mut files: Vec<(String, &Place)> = Vec::new();
    for layer in places.iter().filter(|p| p.parent.is_none()) {
        for path in code_files(tree, &layer.path, |name| layout.is_code(name))? {
            if let Some(from) = place_of(&path, places) {
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
            let Some(target) = lands(tree, &aliases, &path, &specifier) else {
                continue;
            };
            let Some(to) = place_of(&target, places) else {
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
            let Some(from) = place_of(&module_parts(&path).join("/"), places) else {
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
                let Some(to) = place_of(&module.join("/"), places) else {
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

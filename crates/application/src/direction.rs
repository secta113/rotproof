//! The direction check, read from the tree: each language's code read through the ports, every import resolved to a
//! place, and judged by the rules in `domain`.

use std::collections::BTreeMap;
use std::io;

use crate::code::lands;
use crate::tree::{code_files, exactly, is_file, read_code, resolved};
use domain::code::{Dependency as ManifestDependency, Manifest, Origin, is_source, module_parts};
use domain::declaration::Declared;
use domain::direction::{Forbidden, judged, place_of, workspace_dirs};
use domain::layers::{Language, Layout, Place, table};
use domain::modules::{Dependency, judged_crate};
use domain::paths::{join, parent};
use domain::rust::is_source as is_rust;
use protocol::outbound::Parsers;
use protocol::outbound::Tree;

/// What the direction check found.
#[derive(Debug, Default)]
pub struct Direction {
    /// Every file that could not be read, or whose imports could not be resolved
    pub problems: Vec<String>,
    /// Every import the table does not allow, before the approvals are read
    pub forbidden: Vec<Forbidden>,
}

/// Every import in the layers of `declared` that the table does not allow, and every file that could not be read. An
/// error is a directory that could not be walked.
pub fn problems(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    declared: &Declared,
) -> io::Result<Direction> {
    let Some(layout) = &declared.layout else {
        // A repository of records only: the structure check already says that the layers were not checked
        return Ok(Direction::default());
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

/// The code files in the layers, each with the place it sits in. `placed_by` turns a file's path into the path its
/// place is looked up by: the path itself, or for Python the one its module is named by. A file that sits in no place
/// is left out.
fn layer_files<'a>(
    tree: &dyn Tree,
    layout: &Layout,
    places: &[&'a Place],
    placed_by: impl Fn(&str) -> String,
) -> io::Result<Vec<(String, &'a Place)>> {
    let mut files = Vec::new();
    for layer in places.iter().filter(|p| p.is_layer()) {
        for path in code_files(tree, &layer.path, |name| layout.is_code(name))? {
            if let Some(from) = place_of(&placed_by(&path), places) {
                files.push((path, from));
            }
        }
    }
    Ok(files)
}

/// The entry file with itself as its place, when it is there as a file. It is code outside the layers, so the walk of
/// the layers does not find it.
fn entry_file<'a>(tree: &dyn Tree, entry: Option<&'a Place>) -> Option<(String, &'a Place)> {
    let entry = entry?;
    is_file(tree, &entry.path).then(|| (entry.path.clone(), entry))
}

/// The finding for a file a reader gave up on at `line`: what it read before is judged, and the rest may be misread.
fn misread(path: &str, language: &str, line: usize, why: &str) -> String {
    format!(
        "{path}:{line}: cannot be read as {language} ({why}), so the imports after it may be misread"
    )
}

/// The direction check of a Rust project: every dependency the manifest of a crate in a layer declares, judged by the
/// place the path it comes from sits in, and then the levels inside each crate.
fn rust(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    layout: &Layout,
    places: &[&Place],
) -> io::Result<Direction> {
    let mut found = Vec::new();
    let mut forbidden = Vec::new();
    let mut manifests = Manifests::new(parsers);
    // Each manifest's dependencies that land in a place, by the name the crate's code uses
    let mut depends: BTreeMap<String, Vec<(String, &Place)>> = BTreeMap::new();
    let mut crates = layer_files(tree, layout, places, str::to_string)?;
    let entry = layout.entry(&table());
    crates.extend(entry_manifest(tree, entry.as_ref()));
    for (path, from) in crates {
        let Some(manifest) = manifests.read(tree, &path, &mut found)? else {
            continue;
        };
        for dependency in &manifest.dependencies {
            let Some((target, through)) =
                manifests.lands(tree, &path, &manifest, dependency, &mut found)?
            else {
                continue;
            };
            let Some(to) = place_of(&target, places) else {
                continue;
            };
            let name = &dependency.name;
            depends
                .entry(path.clone())
                .or_default()
                .push((name.clone(), to));
            forbidden.extend(judged(
                &path,
                dependency.line,
                name,
                &format!("depends on {name} ({target}{through})"),
                from,
                to,
            ));
        }
    }
    forbidden.extend(levels(tree, places, &depends, &mut found)?);
    Ok(Direction {
        problems: found,
        forbidden,
    })
}

/// The manifest of the package of the entry file, with the entry as its place, when it is there as a file. Cargo
/// compiles the entry file against what that manifest declares.
fn entry_manifest<'a>(tree: &dyn Tree, entry: Option<&'a Place>) -> Option<(String, &'a Place)> {
    let entry = entry?;
    let manifest = join(parent(parent(&entry.path)), "Cargo.toml");
    is_file(tree, &manifest).then_some((manifest, entry))
}

/// The levels inside each crate that has them, which its manifest cannot tell apart: its code is read, and judged with
/// what the crate depends on in other layers (`depends`, by the path of each manifest). Every file that could not be
/// read goes into `found`.
fn levels(
    tree: &dyn Tree,
    places: &[&Place],
    depends: &BTreeMap<String, Vec<(String, &Place)>>,
    found: &mut Vec<String>,
) -> io::Result<Vec<Forbidden>> {
    let mut forbidden = Vec::new();
    for layer in places.iter().filter(|p| p.is_layer()) {
        let levels: Vec<&Place> = places
            .iter()
            .copied()
            .filter(|p| p.is_level_of(layer))
            .collect();
        if levels.is_empty() {
            continue;
        }
        let mut files = Vec::new();
        for path in code_files(tree, &join(&layer.path, "src"), is_rust)? {
            if let Some(source) = read_code(tree, &path, "paths", found)? {
                files.push((path, source));
            }
        }
        let dependencies = dependencies_of(layer, places, depends);
        let inside = judged_crate(&files, layer, &levels, &dependencies);
        found.extend(inside.problems);
        forbidden.extend(inside.forbidden);
    }
    Ok(forbidden)
}

/// What the crate of `layer` depends on in other layers, by the name its code uses, from what the manifest of the crate
/// declares (`depends`).
fn dependencies_of<'a>(
    layer: &Place,
    places: &[&'a Place],
    depends: &BTreeMap<String, Vec<(String, &'a Place)>>,
) -> Vec<Dependency<'a>> {
    depends
        .get(&join(&layer.path, "Cargo.toml"))
        .into_iter()
        .flatten()
        .filter(|(_, to)| to.name != layer.name)
        .map(|(name, to)| Dependency {
            name: name.replace('-', "_"),
            layer: to,
            levels: places
                .iter()
                .copied()
                .filter(|p| p.is_level_of(to))
                .collect(),
        })
        .collect()
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

    /// The path from the root that `dependency`, of the manifest `manifest` at `path`, comes from, and what the message
    /// says of the way there. `None` when it names no path to judge: one from a registry or git, or one from a
    /// workspace that cannot be found, which is said in `found`.
    fn lands(
        &mut self,
        tree: &dyn Tree,
        path: &str,
        manifest: &Manifest,
        dependency: &ManifestDependency,
        found: &mut Vec<String>,
    ) -> io::Result<Option<(String, &'static str)>> {
        let dir = parent(path);
        let name = &dependency.name;
        let at = format!("{path}:{}", dependency.line);
        match &dependency.origin {
            Origin::Path(rel) => Ok(tree.landed(dir, rel).map(|target| (target, ""))),
            Origin::Workspace => {
                let Some((workspace, workspace_manifest)) =
                    self.workspace(tree, dir, manifest, found)?
                else {
                    let looked = match &manifest.workspace {
                        Some(rel) => {
                            format!("{rel}, which [package] workspace names, is no workspace")
                        }
                        None => {
                            format!("no Cargo.toml from {dir} up to the root declares one")
                        }
                    };
                    found.push(format!(
                        "{at}: {name} comes from the workspace, and {looked}, so it is not checked"
                    ));
                    return Ok(None);
                };
                let Some(rel) = workspace_manifest.workspace_dependencies.get(name) else {
                    found.push(format!(
                        "{at}: {name} comes from the workspace, and {} has no {name} in \
                         [workspace.dependencies], so it is not checked",
                        join(&workspace, "Cargo.toml")
                    ));
                    return Ok(None);
                };
                // One from a registry or git names no place
                let Some(rel) = rel else {
                    return Ok(None);
                };
                Ok(tree
                    .landed(&workspace, rel)
                    .map(|target| (target, ", from the workspace")))
            }
        }
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
            if is_file(tree, &path)
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
) -> io::Result<Direction> {
    let mut forbidden = Vec::new();
    let aliases = parsers.typescript_aliases(tree)?;
    let mut found: Vec<String> = aliases
        .problems
        .iter()
        .map(|why| format!("{why}; imports through it are not checked"))
        .collect();
    let entry = layout.entry(&table());
    let mut files = layer_files(tree, layout, places, str::to_string)?;
    files.extend(entry_file(tree, entry.as_ref()));
    for (path, from) in files {
        if !is_source(&path) {
            continue;
        }
        let Some(source) = read_code(tree, &path, "imports", &mut found)? else {
            continue;
        };
        let read = parsers.typescript(&source, &path);
        if let Some((line, why)) = read.error {
            found.push(misread(&path, "TypeScript", line, &why));
        }
        for (line, specifier) in read.imports {
            let Some(target) = lands(tree, &aliases, &path, &specifier) else {
                continue;
            };
            let target = resolved(tree, &target);
            let Some(to) = place_of(&target, places) else {
                continue;
            };
            forbidden.extend(judged(
                &path,
                line,
                &specifier,
                &format!("imports {specifier} ({target})"),
                from,
                to,
            ));
        }
    }
    Ok(Direction {
        problems: found,
        forbidden,
    })
}

/// The direction check of a Python project: every import of a `.py` file in a layer, and of the entry file, as the
/// module it names.
fn python(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    layout: &Layout,
    places: &[&Place],
) -> io::Result<Direction> {
    let mut found = Vec::new();
    let mut forbidden = Vec::new();
    let entry = layout.entry(&table());
    let mut files = layer_files(tree, layout, places, |path| module_parts(path).join("/"))?;
    files.extend(entry_file(tree, entry.as_ref()));
    for (path, from) in files {
        let Some(source) = read_code(tree, &path, "imports", &mut found)? else {
            continue;
        };
        let read = parsers.python(&source, &path);
        if let Some((line, why)) = read.error {
            found.push(misread(&path, "Python", line, &why));
        }
        for (line, module) in read.imports {
            let Some(to) = place_of(&resolved(tree, &module.join("/")), places) else {
                continue;
            };
            let dotted = module.join(".");
            forbidden.extend(judged(
                &path,
                line,
                &dotted,
                &format!("imports {dotted}"),
                from,
                to,
            ));
        }
    }
    Ok(Direction {
        problems: found,
        forbidden,
    })
}

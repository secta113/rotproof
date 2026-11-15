//! The direction check of a Rust project, read from the tree.

use std::collections::BTreeMap;
use std::io;

use super::{Direction, layer_files};
use crate::tree::{code_files, is_file, read_code};
use domain::code::{Dependency as ManifestDependency, Manifest, Origin};
use domain::direction::{Forbidden, edge, judged, place_of, workspace_dirs};
use domain::layers::{Layout, Place};
use domain::modules::{Dependency, judged_crate};
use domain::paths::{join, parent};
use domain::rust::is_source as is_rust;
use protocol::outbound::Parsers;
use protocol::outbound::Tree;

/// The direction check of a Rust project: every dependency the manifest of a crate in a layer declares, judged by the
/// place the path it comes from sits in, and then the levels inside each crate.
pub(super) fn rust(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    layout: &Layout,
    places: &[&Place],
    entries: &[Place],
) -> io::Result<Direction> {
    let mut found = Vec::new();
    let mut forbidden = Vec::new();
    let mut edges = Vec::new();
    let mut manifests = Manifests::new(parsers);
    // Each manifest's dependencies that land in a place, by the name the crate's code uses
    let mut depends: BTreeMap<String, Vec<(String, &Place)>> = BTreeMap::new();
    let mut crates = layer_files(tree, layout, places, str::to_string)?;
    crates.extend(
        entries
            .iter()
            .filter_map(|entry| entry_manifest(tree, entry)),
    );
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
            edges.extend(edge(&path, dependency.line, name, from, to));
        }
    }
    forbidden.extend(levels(tree, places, &depends, &mut found)?);
    Ok(Direction {
        problems: found,
        forbidden,
        edges,
    })
}

/// The manifest of the package of the entry file, with the entry as its place, when it is there as a file. Cargo
/// compiles the entry file against what that manifest declares.
fn entry_manifest<'a>(tree: &dyn Tree, entry: &'a Place) -> Option<(String, &'a Place)> {
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
        .filter(|(_, to)| !to.is_same(layer))
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

#[cfg(test)]
mod tests {
    use super::*;
    use domain::layers::{UnitTag, layout, table};

    #[test]
    fn a_crate_depends_on_the_layer_of_another_unit_that_has_its_name() {
        let table = table();
        let layout = layout("rust", "none").unwrap();
        let unit = |name: &str| UnitTag {
            name: name.into(),
            shared: false,
            app: false,
        };
        let here = layout.places_in(&table, "crates", &unit("a"));
        let there = layout.places_in(&table, "packages/b", &unit("b"));
        let places: Vec<&Place> = here.iter().chain(&there).collect();
        let named = |places: &[Place], name: &str| places.iter().position(|p| p.name == name);
        let layer = &here[named(&here, "infrastructure").unwrap()];
        let manifest = join(&layer.path, "Cargo.toml");
        let own = &here[named(&here, "domain").unwrap()];
        let same_name = &there[named(&there, "infrastructure").unwrap()];
        let depends = BTreeMap::from([(
            manifest,
            vec![
                ("domain".to_string(), own),
                ("b-infrastructure".to_string(), same_name),
            ],
        )]);
        let dependencies = dependencies_of(layer, &places, &depends);
        let names: Vec<&str> = dependencies.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["domain", "b_infrastructure"]);
        // The levels are those of the unit the dependency is in
        assert!(dependencies[1].levels.iter().all(|l| l.unit.name == "b"));
        assert!(!dependencies[1].levels.is_empty());
    }
}

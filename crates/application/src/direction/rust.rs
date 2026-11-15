//! The direction check of a Rust project, read from the tree.

use std::collections::BTreeMap;
use std::io;

use super::{Direction, layer_files};
use crate::tree::{code_files, is_file, read_code};
use domain::code::{Dependency as ManifestDependency, Manifest, Origin};
use domain::direction::{
    Forbidden, Landing, Way, edge, judged_landing, landing_of, workspace_dirs,
};
use domain::layers::{Layout, Place};
use domain::modules::{Dependency, judged_crate};
use domain::paths::{join, parent};
use domain::readers::{Read, read_of};
use domain::rust::{code, is_source as is_rust, paths, renames};
use domain::units::Unit;
use domain::units_doc::link;
use protocol::outbound::Parsers;
use protocol::outbound::Tree;

/// The direction check of a Rust project: every dependency the manifest of a crate in a layer, or of the package of an
/// entry file of any unit, declares, judged by the place (or the unit's code outside its layers) the path it comes
/// from lands in, and then the levels inside each crate.
pub(super) fn rust<'a>(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    layout: &Layout,
    places: &[&'a Place],
    entries: &'a [Place],
    units: &'a [Unit],
) -> io::Result<Direction> {
    let mut found = Vec::new();
    let mut forbidden = Vec::new();
    let mut edges = Vec::new();
    let mut reads = Vec::new();
    let mut links = Vec::new();
    let mut manifests = Manifests::new(parsers);
    // Each manifest's dependencies that land in a place, by the name the crate's code uses
    let mut depends: BTreeMap<String, Vec<(String, &Place)>> = BTreeMap::new();
    let mut crates = layer_files(tree, layout, places, str::to_string)?;
    for entry in entries.iter().filter(|entry| is_file(tree, &entry.path)) {
        match manifests.package_of(tree, parent(&entry.path), &mut found)? {
            // Entry files of one package, `src/main.rs` and `src/bin/w.rs`, are read through its manifest once
            Some(path) => {
                if !crates
                    .iter()
                    .any(|(at, from)| *at == path && from.unit.name == entry.unit.name)
                {
                    crates.push((path, entry));
                }
            }
            None => found.push(no_package(entry)),
        }
    }
    for (path, from) in crates {
        let Some(manifest) = manifests.read(tree, &path, &mut found)? else {
            continue;
        };
        // The crates of other shared units this crate depends on, by the name its code uses, and where each lands
        let mut shared: Vec<(String, &Place)> = Vec::new();
        for dependency in &manifest.dependencies {
            let Some((target, through)) =
                manifests.lands(tree, &path, &manifest, dependency, &mut found)?
            else {
                continue;
            };
            let Some(to) = landing_of(&target, places, units) else {
                continue;
            };
            let name = &dependency.name;
            // What the levels of a crate are judged by is a crate of a place, not the code of a unit beside its layers
            if let Landing::Place(to) = to {
                depends
                    .entry(path.clone())
                    .or_default()
                    .push((name.clone(), to));
            }
            forbidden.extend(judged_landing(
                &path,
                dependency.line,
                name,
                &format!("depends on {name} ({target}{through})"),
                from,
                to,
                &Way::Crate,
            ));
            edges.extend(edge(&path, dependency.line, name, from, to));
            links.extend(link(from, to));
            if let Landing::Place(to) = to
                && read_of(from, to, name).is_some()
            {
                shared.push((name.replace('-', "_"), to));
            }
        }
        if !shared.is_empty() {
            reads.extend(read_of_crate(tree, &path, from, &shared, &mut found)?);
        }
    }
    forbidden.extend(levels(tree, places, &depends, &mut found)?);
    Ok(Direction {
        problems: found,
        forbidden,
        edges,
        reads,
        links,
        moving: None,
    })
}

/// The names the crate whose manifest is `manifest` reads of the crates of shared units it depends on (`shared`: the
/// name its code uses for each, and the place it lands in), in the `.rs` files of its `src/`: every path that starts
/// with one of those names, or with a name bound to one with `as`, written from the crate
/// (`core_domain::score::count`).
/// A module imported whole (`use core_domain::score;`) is the module, and a crate bound to a name (`use core_domain as
/// cd;`) is the crate. Every file that could not be read goes into `found`.
fn read_of_crate(
    tree: &dyn Tree,
    manifest: &str,
    from: &Place,
    shared: &[(String, &Place)],
    found: &mut Vec<String>,
) -> io::Result<Vec<Read>> {
    let mut reads = Vec::new();
    let crates: Vec<&str> = shared.iter().map(|(name, _)| name.as_str()).collect();
    let landing = |name: &str| {
        shared
            .iter()
            .find(|(known, _)| known == name)
            .map(|(_, to)| *to)
    };
    for file in code_files(tree, &join(parent(manifest), "src"), is_rust)? {
        let Some(source) = read_code(tree, &file, "paths", found)? else {
            continue;
        };
        let code = code(&source);
        // The names this file binds to a path in one of those crates
        let bound: BTreeMap<String, Vec<String>> = renames(&code)
            .into_iter()
            .filter(|(_, _, path)| {
                path.first()
                    .is_some_and(|name| crates.contains(&name.as_str()))
            })
            .map(|(_, name, path)| (name, path))
            .collect();
        let mut starts = crates.clone();
        starts.extend(bound.keys().map(String::as_str));
        let mut named: Vec<Vec<String>> = bound.values().cloned().collect();
        for (_, segments) in paths(&code, &starts) {
            named.push(match bound.get(&segments[0]) {
                Some(renamed) => renamed.iter().chain(&segments[1..]).cloned().collect(),
                None => segments,
            });
        }
        for mut segments in named {
            if segments.len() > 1 && segments.last().map(String::as_str) == Some("self") {
                segments.pop();
            }
            if let Some(to) = landing(&segments[0]) {
                reads.extend(read_of(from, to, &segments.join("::")));
            }
        }
    }
    Ok(reads)
}

/// The finding for an entry file that is there and has no package to be compiled in: what it imports is not read.
fn no_package(entry: &Place) -> String {
    let of = if entry.unit.name.is_empty() {
        String::new()
    } else {
        format!(" of unit {}", entry.unit.name)
    };
    format!(
        "{}: the entry{of} has no Cargo.toml with a [package] from its directory up to the root, so what it imports \
         cannot be judged: give it a package, or take the entry file out",
        entry.path
    )
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

    /// The manifest of the package an entry file in `dir` is compiled in, as Cargo finds it: the first `Cargo.toml`
    /// from `dir` up to the root that declares a `[package]` (a manifest of a workspace alone builds nothing), by its
    /// path from the root. A manifest that cannot be read is the one found, said once in `found`. `None` when there
    /// is none.
    fn package_of(
        &mut self,
        tree: &dyn Tree,
        dir: &str,
        found: &mut Vec<String>,
    ) -> io::Result<Option<String>> {
        for at in workspace_dirs(dir) {
            let path = join(&at, "Cargo.toml");
            if !is_file(tree, &path) {
                continue;
            }
            match self.read(tree, &path, found)? {
                Some(manifest) if !manifest.is_package => {}
                _ => return Ok(Some(path)),
            }
        }
        Ok(None)
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
                        "{path}:{line}: cannot be read as TOML ({why}), so its dependencies cannot be judged: fix the file"
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
                        "{at}: {name} comes from the workspace, and {looked}, so it cannot be judged: declare the \
                         workspace, or give {name} a path"
                    ));
                    return Ok(None);
                };
                let Some(rel) = workspace_manifest.workspace_dependencies.get(name) else {
                    found.push(format!(
                        "{at}: {name} comes from the workspace, and {} has no {name} in \
                         [workspace.dependencies], so it cannot be judged: add it there, or give it a path",
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

    use crate::direction::problems;
    use crate::tree::fake::Fake;
    use domain::declaration::{Declared, parse_declaration};
    use infrastructure::outbound::readers::Readers;

    /// The text of the manifest of a package `name` with the path dependencies `deps` (name, path from its directory).
    fn package(name: &str, deps: &[(&str, &str)]) -> String {
        let mut text =
            format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n\n[dependencies]\n");
        for (dep, path) in deps {
            text.push_str(&format!("{dep} = {{ path = \"{path}\" }}\n"));
        }
        text
    }

    /// The units of a test project: name, path, whether it is shared, and its entry files.
    type Units<'a> = &'a [(&'a str, &'a str, bool, &'a [&'a str])];

    /// A Rust project of `units`, with `files`.
    fn project(units: Units, files: &[(&str, &str)]) -> (Fake, Declared) {
        let mut declared = Declared::new(
            parse_declaration("stack = \"rust\"\nareas = []\nui = \"none\"\n").unwrap(),
        )
        .unwrap();
        let layout = declared.layout.clone().unwrap();
        declared.units = units
            .iter()
            .map(|(name, path, shared, entries)| Unit {
                name: name.to_string(),
                path: path.to_string(),
                root: path.to_string(),
                shared: *shared,
                entries: entries.iter().map(|e| e.to_string()).collect(),
                absent: Vec::new(),
                declared: true,
            })
            .collect();
        declared.places = declared
            .units
            .iter()
            .flat_map(|unit| unit.places(&layout, &table()))
            .collect();
        (Fake::new(files), declared)
    }

    /// The problems and the details of the forbidden imports the check finds.
    fn details(tree: &Fake, declared: &Declared) -> (Vec<String>, Vec<String>) {
        let found = problems(tree, &Readers, declared).unwrap();
        let forbidden = found.forbidden.into_iter().map(|f| f.detail).collect();
        (found.problems, forbidden)
    }

    const UNITS: Units = &[
        ("core", "packages/core", true, &[]),
        ("billing", "packages/billing", false, &[]),
        ("shipping", "packages/shipping", false, &[]),
        ("cli", "apps/cli", false, &["apps/cli/src/main.rs"]),
    ];

    #[test]
    fn a_crate_of_a_unit_that_is_not_shared_depends_on_no_crate_of_another_such_unit() {
        let (tree, declared) = project(
            UNITS,
            &[
                (
                    "packages/shipping/infrastructure/Cargo.toml",
                    &package(
                        "shipping_infrastructure",
                        &[("billing_protocol", "../../billing/protocol")],
                    ),
                ),
                (
                    "packages/billing/protocol/Cargo.toml",
                    &package("billing_protocol", &[]),
                ),
            ],
        );
        let (problems, forbidden) = details(&tree, &declared);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(
            forbidden,
            [
                "packages/shipping/infrastructure/Cargo.toml:6: depends on billing_protocol \
                 (packages/billing/protocol), in protocol of unit billing; shipping imports no unit that is not \
                 shared, and billing is not: an app's composition wires the two"
            ]
        );
    }

    #[test]
    fn a_crate_depends_on_the_crates_of_a_shared_unit_as_on_its_own_and_on_no_application() {
        let (tree, declared) = project(
            UNITS,
            &[
                (
                    "packages/billing/infrastructure/Cargo.toml",
                    &package(
                        "billing_infrastructure",
                        &[
                            ("core_protocol", "../../core/protocol"),
                            ("core_domain", "../../core/domain"),
                        ],
                    ),
                ),
                (
                    "packages/billing/infrastructure/src/inbound/mod.rs",
                    "use core_protocol::inbound::Commands;\nuse core_protocol::outbound::Port;\n\
                     use ::core_protocol as p;\nuse p::outbound::Q;\n",
                ),
                (
                    "packages/billing/infrastructure/src/lib.rs",
                    "pub mod inbound;\n",
                ),
                (
                    "packages/billing/application/Cargo.toml",
                    &package(
                        "billing_application",
                        &[("core_application", "../../core/application")],
                    ),
                ),
                (
                    "packages/core/application/Cargo.toml",
                    &package("core_application", &[]),
                ),
                (
                    "packages/core/domain/Cargo.toml",
                    &package("core_domain", &[]),
                ),
                (
                    "packages/core/protocol/Cargo.toml",
                    &package("core_protocol", &[]),
                ),
                (
                    "packages/core/protocol/src/lib.rs",
                    "pub mod inbound;\npub mod outbound;\n",
                ),
                ("packages/core/protocol/src/inbound/mod.rs", ""),
                ("packages/core/protocol/src/outbound/mod.rs", ""),
            ],
        );
        let (problems, forbidden) = details(&tree, &declared);
        assert!(problems.is_empty(), "{problems:?}");
        // The crate may depend on the shared unit's `protocol` and `domain` as `infrastructure` does on its own, and
        // read its `protocol::inbound`; its use of `protocol::outbound` fails by the level it is in, and its
        // `application` may not depend on the shared unit's
        assert_eq!(
            forbidden,
            [
                "packages/billing/application/Cargo.toml:6: depends on core_application \
                 (packages/core/application), in application of unit core; application never imports another \
                 unit's application: composition passes a shared unit's use cases in",
                "packages/billing/infrastructure/src/inbound/mod.rs:2: uses core_protocol::outbound::Port, in \
                 protocol.outbound of unit core; infrastructure.inbound may import `protocol.inbound`",
                // A name bound with `as` to the crate of another unit, through a path with a leading `::`, is judged
                "packages/billing/infrastructure/src/inbound/mod.rs:4: uses core_protocol::outbound::Q through \
                 `p`, in protocol.outbound of unit core; infrastructure.inbound may import `protocol.inbound`",
            ]
        );
    }

    #[test]
    fn the_inbound_module_of_a_protocol_crate_names_the_domain_crate_of_a_shared_unit_and_no_other()
    {
        let (tree, declared) = project(
            UNITS,
            &[
                (
                    "packages/billing/protocol/Cargo.toml",
                    &package(
                        "billing_protocol",
                        &[
                            ("core_domain", "../../core/domain"),
                            ("billing_domain", "../domain"),
                        ],
                    ),
                ),
                (
                    "packages/billing/protocol/src/lib.rs",
                    "pub mod inbound;\npub mod outbound;\n",
                ),
                (
                    "packages/billing/protocol/src/inbound/mod.rs",
                    "use core_domain::Money;\nuse billing_domain::Invoice;\n",
                ),
                (
                    "packages/billing/protocol/src/outbound/mod.rs",
                    "use core_domain::Money;\nuse billing_domain::Invoice;\n",
                ),
                (
                    "packages/billing/domain/Cargo.toml",
                    &package("billing_domain", &[]),
                ),
                (
                    "packages/core/domain/Cargo.toml",
                    &package("core_domain", &[]),
                ),
            ],
        );
        let (problems, forbidden) = details(&tree, &declared);
        assert!(problems.is_empty(), "{problems:?}");
        // The crate depends on both, as `protocol` may depend on `domain`; the level that is for ports of the use
        // cases takes the shared unit's, and its own unit's no more than it takes its own in any project
        assert_eq!(
            forbidden,
            [
                "packages/billing/protocol/src/inbound/mod.rs:2: uses billing_domain::Invoice, in domain; \
                 protocol.inbound imports no other layer"
            ]
        );
    }

    #[test]
    fn what_is_read_of_a_shared_unit_is_the_path_a_crate_writes_through_its_dependency_on_it() {
        let (tree, declared) = project(
            &[
                ("core", "packages/core", true, &[]),
                ("billing", "packages/billing", false, &[]),
                ("steps", "packages/steps", false, &[]),
            ],
            &[
                (
                    "packages/billing/application/Cargo.toml",
                    &package(
                        "billing_application",
                        &[("core-domain", "../../core/domain")],
                    ),
                ),
                (
                    "packages/billing/application/src/lib.rs",
                    "use core_domain::score::count;\nuse core_domain::{rank, tally::{a, self}};\n\
                     use core_domain::score;\nfn f() -> u8 { core_domain::score::other() }\n\
                     // core_domain::in::comment\nuse std::fmt;\n",
                ),
                (
                    "packages/steps/application/Cargo.toml",
                    &package("steps_application", &[("core_domain", "../../core/domain")]),
                ),
                (
                    "packages/steps/application/src/lib.rs",
                    "use core_domain as cd;\nuse core_domain::score as s;\nfn f() { cd::tally::b(); s::count(); }\n",
                ),
                (
                    "packages/steps/application/src/more.rs",
                    "use crate::lib_item;\n",
                ),
                (
                    "packages/core/domain/Cargo.toml",
                    &package("core_domain", &[]),
                ),
                // A crate that depends on no other unit is not read
                (
                    "packages/core/application/Cargo.toml",
                    &package("core_application", &[("core_domain", "../domain")]),
                ),
                (
                    "packages/core/application/src/lib.rs",
                    "use core_domain::inside;\n",
                ),
            ],
        );
        let found = problems(&tree, &Readers, &declared).unwrap();
        // A crate depending on a crate of another unit is a link, and one depending on a crate of its own unit is none
        let mut links: Vec<(&str, &str)> = found
            .links
            .iter()
            .map(|link| (link.from.as_str(), link.to.as_str()))
            .collect();
        links.sort();
        links.dedup();
        assert_eq!(links, [("billing", "core"), ("steps", "core")]);
        let mut reads: Vec<(String, String)> = found
            .reads
            .iter()
            .map(|read| (read.name.clone(), read.by.clone()))
            .collect();
        // A name read twice, as `use a::b as c;` is a path and a rename, is listed once in the file
        reads.sort();
        reads.dedup();
        let said = |name: &str, by: &str| (name.to_string(), by.to_string());
        // A module imported whole is the module, a crate bound to a name is the crate, and what is reached through a
        // name bound with `as` is written from the crate
        assert_eq!(
            reads,
            [
                said("core_domain", "steps"),
                said("core_domain::rank", "billing"),
                said("core_domain::score", "billing"),
                said("core_domain::score", "steps"),
                said("core_domain::score::count", "billing"),
                said("core_domain::score::count", "steps"),
                said("core_domain::score::other", "billing"),
                said("core_domain::tally", "billing"),
                said("core_domain::tally::a", "billing"),
                said("core_domain::tally::b", "steps"),
            ]
        );
    }

    #[test]
    fn a_crate_of_a_unit_outside_its_layers_is_told_to_depend_on_the_crate_of_a_layer() {
        let (tree, declared) = project(
            UNITS,
            &[
                (
                    "packages/billing/application/Cargo.toml",
                    &package(
                        "billing_application",
                        &[("core_extras", "../../core/extras")],
                    ),
                ),
                (
                    "packages/core/extras/Cargo.toml",
                    &package("core_extras", &[]),
                ),
            ],
        );
        let (problems, forbidden) = details(&tree, &declared);
        assert!(problems.is_empty(), "{problems:?}");
        // A dependency of Cargo is no import of a module, so the finding says to depend on a crate
        assert_eq!(
            forbidden,
            [
                "packages/billing/application/Cargo.toml:6: depends on core_extras (packages/core/extras), in the \
                 unit core outside its layers; depend on the crate of a layer, such as core_protocol"
            ]
        );
    }

    #[test]
    fn the_entry_files_of_a_package_are_read_through_its_manifest_wherever_they_sit_in_it() {
        let both: Units = &[
            ("billing", "packages/billing", false, &[]),
            (
                "cli",
                "apps/cli",
                false,
                &["apps/cli/src/main.rs", "apps/cli/src/bin/w.rs"],
            ),
        ];
        let (tree, declared) = project(
            both,
            &[
                (
                    "apps/cli/Cargo.toml",
                    &package(
                        "cli",
                        &[
                            ("cli_composition", "composition"),
                            ("billing_domain", "../../packages/billing/domain"),
                        ],
                    ),
                ),
                ("apps/cli/src/main.rs", "fn main() {}\n"),
                ("apps/cli/src/bin/w.rs", "fn main() {}\n"),
                (
                    "apps/cli/composition/Cargo.toml",
                    &package("cli_composition", &[]),
                ),
                (
                    "packages/billing/domain/Cargo.toml",
                    &package("billing_domain", &[]),
                ),
            ],
        );
        // The manifest is the one of `apps/cli`, and its dependency is judged once for the two entry files
        let (problems, forbidden) = details(&tree, &declared);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(forbidden.len(), 1, "{forbidden:?}");
        assert!(
            forbidden[0].starts_with(
                "apps/cli/Cargo.toml:7: depends on billing_domain (packages/billing/domain), in domain of unit \
                 billing;"
            ),
            "{forbidden:?}"
        );
        // An entry file in `src/bin/` alone is read all the same
        let bin: Units = &[
            ("billing", "packages/billing", false, &[]),
            ("cli", "apps/cli", false, &["apps/cli/src/bin/w.rs"]),
        ];
        let (tree, declared) = project(
            bin,
            &[
                (
                    "apps/cli/Cargo.toml",
                    &package(
                        "cli",
                        &[("billing_domain", "../../packages/billing/domain")],
                    ),
                ),
                ("apps/cli/src/bin/w.rs", "fn main() {}\n"),
                (
                    "packages/billing/domain/Cargo.toml",
                    &package("billing_domain", &[]),
                ),
            ],
        );
        let (problems, forbidden) = details(&tree, &declared);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(forbidden.len(), 1, "{forbidden:?}");
    }

    #[test]
    fn an_entry_file_with_no_package_from_its_directory_up_to_the_root_is_a_problem() {
        let bin: Units = &[("cli", "apps/cli", false, &["apps/cli/src/bin/w.rs"])];
        let (tree, declared) = project(
            bin,
            &[
                ("apps/cli/src/bin/w.rs", "fn main() {}\n"),
                // Neither a workspace alone nor a manifest below the entry's directory builds the entry
                ("Cargo.toml", "[workspace]\nmembers = [\"apps/*\"]\n"),
                ("apps/cli/src/bin/other/Cargo.toml", &package("other", &[])),
            ],
        );
        let (problems, forbidden) = details(&tree, &declared);
        assert_eq!(
            problems,
            [
                "apps/cli/src/bin/w.rs: the entry of unit cli has no Cargo.toml with a [package] from its \
                 directory up to the root, so what it imports cannot be judged: give it a package, or take the \
                 entry file out"
            ]
        );
        assert!(forbidden.is_empty());
        // The implicit unit names no unit
        let declared = Declared::new(
            parse_declaration("stack = \"rust\"\nareas = []\nui = \"none\"\n").unwrap(),
        )
        .unwrap();
        let domain = package("domain", &[]);
        let tree = Fake::new(&[
            ("src/main.rs", "fn main() {}\n"),
            ("crates/domain/Cargo.toml", &domain),
        ]);
        let (problems, _) = details(&tree, &declared);
        assert_eq!(
            problems,
            [
                "src/main.rs: the entry has no Cargo.toml with a [package] from its directory up to the root, so \
                 what it imports cannot be judged: give it a package, or take the entry file out"
            ]
        );
        // With the package at the root, the walk from `src/` finds it
        let root = package("rotproof", &[]);
        let tree = Fake::new(&[
            ("src/main.rs", "fn main() {}\n"),
            ("Cargo.toml", &root),
            ("crates/domain/Cargo.toml", &domain),
        ]);
        assert_eq!(details(&tree, &declared), (vec![], vec![]));
        // An entry file that is not there says nothing
        let tree = Fake::new(&[("crates/domain/Cargo.toml", &domain)]);
        assert_eq!(details(&tree, &declared), (vec![], vec![]));
    }
}

//! The direction check of a TypeScript project, read from the tree.

use std::io;

use super::moving::Moving;
use super::{Direction, Landings, entry_files, layer_files, misread};
use crate::code::{is_test_module, lands, module_candidates};
use crate::tree::{read_code, resolved};
use domain::code::{Aliases, Reach, is_source, without_extension};
use domain::declaration::Declared;
use domain::direction::{Landing, Way, edge, imports_test, judged_landing, landing_of};
use domain::layers::{Layout, Place};
use domain::outside::{asks, outside as outside_of};
use domain::paths::{join, parent, relative};
use domain::readers::read_of;
use domain::units::Unit;
use domain::units_doc::link;
use domain::vocabulary;
use protocol::outbound::Parsers;
use protocol::outbound::Tree;

/// The direction check of a TypeScript project: every import of a source file in a layer, resolved to a path
/// (`typescript.rs`), judged by the place that path sits in, or by the unit it sits in outside the places. An import
/// through the name of a unit's package resolves through its `package.json`, as `exports` says; one of a subpath the
/// package does not export is a problem, since where it lands is not known, and so is one it exports as a path where no
/// file is (`"./*": "./src/*/index.ts"` takes `domain/invoice.ts` to `src/domain/invoice.ts/index.ts`). What a
/// `protocol.inbound` imports from the `domain` of its own unit is judged by the names the import takes and what the
/// file it lands in defines (`domain::vocabulary`), a file being the first that can make the module that is there; a
/// file that is not, or cannot be parsed, leaves the import to the table, which forbids it.
pub(super) fn typescript<'a>(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    declared: &'a Declared,
    layout: &Layout,
    places: &[&'a Place],
    entries: &'a [Place],
    moving: &mut Moving<'a>,
) -> io::Result<Direction> {
    let units = &declared.units;
    let mut forbidden = Vec::new();
    let mut edges = Vec::new();
    let mut reads = Vec::new();
    let mut links = Vec::new();
    let mut outside = Vec::new();
    // The packages are the declared units': the implicit unit of a project that declares none is no package
    let roots: Vec<String> = units
        .iter()
        .filter(|unit| unit.declared)
        .map(|unit| unit.root.clone())
        .collect();
    let aliases = parsers.typescript_aliases(tree, &roots)?;
    let mut found: Vec<String> = aliases
        .problems
        .iter()
        .map(|why| format!("{why}; imports through it cannot be judged until it is fixed"))
        .collect();
    let mut files = layer_files(tree, layout, places, str::to_string)?;
    files.extend(entry_files(tree, entries));
    let mut landings = Landings::new(tree, parsers, layout.language);
    for (path, from) in files {
        // A test is no code of a layer: its imports are no layer's, as the imports of Python's `tests/` are read by no
        // check
        if !is_source(&path) || layout.is_test_file(&path) {
            continue;
        }
        let Some(source) = read_code(tree, &path, "imports", &mut found)? else {
            continue;
        };
        let read = parsers.typescript(&source, &path);
        if let Some((line, why)) = read.error {
            found.push(misread(&path, "TypeScript", line, &why));
        }
        for (line, import) in read.imports {
            let specifier = import.specifier;
            let target = match lands(tree, &aliases, &path, &specifier) {
                Reach::At(Some(target)) => resolved(tree, &target),
                Reach::At(None) => continue,
                Reach::Unlisted { config } => {
                    found.push(format!(
                        "{path}:{line}: imports {specifier}, which {config} does not export, so Node and tsc fail it \
                         and its layer cannot be judged: export the subpath in {config}, or import one that is exported"
                    ));
                    continue;
                }
                Reach::Missing {
                    config,
                    path: leads,
                } => {
                    let leads = resolved(tree, &leads);
                    // Only where it would be judged: an export that leads to build output not there yet (`./dist/`)
                    // lands in no unit, and is not judged. Where it is, the import is judged by where it leads all
                    // the same, so that a rule it breaks is not hidden by the path being wrong
                    if landing_of(&leads, places, units).is_none() {
                        continue;
                    }
                    found.push(format!(
                        "{path}:{line}: imports {specifier}, which {config} exports as {leads}, where no file is: \
                         the import names no file the package exports. Fix that export in {config}, or import a \
                         subpath that leads to a file"
                    ));
                    leads
                }
            };
            moving.read_by(from, || module_candidates(&target));
            if is_test_module(tree, layout, &target) {
                let patterns = layout.tests.as_ref().map(|tests| tests.patterns());
                forbidden.push(imports_test(
                    &path,
                    line,
                    &specifier,
                    &format!("imports {specifier} ({target})"),
                    &format!("a test ({})", patterns.unwrap_or_default().join(", ")),
                ));
                continue;
            }
            // What an inner layer reads of the code that no check reads, which the direction check lets through
            if asks(declared, from) {
                outside.extend(outside_of(
                    declared,
                    from,
                    &path,
                    line,
                    &specifier,
                    &module_candidates(&target),
                ));
            }
            let Some(to) = landing_of(&target, places, units) else {
                continue;
            };
            let what = format!("imports {specifier} ({target})");
            let by_table = || {
                judged_landing(
                    &path,
                    line,
                    &specifier,
                    &what,
                    from,
                    to,
                    &way_of(to, &aliases, &path),
                )
            };
            // A `protocol.inbound` importing the `domain` of its own unit is judged by what the file it lands in
            // defines, and by the table when that cannot be told: no file is there, or it cannot be parsed
            let finding = match (to, landing_file(tree, &target)) {
                (Landing::Place(place), Some(file)) if vocabulary::applies(from, place) => {
                    match landings.defined(&file)? {
                        Some(defined) => vocabulary::judged(
                            &path,
                            line,
                            &specifier,
                            &what,
                            &import.names,
                            defined,
                        ),
                        None => by_table(),
                    }
                }
                _ => by_table(),
            };
            forbidden.extend(finding);
            edges.extend(edge(&path, line, &specifier, from, to));
            links.extend(link(from, to));
            // A name is the module the import lands in and each name it takes from it, or the module alone
            let module = without_extension(&target);
            if import.names.is_empty() {
                reads.extend(read_of(from, to, module));
            }
            for name in &import.names {
                reads.extend(read_of(from, to, &format!("{module}: {name}")));
            }
        }
    }
    read_moving(tree, parsers, &aliases, places, units, moving)?;
    Ok(Direction {
        problems: found,
        forbidden,
        edges,
        reads,
        links,
        moving: None,
        templates: None,
        outside,
    })
}

/// The file a module that an import lands at is, `target` being the path from the root as the import writes it: the
/// first of the files that can make it ([`module_candidates`]) that is there. `None` when none is.
fn landing_file(tree: &dyn Tree, target: &str) -> Option<String> {
    module_candidates(target)
        .into_iter()
        .find(|file| tree.found(file) == Some(false))
}

/// What each of the files that are moving imports, resolved as it is for the files in the layers: a file that is
/// moving too, or else where it lands in the layers. An import that cannot be resolved, and a file that cannot be read,
/// are no finding: the code that is moving is not checked.
fn read_moving<'a>(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    aliases: &Aliases,
    places: &[&'a Place],
    units: &'a [Unit],
    moving: &mut Moving<'a>,
) -> io::Result<()> {
    for path in moving.files() {
        let Some(source) = read_code(tree, &path, "imports", &mut Vec::new())? else {
            continue;
        };
        for (_, import) in parsers.typescript(&source, &path).imports {
            if let Reach::At(Some(target)) = lands(tree, aliases, &path, &import.specifier) {
                let at = resolved(tree, &target);
                moving.imports(
                    &path,
                    module_candidates(&at),
                    landing_of(&at, places, units),
                );
            }
        }
    }
    Ok(())
}

/// How an import of a layer of the unit `to` is written from the file `from`, for the finding that says an import
/// landed in the unit outside its layers (an import that lands in a place needs no words). Through the unit's package
/// when it has one: `<name>/<layer>` where its `exports` lists subpaths (a pattern such as `./protocol/*` makes it
/// so), else the path of the layer in the package, as Node reads a package with no `exports`
/// (`@acme/billing/src/<layer>`); a unit with no package by the relative path from the importing file. What is given
/// is where the layers begin, and the example of a layer follows ([`Way`]).
fn way_of(to: Landing, aliases: &Aliases, from: &str) -> Way {
    let Landing::InUnit(unit) = to else {
        return Way::default();
    };
    match aliases.packages.iter().find(|p| p.root == unit.root) {
        Some(package) => {
            let dir = if package.exports.is_some() {
                String::new()
            } else {
                match relative(&package.root, &unit.path).as_str() {
                    "." => String::new(),
                    inside => inside.to_string(),
                }
            };
            Way::Package(if dir.is_empty() {
                package.name.clone()
            } else {
                join(&package.name, &dir)
            })
        }
        None => {
            let path = relative(parent(from), &unit.path);
            Way::Path(if path.starts_with('.') {
                path
            } else {
                format!("./{path}")
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::problems;
    use crate::tree::fake::Fake;
    use domain::declaration::{Declared, parse_declaration};
    use infrastructure::outbound::readers::Readers;

    /// An npm workspaces monorepo: `billing` (a package with no `exports`), `db` (shared, with `exports`) and `web`
    /// (an app), each package in `packages/` or `apps/` with its layers in its `src/`.
    const DECLARATION: &str = "stack = \"typescript\"\nareas = []\nui_design = \"none\"\n\
        [[unit]]\nname = \"billing\"\npath = \"packages/billing/src\"\nroot = \"packages/billing\"\n\
        [[unit]]\nname = \"db\"\npath = \"packages/db/src\"\nroot = \"packages/db\"\nshared = true\n\
        [[unit]]\nname = \"web\"\npath = \"apps/web/src\"\nroot = \"apps/web\"\nentry = [\"apps/web/src/main.ts\"]\n";

    const DB: &str = "{ \"name\": \"@acme/db\", \"exports\": { \".\": \"./src/index.ts\", \
        \"./client\": { \"types\": \"./src/infrastructure/outbound/client.ts\", \"default\": \"./dist/client.js\" }, \
        \"./protocol/*\": \"./src/protocol/*/index.ts\", \"./internal/*\": null } }";

    fn read(files: &[(&str, &str)]) -> super::Direction {
        let mut all = vec![
            (
                "packages/billing/package.json",
                "{ \"name\": \"@acme/billing\" }",
            ),
            ("packages/db/package.json", DB),
            (
                "apps/web/package.json",
                "{ \"name\": \"@acme/web\", \"private\": true }",
            ),
            ("packages/billing/src/domain/index.ts", ""),
            ("packages/billing/src/application/index.ts", ""),
            ("packages/billing/src/index.ts", ""),
            ("packages/db/src/index.ts", ""),
            ("packages/db/src/domain/index.ts", ""),
            ("packages/db/src/protocol/inbound/index.ts", ""),
            ("packages/db/src/infrastructure/outbound/client.ts", ""),
            ("apps/web/src/composition/index.ts", ""),
            ("apps/web/src/main.ts", ""),
        ];
        all.extend_from_slice(files);
        let declared = Declared::new(parse_declaration(DECLARATION).unwrap()).unwrap();
        problems(&Fake::new(&all), &Readers, &declared).unwrap()
    }

    fn details(direction: &super::Direction) -> Vec<&str> {
        direction
            .forbidden
            .iter()
            .map(|f| f.detail.as_str())
            .collect()
    }

    #[test]
    fn a_unit_package_name_with_a_subpath_that_lands_in_a_layer_of_another_unit_is_judged() {
        // What passed unseen while `@acme/db/client` was taken for a third-party package: `domain` of one unit reaching
        // the infrastructure of another
        let found = read(&[(
            "packages/billing/src/domain/song.ts",
            "import { x } from \"@acme/db/client\";\n",
        )]);
        assert!(found.problems.is_empty(), "{:?}", found.problems);
        assert_eq!(
            details(&found),
            [
                "packages/billing/src/domain/song.ts:1: imports @acme/db/client \
                 (packages/db/src/infrastructure/outbound/client.ts), in infrastructure.outbound of unit db; \
                 domain imports no other layer"
            ]
        );
    }

    #[test]
    fn a_pattern_in_exports_reaches_the_layer_it_names_and_the_table_allows_what_it_allows() {
        let found = read(&[
            (
                "packages/billing/src/application/play.ts",
                "import type { In } from \"@acme/db/protocol/inbound\";\n\
                 import { d } from \"@acme/db/protocol/inbound?raw\";\n",
            ),
            (
                "packages/billing/src/domain/song.ts",
                "import { In } from \"@acme/db/protocol/inbound\";\n",
            ),
        ]);
        assert_eq!(found.problems, Vec::<String>::new());
        assert_eq!(
            details(&found),
            [
                "packages/billing/src/domain/song.ts:1: imports @acme/db/protocol/inbound \
                 (packages/db/src/protocol/inbound/index.ts), in protocol.inbound of unit db; domain imports no \
                 other layer"
            ]
        );
    }

    #[test]
    fn the_package_itself_lands_in_the_unit_outside_its_layers() {
        let found = read(&[(
            "packages/billing/src/application/play.ts",
            "import { x } from \"@acme/db\";\n",
        )]);
        assert_eq!(found.problems, Vec::<String>::new());
        // A package with `exports`, which the pattern `./protocol/*` makes reach the layer
        assert_eq!(
            details(&found),
            [
                "packages/billing/src/application/play.ts:1: imports @acme/db (packages/db/src/index.ts), in the \
                 unit db outside its layers; import a layer through the package of the unit, such as \
                 @acme/db/protocol/inbound"
            ]
        );
    }

    /// What the units read of the shared ones, as (name, unit that reads it), in order.
    fn reads(direction: &super::Direction) -> Vec<(String, String)> {
        let mut reads: Vec<_> = direction
            .reads
            .iter()
            .map(|read| (read.name.clone(), read.by.clone()))
            .collect();
        reads.sort();
        reads
    }

    #[test]
    fn what_is_read_of_a_shared_unit_is_the_path_it_lands_in_and_each_name_the_import_takes() {
        let found = read(&[
            (
                "packages/billing/src/application/play.ts",
                "import type { In, Out as O } from \"@acme/db/protocol/inbound\";\n\
                 import * as all from \"@acme/db/protocol/inbound\";\n\
                 import client from \"@acme/db/client\";\n\
                 import \"@acme/db/client\";\n\
                 import { own } from \"../domain\";\n",
            ),
            (
                "apps/web/src/composition/wire.ts",
                "import { In } from \"@acme/db/protocol/inbound\";\n\
                 export * from \"@acme/db/client\";\n",
            ),
            // The package itself lands in the unit outside its layers, which is no name of a layer
            (
                "packages/billing/src/domain/song.ts",
                "import { x } from \"@acme/db\";\n",
            ),
        ]);
        // Each import of the package, by the unit it is in, is a link, with the one that lands outside the layers
        let mut links: Vec<(&str, &str)> = found
            .links
            .iter()
            .map(|link| (link.from.as_str(), link.to.as_str()))
            .collect();
        links.sort();
        links.dedup();
        assert_eq!(links, [("billing", "db"), ("web", "db")]);
        let said = |name: &str, by: &str| (name.to_string(), by.to_string());
        let inbound = "packages/db/src/protocol/inbound/index";
        let client = "packages/db/src/infrastructure/outbound/client";
        assert_eq!(
            reads(&found),
            [
                said(client, "billing"),
                said(&format!("{client}: *"), "web"),
                said(&format!("{client}: default"), "billing"),
                said(&format!("{inbound}: *"), "billing"),
                said(&format!("{inbound}: In"), "billing"),
                said(&format!("{inbound}: In"), "web"),
                said(&format!("{inbound}: Out"), "billing"),
            ]
        );
    }

    #[test]
    fn a_package_with_no_exports_is_told_to_import_a_layer_by_the_path_of_its_files() {
        let found = read(&[(
            "apps/web/src/composition/wire.ts",
            "import { x } from \"@acme/billing/src/index\";\n",
        )]);
        assert_eq!(found.problems, Vec::<String>::new());
        assert_eq!(
            details(&found),
            [
                "apps/web/src/composition/wire.ts:1: imports @acme/billing/src/index (packages/billing/src/index), \
                 in the unit billing outside its layers; import a layer through the package of the unit, such as \
                 @acme/billing/src/protocol/inbound"
            ]
        );
    }

    #[test]
    fn a_unit_with_no_package_is_told_to_import_a_layer_by_its_path_from_the_importing_file() {
        let declared = Declared::new(parse_declaration(DECLARATION).unwrap()).unwrap();
        let tree = Fake::new(&[
            ("packages/billing/src/domain/index.ts", ""),
            ("packages/billing/src/application/index.ts", ""),
            ("packages/billing/src/index.ts", ""),
            ("packages/db/src/domain/index.ts", ""),
            ("packages/db/src/index.ts", ""),
            ("apps/web/src/composition/index.ts", ""),
            ("apps/web/src/main.ts", ""),
            (
                "apps/web/src/composition/wire.ts",
                "import { y } from \"../../../../packages/billing/src/index\";\n",
            ),
            (
                "packages/billing/src/application/play.ts",
                "import { x } from \"../../../db/src/index\";\n",
            ),
        ]);
        let found = problems(&tree, &Readers, &declared).unwrap();
        assert_eq!(found.problems, Vec::<String>::new());
        assert_eq!(
            details(&found),
            [
                "packages/billing/src/application/play.ts:1: imports ../../../db/src/index \
                 (packages/db/src/index), in the unit db outside its layers; import a layer by its path, such as \
                 ../../../db/src/protocol/inbound",
                "apps/web/src/composition/wire.ts:1: imports ../../../../packages/billing/src/index \
                 (packages/billing/src/index), in the unit billing outside its layers; import a layer by its path, \
                 such as ../../../../packages/billing/src/protocol/inbound"
            ]
        );
    }

    #[test]
    fn a_subpath_exports_does_not_list_is_a_problem_and_not_a_silent_package() {
        let found = read(&[(
            "packages/billing/src/domain/song.ts",
            "import a from \"@acme/db/domain\";\nimport b from \"@acme/db/internal/x\";\n\
             import c from \"@acme/db/client/x\";\n",
        )]);
        assert!(found.forbidden.is_empty(), "{:?}", found.forbidden);
        let said = |line: usize, specifier: &str| {
            format!(
                "packages/billing/src/domain/song.ts:{line}: imports {specifier}, which packages/db/package.json \
                 does not export, so Node and tsc fail it and its layer cannot be judged: export the subpath in \
                 packages/db/package.json, or import one that is exported"
            )
        };
        assert_eq!(
            found.problems,
            [
                said(1, "@acme/db/domain"),
                said(2, "@acme/db/internal/x"),
                said(3, "@acme/db/client/x")
            ]
        );
    }

    #[test]
    fn an_export_that_leads_to_no_file_in_a_unit_is_a_problem_that_names_where_it_leads() {
        let declared = Declared::new(parse_declaration(DECLARATION).unwrap()).unwrap();
        let exports = "{ \"name\": \"@acme/db\", \"exports\": { \"./*\": \"./src/*/index.ts\", \
            \"./built/*\": \"./dist/*.js\", \"./esm/*\": \"./src/esm/*\", \"./typed/*\": \"./src/typed/*\" } }";
        let tree = Fake::new(&[
            (
                "packages/billing/package.json",
                "{ \"name\": \"@acme/billing\", \"exports\": { \"./*\": \"./src/*/index.ts\" } }",
            ),
            ("packages/db/package.json", exports),
            ("packages/billing/src/domain/index.ts", ""),
            ("packages/db/src/domain/index.ts", ""),
            ("packages/db/src/esm/x.ts", ""),
            ("packages/db/src/typed/y.d.ts", ""),
            // A rule the import breaks is still found where the path is wrong
            (
                "packages/db/src/domain/row.ts",
                "import a from \"@acme/billing/domain/nothing\";\n",
            ),
            (
                "packages/billing/src/domain/song.ts",
                "import a from \"@acme/db/domain\";\n\
                 import b from \"@acme/db/domain/invoice.ts\";\n\
                 import c from \"@acme/db/built/client\";\n\
                 import d from \"@acme/db/esm/x.js\";\n\
                 import e from \"@acme/db/typed/y\";\n\
                 import f from \"@acme/db/domain/invoice\";\n",
            ),
        ]);
        let found = problems(&tree, &Readers, &declared).unwrap();
        // `invoice.ts/index.ts` is no file, and `invoice/index.ts` is none either. What `./dist/` leads to is not in
        // the tree and lands in no unit, so it is not judged; `x.js` is built from `x.ts`, and `y` is `y.d.ts`
        let said = |file: &str, line: usize, specifier: &str, package: &str, leads: &str| {
            format!(
                "{file}:{line}: imports {specifier}, which packages/{package}/package.json exports as {leads}, where \
                 no file is: the import names no file the package exports. Fix that export in \
                 packages/{package}/package.json, or import a subpath that leads to a file"
            )
        };
        let song = "packages/billing/src/domain/song.ts";
        assert_eq!(
            found.problems,
            [
                said(
                    song,
                    2,
                    "@acme/db/domain/invoice.ts",
                    "db",
                    "packages/db/src/domain/invoice.ts/index.ts"
                ),
                said(
                    song,
                    6,
                    "@acme/db/domain/invoice",
                    "db",
                    "packages/db/src/domain/invoice/index.ts"
                ),
                said(
                    "packages/db/src/domain/row.ts",
                    1,
                    "@acme/billing/domain/nothing",
                    "billing",
                    "packages/billing/src/domain/nothing/index.ts"
                ),
            ]
        );
        // The rule between units is still judged, by where the export leads
        let judged: Vec<&str> = details(&found)
            .into_iter()
            .filter(|detail| detail.starts_with("packages/db/"))
            .collect();
        assert_eq!(
            judged,
            [
                "packages/db/src/domain/row.ts:1: imports @acme/billing/domain/nothing \
                 (packages/billing/src/domain/nothing/index.ts), in domain of unit billing; the shared unit db \
                 imports only shared units, and billing is not shared"
            ]
        );
    }

    #[test]
    fn a_package_without_exports_is_reached_by_the_path_of_its_files() {
        let found = read(&[
            ("packages/billing/src/composition/index.ts", ""),
            (
                "apps/web/src/composition/wire.ts",
                "import { a } from \"@acme/billing/src/domain\";\n\
                 import { b } from \"@acme/billing/src/application/index\";\n\
                 import { c } from \"@acme/billing/src/composition\";\n",
            ),
            (
                "packages/db/src/domain/row.ts",
                "import { a } from \"@acme/billing/src/domain\";\n",
            ),
        ]);
        assert_eq!(found.problems, Vec::<String>::new());
        // The composition of an app wires a unit that is not shared, through its composition and protocol alone; a
        // shared unit imports shared units only
        let wired = "the composition of an app imports only the composition and protocol of a unit that is not \
                     shared: billing builds its own insides in its own composition";
        assert_eq!(
            details(&found),
            [
                "packages/db/src/domain/row.ts:1: imports @acme/billing/src/domain (packages/billing/src/domain), \
                 in domain of unit billing; the shared unit db imports only shared units, and billing is not shared",
                &format!(
                    "apps/web/src/composition/wire.ts:1: imports @acme/billing/src/domain \
                     (packages/billing/src/domain), in domain of unit billing; {wired}"
                ),
                &format!(
                    "apps/web/src/composition/wire.ts:2: imports @acme/billing/src/application/index \
                     (packages/billing/src/application/index), in application of unit billing; {wired}"
                ),
            ]
        );
    }

    #[test]
    fn a_relative_path_into_another_unit_is_judged_by_that_unit_too() {
        let found = read(&[(
            "packages/billing/src/domain/song.ts",
            "import { x } from \"../../../db/src/infrastructure/outbound/client\";\n",
        )]);
        assert_eq!(found.problems, Vec::<String>::new());
        assert_eq!(found.forbidden.len(), 1, "{:?}", details(&found));
        assert!(details(&found)[0].contains("in infrastructure.outbound of unit db"));
    }

    #[test]
    fn a_name_that_is_no_units_package_is_not_judged() {
        let found = read(&[(
            "packages/billing/src/domain/song.ts",
            "import React from \"react\";\nimport { x } from \"@acme/other/client\";\n\
             import { y } from \"@acme/db-extra\";\n",
        )]);
        assert_eq!(found.problems, Vec::<String>::new());
        assert!(found.forbidden.is_empty());
    }

    #[test]
    fn a_package_json_that_cannot_be_read_is_a_problem() {
        let declared = Declared::new(parse_declaration(DECLARATION).unwrap()).unwrap();
        let tree = Fake::new(&[
            (
                "packages/db/package.json",
                "{ \"name\": \"@acme/db\", \"exports\": 3 }",
            ),
            ("packages/billing/src/domain/index.ts", ""),
            ("packages/db/src/domain/index.ts", ""),
        ]);
        let found = problems(&tree, &Readers, &declared).unwrap();
        assert_eq!(found.problems.len(), 1, "{:?}", found.problems);
        assert!(
            found.problems[0].starts_with("packages/db/package.json: exports holds 3"),
            "{:?}",
            found.problems
        );
        assert!(
            found.problems[0].ends_with(
                "so the package @acme/db is not known; imports through it cannot be judged until it is fixed"
            )
        );
    }

    #[test]
    fn a_project_that_declares_no_unit_reads_no_package_json() {
        let declared = Declared::new(
            parse_declaration("stack = \"typescript\"\nareas = []\nui_design = \"none\"\n")
                .unwrap(),
        )
        .unwrap();
        let tree = Fake::new(&[
            (
                "src/package.json",
                "{ \"name\": \"@acme/db\", \"exports\": {} }",
            ),
            (
                "src/domain/song.ts",
                "import { x } from \"@acme/db/client\";\n",
            ),
            ("src/application/play.ts", ""),
        ]);
        let found = problems(&tree, &Readers, &declared).unwrap();
        assert!(
            found.problems.is_empty() && found.forbidden.is_empty(),
            "{found:?}"
        );
        assert_eq!(tree.reads_of("src/package.json"), 0);
    }

    #[test]
    fn a_test_is_no_layers_code_so_what_it_imports_is_not_judged_and_what_imports_it_is() {
        let declared = Declared::new(
            parse_declaration("stack = \"typescript\"\nareas = []\nui_design = \"none\"\n")
                .unwrap(),
        )
        .unwrap();
        let tree = Fake::new(&[
            ("src/domain/index.ts", ""),
            ("src/application/index.ts", ""),
            ("src/application/play.ts", ""),
            // Tests import what their layer may not, and one another
            (
                "src/domain/song.test.ts",
                "import { play } from \"../application/play\";\nimport { h } from \"./__tests__/helper\";\n",
            ),
            (
                "src/domain/__tests__/helper.ts",
                "import { play } from \"../../application/play\";\n",
            ),
            ("src/domain/__tests__/index.ts", ""),
            (
                "src/domain/song.spec.js",
                "import { play } from \"../application/play.ts\";\n",
            ),
            // Code of a layer does not import one, by any spelling of it
            (
                "src/application/pay.ts",
                "import { a } from \"../domain/song.test\";\nimport { b } from \"../domain/song.spec.js\";\n\
                 import { c } from \"../domain/__tests__\";\nimport { d } from \"../domain/__tests__/helper.ts\";\n\
                 import { e } from \"../domain/song.test.json\";\nimport { f } from \"../domain/song\";\n",
            ),
            // These are no tests: the name has to be whole, and a JSON file with a test's name is no source
            ("src/domain/song.test.json", "{}"),
            ("src/domain/song.ts", ""),
        ]);
        let found = problems(&tree, &Readers, &declared).unwrap();
        assert!(found.problems.is_empty(), "{:?}", found.problems);
        let said: Vec<&str> = found.forbidden.iter().map(|f| f.detail.as_str()).collect();
        let at = |line: usize, import: &str, target: &str| {
            format!(
                "src/application/pay.ts:{line}: imports {import} ({target}), a test (*.test.*, *.spec.*, \
                 __tests__/); code that is not a test imports no test: move what it needs out of the test into a \
                 layer"
            )
        };
        assert_eq!(
            said,
            [
                at(1, "../domain/song.test", "src/domain/song.test"),
                at(2, "../domain/song.spec.js", "src/domain/song.spec.js"),
                at(3, "../domain/__tests__", "src/domain/__tests__"),
                at(
                    4,
                    "../domain/__tests__/helper.ts",
                    "src/domain/__tests__/helper.ts"
                ),
            ]
        );
        // Judged as the finding of an import is: an approval keeps it, by its file and import
        assert_eq!(found.forbidden[0].file, "src/application/pay.ts");
        assert_eq!(found.forbidden[0].import, "../domain/song.test");
    }

    /// What `domain` of billing defines, for what its `protocol.inbound` imports of it.
    const KINDS: &str = "export const KINDS = [\"a\", \"b\"] as const;\n\
        export type Kind = (typeof KINDS)[number];\n\
        export interface Song { title: string }\n\
        export class Album {}\n\
        export function play(): void {}\n\
        export enum Mode { On }\n";

    const INBOUND: &str = "packages/billing/src/protocol/inbound/play.ts";

    /// The findings for the import statements of billing's `protocol.inbound`, with `KINDS` as `domain/kinds.ts` and
    /// a barrel `domain/index.ts` that only re-exports.
    fn inbound(imports: &str) -> super::Direction {
        read(&[
            ("packages/billing/src/domain/kinds.ts", KINDS),
            (
                "packages/billing/src/domain/index.ts",
                "export { KINDS } from \"./kinds\";\n",
            ),
            (INBOUND, imports),
        ])
    }

    fn said(line: usize, import: &str, target: &str, ways: &str) -> String {
        format!(
            "{INBOUND}:{line}: imports {import} ({target}), in domain; protocol.inbound may import only vocabularies, \
             by name, from the file of domain that defines them: {ways}"
        )
    }

    #[test]
    fn a_protocol_inbound_imports_the_vocabularies_of_its_own_domain_by_name_from_the_file_that_defines_them()
     {
        let found = inbound(
            "import { KINDS, type Kind } from \"../../domain/kinds\";\n\
             import type { Kind as K } from \"../../domain/kinds.ts\";\n\
             import { KINDS as ALL } from \"../../domain/kinds\";\n\
             export { KINDS, type Kind } from \"../../domain/kinds\";\n",
        );
        assert_eq!(found.problems, Vec::<String>::new());
        assert_eq!(details(&found), Vec::<&str>::new());
    }

    #[test]
    fn what_is_no_vocabulary_of_the_file_fails_with_how_to_write_it_instead() {
        let found = inbound(
            "import { Song } from \"../../domain/kinds\";\n\
             import { Album, play } from \"../../domain/kinds\";\n\
             import { Mode } from \"../../domain/kinds\";\n\
             import { Kind, Absent } from \"../../domain/kinds\";\n",
        );
        let target = "packages/billing/src/domain/kinds";
        let specifier = "../../domain/kinds";
        let shape = "a record, class, function or type, no vocabulary: write the shape the port shows in \
                     protocol.inbound, and convert in application";
        assert_eq!(
            details(&found),
            [
                said(1, specifier, target, &format!("`Song` is {shape}")),
                said(
                    2,
                    specifier,
                    target,
                    &format!("`Album`, `play` are each {shape}")
                ),
                said(
                    3,
                    specifier,
                    target,
                    "`Mode` is a TypeScript enum, which no layer holds: write it as an array `as const`, with a type \
                     of its elements, and import that"
                ),
                said(
                    4,
                    specifier,
                    target,
                    "`Absent` is not defined in this file (a name it only re-exports is not followed): import it \
                     from the file that defines it"
                ),
            ]
        );
    }

    #[test]
    fn a_barrel_that_only_re_exports_a_vocabulary_does_not_define_it() {
        let found = inbound("import { KINDS } from \"../../domain\";\n");
        assert_eq!(
            details(&found),
            [said(
                1,
                "../../domain",
                "packages/billing/src/domain",
                "`KINDS` is not defined in this file (a name it only re-exports is not followed): import it from \
                 the file that defines it"
            )]
        );
    }

    #[test]
    fn an_import_that_names_nothing_fails_for_it() {
        let found = inbound(
            "import * as kinds from \"../../domain/kinds\";\n\
             import kinds2 from \"../../domain/kinds\";\n\
             import \"../../domain/kinds\";\n\
             export * from \"../../domain/kinds\";\n",
        );
        let nothing = "this import takes no name, and what it brings in cannot be told: import each vocabulary by name";
        let target = "packages/billing/src/domain/kinds";
        assert_eq!(
            details(&found),
            (1..=4)
                .map(|line| said(line, "../../domain/kinds", target, nothing))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn the_approval_of_such_an_import_is_keyed_by_the_file_and_the_specifier_as_it_always_was() {
        let found = inbound("import { Song } from \"../../domain/kinds\";\n");
        assert_eq!(found.forbidden.len(), 1);
        assert_eq!(found.forbidden[0].file, INBOUND);
        assert_eq!(found.forbidden[0].import, "../../domain/kinds");
        assert_eq!(found.forbidden[0].rule, domain::direction::Rule::Direction);
    }

    #[test]
    fn a_landing_file_that_is_not_there_or_cannot_be_parsed_is_judged_by_the_table() {
        let found = read(&[
            (
                "packages/billing/src/domain/broken.ts",
                "export const = ;\nexport {\n",
            ),
            (
                INBOUND,
                "import { A } from \"../../domain/broken\";\nimport { B } from \"../../domain/nowhere\";\n",
            ),
        ]);
        let table = |line: usize, specifier: &str, target: &str| {
            format!(
                "{INBOUND}:{line}: imports {specifier} ({target}), in domain; protocol.inbound imports no other layer"
            )
        };
        assert_eq!(
            details(&found),
            [
                table(
                    1,
                    "../../domain/broken",
                    "packages/billing/src/domain/broken"
                ),
                table(
                    2,
                    "../../domain/nowhere",
                    "packages/billing/src/domain/nowhere"
                ),
            ]
        );
    }

    #[test]
    fn another_place_importing_the_domain_and_another_units_domain_are_judged_as_before() {
        let found = read(&[
            ("packages/billing/src/domain/kinds.ts", KINDS),
            ("packages/db/src/domain/kinds.ts", KINDS),
            (
                "packages/billing/src/application/play.ts",
                "import { Song, Mode } from \"../domain/kinds\";\n",
            ),
            // The domain of a shared unit is read whole by the others, as it always was
            (
                INBOUND,
                "import { Song } from \"../../../../db/src/domain/kinds\";\n",
            ),
            // The domain of a unit that is not shared is not read by another unit
            (
                "apps/web/src/protocol/inbound/play.ts",
                "import { KINDS } from \"../../../../../packages/billing/src/domain/kinds\";\n",
            ),
        ]);
        assert_eq!(found.problems, Vec::<String>::new());
        assert_eq!(
            details(&found),
            [
                "apps/web/src/protocol/inbound/play.ts:1: imports ../../../../../packages/billing/src/domain/kinds \
                 (packages/billing/src/domain/kinds), in domain of unit billing; web imports no unit that is not \
                 shared, and billing is not: an app's composition wires the two"
            ]
        );
    }
}

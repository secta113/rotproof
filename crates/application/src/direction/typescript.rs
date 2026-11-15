//! The direction check of a TypeScript project, read from the tree.

use std::io;

use super::{Direction, entry_files, layer_files, misread};
use crate::code::lands;
use crate::tree::{read_code, resolved};
use domain::code::{Aliases, Reach, is_source, without_extension};
use domain::direction::{Landing, Way, edge, judged_landing, landing_of};
use domain::layers::{Layout, Place};
use domain::paths::{join, parent, relative};
use domain::readers::read_of;
use domain::units::Unit;
use protocol::outbound::Parsers;
use protocol::outbound::Tree;

/// The direction check of a TypeScript project: every import of a source file in a layer, resolved to a path
/// (`typescript.rs`), judged by the place that path sits in, or by the unit it sits in outside the places. An import
/// through the name of a unit's package resolves through its `package.json`, as `exports` says; one of a subpath the
/// package does not export is a problem, since where it lands is not known.
pub(super) fn typescript(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    layout: &Layout,
    places: &[&Place],
    entries: &[Place],
    units: &[Unit],
) -> io::Result<Direction> {
    let mut forbidden = Vec::new();
    let mut edges = Vec::new();
    let mut reads = Vec::new();
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
        .map(|why| format!("{why}; imports through it are not checked"))
        .collect();
    let mut files = layer_files(tree, layout, places, str::to_string)?;
    files.extend(entry_files(tree, entries));
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
        for (line, import) in read.imports {
            let specifier = import.specifier;
            let target = match lands(tree, &aliases, &path, &specifier) {
                Reach::At(Some(target)) => resolved(tree, &target),
                Reach::At(None) => continue,
                Reach::Unlisted { config } => {
                    found.push(format!(
                        "{path}:{line}: imports {specifier}, which {config} does not export, so it is not checked"
                    ));
                    continue;
                }
            };
            let Some(to) = landing_of(&target, places, units) else {
                continue;
            };
            forbidden.extend(judged_landing(
                &path,
                line,
                &specifier,
                &format!("imports {specifier} ({target})"),
                from,
                to,
                &way_of(to, &aliases, &path),
            ));
            edges.extend(edge(&path, line, &specifier, from, to));
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
    Ok(Direction {
        problems: found,
        forbidden,
        edges,
        reads,
    })
}

/// How an import of a layer of the unit `to` is written from the file `from`, for the finding that says an import
/// landed in the unit outside its layers (an import that lands in a place needs no words). Through the unit's package
/// when it has one: `<name>/protocol/inbound` where its `exports` lists subpaths (a pattern such as `./protocol/*`
/// makes it so), else the path of the layer in the package, as Node reads a package with no `exports`
/// (`@acme/billing/src/protocol/inbound`); a unit with no package by the relative path from the importing file.
fn way_of(to: Landing, aliases: &Aliases, from: &str) -> Way {
    let Landing::InUnit(unit) = to else {
        return Way::default();
    };
    let layer = |dir: &str| join(dir, "protocol/inbound");
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
            Way::Package(format!("{}/{}", package.name, layer(&dir)))
        }
        None => {
            let path = relative(parent(from), &layer(&unit.path));
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
    const DECLARATION: &str = "stack = \"typescript\"\nareas = []\nui = \"none\"\n\
        [[unit]]\nname = \"billing\"\npath = \"packages/billing/src\"\nroot = \"packages/billing\"\n\
        [[unit]]\nname = \"db\"\npath = \"packages/db/src\"\nroot = \"packages/db\"\nshared = true\n\
        [[unit]]\nname = \"web\"\npath = \"apps/web/src\"\nroot = \"apps/web\"\nentry = \"apps/web/src/main.ts\"\n";

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
                 does not export, so it is not checked"
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
    fn a_package_without_exports_is_reached_by_the_path_of_its_files() {
        let found = read(&[
            (
                "apps/web/src/composition/wire.ts",
                "import { a } from \"@acme/billing/src/domain\";\n\
                 import { b } from \"@acme/billing/src/application/index\";\n",
            ),
            (
                "packages/db/src/domain/row.ts",
                "import { a } from \"@acme/billing/src/domain\";\n",
            ),
        ]);
        assert_eq!(found.problems, Vec::<String>::new());
        // The composition of an app wires a unit that is not shared; a shared unit imports shared units only
        assert_eq!(
            details(&found),
            [
                "packages/db/src/domain/row.ts:1: imports @acme/billing/src/domain (packages/billing/src/domain), \
                 in domain of unit billing; the shared unit db imports only shared units, and billing is not shared"
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
                "so the package @acme/db is not known; imports through it are not checked"
            )
        );
    }

    #[test]
    fn a_project_that_declares_no_unit_reads_no_package_json() {
        let declared = Declared::new(
            parse_declaration("stack = \"typescript\"\nareas = []\nui = \"none\"\n").unwrap(),
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
}

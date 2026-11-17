//! The dependencies check of a TypeScript project: what each `package.json` lists, against what the source files of its
//! unit import.
//!
//! The files are those the direction check reads: every source file in a layer, and the entry files; and the tests, the
//! files that the layout names as such (`*.test.*`, `__tests__/`), wherever they sit in the unit's `path`. A test uses
//! `devDependencies` and the code of a layer does not, and a unit below the root of the project may use those of the
//! project's own `package.json` too, which npm hoists to every package. An import is one of
//!
//! - **a file of the project**: a relative path, one from the root, an alias of `paths` or a module under a `baseUrl`:
//!   nothing is needed;
//! - **a unit's package**: a specifier that starts with the `name` of the `package.json` at another unit's root, which
//!   the importer's manifest lists by that name (`"@shop/kernel": "workspace:*"`);
//! - **a builtin of Node**, a path, a subpath import (`#name`) or a scheme: nothing is needed. A name Node reads as a
//!   builtin without `node:` (`fs`, `buffer`) is a package when the manifest lists one of that name;
//! - **a package**: `lodash/fp` is `lodash`, and `@shop/kernel/utils` is `@shop/kernel`.
//!
//! A unit whose root holds no `package.json` is not compared with one.

use std::collections::BTreeMap;
use std::io;

use super::{Read, listing, without_import};
use crate::code::lands;
use crate::direction::{entry_files, layer_files};
use crate::tree::{code_files, read_code};
use domain::code::{Aliases, Package, Reach, is_source};
use domain::declaration::Declared;
use domain::dependencies::typescript::{Specifier, specifier};
use domain::dependencies::{Dep, Kind, Listing, judge};
use domain::layers::{Language, Layout, Place, table};
use domain::paths::{join, within};
use domain::units::Unit;
use protocol::outbound::{Parsers, Tree};

/// What the code of one unit needs of a specifier it imports.
enum Needs {
    Nothing,
    /// Another unit's package, by its name, and the unit that holds it
    Unit(String, String),
    /// A package of npm
    Package(String),
}

/// What an import needs, from the file at `path` of the unit whose manifest is in `directory`: where it lands says what
/// it is, as the direction check reads it.
fn needs(
    tree: &dyn Tree,
    aliases: &Aliases,
    units: &[Unit],
    listing: &Listing,
    directory: &str,
    path: &str,
    from: &str,
) -> Needs {
    let of = |package: &Package| {
        if package.root == directory {
            return Needs::Nothing;
        }
        let name = units
            .iter()
            .find(|unit| unit.declared && unit.root == package.root)
            .map_or(package.name.clone(), |unit| unit.name.clone());
        Needs::Unit(package.name.clone(), name)
    };
    match lands(tree, aliases, path, from) {
        // What lands in a unit's directory through its name is its package; a path, an alias and a `baseUrl` land
        // there too, and are no package
        Reach::At(Some(target)) => match aliases.package_of(from) {
            Some((package, _)) if package.root.is_empty() || within(&target, &package.root) => {
                of(package)
            }
            _ => Needs::Nothing,
        },
        Reach::At(None) => match specifier(from) {
            Specifier::Neither => Needs::Nothing,
            Specifier::Bare(name) if listing.find(&name, &ALL).is_none() => Needs::Nothing,
            Specifier::Bare(name) | Specifier::Package(name) => Needs::Package(name),
        },
        // The package is a unit's, and the direction check says what is wrong with the subpath
        Reach::Unlisted { .. } | Reach::Missing { .. } => aliases
            .package_of(from)
            .map_or(Needs::Nothing, |(package, _)| of(package)),
    }
}

/// Every kind of dependency, for a name that is listed at all.
const ALL: [Kind; 3] = [Kind::Runtime, Kind::Optional, Kind::Development];

/// The dependencies check of a TypeScript project, into `found`.
pub(super) fn typescript(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    declared: &Declared,
    layout: &Layout,
    places: &[&Place],
    moving: &[String],
    found: &mut Vec<String>,
) -> io::Result<Read> {
    let units = &declared.units;
    let entries = declared.entries(&table());
    let directory_of = |unit: &Unit| {
        if unit.declared {
            unit.root.clone()
        } else {
            String::new()
        }
    };
    let manifest_of = |unit: &Unit| join(&directory_of(unit), &layout.manifest);
    let mut listings: BTreeMap<String, Listing> = BTreeMap::new();
    let mut tried: Vec<String> = Vec::new();
    for unit in units {
        let path = manifest_of(unit);
        if !tried.contains(&path) {
            tried.push(path.clone());
            if let Some(read) = listing(tree, parsers, Language::TypeScript, &path, found)? {
                listings.insert(path, read);
            }
        }
    }
    // The project's own manifest, whose development dependencies every package of a workspace may use
    let root_manifest = layout.manifest.clone();
    let root = match listings.get(&root_manifest) {
        Some(root) => Some(root.clone()),
        None if units.iter().any(|unit| !directory_of(unit).is_empty()) => {
            listing(tree, parsers, Language::TypeScript, &root_manifest, found)?
        }
        None => None,
    };
    let roots: Vec<String> = units
        .iter()
        .filter(|unit| unit.declared)
        .map(|unit| unit.root.clone())
        .collect();
    let aliases = parsers.typescript_aliases(tree, &roots)?;
    let mut files: Vec<(String, String)> = layer_files(tree, layout, places, str::to_string)?
        .into_iter()
        .map(|(path, place)| (path, place.unit.name.clone()))
        .collect();
    files.extend(
        entry_files(tree, &entries)
            .into_iter()
            .map(|(path, place)| (path, place.unit.name.clone())),
    );
    // The tests that sit in no layer (`src/helpers.test.ts`) are read as tests too, in the unit that holds their path
    // closest: no check but this one reads them, since structure and direction leave a test alone
    for unit in units {
        for path in code_files(tree, &unit.path, is_source)? {
            let held = units
                .iter()
                .filter(|other| within(&path, &other.path))
                .max_by_key(|other| other.path.len());
            if layout.is_test_file(&path)
                && held.is_some_and(|held| held.name == unit.name)
                && !declared.is_not_checked(&path)
                && !files.iter().any(|(read, _)| *read == path)
            {
                files.push((path, unit.name.clone()));
            }
        }
    }
    let mut uses: BTreeMap<String, Vec<domain::dependencies::Use>> = BTreeMap::new();
    // Every import of a package, whether or not the unit has a manifest to compare it with: a unit without one lists
    // nothing, so a name Node also has as a builtin (`fs`) is Node's
    let mut packages: Vec<domain::dependencies::Use> = Vec::new();
    let nothing = Listing::default();
    for (path, unit_name) in files.iter().filter(|(path, _)| is_source(path)) {
        let Some(unit) = units.iter().find(|unit| unit.name == *unit_name) else {
            continue;
        };
        let manifest = manifest_of(unit);
        let own = listings.get(&manifest).unwrap_or(&nothing);
        let Some(source) = read_code(tree, path, "imports", &mut Vec::new())? else {
            continue;
        };
        for (line, import) in parsers.typescript(&source, path).imports {
            let (package, named) = match needs(
                tree,
                &aliases,
                units,
                own,
                &directory_of(unit),
                path,
                &import.specifier,
            ) {
                Needs::Nothing => continue,
                Needs::Unit(package, name) => (package, Some(name)),
                Needs::Package(package) => (package, None),
            };
            let used = domain::dependencies::Use {
                file: path.clone(),
                line,
                shown: import.specifier,
                package,
                test: layout.is_test_file(path),
                unit: named,
            };
            if used.unit.is_none() {
                packages.push(used.clone());
            }
            uses.entry(manifest.clone()).or_default().push(used);
        }
    }
    // The files that are moving, for the packages they import alone: no manifest is compared with them, as none is with
    // the code outside the layers
    let mut moved: Vec<domain::dependencies::Use> = Vec::new();
    for path in moving.iter().filter(|path| is_source(path)) {
        let Some(unit) = units
            .iter()
            .filter(|unit| within(path, &unit.path))
            .max_by_key(|unit| unit.path.len())
            .or(units.first())
        else {
            continue;
        };
        let own = listings.get(&manifest_of(unit)).unwrap_or(&nothing);
        let Some(source) = read_code(tree, path, "imports", &mut Vec::new())? else {
            continue;
        };
        for (line, import) in parsers.typescript(&source, path).imports {
            if let Needs::Package(package) = needs(
                tree,
                &aliases,
                units,
                own,
                &directory_of(unit),
                path,
                &import.specifier,
            ) {
                moved.push(domain::dependencies::Use {
                    file: path.clone(),
                    line,
                    shown: import.specifier,
                    package,
                    test: false,
                    unit: None,
                });
            }
        }
    }
    let without = without_import(declared, Language::TypeScript);
    let mut read = Read {
        packages,
        moving: moved,
        ..Read::default()
    };
    for (manifest, own) in &listings {
        read.any = true;
        read.runtime.extend(
            own.deps
                .iter()
                .filter(|dep| dep.kind == Kind::Runtime)
                .map(|dep| dep.name.clone()),
        );
        read.listed
            .extend(own.deps.iter().map(|dep| dep.name.clone()));
        let mut judged = own.clone();
        if let Some(root) = root.as_ref().filter(|_| *manifest != root_manifest) {
            let own = judged.clone();
            for dep in root
                .deps
                .iter()
                .filter(|dep| own.find(&dep.name, &ALL).is_none())
            {
                judged.deps.push(Dep {
                    kind: Kind::Development,
                    line: 0,
                    ..dep.clone()
                });
            }
        }
        let used = uses.get(manifest).map_or(&[][..], Vec::as_slice);
        for problem in judge(Language::TypeScript, &judged, used, &without) {
            found.push(problem.said(Language::TypeScript, manifest, None));
        }
    }
    // What the project's own manifest lists is usable in every package of a workspace, hoisted
    if let Some(root) = &root {
        read.listed
            .extend(root.deps.iter().map(|dep| dep.name.clone()));
    }
    Ok(read)
}

#[cfg(test)]
mod tests {
    use super::super::problems;
    use crate::tree::fake::Fake;
    use domain::declaration::{Declared, parse_declaration};
    use infrastructure::outbound::readers::Readers;

    /// What the dependencies check finds in a TypeScript project declared as `declaration` (the fields after `ui`),
    /// over `files`.
    fn check(declaration: &str, files: &[(&str, &str)]) -> Vec<String> {
        let text = format!(
            "stack = \"typescript\"\nareas = []\nui = \"none\"\nfiles = \"2.0.0\"\n{declaration}"
        );
        let declared =
            Declared::new(parse_declaration(&text).unwrap()).unwrap_or_else(|e| panic!("{e:#?}"));
        problems(&Fake::new(files), &Readers, &declared).unwrap()
    }

    const ONE: [(&str, &str); 2] = [
        ("src/domain/index.ts", ""),
        ("src/application/index.ts", ""),
    ];

    fn with<'a>(files: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
        let mut all = ONE.to_vec();
        all.extend_from_slice(files);
        all
    }

    const PACKAGE: &str = "{\n  \"name\": \"shop\",\n  \"dependencies\": {\n    \"react\": \"^18\",\n    \"lodash\": \"^4\"\n  \
                           },\n  \"devDependencies\": {\n    \"vitest\": \"^2\",\n    \"@types/node\": \"^22\"\n  }\n}\n";

    #[test]
    fn a_manifest_that_agrees_with_the_imports_passes() {
        let found = check(
            "",
            &with(&[
                ("package.json", PACKAGE),
                (
                    "src/application/a.ts",
                    "import { x } from \"./b\";\nimport fp from \"lodash/fp\";\nimport * as fs from \"node:fs\";\n\
                     import path from \"path\";\nimport React from \"react\";\nimport { y } from \"../domain\";\n",
                ),
                ("src/application/b.ts", "export const x = 1;\n"),
                (
                    "src/application/b.test.ts",
                    "import { test } from \"vitest\";\nimport { x } from \"./b\";\n",
                ),
            ]),
        );
        assert!(found.is_empty(), "{found:#?}");
    }

    #[test]
    fn a_package_imported_and_not_listed_is_missing_and_one_listed_and_not_imported_is_unused() {
        let found = check(
            "",
            &with(&[
                ("package.json", PACKAGE),
                (
                    "src/application/a.ts",
                    "import React from \"react\";\nimport z from \"zod\";\nimport { a } from \"zod/v4\";\n",
                ),
            ]),
        );
        assert_eq!(found.len(), 2, "{found:#?}");
        assert_eq!(
            found[0],
            "src/application/a.ts:2: imports zod, which needs the package zod, and package.json does not list it \
             (1 more import of it): add it to dependencies"
        );
        assert_eq!(
            found[1],
            "package.json:5: lists the dependency lodash, which no code imports: take it out, or, when the code uses \
             it without importing it, list it in without_import under [dependencies] in .config/rotproof.toml"
        );
    }

    #[test]
    fn a_name_node_has_without_the_prefix_is_a_package_when_the_manifest_lists_one() {
        let package = PACKAGE.replace(
            "\"react\": \"^18\"",
            "\"buffer\": \"^6\", \"react\": \"^18\"",
        );
        let uses = "import { Buffer } from \"buffer\";
import React from \"react\";
import _ from \"lodash\";
";
        let found = check(
            "",
            &with(&[("package.json", &package), ("src/application/a.ts", uses)]),
        );
        assert!(found.is_empty(), "{found:#?}");
        // Imported as Node's own, `buffer` is no use of the one listed
        let builtin = "import fs from \"fs\";
import React from \"react\";
import _ from \"lodash\";
";
        let found = check(
            "",
            &with(&[
                ("package.json", &package),
                ("src/application/a.ts", builtin),
            ]),
        );
        assert_eq!(found.len(), 1, "{found:#?}");
        assert!(
            found[0].contains("lists the dependency buffer"),
            "{found:#?}"
        );
    }

    #[test]
    fn a_development_dependency_is_used_by_a_test_alone_and_never_unused_and_types_are_not_imported()
     {
        let uses = "import React from \"react\";\nimport _ from \"lodash\";\n";
        let tests = [
            // By its name, beside the code, in a directory of tests, and in no layer
            ("src/domain/a.test.ts", "import { test } from \"vitest\";\n"),
            (
                "src/domain/b.spec.tsx",
                "import { test } from \"vitest\";\n",
            ),
            (
                "src/domain/__tests__/c.ts",
                "import { test } from \"vitest\";\n",
            ),
            ("src/helper.test.ts", "import { test } from \"vitest\";\n"),
        ];
        let mut files = vec![("package.json", PACKAGE), ("src/domain/a.ts", uses)];
        files.extend(tests);
        let found = check("", &with(&files));
        assert!(found.is_empty(), "{found:#?}");
        // The code of a layer does not: the first import is said, with how many more there are
        let mut files = vec![
            ("package.json", PACKAGE),
            ("src/domain/a.ts", uses),
            (
                "src/domain/b.ts",
                "import { test } from \"vitest\";\nimport { it } from \"vitest\";\n",
            ),
            (
                "src/main.tsx",
                "import { test } from \"vitest\";\nimport { y } from \"./domain\";\n",
            ),
        ];
        files.extend(tests);
        let found = check("", &with(&files));
        assert_eq!(
            found,
            [
                "src/domain/b.ts:1: imports vitest, which package.json:8 lists for development only (2 more imports \
                 of it): the code of a layer uses runtime dependencies only, so move vitest to dependencies, or \
                 import it in a test alone (a file named *.test.* or *.spec.*, or in __tests__/)"
            ]
        );
        // A file outside `src/` is not read: `vite.config.ts` imports what it needs
        let mut files = vec![
            ("package.json", PACKAGE),
            ("src/domain/a.ts", uses),
            (
                "vite.config.ts",
                "import { defineConfig } from \"vitest/config\";\n",
            ),
        ];
        files.extend(tests);
        let found = check("", &with(&files));
        assert!(found.is_empty(), "{found:#?}");
        // A test in a path `unchecked` keeps is not read
        let found = check(
            "unchecked = [\"src/legacy\"]\n",
            &with(&[
                (
                    "package.json",
                    "{ \"name\": \"shop\", \"dependencies\": {} }",
                ),
                ("src/legacy/a.test.ts", "import z from \"zod\";\n"),
            ]),
        );
        assert!(found.is_empty(), "{found:#?}");
        // `@types/*` in dependencies is imported by nothing and is not unused
        let found = check(
            "",
            &with(&[
                (
                    "package.json",
                    "{ \"name\": \"shop\", \"dependencies\": { \"@types/node\": \"^22\" } }",
                ),
                ("src/domain/a.ts", "export const a = 1;\n"),
            ]),
        );
        assert!(found.is_empty(), "{found:#?}");
    }

    #[test]
    fn a_dependency_listed_in_without_import_counts_as_used() {
        let files = with(&[
            ("package.json", PACKAGE),
            ("src/domain/a.ts", "import React from \"react\";\n"),
        ]);
        assert_eq!(check("", &files).len(), 1);
        let found = check("[dependencies]\nwithout_import = [\"lodash\"]\n", &files);
        assert!(found.is_empty(), "{found:#?}");
    }

    #[test]
    fn a_unit_lists_the_packages_of_the_units_it_imports_and_may_use_the_root_dev_dependencies() {
        let declaration = "[[unit]]\nname = \"billing\"\npath = \"packages/billing/src\"\nroot = \"packages/billing\"\n\
                           [[unit]]\nname = \"kernel\"\npath = \"packages/kernel/src\"\nroot = \"packages/kernel\"\nshared = true\n";
        let files = |billing: &'static str| -> Vec<(&'static str, &'static str)> {
            vec![
                (
                    "package.json",
                    "{ \"private\": true, \"devDependencies\": { \"vitest\": \"^2\" } }",
                ),
                (
                    "packages/kernel/package.json",
                    "{ \"name\": \"@shop/kernel\", \"exports\": { \"./domain\": \"./src/domain/index.ts\" } }",
                ),
                ("packages/kernel/src/domain/index.ts", ""),
                ("packages/billing/package.json", billing),
                ("packages/billing/src/domain/index.ts", ""),
                (
                    "packages/billing/src/application/index.ts",
                    "import { m } from \"@shop/kernel/domain\";\nimport x from \"leftpad\";\n",
                ),
                // The root's development dependencies are the tests' of every package
                (
                    "packages/billing/src/application/index.test.ts",
                    "import { test } from \"vitest\";\n",
                ),
            ]
        };
        let found = check(
            declaration,
            &files("{ \"name\": \"@shop/billing\", \"dependencies\": { \"leftpad\": \"1\" } }"),
        );
        assert_eq!(found.len(), 1, "{found:#?}");
        assert_eq!(
            found[0],
            "packages/billing/src/application/index.ts:1: imports @shop/kernel/domain, the unit kernel, and \
             packages/billing/package.json does not list its package @shop/kernel: add it to dependencies"
        );
        let found = check(
            declaration,
            &files(
                "{ \"name\": \"@shop/billing\", \"dependencies\": { \"leftpad\": \"1\", \"@shop/kernel\": \"workspace:*\" } }",
            ),
        );
        assert!(found.is_empty(), "{found:#?}");
        // The code of a layer does not use them: the line is the root's, which the finding does not give
        let mut with_code = files(
            "{ \"name\": \"@shop/billing\", \"dependencies\": { \"leftpad\": \"1\", \"@shop/kernel\": \"workspace:*\" } }",
        );
        with_code.push((
            "packages/billing/src/domain/clock.ts",
            "import { vi } from \"vitest\";\n",
        ));
        let found = check(declaration, &with_code);
        assert_eq!(
            found,
            [
                "packages/billing/src/domain/clock.ts:1: imports vitest, which the manifest of the project's root \
                 lists for development only: the code of a layer uses runtime dependencies only, so list vitest in \
                 dependencies of packages/billing/package.json, or import it in a test alone (a file named *.test.* \
                 or *.spec.*, or in __tests__/)"
            ]
        );
    }

    #[test]
    fn a_manifest_that_cannot_be_read_is_a_finding_and_a_missing_one_is_not() {
        let found = check(
            "",
            &with(&[
                ("package.json", "{ \"name\": "),
                ("src/domain/a.ts", "import z from \"zod\";\n"),
            ]),
        );
        assert_eq!(found.len(), 1, "{found:#?}");
        assert!(
            found[0].starts_with("package.json:1: cannot be read as JSON"),
            "{found:#?}"
        );
        let found = check(
            "",
            &with(&[("src/domain/a.ts", "import z from \"zod\";\n")]),
        );
        assert!(found.is_empty(), "{found:#?}");
    }
}

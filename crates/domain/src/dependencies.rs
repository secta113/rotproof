//! The dependencies check: what a package's manifest lists agrees with what its code imports, either way.
//!
//! The manifest of a package (`pyproject.toml`, `package.json`, a crate's `Cargo.toml`) is the file that says, in the
//! tree, what the code depends on. It is judged against the code as a layer is against the table: from the tree and
//! the declaration alone, and never from the packages installed on a machine, so the answer is the same on every
//! machine and in CI, which installs nothing.
//!
//! - **Missing:** code imports a package, or another unit's package, that the manifest does not list. Python and
//!   TypeScript. Rust has none: Cargo stops a crate that names a crate it does not list.
//! - **Unused:** the manifest lists a runtime dependency that no code imports. A dependency the code uses without an
//!   import (a driver a library loads by name, what `pandas` uses) is listed in `without_import` of the declaration.
//!   Development dependencies are not judged: they hold tools (`ruff`, `mypy`) that nothing imports. The imports of the
//!   code that is moving into the layers count as uses of a runtime dependency ([`judge_with_moving`]), and of no
//!   other rule.
//! - **Runtime and development:** code in a layer uses runtime dependencies only, and a test uses both. A Python test
//!   is a file of `tests/`, and a TypeScript one a file the layout names as a test (`*.test.*`, `__tests__/`); Rust,
//!   where Cargo keeps `[dev-dependencies]` apart, is not judged. A package of `@types/` is types, erased when the
//!   code is built, and may be named anywhere.
//! - **What is not a dependency:** the standard library and the project's own modules, which the readers in
//!   `application` tell apart; what is left of an import is the package it needs, as [`Use::package`] names it.
//!
//! The names of packages are compared as the language compares them: Python's by PEP 503 ([`python::normalize`]),
//! a crate's with `-` and `_` the same ([`rust::normalize`]), and npm's as written. Where the language reads its names
//! and the packages from lists that differ by version, in `python.rs` and `typescript.rs`, they are constants of
//! Rotproof's, since the answer must not depend on the machine.

pub mod python;
pub mod rust;
pub mod typescript;

use std::collections::BTreeSet;

use crate::layers::{Language, NO_UI, layout};

const GUIDE_PYTHON: &str = include_str!("../../../project/rotproof/dependencies-python.md.in");
const GUIDE_TYPESCRIPT: &str =
    include_str!("../../../project/rotproof/dependencies-typescript.md.in");
const GUIDE_RUST: &str = include_str!("../../../project/rotproof/dependencies-rust.md.in");

/// What the guide says of this rule in the section on the layers, for a project in `language`.
pub fn guide(language: Language) -> &'static str {
    match language {
        Language::Python => GUIDE_PYTHON,
        Language::TypeScript => GUIDE_TYPESCRIPT,
        Language::Rust => GUIDE_RUST,
    }
}

/// What kind of dependency a manifest lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A dependency of the package as it runs: `[project] dependencies`, `dependencies`, `[dependencies]`
    Runtime,
    /// One the package may use and need not: the extras of Python (`[project.optional-dependencies]`), the peer and
    /// optional dependencies of npm, a crate's `[build-dependencies]`. Code may use it, and it is never unused
    Optional,
    /// One for development: a group of `[dependency-groups]`, `devDependencies`, `[dev-dependencies]`
    Development,
}

/// One dependency a manifest lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dep {
    /// Its name as the language compares it ([`python::normalize`], [`rust::normalize`], npm's as written)
    pub name: String,
    /// Its name as the manifest writes it, for a message
    pub shown: String,
    /// The line (from 1) it is written on, 0 when it comes from another manifest
    pub line: usize,
    pub kind: Kind,
    /// The group it is in: the extra of Python's `[project.optional-dependencies]` or the group of
    /// `[dependency-groups]` it is listed in, "" for the others
    pub group: String,
}

/// What a manifest lists: the name of its own package, when it has one, and its dependencies in the order written.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Listing {
    /// The package's own name, as the language compares names: what another unit lists to depend on it
    pub name: Option<String>,
    pub deps: Vec<Dep>,
}

impl Listing {
    /// The first dependency of this name, of a kind `kinds` holds.
    pub fn find(&self, name: &str, kinds: &[Kind]) -> Option<&Dep> {
        self.deps
            .iter()
            .find(|dep| dep.name == name && kinds.contains(&dep.kind))
    }
}

/// One place where code needs a package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Use {
    pub file: String,
    pub line: usize,
    /// What the code says (`dotenv`, `@shop/kernel/utils`, `serde_json`), for a message
    pub shown: String,
    /// The package it needs, as the language compares names
    pub package: String,
    /// Whether the file is a test
    pub test: bool,
    /// The unit whose package this is, when it is one
    pub unit: Option<String>,
}

/// What is wrong between a manifest and the code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// The code needs a package the manifest does not list, first at `first`, and `more` places beside it.
    /// `tests_only` when every place is a test, so that the package belongs in development
    Missing {
        package: String,
        first: Use,
        more: usize,
        tests_only: bool,
    },
    /// The code of a layer needs a package the manifest lists for development only
    NotRuntime { dep: Dep, first: Use, more: usize },
    /// A runtime dependency that no code imports
    Unused { dep: Dep },
    /// A runtime dependency that only tests import
    TestsOnly { dep: Dep },
}

/// How a language is judged.
struct Rules {
    /// Whether the code needs a package the manifest does not list
    missing: bool,
    /// Whether a development dependency may be used anywhere, and not only in a test
    dev_everywhere: bool,
    /// Whether a test is told from the code of a layer
    tests: bool,
}

impl Rules {
    fn of(language: Language) -> Rules {
        match language {
            Language::Python => Rules {
                missing: true,
                dev_everywhere: false,
                tests: true,
            },
            // A test is told by its name, as the layout says (`*.test.*`, `__tests__/`)
            Language::TypeScript => Rules {
                missing: true,
                dev_everywhere: false,
                tests: true,
            },
            // Cargo stops a crate that uses one it does not list, and a dependency of tests is listed apart
            Language::Rust => Rules {
                missing: false,
                dev_everywhere: true,
                tests: false,
            },
        }
    }
}

/// Every problem between a manifest's `listing` and what the code of its package needs (`uses`, in the order of the
/// files and the lines). `without_import` names the dependencies the code uses without importing them, as the
/// language compares names.
pub fn judge(
    language: Language,
    listing: &Listing,
    uses: &[Use],
    without_import: &[String],
) -> Vec<Problem> {
    judge_with_moving(language, listing, uses, &[], without_import)
}

/// [`judge`], with the imports of the code that is moving into the layers (`moved`) counted for one rule alone: a
/// runtime dependency that moving code imports is used, by code that is no test. Nothing else is judged of them, since
/// no check judges the code that is moving: a package they import that the manifest lacks, or lists for development
/// only, is no finding.
pub fn judge_with_moving(
    language: Language,
    listing: &Listing,
    uses: &[Use],
    moved: &[Use],
    without_import: &[String],
) -> Vec<Problem> {
    let rules = Rules::of(language);
    let mut problems = Vec::new();
    // Each package in the order it is first needed, with the places that need it
    let mut needed: Vec<(&str, Vec<&Use>)> = Vec::new();
    for used in uses {
        match needed.iter_mut().find(|(name, _)| *name == used.package) {
            Some((_, places)) => places.push(used),
            None => needed.push((&used.package, vec![used])),
        }
    }
    let mut development = Vec::new();
    for (package, places) in &needed {
        // The package's own name is its own code, installed as a dependency of nothing
        if listing.name.as_deref() == Some(package) {
            continue;
        }
        if listing
            .find(package, &[Kind::Runtime, Kind::Optional])
            .is_some()
        {
            continue;
        }
        if let Some(dep) = listing.find(package, &[Kind::Development]) {
            // Types are erased when the code is built, so a package of them is no dependency of the code that names it
            if rules.dev_everywhere || types_only(language, package) {
                continue;
            }
            let outside: Vec<&&Use> = places.iter().filter(|used| !used.test).collect();
            if let Some(first) = outside.first() {
                development.push(Problem::NotRuntime {
                    dep: dep.clone(),
                    first: (***first).clone(),
                    more: outside.len() - 1,
                });
            }
            continue;
        }
        if rules.missing {
            problems.push(Problem::Missing {
                package: package.to_string(),
                first: places[0].clone(),
                more: places.len() - 1,
                tests_only: rules.tests && places.iter().all(|used| used.test),
            });
        }
    }
    problems.extend(development);
    let mut seen = BTreeSet::new();
    for dep in listing
        .deps
        .iter()
        .filter(|dep| dep.kind == Kind::Runtime && seen.insert(dep.name.as_str()))
    {
        if without_import.contains(&dep.name) || types_only(language, &dep.name) {
            continue;
        }
        let places: Vec<&Use> = uses
            .iter()
            .chain(moved)
            .filter(|used| used.package == dep.name)
            .collect();
        if places.is_empty() {
            problems.push(Problem::Unused { dep: dep.clone() });
        } else if rules.tests && places.iter().all(|used| used.test) {
            problems.push(Problem::TestsOnly { dep: dep.clone() });
        }
    }
    problems
}

/// The entries of `without_import` that no manifest of the project lists as a runtime dependency, as the language
/// compares names: the declaration names what does not exist.
pub fn stale<'a>(without_import: &'a [String], listed: &BTreeSet<String>) -> Vec<&'a String> {
    without_import
        .iter()
        .filter(|name| !listed.contains(*name))
        .collect()
}

impl Problem {
    /// The finding, as `rotproof check` prints it, for the manifest at `manifest`. `hint` is what the caller may say
    /// of how to fix a missing dependency (a distribution the environment on the machine has for the import).
    pub fn said(&self, language: Language, manifest: &str, hint: Option<&str>) -> String {
        let (runtime, development) = places_of(language);
        let more = |n: usize| match n {
            0 => String::new(),
            1 => " (1 more import of it)".to_string(),
            n => format!(" ({n} more imports of it)"),
        };
        match self {
            Problem::Missing {
                package,
                first,
                more: n,
                tests_only,
            } => {
                let at = format!("{}:{}", first.file, first.line);
                let what = match &first.unit {
                    Some(unit) => format!(
                        "imports {}, the unit {unit}, and {manifest} does not list its package {package}",
                        first.shown
                    ),
                    None => format!(
                        "imports {}, which needs the package {package}, and {manifest} does not list it",
                        first.shown
                    ),
                };
                let hint = hint.map(|h| format!(" ({h})")).unwrap_or_default();
                // What only tests import is development, and listing it as runtime fails the next check
                let place = if *tests_only { development } else { runtime };
                format!("{at}: {what}{}: add it to {place}{hint}", more(*n))
            }
            Problem::NotRuntime {
                dep,
                first,
                more: n,
            } => {
                // A dependency of the manifest of the root, which npm hoists to every package: no line of this manifest
                let (lists, fix) = if dep.line == 0 {
                    (
                        "the manifest of the project's root".to_string(),
                        format!("list {} in {runtime} of {manifest}", dep.shown),
                    )
                } else {
                    (
                        format!("{manifest}:{}", dep.line),
                        format!("move {} to {runtime}", dep.shown),
                    )
                };
                format!(
                    "{}:{}: imports {}, which {lists} lists for development only{}: the code of a layer uses \
                     runtime dependencies only, so {fix}, or import it in a test alone{}",
                    first.file,
                    first.line,
                    first.shown,
                    more(*n),
                    what_a_test_is(language)
                )
            }
            Problem::Unused { dep } => match language {
                Language::Rust => format!(
                    "{manifest}:{}: lists the dependency {} in [dependencies], and no `use` or path in the crate \
                     names it: take it out, or, when the code uses it without naming it, list it in without_import \
                     under [dependencies] in {}",
                    dep.line,
                    dep.shown,
                    crate::declaration::DECLARATION
                ),
                _ => format!(
                    "{manifest}:{}: lists the dependency {}, which no code imports: take it out, or, when the code \
                     uses it without importing it, list it in without_import under [dependencies] in {}",
                    dep.line,
                    dep.shown,
                    crate::declaration::DECLARATION
                ),
            },
            Problem::TestsOnly { dep } => format!(
                "{manifest}:{}: lists the dependency {} in {runtime}, and only tests import it: move it to \
                 {development}",
                dep.line, dep.shown
            ),
        }
    }
}

/// What a message says of how a test is told, where the language tells one by its name: the names its layout gives,
/// after a space. Nothing where a test is a place (Python's `tests/`) or Cargo keeps its own.
fn what_a_test_is(language: Language) -> String {
    let tests = match language {
        Language::TypeScript => layout("typescript", NO_UI).and_then(|layout| layout.tests),
        Language::Python | Language::Rust => None,
    };
    tests.map_or_else(String::new, |tests| {
        let directories: Vec<String> = tests.directories.iter().map(|d| format!("{d}/")).collect();
        format!(
            " (a file named {}, or in {})",
            tests.names.join(" or "),
            directories.join(" or ")
        )
    })
}

/// Whether a dependency only declares types, which no code imports: `@types/node` for `node:fs`. TypeScript's.
fn types_only(language: Language, name: &str) -> bool {
    language == Language::TypeScript && name.starts_with("@types/")
}

/// Where a manifest lists the dependencies of the package as it runs, and those for development, as a message says it.
fn places_of(language: Language) -> (&'static str, &'static str) {
    match language {
        Language::Python => (
            "dependencies in [project]",
            "a group of [dependency-groups]",
        ),
        Language::TypeScript => ("dependencies", "devDependencies"),
        Language::Rust => ("[dependencies]", "[dev-dependencies]"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dep(name: &str, line: usize, kind: Kind) -> Dep {
        Dep {
            name: name.to_string(),
            shown: name.to_string(),
            line,
            kind,
            group: String::new(),
        }
    }

    fn used(package: &str, file: &str, test: bool) -> Use {
        Use {
            file: file.to_string(),
            line: 3,
            shown: package.to_string(),
            package: package.to_string(),
            test,
            unit: None,
        }
    }

    fn listing(deps: Vec<Dep>) -> Listing {
        Listing {
            name: Some("mine".to_string()),
            deps,
        }
    }

    #[test]
    fn a_package_the_code_needs_and_the_manifest_lacks_is_missing_once_with_the_first_place() {
        let listing = listing(vec![dep("requests", 4, Kind::Runtime)]);
        let uses = [
            used("requests", "a.py", false),
            used("dotenv", "a.py", false),
            used("dotenv", "b.py", false),
            used("dotenv", "tests/c.py", true),
        ];
        let found = judge(Language::Python, &listing, &uses, &[]);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            matches!(&found[0], Problem::Missing { package, more: 2, .. } if package == "dotenv")
        );
        assert_eq!(
            found[0].said(Language::Python, "pyproject.toml", None),
            "a.py:3: imports dotenv, which needs the package dotenv, and pyproject.toml does not list it (2 more \
             imports of it): add it to dependencies in [project]"
        );
    }

    #[test]
    fn a_package_only_tests_import_is_to_be_added_for_development_and_not_to_runtime() {
        let listing = listing(vec![]);
        let only_tests = [
            used("pytest", "tests/a.py", true),
            used("pytest", "tests/b.py", true),
        ];
        let found = judge(Language::Python, &listing, &only_tests, &[]);
        assert_eq!(
            found[0].said(Language::Python, "pyproject.toml", None),
            "tests/a.py:3: imports pytest, which needs the package pytest, and pyproject.toml does not list it (1 \
             more import of it): add it to a group of [dependency-groups]"
        );
        let found = judge(Language::TypeScript, &listing, &only_tests, &[]);
        assert_eq!(
            found[0].said(Language::TypeScript, "package.json", None),
            "tests/a.py:3: imports pytest, which needs the package pytest, and package.json does not list it (1 \
             more import of it): add it to devDependencies"
        );
        // One import outside a test, and it is runtime again
        let one_in_code = [
            used("pytest", "tests/a.py", true),
            used("pytest", "domain/b.py", false),
        ];
        let found = judge(Language::Python, &listing, &one_in_code, &[]);
        assert!(
            found[0]
                .said(Language::Python, "pyproject.toml", None)
                .ends_with(": add it to dependencies in [project]"),
            "{found:?}"
        );
    }

    #[test]
    fn the_package_of_the_manifest_itself_is_no_dependency() {
        let found = judge(
            Language::Python,
            &listing(vec![]),
            &[used("mine", "a.py", false)],
            &[],
        );
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn a_development_dependency_serves_a_test_and_not_the_code_of_a_layer_in_python() {
        let listing = listing(vec![dep("pytest", 9, Kind::Development)]);
        let in_test = [used("pytest", "tests/a.py", true)];
        assert!(judge(Language::Python, &listing, &in_test, &[]).is_empty());
        let both = [
            used("pytest", "domain/a.py", false),
            used("pytest", "tests/a.py", true),
        ];
        let found = judge(Language::Python, &listing, &both, &[]);
        assert_eq!(
            found[0].said(Language::Python, "pyproject.toml", None),
            "domain/a.py:3: imports pytest, which pyproject.toml:9 lists for development only: the code of a layer \
             uses runtime dependencies only, so move pytest to dependencies in [project], or import it in a test alone"
        );
        // Rust keeps `[dev-dependencies]` apart, and is not judged
        assert!(judge(Language::Rust, &listing, &both, &[]).is_empty());
    }

    #[test]
    fn a_development_dependency_serves_a_test_and_not_the_code_of_a_layer_in_typescript_too() {
        let listed = listing(vec![
            dep("vitest", 9, Kind::Development),
            dep("@types/node", 10, Kind::Development),
        ]);
        let in_test = [used("vitest", "src/domain/a.test.ts", true)];
        assert!(judge(Language::TypeScript, &listed, &in_test, &[]).is_empty());
        let both = [
            used("vitest", "src/domain/a.ts", false),
            used("vitest", "src/domain/b.ts", false),
            used("vitest", "src/domain/a.test.ts", true),
        ];
        let found = judge(Language::TypeScript, &listed, &both, &[]);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(
            found[0].said(Language::TypeScript, "package.json", None),
            "src/domain/a.ts:3: imports vitest, which package.json:9 lists for development only (1 more import of \
             it): the code of a layer uses runtime dependencies only, so move vitest to dependencies, or import it \
             in a test alone (a file named *.test.* or *.spec.*, or in __tests__/)"
        );
        // A hoisted dependency has no line in the manifest it is judged for
        let hoisted = listing(vec![dep("vitest", 0, Kind::Development)]);
        let found = judge(Language::TypeScript, &hoisted, &both[..1], &[]);
        assert!(
            found[0]
                .said(Language::TypeScript, "packages/a/package.json", None)
                .contains(
                    "which the manifest of the project's root lists for development only: the code of a layer uses \
                     runtime dependencies only, so list vitest in dependencies of packages/a/package.json, or"
                ),
            "{found:?}"
        );
        // What declares types is erased when the code is built: it is named anywhere
        let types = [used("@types/node", "src/domain/a.ts", false)];
        assert!(judge(Language::TypeScript, &listed, &types, &[]).is_empty());
    }

    #[test]
    fn a_runtime_dependency_no_code_imports_is_unused_and_a_development_one_never_is() {
        let listing = listing(vec![
            dep("requests", 4, Kind::Runtime),
            dep("openpyxl", 5, Kind::Runtime),
            dep("ruff", 8, Kind::Development),
            dep("extra", 6, Kind::Optional),
        ]);
        let found = judge(Language::Python, &listing, &[], &["openpyxl".to_string()]);
        assert_eq!(
            found,
            [Problem::Unused {
                dep: dep("requests", 4, Kind::Runtime)
            }]
        );
        assert_eq!(
            found[0].said(Language::Python, "pyproject.toml", None),
            "pyproject.toml:4: lists the dependency requests, which no code imports: take it out, or, when the \
             code uses it without importing it, list it in without_import under [dependencies] in \
             .config/rotproof.toml"
        );
    }

    #[test]
    fn a_runtime_dependency_only_tests_import_is_moved_to_development_in_python_and_typescript() {
        let listing = listing(vec![dep("pytest", 4, Kind::Runtime)]);
        let uses = [used("pytest", "tests/a.py", true)];
        let found = judge(Language::Python, &listing, &uses, &[]);
        assert_eq!(
            found,
            [Problem::TestsOnly {
                dep: dep("pytest", 4, Kind::Runtime)
            }]
        );
        assert_eq!(
            found[0].said(Language::Python, "pyproject.toml", None),
            "pyproject.toml:4: lists the dependency pytest in dependencies in [project], and only tests import it: \
             move it to a group of [dependency-groups]"
        );
        // Rust's tests are judged by Cargo
        assert!(judge(Language::Rust, &listing, &uses, &[]).is_empty());
        let uses = [used("vitest", "src/domain/a.test.ts", true)];
        let listing = Listing {
            name: None,
            deps: vec![dep("vitest", 4, Kind::Runtime)],
        };
        let found = judge(Language::TypeScript, &listing, &uses, &[]);
        assert_eq!(
            found[0].said(Language::TypeScript, "package.json", None),
            "package.json:4: lists the dependency vitest in dependencies, and only tests import it: move it to \
             devDependencies"
        );
    }

    #[test]
    fn the_code_that_is_moving_makes_a_runtime_dependency_used_and_is_judged_by_no_other_rule() {
        let listing = listing(vec![
            dep("requests", 4, Kind::Runtime),
            dep("pytest", 5, Kind::Runtime),
            dep("ruff", 8, Kind::Development),
        ]);
        let in_tests = [used("pytest", "tests/a.py", true)];
        let moved = [
            used("requests", "legacy/a.py", false),
            used("pytest", "legacy/b.py", false),
            // Not listed, and listed for development only: neither is a finding of code that is moving
            used("flask", "legacy/a.py", false),
            used("ruff", "legacy/a.py", false),
        ];
        let found = judge_with_moving(Language::Python, &listing, &in_tests, &moved, &[]);
        assert!(found.is_empty(), "{found:?}");
        // Without them, one dependency is unused and the other is used by tests only
        let found = judge_with_moving(Language::Python, &listing, &in_tests, &[], &[]);
        assert_eq!(
            found,
            [
                Problem::Unused {
                    dep: dep("requests", 4, Kind::Runtime)
                },
                Problem::TestsOnly {
                    dep: dep("pytest", 5, Kind::Runtime)
                }
            ]
        );
    }

    #[test]
    fn rust_judges_a_dependency_that_is_unused_and_nothing_missing() {
        let listing = listing(vec![
            dep("serde", 7, Kind::Runtime),
            dep("tokio", 8, Kind::Runtime),
        ]);
        let uses = [
            used("serde", "src/lib.rs", false),
            used("anyhow", "src/lib.rs", false),
        ];
        let found = judge(Language::Rust, &listing, &uses, &[]);
        assert_eq!(
            found,
            [Problem::Unused {
                dep: dep("tokio", 8, Kind::Runtime)
            }]
        );
        assert!(found[0].said(Language::Rust, "crates/a/Cargo.toml", None).starts_with(
            "crates/a/Cargo.toml:8: lists the dependency tokio in [dependencies], and no `use` or path in the crate"
        ));
    }

    #[test]
    fn a_package_listed_twice_is_judged_once() {
        let listing = listing(vec![dep("a", 1, Kind::Runtime), dep("a", 2, Kind::Runtime)]);
        assert_eq!(judge(Language::Rust, &listing, &[], &[]).len(), 1);
    }

    #[test]
    fn a_unit_that_is_not_listed_is_said_by_its_package() {
        let mut core = used("shop-core", "billing/domain/a.py", false);
        core.shown = "core.domain.money".to_string();
        core.unit = Some("core".to_string());
        let found = judge(Language::Python, &listing(vec![]), &[core], &[]);
        assert_eq!(
            found[0].said(Language::Python, "billing/pyproject.toml", None),
            "billing/domain/a.py:3: imports core.domain.money, the unit core, and billing/pyproject.toml does not \
             list its package shop-core: add it to dependencies in [project]"
        );
    }

    #[test]
    fn the_names_without_import_that_no_manifest_lists_are_stale() {
        let listed: BTreeSet<String> = ["openpyxl".to_string()].into();
        let names = ["openpyxl".to_string(), "gone".to_string()];
        assert_eq!(stale(&names, &listed), [&"gone".to_string()]);
    }
}

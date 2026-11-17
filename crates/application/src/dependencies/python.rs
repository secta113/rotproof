//! The dependencies check of a Python project: what each `pyproject.toml` lists, against what the code of its units
//! imports.
//!
//! The files are those the direction check reads (every `.py` file in a layer, the entry files and the `__init__.py`
//! of a unit) and, for each unit, the tests: the code files in the paths that are no layers of its directory or of its
//! root (`tests/`). An import is one of
//!
//! - **a module of the unit itself**: nothing is needed;
//! - **a unit**: another unit's package, which the importer's manifest lists by the name its own manifest gives
//!   (`[project] name`), or by the name of the unit when that has none;
//! - **the standard library**: nothing is needed;
//! - **a module of the project** that is in no layer (a file at the root, `tests/helpers.py`): nothing is needed. It is
//!   one when a directory or a `.py` file of its name is where Python would find it: in a directory above the importing
//!   file, in the directory that holds a unit, and in the root and the path of each unit;
//! - **a package**: of the distribution the declaration maps it to (`imports` under `[dependencies]`), or else of the
//!   one named like its first part.
//!
//! A unit whose root holds no `pyproject.toml` is not compared with one.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::io;

use super::venv::Venv;
use super::{Ask, Read, listing, without_import};
use crate::direction::python::{package_files, path_of};
use crate::direction::{entry_files, layer_files};
use crate::tree::{Resolver, code_files, read_code};
use domain::code::module_parts;
use domain::declaration::{DECLARATION, Declared};
use domain::dependencies::python::{distribution_of, normalize};
use domain::dependencies::{Kind, Listing, Problem, Use, judge};
use domain::direction::{Landing, landing_of};
use domain::layers::{Language, Layout, Place, table};
use domain::paths::{join, parent, within};
use domain::python_stdlib::is_standard_library;
use domain::units::Unit;
use protocol::outbound::{Parsers, Tree};

/// What the code of one unit needs of a module it imports.
enum Needs {
    /// Nothing: its own code, the standard library, a module of the project
    Nothing,
    /// Another unit's package, by the name of its distribution, and the name of the unit
    Unit(String, String),
    /// A package, by its distribution and, when the declaration did not map it, the first part of the import
    Package(String, Option<String>),
}

/// Where an import is looked up, and what it is.
struct Imports<'a> {
    tree: &'a dyn Tree,
    units: &'a [Unit],
    places: &'a [&'a Place],
    /// Each unit that has a directory to name it, by the name Python imports it as, and the directory that holds it
    imported: BTreeMap<&'a str, &'a str>,
    mapped: &'a BTreeMap<String, String>,
    /// The distribution of each unit, by its name
    distributions: BTreeMap<String, String>,
    /// The directories modules of the project are found in, besides those above the importing file
    bases: Vec<String>,
    /// What the directories asked about hold, each read once
    directories: RefCell<BTreeMap<String, Vec<(String, bool)>>>,
    /// Where a path lands, each asked once: the same module is imported by many files
    resolver: Resolver<'a>,
}

impl Imports<'_> {
    /// What the code of `unit`, in the file at `file`, needs of `module`.
    fn needs(&self, file: &str, unit: &Unit, module: &[String]) -> Needs {
        let Some(first) = module.first() else {
            return Needs::Nothing;
        };
        let (target, _) = path_of(module, &self.imported);
        let at = self.resolver.of(&target);
        if let Some(landing) = landing_of(&at, self.places, self.units) {
            let to = match landing {
                Landing::Place(place) => &place.unit.name,
                Landing::InUnit(unit) => &unit.name,
            };
            return match self.distributions.get(to) {
                Some(distribution) if *to != unit.name => {
                    Needs::Unit(distribution.clone(), to.clone())
                }
                _ => Needs::Nothing,
            };
        }
        if is_standard_library(first) || self.is_module_of_project(file, first) {
            return Needs::Nothing;
        }
        let (package, mapped) = distribution_of(module, self.mapped);
        Needs::Package(package, (!mapped).then(|| first.clone()))
    }

    /// Whether `name` is a module of the project that the file at `file` imports: a directory or a `.py` file of that
    /// name in a directory above the file, or in one of the bases.
    fn is_module_of_project(&self, file: &str, name: &str) -> bool {
        let mut at = parent(file);
        let mut directories = vec![at.to_string()];
        while !at.is_empty() {
            at = parent(at);
            directories.push(at.to_string());
        }
        directories
            .iter()
            .chain(&self.bases)
            .any(|directory| self.holds(directory, name))
    }

    /// Whether `directory` holds a directory named `name`, or a file `name.py`.
    fn holds(&self, directory: &str, name: &str) -> bool {
        let mut read = self.directories.borrow_mut();
        let entries = read
            .entry(directory.to_string())
            .or_insert_with(|| self.tree.entries(directory).unwrap_or_default());
        entries.iter().any(|(entry, is_dir)| {
            if *is_dir {
                entry == name
            } else {
                entry.strip_suffix(".py") == Some(name)
            }
        })
    }
}

/// A code file the check reads, with the name of the unit it belongs to and whether it is a test.
struct File {
    path: String,
    unit: String,
    test: bool,
}

/// The dependencies check of a Python project, into `found`.
pub(super) fn python(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    declared: &Declared,
    layout: &Layout,
    places: &[&Place],
    ask: Ask,
    found: &mut Vec<String>,
) -> io::Result<Read> {
    let units = &declared.units;
    let entries = declared.entries(&table());
    let manifest_of = |unit: &Unit| {
        let dir = if unit.declared {
            unit.root.as_str()
        } else {
            ""
        };
        join(dir, &layout.manifest)
    };
    let mut listings: BTreeMap<String, Listing> = BTreeMap::new();
    let mut tried: BTreeSet<String> = BTreeSet::new();
    for unit in units {
        let path = manifest_of(unit);
        if tried.insert(path.clone())
            && let Some(read) = listing(tree, parsers, Language::Python, &path, found)?
        {
            listings.insert(path, read);
        }
    }
    let mut bases = vec![String::new()];
    for unit in units.iter().filter(|unit| unit.declared) {
        for base in [&unit.root, &unit.path, &parent(&unit.path).to_string()] {
            if !bases.contains(base) {
                bases.push(base.clone());
            }
        }
    }
    let imports = Imports {
        tree,
        units,
        places,
        imported: units.iter().filter_map(Unit::python_import).collect(),
        mapped: &declared.declaration.dependencies.imports,
        distributions: units
            .iter()
            .map(|unit| {
                let named = listings
                    .get(&manifest_of(unit))
                    .and_then(|l| l.name.clone());
                (
                    unit.name.clone(),
                    named.unwrap_or_else(|| normalize(&unit.name)),
                )
            })
            .collect(),
        bases,
        directories: RefCell::new(BTreeMap::new()),
        resolver: Resolver::new(tree),
    };
    // The files that are moving, for the packages they import alone: no manifest is compared with them, as none is with
    // the code outside the layers
    if let Ask::Moving(moving) = ask {
        let mut moved: Vec<Use> = Vec::new();
        for path in moving {
            let Some(unit) = units
                .iter()
                .filter(|unit| within(path, &unit.path))
                .max_by_key(|unit| unit.path.len())
                .or(units.first())
            else {
                continue;
            };
            let Some(source) = read_code(tree, path, "imports", &mut Vec::new())? else {
                continue;
            };
            for (line, module) in parsers.python(&source, path).imports {
                if let Needs::Package(package, _) = imports.needs(path, unit, &module) {
                    moved.push(Use {
                        file: path.clone(),
                        line,
                        shown: module.join("."),
                        package,
                        test: false,
                        unit: None,
                    });
                }
            }
        }
        // The code of the layers is no part of what was asked for
        return Ok(Read {
            moving: moved,
            ..Read::default()
        });
    }
    let mut files: Vec<File> = Vec::new();
    let package_files = package_files(tree, layout, &table(), units)?;
    let mut code = layer_files(tree, layout, places, |path| module_parts(path).join("/"))?;
    code.extend(entry_files(tree, &entries));
    code.extend(
        package_files
            .iter()
            .map(|(path, place)| (path.clone(), place)),
    );
    files.extend(code.into_iter().map(|(path, place)| File {
        path,
        unit: place.unit.name.clone(),
        test: false,
    }));
    for unit in units {
        let mut directories = layout.not_layers_in(&unit.path);
        directories.extend(layout.not_layers_in(&unit.root));
        directories.dedup();
        for directory in directories {
            for path in code_files(tree, &directory, |name| layout.is_code(name))? {
                if !declared.is_not_checked(&path) && !files.iter().any(|file| file.path == path) {
                    files.push(File {
                        path,
                        unit: unit.name.clone(),
                        test: true,
                    });
                }
            }
        }
    }
    let mut uses: BTreeMap<String, Vec<Use>> = BTreeMap::new();
    // Every import of a package, whether or not the unit has a manifest to compare it with
    let mut packages: Vec<Use> = Vec::new();
    // The first part of the import of each package that is taken to be of the distribution named like it
    let mut guesses: BTreeMap<(String, String), String> = BTreeMap::new();
    for file in &files {
        let Some(unit) = units.iter().find(|unit| unit.name == file.unit) else {
            continue;
        };
        let manifest = manifest_of(unit);
        let Some(source) = read_code(tree, &file.path, "imports", &mut Vec::new())? else {
            continue;
        };
        for (line, module) in parsers.python(&source, &file.path).imports {
            let (package, named, guess) = match imports.needs(&file.path, unit, &module) {
                Needs::Nothing => continue,
                Needs::Unit(package, name) => (package, Some(name), None),
                Needs::Package(package, guess) => (package, None, guess),
            };
            if let Some(guess) = guess {
                guesses
                    .entry((manifest.clone(), package.clone()))
                    .or_insert(guess);
            }
            let used = Use {
                file: file.path.clone(),
                line,
                shown: module.join("."),
                package,
                test: file.test,
                unit: named,
            };
            if used.unit.is_none() {
                packages.push(used.clone());
            }
            uses.entry(manifest.clone()).or_default().push(used);
        }
    }
    let without = without_import(declared, Language::Python);
    let mut read = Read {
        packages,
        ..Read::default()
    };
    let mut venvs = Venvs::new(tree);
    for (manifest, listing) in &listings {
        read.any = true;
        read.runtime.extend(
            listing
                .deps
                .iter()
                .filter(|dep| dep.kind == Kind::Runtime)
                .map(|dep| dep.name.clone()),
        );
        read.listed
            .extend(listing.deps.iter().map(|dep| dep.name.clone()));
        let used = uses.get(manifest).map_or(&[][..], Vec::as_slice);
        for problem in judge(Language::Python, listing, used, &without) {
            let hint = match &problem {
                Problem::Missing { package, .. } => guesses
                    .get(&(manifest.clone(), package.clone()))
                    .map(|import| venvs.hint(parent(manifest), import, package)),
                _ => None,
            };
            found.push(problem.said(Language::Python, manifest, hint.as_deref()));
        }
    }
    Ok(read)
}

/// The environments read for the hint of a message, each once.
struct Venvs<'a> {
    tree: &'a dyn Tree,
    read: BTreeMap<String, Option<Venv>>,
}

impl<'a> Venvs<'a> {
    fn new(tree: &'a dyn Tree) -> Self {
        Venvs {
            tree,
            read: BTreeMap::new(),
        }
    }

    /// What to say of an import of `import` that is taken to be of the distribution `package`, which the manifest in
    /// `directory` does not list: what the environment in the `.venv` of the directory (or of the root) provides it
    /// as, when there is one, and else how to say which distribution it is of.
    fn hint(&mut self, directory: &str, import: &str, package: &str) -> String {
        let tree = self.tree;
        let found = [join(directory, ".venv"), ".venv".to_string()]
            .into_iter()
            .find(|dir| {
                self.read
                    .entry(dir.clone())
                    .or_insert_with(|| Venv::read(tree, dir))
                    .is_some()
            });
        let venv = found.and_then(|dir| self.read.get(&dir).and_then(Option::as_ref));
        let providers = venv.map_or(&[][..], |venv| venv.providers(import));
        match (venv, providers.first()) {
            (Some(venv), Some(_)) if providers.iter().any(|name| name == package) => {
                format!("the environment in {} has it installed", venv.dir)
            }
            (Some(venv), Some(provider)) => format!(
                "the environment in {} provides {import} with {provider}: list {provider}, and map it with {import} \
                 = \"{provider}\" under imports in [dependencies] of {DECLARATION}",
                venv.dir
            ),
            _ => format!(
                "if {import} is of a distribution of another name, list that, and map it with {import} = \
                 \"<distribution>\" under imports in [dependencies] of {DECLARATION}"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::problems;
    use crate::tree::fake::Fake;
    use domain::declaration::{Declared, parse_declaration};
    use infrastructure::outbound::readers::Readers;

    /// What the dependencies check finds in a Python project declared as `declaration` (the fields after `ui`) over
    /// `files`.
    fn check(declaration: &str, files: &[(&str, &str)]) -> Vec<String> {
        let text = format!(
            "stack = \"python\"\nareas = []\nui = \"none\"\nfiles = \"2.0.0\"\n{declaration}"
        );
        let declared =
            Declared::new(parse_declaration(&text).unwrap()).unwrap_or_else(|e| panic!("{e:#?}"));
        let mut all: Vec<(&str, &str)> =
            vec![("domain/__init__.py", ""), ("application/__init__.py", "")];
        all.extend_from_slice(files);
        problems(&Fake::new(&all), &Readers, &declared).unwrap()
    }

    const PYPROJECT: &str = "[project]\nname = \"shop\"\ndependencies = [\"requests>=2\", \"python-dotenv\"]\n\
                             \n[dependency-groups]\ndev = [\"pytest\"]\n";

    #[test]
    fn a_project_whose_manifest_agrees_with_its_imports_passes() {
        let found = check(
            "[dependencies]\nimports = { dotenv = \"python-dotenv\" }\n",
            &[
                ("pyproject.toml", PYPROJECT),
                (
                    "application/a.py",
                    "import os\nimport requests\nfrom dotenv import load_dotenv\nfrom domain import x\nimport helper\n",
                ),
                ("domain/x.py", "from . import y\nimport typing\n"),
                ("helper.py", ""),
                (
                    "tests/test_a.py",
                    "import pytest\nimport requests\nfrom helper import f\nfrom domain import x\n",
                ),
            ],
        );
        assert!(found.is_empty(), "{found:#?}");
    }

    #[test]
    fn an_import_the_declaration_does_not_map_is_of_the_distribution_named_like_it() {
        let found = check(
            "",
            &[
                ("pyproject.toml", PYPROJECT),
                (
                    "application/a.py",
                    "import requests\nfrom dotenv import load_dotenv\n",
                ),
            ],
        );
        // `dotenv` is of `dotenv`, which the manifest does not list, and `python-dotenv` is imported by nothing
        assert_eq!(found.len(), 2, "{found:#?}");
        assert!(found[0].starts_with("application/a.py:2: imports dotenv.load_dotenv, which needs the package dotenv, and pyproject.toml does not list it"), "{found:#?}");
        assert!(
            found[0].contains(
                "map it with dotenv = \"<distribution>\" under imports in [dependencies]"
            )
        );
        assert!(
            found[1].starts_with("pyproject.toml:3: lists the dependency python-dotenv"),
            "{found:#?}"
        );
    }

    #[test]
    fn a_mapped_import_is_of_the_distribution_the_declaration_names() {
        let found = check(
            "[dependencies]\nimports = { dotenv = \"python-dotenv\" }\n",
            &[
                ("pyproject.toml", PYPROJECT),
                (
                    "application/a.py",
                    "import requests\nfrom dotenv import load_dotenv\n",
                ),
            ],
        );
        assert!(found.is_empty(), "{found:#?}");
    }

    #[test]
    fn a_dependency_no_code_imports_is_unused_unless_the_declaration_says_it_is_used_without_an_import()
     {
        let files = [
            ("pyproject.toml", PYPROJECT),
            ("application/a.py", "import requests\n"),
        ];
        let found = check("", &files);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert!(found[0].starts_with(
            "pyproject.toml:3: lists the dependency python-dotenv, which no code imports"
        ));
        let found = check(
            "[dependencies]\nwithout_import = [\"Python_Dotenv\"]\n",
            &files,
        );
        assert!(found.is_empty(), "{found:#?}");
    }

    #[test]
    fn a_name_in_without_import_that_no_manifest_lists_is_stale() {
        let found = check(
            "[dependencies]\nwithout_import = [\"gone\", \"python-dotenv\"]\n",
            &[
                ("pyproject.toml", PYPROJECT),
                ("application/a.py", "import requests\n"),
            ],
        );
        assert_eq!(found.len(), 1, "{found:#?}");
        assert!(
            found[0].starts_with(
                ".config/rotproof.toml: without_import under [dependencies] lists gone"
            )
        );
    }

    #[test]
    fn a_development_dependency_serves_a_test_and_not_a_layer() {
        let found = check(
            "",
            &[
                (
                    "pyproject.toml",
                    "[project]\nname = \"shop\"\ndependencies = []\n[dependency-groups]\ndev = [\"pytest\"]\n",
                ),
                ("domain/a.py", "import pytest\n"),
                ("tests/test_a.py", "import pytest\n"),
            ],
        );
        assert_eq!(found.len(), 1, "{found:#?}");
        assert!(
            found[0].starts_with(
                "domain/a.py:1: imports pytest, which pyproject.toml:5 lists for development only"
            ),
            "{found:#?}"
        );
    }

    #[test]
    fn a_runtime_dependency_only_the_tests_import_is_to_move_to_development() {
        let found = check(
            "",
            &[
                (
                    "pyproject.toml",
                    "[project]\nname = \"shop\"\ndependencies = [\"pytest\"]\n",
                ),
                ("domain/a.py", "import os\n"),
                ("tests/test_a.py", "import pytest\n"),
            ],
        );
        assert_eq!(found.len(), 1, "{found:#?}");
        assert!(
            found[0].contains("only tests import it: move it to a group of [dependency-groups]"),
            "{found:#?}"
        );
    }

    #[test]
    fn a_unit_imports_another_by_the_distribution_its_manifest_names() {
        let declaration = "[[unit]]\nname = \"billing\"\npath = \"billing\"\n\
                           [[unit]]\nname = \"core\"\npath = \"core\"\nshared = true\n";
        let layers = [
            ("billing/domain/__init__.py", ""),
            ("billing/application/__init__.py", ""),
            ("core/domain/__init__.py", ""),
            ("core/domain/money.py", ""),
            ("core/pyproject.toml", "[project]\nname = \"Shop_Core\"\n"),
            (
                "billing/application/pay.py",
                "from core.domain import money\n",
            ),
        ];
        let mut with = layers.to_vec();
        with.push((
            "billing/pyproject.toml",
            "[project]\nname = \"billing\"\ndependencies = []\n",
        ));
        let found = check(declaration, &with);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert!(found[0].starts_with("billing/application/pay.py:1: imports core.domain.money, the unit core, and billing/pyproject.toml does not list its package shop-core"), "{found:#?}");
        let mut with = layers.to_vec();
        with.push((
            "billing/pyproject.toml",
            "[project]\nname = \"billing\"\ndependencies = [\"shop-core\"]\n",
        ));
        assert!(check(declaration, &with).is_empty());
        // A unit nobody imports is a dependency nobody uses
        let mut with = layers.to_vec();
        with.push(("billing/application/pay.py", "import os\n"));
        with.push((
            "billing/pyproject.toml",
            "[project]\nname = \"billing\"\ndependencies = [\"shop-core\"]\n",
        ));
        let found = check(declaration, &with);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert!(found[0].starts_with(
            "billing/pyproject.toml:3: lists the dependency shop-core, which no code imports"
        ));
    }

    #[test]
    fn a_unit_whose_root_has_no_manifest_is_not_compared() {
        let found = check("", &[("application/a.py", "import requests\n")]);
        assert!(found.is_empty(), "{found:#?}");
    }

    #[test]
    fn a_manifest_that_cannot_be_read_is_a_finding() {
        let found = check(
            "",
            &[
                ("pyproject.toml", "[project\n"),
                ("application/a.py", "import requests\n"),
            ],
        );
        assert_eq!(found.len(), 1, "{found:#?}");
        assert!(
            found[0].starts_with("pyproject.toml:1: cannot be read as TOML"),
            "{found:#?}"
        );
    }

    #[test]
    fn what_the_environment_provides_is_said_in_the_message_and_never_decides_it() {
        let files = [
            (
                "pyproject.toml",
                "[project]\nname = \"shop\"\ndependencies = []\n",
            ),
            ("application/a.py", "from dotenv import load_dotenv\n"),
            (
                ".venv/Lib/site-packages/python_dotenv-1.0.1.dist-info/RECORD",
                "dotenv/__init__.py,sha256=x,1\n",
            ),
        ];
        let found = check("", &files);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert_eq!(
            found[0],
            "application/a.py:1: imports dotenv.load_dotenv, which needs the package dotenv, and pyproject.toml does \
             not list it: add it to dependencies in [project] (the environment in .venv provides dotenv with \
             python-dotenv: list python-dotenv, and map it with dotenv = \"python-dotenv\" under imports in \
             [dependencies] of .config/rotproof.toml)"
        );
        // With no environment the verdict is the same
        assert_eq!(check("", &files[..2]).len(), 1);
    }

    #[test]
    fn a_namespace_package_is_mapped_by_a_dotted_name() {
        let found = check(
            "[dependencies]\nimports = { \"google.cloud.storage\" = \"google-cloud-storage\" }\n",
            &[
                (
                    "pyproject.toml",
                    "[project]\nname = \"shop\"\ndependencies = [\"google-cloud-storage\"]\n",
                ),
                (
                    "application/a.py",
                    "from google.cloud import storage\nfrom google.cloud.storage import blob\n",
                ),
            ],
        );
        // `from google.cloud import storage` names `google.cloud.storage`, which the entry covers
        assert!(found.is_empty(), "{found:#?}");
    }
}

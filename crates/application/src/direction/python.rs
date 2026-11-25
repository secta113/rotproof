//! The direction check of a Python project, read from the tree.
//!
//! A module is placed by its dotted name as a path from the root, but for a unit that sits below the root: Python
//! imports a unit as the name of its directory (`billing`), from a directory that is on its path, so an import whose
//! first part is such a name is looked up in the directory that holds the unit (`packages/billing/src`), and lands in
//! its layers or, `import billing` and the like, in the unit outside them. A bare layer name (`domain`) is looked up
//! from the root, as Python does when the program starts there: it lands in the layer of a unit at the root and, where
//! the root has none, nowhere, so what it imports is not judged. The `__init__.py` directly in a unit's directory is
//! read too, as a place that imports no layer.
//!
//! An import by a `protocol.inbound` file of its own unit's `domain` is judged by the names it takes, as the reader
//! says it was written (`Source::forms`), not by the table: a module and a glob take no name, and a name is judged by
//! what the file of the module defines (`named_in`, `domain::vocabulary`). Every other import is judged by the table.

use std::collections::BTreeMap;
use std::io;

use super::moving::{Moving, python_files};
use super::templates::templates;
use super::{Direction, Landings, entry_files, layer_files, misread, test_dirs};
use crate::tree::{Resolver, exactly, is_file, read_code};
use domain::code::{Form, module_parts};
use domain::declaration::Declared;
use domain::direction::{Landing, Way, edge, imports_test, judged_landing, landing_of};
use domain::layers::{Layout, Place, Table, table};
use domain::outside::{asks, outside as outside_of};
use domain::paths::{join, within};
use domain::readers::read_of;
use domain::units::Unit;
use domain::units_doc::link;
use domain::vocabulary::{self, Defined};
use protocol::outbound::Parsers;
use protocol::outbound::Tree;

/// The direction check of a Python project: every import of a `.py` file in a layer, of the entry file and of the
/// package file of a unit, as the module it names.
pub(super) fn python<'a>(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    declared: &'a Declared,
    layout: &Layout,
    places: &[&'a Place],
    entries: &'a [Place],
    moving: &mut Moving<'a>,
) -> io::Result<Direction> {
    let units = &declared.units;
    let mut found = Vec::new();
    let mut forbidden = Vec::new();
    let mut edges = Vec::new();
    let mut reads = Vec::new();
    let mut links = Vec::new();
    let mut outside = Vec::new();
    let package_files = package_files(tree, layout, &table(), units)?;
    let mut files = layer_files(tree, layout, places, |path| module_parts(path).join("/"))?;
    files.extend(entry_files(tree, entries));
    files.extend(
        package_files
            .iter()
            .map(|(path, place)| (path.clone(), place)),
    );
    // Where each unit is looked up: the directory that holds it, by the name Python imports it as
    let imported: BTreeMap<&str, &str> = units.iter().filter_map(Unit::python_import).collect();
    // The files read are the layers', the entry files' and the packages': none is a test, and no test is read
    let tests = test_dirs(tree, layout, units);
    let resolver = Resolver::new(tree);
    // What the files that `protocol.inbound` imports from its own unit's `domain` define, each read once
    let mut landings = Landings::new(tree, parsers, layout.language);
    for (path, from) in files {
        let Some(source) = read_code(tree, &path, "imports", &mut found)? else {
            continue;
        };
        let read = parsers.python(&source, &path);
        if let Some((line, why)) = read.error {
            found.push(misread(&path, "Python", line, &why));
        }
        for (index, (line, module)) in read.imports.into_iter().enumerate() {
            let (target, by_unit) = path_of(&module, &imported);
            let at = resolver.of(&target);
            moving.read_by(from, || python_files(&at));
            let dotted = module.join(".");
            if let Some(dir) = tests.iter().find(|dir| within(&at, dir)) {
                forbidden.push(imports_test(
                    &path,
                    line,
                    &dotted,
                    &format!("imports {dotted}"),
                    &format!("in {dir}/, which holds tests"),
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
                    &dotted,
                    &python_files(&at),
                ));
            }
            let Some(to) = landing_of(&at, places, units) else {
                continue;
            };
            let what = format!("imports {dotted}");
            // A `protocol.inbound` importing its own unit's `domain` is judged by the names it takes, not by the table
            let named = match (to, read.forms.get(index)) {
                (Landing::Place(place), Some(form)) if vocabulary::applies(from, place) => {
                    named_in(tree, &resolver, &imported, &mut landings, place, form)?
                }
                _ => None,
            };
            if let Some((taken, defined)) = named {
                forbidden.extend(vocabulary::judged(
                    &path, line, &dotted, &what, &taken, &defined,
                ));
            } else {
                // A name that no unit gives, found in a layer of a unit at the root
                let bare = !by_unit && at_root(to, units);
                forbidden.extend(judged_landing(
                    &path,
                    line,
                    &dotted,
                    &what,
                    from,
                    to,
                    &Way::Module { bare },
                ));
            }
            edges.extend(edge(&path, line, &dotted, from, to));
            reads.extend(read_of(from, to, &dotted));
            links.extend(link(from, to));
        }
    }
    read_moving(tree, parsers, &resolver, &imported, places, units, moving)?;
    let templates = templates(tree, layout, places, units, &mut found, &mut forbidden)?;
    Ok(Direction {
        problems: found,
        forbidden,
        edges,
        reads,
        links,
        moving: None,
        templates,
        outside,
    })
}

/// What each of the files that are moving imports, as the module it names: a file or a package of another that is
/// moving, or else where it lands in the layers. A file it cannot read is no finding: the code that is moving is not
/// checked.
fn read_moving<'a>(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    resolver: &Resolver,
    imported: &BTreeMap<&str, &str>,
    places: &[&'a Place],
    units: &'a [Unit],
    moving: &mut Moving<'a>,
) -> io::Result<()> {
    for path in moving.files() {
        let Some(source) = read_code(tree, &path, "imports", &mut Vec::new())? else {
            continue;
        };
        for (_, module) in parsers.python(&source, &path).imports {
            let (target, _) = path_of(&module, imported);
            let at = resolver.of(&target);
            moving.imports(&path, python_files(&at), landing_of(&at, places, units));
        }
    }
    Ok(())
}

/// The path a module is looked up at, from the root, and whether a unit named it: a module whose first part is the
/// name of a unit is in the directory that holds the unit, and any other is a path from the root.
pub(crate) fn path_of(module: &[String], imported: &BTreeMap<&str, &str>) -> (String, bool) {
    match module
        .first()
        .and_then(|first| imported.get(first.as_str()))
    {
        Some(directory) => (join(directory, &module.join("/")), true),
        None => (module.join("/"), false),
    }
}

/// What an import of the `domain` of `to` takes, and what the file of its module defines, for the rule that lets a
/// `protocol.inbound` read the vocabularies of its own unit's `domain`. The reader says how the import was written
/// (`Form`): the module and the name it takes. The file of the module is `m.py` or `m/__init__.py`, inside `domain`.
///
/// - **`from m import n`:** `n` is taken from that file, which must define it. A name that is also a module of `m`
///   (`m/n.py` or `m/n/__init__.py`, inside `domain`) is the module, so it takes no name.
/// - **`from m import *` and `import m.n`:** the first takes `*`, which names nothing, and the second takes none.
///
/// `None` when the file of the module is not in `domain` or cannot be parsed: the table judges the import and forbids
/// it, as before.
fn named_in(
    tree: &dyn Tree,
    resolver: &Resolver,
    imported: &BTreeMap<&str, &str>,
    landings: &mut Landings,
    to: &Place,
    form: &Form,
) -> io::Result<Option<(Vec<String>, Defined)>> {
    let (target, _) = path_of(&form.module, imported);
    if target.is_empty() {
        return Ok(None);
    }
    let at = resolver.of(&target);
    let in_domain = |file: &String| is_file(tree, file) && within(file, &to.path);
    let Some(file) = [format!("{at}.py"), join(&at, "__init__.py")]
        .into_iter()
        .find(in_domain)
    else {
        return Ok(None);
    };
    let taken = match form.name.as_deref() {
        None => Vec::new(),
        Some("*") => vec!["*".to_string()],
        Some(name) => {
            let own = join(&at, name);
            if [format!("{own}.py"), join(&own, "__init__.py")]
                .iter()
                .any(in_domain)
            {
                Vec::new()
            } else {
                vec![name.to_string()]
            }
        }
    };
    Ok(landings
        .defined(&file)?
        .map(|defined| (taken, defined.clone())))
}

/// Whether an import lands in a place of the unit that sits at the root of the project.
fn at_root(to: Landing, units: &[Unit]) -> bool {
    match to {
        Landing::Place(place) => units
            .iter()
            .any(|unit| unit.name == place.unit.name && unit.path.is_empty()),
        Landing::InUnit(_) => false,
    }
}

/// The code files that sit directly in the directory of a unit and belong to its package (`__init__.py`), each as a
/// place that imports no layer. A unit at the root has none: its package file is the project's, and a unit whose
/// directory is not there is the structure check's to say.
pub(crate) fn package_files(
    tree: &dyn Tree,
    layout: &Layout,
    table: &Table,
    units: &[Unit],
) -> io::Result<Vec<(String, Place)>> {
    let mut found = Vec::new();
    for unit in units
        .iter()
        .filter(|unit| unit.declared && !unit.path.is_empty())
    {
        if !exactly(tree, &unit.path).is_ok_and(|(_, is_dir)| is_dir) {
            continue;
        }
        for (name, is_dir) in tree.entries(&unit.path)? {
            if !is_dir && layout.is_unit_file(&name) {
                let path = join(&unit.path, &name);
                let place = unit.own_file_place(layout, table, &path);
                found.push((path, place));
            }
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::super::problems;
    use crate::tree::fake::Fake;
    use domain::declaration::{Declared, parse_declaration};
    use domain::direction::Rule;
    use domain::paths::join;
    use infrastructure::outbound::readers::Readers;

    /// A `[[unit]]` table: `more` is the lines after its `path`, each ending in a newline.
    fn unit(name: &str, path: &str, more: &str) -> String {
        format!("[[unit]]\nname = \"{name}\"\npath = \"{path}\"\n{more}")
    }

    /// The declaration of a Python project with the `[[unit]]` tables `units`.
    fn declaration(units: &[String]) -> String {
        format!(
            "stack = \"python\"\nareas = []\nui_design = \"none\"\nfiles = \"2.0.0\"\n{}",
            units.join("\n")
        )
    }

    /// Every layer and level there is, which a unit that lacks nothing holds.
    const ALL: &[&str] = &[
        "domain",
        "application",
        "protocol/inbound",
        "protocol/outbound",
        "infrastructure/inbound",
        "infrastructure/outbound",
        "composition",
    ];

    /// What an app that has an entry holds.
    const APP: &[&str] = &[
        "infrastructure/inbound",
        "infrastructure/outbound",
        "composition",
    ];

    /// The line of an app's `absent`.
    const APP_ABSENT: &str = "absent = [\"domain\", \"application\", \"protocol\"]\n";

    /// What the direction check finds in a project that declares `units`, in the tree made of the layers each unit
    /// holds (a directory and the layers in it, each an empty package) and of `files`: what the table forbids as the
    /// findings say it, and every problem.
    fn check(
        units: &[String],
        layers: &[(&str, &[&str])],
        files: &[(&str, &str)],
    ) -> (Vec<String>, Vec<String>) {
        let declared = Declared::new(parse_declaration(&declaration(units)).unwrap())
            .unwrap_or_else(|e| panic!("{e:#?}"));
        let mut tree: Vec<(String, &str)> = Vec::new();
        for (at, names) in layers {
            tree.extend(
                names
                    .iter()
                    .map(|name| (join(at, &format!("{name}/__init__.py")), "")),
            );
        }
        tree.extend(files.iter().map(|(path, text)| (path.to_string(), *text)));
        let tree: Vec<(&str, &str)> = tree.iter().map(|(p, t)| (p.as_str(), *t)).collect();
        let found = problems(&Fake::new(&tree), &Readers, &declared).unwrap();
        // By file, as the layers' files come before the entry files
        let mut forbidden: Vec<String> = found.forbidden.into_iter().map(|f| f.detail).collect();
        forbidden.sort();
        (forbidden, found.problems)
    }

    /// A modular monolith: the units sit side by side at the root, where the program starts.
    fn monolith() -> Vec<String> {
        vec![
            unit("core", "core", "shared = true\n"),
            unit("billing", "billing", ""),
            unit("shipping", "shipping", ""),
            unit(
                "cli",
                "cli",
                &format!("entry = [\"cli/__main__.py\"]\n{APP_ABSENT}"),
            ),
        ]
    }

    const MONOLITH: &[(&str, &[&str])] = &[
        ("core", ALL),
        ("billing", ALL),
        ("shipping", ALL),
        ("cli", APP),
    ];

    #[test]
    fn in_a_modular_monolith_a_unit_is_imported_by_its_name_from_the_root() {
        let (forbidden, problems) = check(
            &monolith(),
            MONOLITH,
            &[
                // The shared unit's domain, the unit's own layers by name and relatively
                (
                    "billing/application/pay.py",
                    "from core.domain import money\nfrom billing.domain import invoice\nfrom ..domain import fee\n",
                ),
                // A unit that is not shared is no one's to import
                (
                    "billing/domain/invoice.py",
                    "from shipping.domain import parcel\n",
                ),
                (
                    "shipping/composition/wire.py",
                    "from billing.domain import invoice\n",
                ),
                // The composition of a unit reads the shared unit through its protocol and its domain (the value types
                // the ports take), and builds none of its insides
                (
                    "billing/composition/wire.py",
                    "from core.protocol.inbound import port\nfrom core.domain import money\n\
                     from core.infrastructure.outbound import db\n",
                ),
                // An app's composition wires the units that are not shared, through their composition and protocol,
                // and builds none of their insides; of the shared unit it reads the domain too
                (
                    "cli/composition/wire.py",
                    "from billing.composition import wire\nimport shipping.protocol.outbound\n\
                     import shipping.infrastructure.outbound.db\nfrom core.domain import money\n\
                     from core.application import pay\n",
                ),
                // The entry file reads its own unit's composition, and not another's
                (
                    "cli/__main__.py",
                    "from cli.composition import wire\nfrom billing.composition import wire as other\n",
                ),
            ],
        );
        assert!(problems.is_empty(), "{problems:#?}");
        assert_eq!(
            forbidden,
            [
                "billing/composition/wire.py:3: imports core.infrastructure.outbound.db, in infrastructure.outbound of \
                 unit core; the composition of a unit imports only the protocol and domain of a shared unit: core \
                 builds its own insides in its own composition",
                "billing/domain/invoice.py:1: imports shipping.domain.parcel, in domain of unit shipping; billing \
                 imports no unit that is not shared, and shipping is not: an app's composition wires the two",
                "cli/__main__.py:2: imports billing.composition.wire, in composition of unit billing; cli imports no \
                 unit that is not shared, and billing is not: an app's composition wires the two",
                "cli/composition/wire.py:3: imports shipping.infrastructure.outbound.db, in infrastructure.outbound \
                 of unit shipping; the composition of an app imports only the composition and protocol of a unit \
                 that is not shared: shipping builds its own insides in its own composition",
                "cli/composition/wire.py:5: imports core.application.pay, in application of unit core; the \
                 composition of an app imports only the composition, protocol and domain of a shared unit: core \
                 builds its own insides in its own composition",
                "shipping/composition/wire.py:1: imports billing.domain.invoice, in domain of unit billing; shipping \
                 imports no unit that is not shared, and billing is not: an app's composition wires the two",
            ]
        );
    }

    #[test]
    fn an_import_by_a_literal_through_importlib_or_dunder_import_is_judged_as_the_import_statement_is()
     {
        let (forbidden, problems) = check(
            &monolith(),
            MONOLITH,
            &[
                (
                    "shipping/application/q1.py",
                    "import importlib\nimportlib.import_module(\"billing.domain.invoice\")\n",
                ),
                (
                    "shipping/application/q2.py",
                    "__import__(\"billing.domain.invoice\")\n",
                ),
                // Built at run time: not seen
                (
                    "shipping/application/q3.py",
                    "import importlib\nname = \"billing.domain.invoice\"\nimportlib.import_module(name)\n",
                ),
            ],
        );
        assert!(problems.is_empty(), "{problems:#?}");
        assert_eq!(
            forbidden,
            [
                "shipping/application/q1.py:2: imports billing.domain.invoice, in domain of unit billing; shipping \
                 imports no unit that is not shared, and billing is not: an app's composition wires the two",
                "shipping/application/q2.py:1: imports billing.domain.invoice, in domain of unit billing; shipping \
                 imports no unit that is not shared, and billing is not: an app's composition wires the two",
            ]
        );
    }

    #[test]
    fn in_a_src_layout_monorepo_a_unit_is_looked_up_in_the_directory_that_holds_it() {
        let units = [
            unit(
                "core",
                "packages/core/src/core",
                "shared = true\nroot = \"packages/core\"\n",
            ),
            unit(
                "billing",
                "packages/billing/src/billing",
                "root = \"packages/billing\"\n",
            ),
            unit(
                "cli",
                "apps/cli/src/cli",
                &format!("entry = [\"apps/cli/__main__.py\"]\nroot = \"apps/cli\"\n{APP_ABSENT}"),
            ),
        ];
        let (forbidden, problems) = check(
            &units,
            &[
                ("packages/core/src/core", ALL),
                ("packages/billing/src/billing", ALL),
                ("apps/cli/src/cli", APP),
            ],
            &[
                // What a unit reads of the shared one, and of itself
                (
                    "packages/billing/src/billing/infrastructure/inbound/api.py",
                    "from core.protocol.inbound import port\nfrom billing.protocol import inbound\n\
                     from core.application import use_case\nfrom ..inbound import x\n",
                ),
                (
                    "packages/billing/src/billing/application/pay.py",
                    "from core import domain\nimport core.domain.money\nimport os\nimport numpy\n",
                ),
                // The root has no `domain`, so a bare name judges nothing, and a name that is nothing is not either
                (
                    "packages/billing/src/billing/application/bare.py",
                    "from domain import money\nimport nothing.at.all\n",
                ),
                (
                    "apps/cli/__main__.py",
                    "from cli.composition import wire\nimport billing\n",
                ),
                (
                    "apps/cli/src/cli/composition/wire.py",
                    "from billing.composition import wire\nfrom core.composition import other\n",
                ),
            ],
        );
        assert!(problems.is_empty(), "{problems:#?}");
        assert_eq!(
            forbidden,
            [
                "apps/cli/__main__.py:2: imports billing, in the unit billing outside its layers; import the module \
                 of a layer, such as billing.protocol.inbound",
                "packages/billing/src/billing/infrastructure/inbound/api.py:3: imports core.application.use_case, \
                 in application of unit core; infrastructure.inbound may import `protocol.inbound`",
            ]
        );
    }

    #[test]
    fn a_bare_layer_name_in_a_nested_unit_is_the_layer_of_the_unit_at_the_root_and_the_finding_says_so()
     {
        // The project holds layers of its own (the unit at the root), and a package beside them
        let units = [
            unit("shop", ".", ""),
            unit("billing", "packages/billing/src/billing", ""),
        ];
        let layers: &[(&str, &[&str])] = &[
            ("", &["domain", "protocol/inbound"]),
            ("packages/billing/src/billing", ALL),
        ];
        let (forbidden, problems) = check(
            &units,
            layers,
            &[
                // Python finds `domain` from where the program starts: the root's, which a unit that is not shared
                // does not import
                (
                    "packages/billing/src/billing/application/pay.py",
                    "from domain import money\nfrom billing.domain import money as own\nfrom ..domain import fee\n",
                ),
                // The root's layers are read by name, as the root's own
                ("application/root.py", "from domain import money\n"),
                // Its own layer by its own name is no finding
                (
                    "packages/billing/src/billing/domain/own.py",
                    "from billing.domain import money\n",
                ),
            ],
        );
        assert!(problems.is_empty(), "{problems:#?}");
        let hint = "(Python finds domain from where the program starts; the unit's own layer is billing.domain)";
        assert_eq!(
            forbidden,
            [format!(
                "packages/billing/src/billing/application/pay.py:1: imports domain.money, in domain of unit shop; \
                 billing imports no unit that is not shared, and shop is not: an app's composition wires the two {hint}"
            )]
        );
        // A shared unit at the root is read by name, and the hint comes only with a finding
        let units = [
            unit("shop", ".", "shared = true\n"),
            unit("billing", "packages/billing/src/billing", ""),
        ];
        let (forbidden, problems) = check(
            &units,
            layers,
            &[
                (
                    "packages/billing/src/billing/application/pay.py",
                    "from domain import money\nfrom protocol.inbound import port\n",
                ),
                (
                    "packages/billing/src/billing/domain/own.py",
                    "from protocol.inbound import port\n",
                ),
            ],
        );
        assert!(problems.is_empty(), "{problems:#?}");
        assert_eq!(
            forbidden,
            [
                "packages/billing/src/billing/domain/own.py:1: imports protocol.inbound.port, in protocol.inbound of \
                 unit shop; domain imports no other layer (Python finds protocol.inbound from where the program \
                 starts; the unit's own layer is billing.protocol.inbound)"
            ]
        );
    }

    #[test]
    fn a_bare_layer_name_is_judged_nowhere_when_the_unit_at_the_root_does_not_have_the_layer() {
        // No unit at the root, so nothing for `domain` to land in: Python would fail on it unless the program started
        // in the unit's own directory, which Rotproof does not know
        let (forbidden, problems) = check(
            &monolith(),
            MONOLITH,
            &[(
                "billing/application/pay.py",
                "from domain import money\nimport application\nimport infrastructure.outbound.db\n",
            )],
        );
        assert!(
            problems.is_empty() && forbidden.is_empty(),
            "{forbidden:#?}"
        );
        // The unit at the root without a `domain` of its own has nothing to land in either
        let units = [
            unit("shop", ".", ""),
            unit("billing", "packages/billing/src/billing", ""),
        ];
        let (forbidden, problems) = check(
            &units,
            &[
                ("", &["application", "protocol/inbound"]),
                ("packages/billing/src/billing", ALL),
            ],
            &[(
                "packages/billing/src/billing/application/pay.py",
                "from domain import money\n",
            )],
        );
        assert!(
            problems.is_empty() && forbidden.is_empty(),
            "{forbidden:#?}"
        );
    }

    #[test]
    fn import_billing_lands_in_the_unit_outside_its_layers_however_it_is_written() {
        let units = [
            unit("core", "packages/core/src/core", "shared = true\n"),
            unit("billing", "packages/billing/src/billing", ""),
            // An app, whose composition wires the units: it may read a layer of another, and the unit as a whole not
            unit(
                "shipping",
                "shipping",
                "entry = [\"shipping/__main__.py\"]\n",
            ),
        ];
        let layers: &[(&str, &[&str])] = &[
            ("packages/core/src/core", ALL),
            ("packages/billing/src/billing", ALL),
            ("shipping", ALL),
        ];
        let outside = |import: &str, of: &str| {
            format!(
                "shipping/composition/wire.py:1: imports {import}, in the unit {of} outside its layers; import the \
                 module of a layer, such as {of}.protocol.inbound"
            )
        };
        for (text, import, of) in [
            ("import billing\n", "billing", "billing"),
            ("import billing as b\n", "billing", "billing"),
            ("from billing import models\n", "billing.models", "billing"),
            ("from billing import *\n", "billing", "billing"),
            ("import billing.models\n", "billing.models", "billing"),
            ("import core\n", "core", "core"),
            ("from core.helpers import x\n", "core.helpers.x", "core"),
        ] {
            let (forbidden, problems) =
                check(&units, layers, &[("shipping/composition/wire.py", text)]);
            assert!(problems.is_empty(), "{problems:#?}");
            assert_eq!(forbidden, [outside(import, of)], "{text}");
        }
        // A layer of a unit is named by its module, and the unit's own code outside its layers is no one's
        let (forbidden, problems) = check(
            &units,
            layers,
            &[
                (
                    "shipping/composition/wire.py",
                    "from billing import composition\nfrom billing.protocol import inbound\nimport core.protocol.outbound\n",
                ),
                (
                    "shipping/domain/own.py",
                    "import shipping\nfrom shipping import helper\n",
                ),
                (
                    "packages/billing/src/billing/domain/own.py",
                    "import billing\nfrom billing import helper\n",
                ),
            ],
        );
        assert!(
            problems.is_empty() && forbidden.is_empty(),
            "{forbidden:#?}"
        );
    }

    #[test]
    fn the_example_in_the_finding_for_the_code_outside_the_layers_is_a_layer_the_unit_holds() {
        let units = [
            // A shared unit of value types: it has no `protocol`
            unit(
                "kernel",
                "kernel",
                "shared = true\nabsent = [\"protocol\"]\n",
            ),
            unit(
                "core",
                "core",
                "shared = true\nabsent = [\"protocol.outbound\"]\n",
            ),
            unit(
                "cli",
                "cli",
                &format!("entry = [\"cli/__main__.py\"]\n{APP_ABSENT}"),
            ),
        ];
        let layers: &[(&str, &[&str])] = &[
            ("kernel", &["domain", "application", "composition"]),
            ("core", &["domain", "protocol/inbound"]),
            ("cli", APP),
        ];
        let (forbidden, problems) = check(
            &units,
            layers,
            &[("cli/composition/wire.py", "import kernel\nimport core\n")],
        );
        assert!(problems.is_empty(), "{problems:#?}");
        let outside = |import: &str, of: &str, example: &str| {
            format!(
                "cli/composition/wire.py:{import}, in the unit {of} outside its layers; import the module of a layer, \
                 such as {of}.{example}"
            )
        };
        assert_eq!(
            forbidden,
            [
                outside("1: imports kernel", "kernel", "domain"),
                outside("2: imports core", "core", "protocol.inbound"),
            ]
        );
    }

    #[test]
    fn the_input_ports_of_any_unit_read_the_domain_of_a_shared_unit_and_no_other() {
        let units = [
            unit("core", "core", "shared = true\n"),
            unit("steps", "steps", "shared = true\n"),
            unit("billing", "billing", ""),
            unit("shipping", "shipping", ""),
        ];
        let layers: &[(&str, &[&str])] = &[
            ("core", ALL),
            ("steps", ALL),
            ("billing", ALL),
            ("shipping", ALL),
        ];
        let (forbidden, problems) = check(
            &units,
            layers,
            &[
                // The shared unit's value types: from a unit that is not shared, and from another shared unit
                (
                    "billing/protocol/inbound/dto.py",
                    "from core.domain import money\nfrom core.protocol.inbound import port\n",
                ),
                (
                    "steps/protocol/inbound/dto.py",
                    "from core.domain import money\n",
                ),
                // The unit's own domain is read by name, which these files do not give (the package file of the
                // domain defines nothing); the domain of a unit that is not shared, and the levels beside it, are not
                (
                    "billing/protocol/inbound/own.py",
                    "from billing.domain import invoice\nfrom shipping.domain import parcel\n\
                     from core.protocol.outbound import port\n",
                ),
                (
                    "core/protocol/inbound/own.py",
                    "from core.domain import money\n",
                ),
                (
                    "steps/protocol/inbound/own.py",
                    "from shipping.domain import parcel\n",
                ),
                // Nothing else reads it further than the table lets it
                (
                    "billing/infrastructure/inbound/api.py",
                    "from core.domain import money\n",
                ),
            ],
        );
        assert!(problems.is_empty(), "{problems:#?}");
        assert_eq!(
            forbidden,
            [
                "billing/infrastructure/inbound/api.py:1: imports core.domain.money, in domain of unit core; \
                 infrastructure.inbound may import `protocol.inbound`",
                "billing/protocol/inbound/own.py:1: imports billing.domain.invoice, in domain; protocol.inbound may \
                 import only vocabularies, by name, from the file of domain that defines them: `invoice` is not \
                 defined in this file (a name it only re-exports is not followed): import it from the file that \
                 defines it",
                "billing/protocol/inbound/own.py:2: imports shipping.domain.parcel, in domain of unit shipping; \
                 billing imports no unit that is not shared, and shipping is not: an app's composition wires the two",
                "billing/protocol/inbound/own.py:3: imports core.protocol.outbound.port, in protocol.outbound of \
                 unit core; protocol.inbound imports no other layer",
                "core/protocol/inbound/own.py:1: imports core.domain.money, in domain; protocol.inbound may import \
                 only vocabularies, by name, from the file of domain that defines them: `money` is not defined in \
                 this file (a name it only re-exports is not followed): import it from the file that defines it",
                "steps/protocol/inbound/own.py:1: imports shipping.domain.parcel, in domain of unit shipping; the \
                 shared unit steps imports only shared units, and shipping is not shared",
            ]
        );
    }

    /// What the domain of `billing` holds for the tests of its input ports: a vocabulary (`Kind`, `Level`), a record
    /// (`Song`), an enum with a rule (`Mood`) and a function (`play`) in `song.py`, a vocabulary in a package, and a
    /// module that only re-exports.
    const DOMAIN_FILES: &[(&str, &str)] = &[
        (
            "billing/domain/song.py",
            "from enum import Enum\n\nclass Kind(Enum):\n    A = 1\n    B = 2\n\n\
             class Level(str, Enum):\n    LOW = \"low\"\n\nclass Song:\n    title = \"\"\n\n\
             class Mood(Enum):\n    HAPPY = 1\n    def shout(self):\n        return 1\n\ndef play():\n    pass\n",
        ),
        (
            "billing/domain/shapes/__init__.py",
            "from enum import Enum\n\nclass Shape(Enum):\n    ROUND = 1\n",
        ),
        (
            "billing/domain/reexport.py",
            "from billing.domain.song import Kind\n",
        ),
    ];

    /// What the direction check finds in the units of a modular monolith, with `DOMAIN_FILES` of `billing` and `files`.
    fn with_domain(files: &[(&str, &str)]) -> (Vec<String>, Vec<String>) {
        let all: Vec<(&str, &str)> = DOMAIN_FILES.iter().chain(files).copied().collect();
        check(&monolith(), MONOLITH, &all)
    }

    #[test]
    fn an_input_port_imports_a_vocabulary_of_its_own_domain_by_name() {
        let (forbidden, problems) = with_domain(&[
            (
                "billing/protocol/inbound/dto.py",
                // By the unit's name and relatively, with an alias, several names at once, and from a package
                "from billing.domain.song import Kind\nfrom ...domain.song import Level\n\
                 from billing.domain.song import Kind as K, Level as L\nfrom billing.domain.shapes import Shape\n",
            ),
            // Where the import is written in the level does not matter
            (
                "billing/protocol/inbound/deep/more.py",
                "from billing.domain.song import Kind\n",
            ),
        ]);
        assert!(problems.is_empty(), "{problems:#?}");
        assert!(forbidden.is_empty(), "{forbidden:#?}");
    }

    #[test]
    fn an_input_port_is_told_why_a_record_an_enum_with_methods_or_a_function_of_its_own_domain_is_no_vocabulary()
     {
        let (forbidden, problems) = with_domain(&[(
            "billing/protocol/inbound/dto.py",
            "from billing.domain.song import Song\nfrom billing.domain.song import Mood\n\
             from billing.domain.song import play\nfrom billing.domain.song import Kind, Song as S\n",
        )]);
        assert!(problems.is_empty(), "{problems:#?}");
        let said = |line: usize, import: &str, name: &str| {
            format!(
                "billing/protocol/inbound/dto.py:{line}: imports {import}, in domain; protocol.inbound may import only \
                 vocabularies, by name, from the file of domain that defines them: `{name}` is a record, class, \
                 function or type, no vocabulary: write the shape the port shows in protocol.inbound, and convert in \
                 application"
            )
        };
        // An enum with a method is told to move it out, as in Rust
        let methods = "billing/protocol/inbound/dto.py:2: imports billing.domain.song.Mood, in domain; protocol.inbound \
                       may import only vocabularies, by name, from the file of domain that defines them: `Mood` is an \
                       enum that has methods, a rule of domain, so it is no vocabulary: move the methods into a \
                       function of domain that takes the kind";
        // The vocabulary beside a record passes, and the record is named as the file defines it, not by its alias
        assert_eq!(
            forbidden,
            [
                said(1, "billing.domain.song.Song", "Song"),
                methods.to_string(),
                said(3, "billing.domain.song.play", "play"),
                said(4, "billing.domain.song.Song", "Song"),
            ]
        );
    }

    #[test]
    fn a_name_the_file_of_its_own_domain_does_not_define_is_told_to_come_from_the_file_that_does() {
        let (forbidden, problems) = with_domain(&[(
            "billing/protocol/inbound/dto.py",
            // Re-exported by a module, and by the package, which defines nothing; and a name nowhere
            "from billing.domain.reexport import Kind\nfrom billing.domain import Kind as K\n\
             from billing.domain.song import Missing\n",
        )]);
        assert!(problems.is_empty(), "{problems:#?}");
        let said = |line: usize, import: &str, name: &str| {
            format!(
                "billing/protocol/inbound/dto.py:{line}: imports {import}, in domain; protocol.inbound may import only \
                 vocabularies, by name, from the file of domain that defines them: `{name}` is not defined in this \
                 file (a name it only re-exports is not followed): import it from the file that defines it"
            )
        };
        assert_eq!(
            forbidden,
            [
                said(1, "billing.domain.reexport.Kind", "Kind"),
                said(2, "billing.domain.Kind", "Kind"),
                said(3, "billing.domain.song.Missing", "Missing"),
            ]
        );
    }

    #[test]
    fn an_import_of_a_module_of_its_own_domain_or_a_glob_of_it_takes_no_name_and_fails() {
        let (forbidden, problems) = with_domain(&[(
            "billing/protocol/inbound/dto.py",
            // `domain.song.Kind` would be read by attribute then, and a submodule that `from` imports is a module too
            "import billing.domain.song\nfrom billing.domain import song\nfrom billing.domain.song import *\n\
             import billing.domain\nfrom billing.domain import *\nimport billing.domain.shapes as s\n\
             from billing.domain import shapes\n",
        )]);
        assert!(problems.is_empty(), "{problems:#?}");
        let said = |line: usize, import: &str| {
            format!(
                "billing/protocol/inbound/dto.py:{line}: imports {import}, in domain; protocol.inbound may import only \
                 vocabularies, by name, from the file of domain that defines them: this import takes no name, and \
                 what it brings in cannot be told: import each vocabulary by name"
            )
        };
        assert_eq!(
            forbidden,
            [
                said(1, "billing.domain.song"),
                said(2, "billing.domain.song"),
                said(3, "billing.domain.song"),
                said(4, "billing.domain"),
                said(5, "billing.domain"),
                said(6, "billing.domain.shapes"),
                said(7, "billing.domain.shapes"),
            ]
        );
    }

    #[test]
    fn an_import_of_a_module_not_in_domain_is_judged_by_the_table_and_fails_as_before() {
        let (forbidden, problems) = check(
            &monolith(),
            MONOLITH,
            &[(
                "billing/protocol/inbound/dto.py",
                // No file of domain is the module of the first, and the module of the second is the unit's own package
                "from billing.domain.nothing import Kind\nfrom billing import domain\n",
            )],
        );
        assert!(problems.is_empty(), "{problems:#?}");
        assert_eq!(
            forbidden,
            [
                "billing/protocol/inbound/dto.py:1: imports billing.domain.nothing.Kind, in domain; protocol.inbound \
                 imports no other layer",
                "billing/protocol/inbound/dto.py:2: imports billing.domain, in domain; protocol.inbound imports no \
                 other layer",
            ]
        );
    }

    #[test]
    fn other_places_and_the_domain_of_another_unit_are_judged_by_the_table_as_before() {
        let (forbidden, problems) = with_domain(&[
            // The layers that the table lets read the domain, and the one it does not
            (
                "billing/application/pay.py",
                "from billing.domain.song import Song\n",
            ),
            (
                "billing/protocol/outbound/port.py",
                "from billing.domain.song import Kind\n",
            ),
            (
                "billing/infrastructure/inbound/api.py",
                "from billing.domain.song import Kind\n",
            ),
            // The input port of a unit reads a shared unit's domain whole, a record too, and the domain of a unit
            // that is not shared not at all
            (
                "billing/protocol/inbound/dto.py",
                "from core.domain.money import Money\nfrom shipping.domain.parcel import Kind\n",
            ),
            ("core/domain/money.py", "class Money:\n    pass\n"),
            (
                "shipping/domain/parcel.py",
                "from enum import Enum\n\nclass Kind(Enum):\n    A = 1\n",
            ),
        ]);
        assert!(problems.is_empty(), "{problems:#?}");
        assert_eq!(
            forbidden,
            [
                "billing/infrastructure/inbound/api.py:1: imports billing.domain.song.Kind, in domain; \
                 infrastructure.inbound may import `protocol.inbound`",
                "billing/protocol/inbound/dto.py:2: imports shipping.domain.parcel.Kind, in domain of unit shipping; \
                 billing imports no unit that is not shared, and shipping is not: an app's composition wires the two",
            ]
        );
    }

    #[test]
    fn the_finding_for_an_import_of_the_own_domain_is_keyed_by_its_file_and_import() {
        let declared =
            Declared::new(parse_declaration(&declaration(&monolith())).unwrap()).unwrap();
        let mut tree: Vec<(String, &str)> = MONOLITH
            .iter()
            .flat_map(|(at, names)| {
                names
                    .iter()
                    .map(move |name| (join(at, &format!("{name}/__init__.py")), ""))
            })
            .collect();
        tree.extend(
            DOMAIN_FILES
                .iter()
                .map(|(path, text)| (path.to_string(), *text)),
        );
        tree.push((
            "billing/protocol/inbound/dto.py".to_string(),
            "from billing.domain.song import Song as S\n",
        ));
        let tree: Vec<(&str, &str)> = tree.iter().map(|(p, t)| (p.as_str(), *t)).collect();
        let found = problems(&Fake::new(&tree), &Readers, &declared).unwrap();
        let [one] = found.forbidden.as_slice() else {
            panic!("{:#?}", found.forbidden);
        };
        // The module and the name, as an import of a layer's file is keyed: the alias is no part of it
        assert_eq!(one.file, "billing/protocol/inbound/dto.py");
        assert_eq!(one.import, "billing.domain.song.Song");
        assert_eq!(one.rule, Rule::Direction);
    }

    #[test]
    fn the_package_file_of_a_unit_is_read_as_a_place_that_imports_no_layer() {
        let units = [
            unit("core", "packages/core/src/core", "shared = true\n"),
            unit("billing", "packages/billing/src/billing", ""),
        ];
        let layers: &[(&str, &[&str])] = &[
            ("packages/core/src/core", ALL),
            ("packages/billing/src/billing", ALL),
        ];
        let (forbidden, problems) = check(
            &units,
            layers,
            &[
                (
                    "packages/billing/src/billing/__init__.py",
                    "from billing.domain import invoice\nfrom .application import pay\nfrom core.domain import money\n\
                     import billing\nimport core\nimport os\n",
                ),
                // Another file there is code outside the layers, which the structure check fails
                (
                    "packages/core/src/core/__init__.py",
                    "\"\"\"The package.\"\"\"\n",
                ),
            ],
        );
        assert!(problems.is_empty(), "{problems:#?}");
        let at = "packages/billing/src/billing/__init__.py";
        assert_eq!(
            forbidden,
            [
                format!(
                    "{at}:1: imports billing.domain.invoice, in domain; {at} imports no other layer"
                ),
                format!(
                    "{at}:2: imports packages.billing.src.billing.application.pay, in application; {at} imports no other layer"
                ),
                format!(
                    "{at}:3: imports core.domain.money, in domain of unit core; {at} imports no other layer"
                ),
                format!(
                    "{at}:5: imports core, in the unit core outside its layers; import the module of a layer, such as \
                     core.protocol.inbound"
                ),
            ]
        );
        // The package of a unit at the root is the project's own, and one in a layer is the layer's
        let (forbidden, problems) = check(
            &[unit("shop", ".", "")],
            &[("", ALL)],
            &[
                ("__init__.py", "from domain import x\n"),
                ("domain/__init__.py", "from application import x\n"),
            ],
        );
        assert!(problems.is_empty(), "{problems:#?}");
        assert_eq!(
            forbidden,
            [
                "domain/__init__.py:1: imports application.x, in application; domain imports no other layer"
            ]
        );
    }

    #[test]
    fn shared_units_of_a_src_layout_that_import_each_other_are_a_cycle() {
        let units = [
            unit("core", "packages/core/src/core", "shared = true\n"),
            unit("steps", "packages/steps/src/steps", "shared = true\n"),
        ];
        let (forbidden, problems) = check(
            &units,
            &[
                ("packages/core/src/core", ALL),
                ("packages/steps/src/steps", ALL),
            ],
            &[
                (
                    "packages/core/src/core/application/x.py",
                    "from steps.domain import a\n",
                ),
                (
                    "packages/steps/src/steps/domain/b.py",
                    "from core.domain import c\n",
                ),
            ],
        );
        // Each import is allowed by the table, as a layer reads the same layer of a shared unit: it is the two together
        assert!(forbidden.is_empty(), "{forbidden:#?}");
        assert_eq!(
            problems,
            [
                "the shared units import each other in a cycle: core \u{2192} steps \u{2192} core \
                 (packages/core/src/core/application/x.py:1 imports steps.domain.a; \
                 packages/steps/src/steps/domain/b.py:1 imports core.domain.c)"
            ]
        );
    }

    #[test]
    fn a_layer_the_entry_file_and_the_package_of_a_unit_import_no_module_of_a_tests_directory() {
        let (forbidden, problems) = check(
            &monolith(),
            MONOLITH,
            &[
                // The tests of a unit, and the tests of the project, where the program starts
                ("billing/tests/helper.py", ""),
                ("tests/helper.py", ""),
                (
                    "billing/application/pay.py",
                    "from billing.tests import helper\nimport tests.helper\nfrom ..domain import invoice\n",
                ),
                ("billing/__init__.py", "from billing.tests import helper\n"),
                ("cli/__main__.py", "import tests\n"),
                // A test imports the code and the tests, which is no import of any layer
                (
                    "billing/tests/test_pay.py",
                    "from billing.application import pay\nfrom billing.tests import helper\nimport tests.helper\n",
                ),
                // A name that is no directory of tests is another module
                (
                    "billing/application/other.py",
                    "import testing\nimport tests_helper\n",
                ),
            ],
        );
        assert!(problems.is_empty(), "{problems:#?}");
        let said = |file: &str, line: usize, import: &str, dir: &str| {
            format!(
                "{file}:{line}: imports {import}, in {dir}/, which holds tests; code that is not a test imports no \
                 test: move what it needs out of the test into a layer"
            )
        };
        assert_eq!(
            forbidden,
            [
                said(
                    "billing/__init__.py",
                    1,
                    "billing.tests.helper",
                    "billing/tests"
                ),
                said(
                    "billing/application/pay.py",
                    1,
                    "billing.tests.helper",
                    "billing/tests"
                ),
                said("billing/application/pay.py", 2, "tests.helper", "tests"),
                said("cli/__main__.py", 1, "tests", "tests"),
            ]
        );
    }

    #[test]
    fn a_unit_named_like_a_module_of_the_standard_library_is_refused_before_any_import_is_read() {
        for (path, name) in [("queue", "queue"), ("packages/string/src/string", "string")] {
            let declaration = declaration(&[unit(name, path, "")]);
            let refused = Declared::new(parse_declaration(&declaration).unwrap()).unwrap_err();
            assert_eq!(
                refused,
                [format!(
                    ".config/rotproof.toml: unit \"{name}\" is at {path}/, named like a module of Python's standard \
                     library: Python would import one or the other depending on how it starts; name the directory \
                     otherwise"
                )]
            );
        }
        // The same name for a project that is no Python's
        let declaration = format!(
            "stack = \"typescript\"\nareas = []\nui_design = \"none\"\nfiles = \"2.0.0\"\n{}",
            unit("queue", "packages/queue/src", "")
        );
        assert!(Declared::new(parse_declaration(&declaration).unwrap()).is_ok());
    }
}

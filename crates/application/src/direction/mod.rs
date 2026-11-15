//! The direction check, read from the tree: each language's code read through the ports, every import resolved to a
//! place, and judged by the rules in `domain`; and, from the imports from a shared unit to another, the cycles among
//! the shared units.

mod moving;
mod python;
mod rust;
mod typescript;

use std::io;

use crate::tree::{code_files, exactly, is_file};
use domain::declaration::Declared;
use domain::direction::{Edge, Forbidden, cycles, place_of};
use domain::layers::{Language, Layout, Place, table};
use domain::moving::Order;
use domain::readers::Read;
use domain::units_doc::Link;
use moving::Moving;
use protocol::outbound::Parsers;
use protocol::outbound::Tree;

/// What the direction check found.
#[derive(Debug, Default)]
pub struct Direction {
    /// Every file that could not be read, or whose imports could not be resolved
    pub problems: Vec<String>,
    /// Every import the table does not allow, before the approvals are read
    pub forbidden: Vec<Forbidden>,
    /// Every import from a shared unit to another, forbidden or not, which the cycles among shared units are read
    /// from
    pub edges: Vec<Edge>,
    /// Every name a unit reads of a shared unit other than its own, which `.rotproof/readers.md` is written from
    pub reads: Vec<Read>,
    /// Every import from a unit to another, whether or not the rules allow it, which the draft of the knowledge
    /// document of the units is made from
    pub links: Vec<Link>,
    /// What to say of the code that is moving into the layers: `None` when the declaration lists no path of it
    pub moving: Option<Order>,
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
    let entries = declared.entries(&table());
    let mut moving = Moving::list(tree, layout, declared)?;
    let mut direction = match layout.language {
        Language::Python => python::python(
            tree,
            parsers,
            layout,
            &places,
            &entries,
            &declared.units,
            &mut moving,
        ),
        Language::TypeScript => typescript::typescript(
            tree,
            parsers,
            layout,
            &places,
            &entries,
            &declared.units,
            &mut moving,
        ),
        Language::Rust => rust::rust(tree, parsers, layout, &places, &entries, &declared.units),
    }?;
    // The imports of the files of a Rust project are its crates' dependencies, which are no files'
    direction.moving = moving.order(
        layout.language != Language::Rust,
        &places,
        &entries,
        &declared.units,
    );
    // A cycle of shared units is no import of a file but the shape of them all, so it is a problem, which no approval
    // covers, and not a forbidden import
    direction.problems.extend(cycles(&direction.edges));
    Ok(direction)
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

/// The entry files that are there as files, each with itself as its place. They are code outside the layers, so the
/// walk of the layers does not find them.
fn entry_files<'a>(tree: &dyn Tree, entries: &'a [Place]) -> Vec<(String, &'a Place)> {
    entries
        .iter()
        .filter(|entry| is_file(tree, &entry.path))
        .map(|entry| (entry.path.clone(), entry))
        .collect()
}

/// The finding for a file a reader gave up on at `line`: what it read before is judged, and the rest may be misread.
fn misread(path: &str, language: &str, line: usize, why: &str) -> String {
    format!(
        "{path}:{line}: cannot be read as {language} ({why}), so the imports after it may be misread"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::fake::Fake;
    use domain::declaration::parse_declaration;
    use domain::units::Unit;
    use infrastructure::outbound::readers::Readers;

    /// A Python project whose units, made in code, are `core` and `steps` (both shared) and `app`, at the directories
    /// of the same names.
    fn project(files: &[(&str, &str)], shared: &[&str]) -> (Fake, Declared) {
        let mut declared = Declared::new(
            parse_declaration("stack = \"python\"\nareas = []\nui = \"none\"\n").unwrap(),
        )
        .unwrap();
        let layout = declared.layout.clone().unwrap();
        declared.units = ["core", "steps", "app"]
            .iter()
            .map(|name| Unit {
                name: name.to_string(),
                path: name.to_string(),
                root: name.to_string(),
                shared: shared.contains(name),
                entries: Vec::new(),
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

    #[test]
    fn shared_units_that_import_each_other_are_a_problem_no_approval_covers() {
        let (tree, declared) = project(
            &[
                (
                    "core/application/x.py",
                    "import os\n\nfrom steps.domain import a\n",
                ),
                ("core/domain/c.py", ""),
                ("steps/domain/a.py", ""),
                ("steps/domain/b.py", "from core.domain import c\n"),
            ],
            &["core", "steps"],
        );
        let found = problems(&tree, &Readers, &declared).unwrap();
        // Both imports are allowed one by one: it is the two together that fail, once
        assert!(found.forbidden.is_empty(), "{:?}", found.forbidden);
        assert_eq!(
            found.problems,
            [
                "the shared units import each other in a cycle: core \u{2192} steps \u{2192} core \
                 (core/application/x.py:3 imports steps.domain.a; steps/domain/b.py:1 imports core.domain.c)"
            ]
        );
        assert_eq!(found.edges.len(), 2);
    }

    #[test]
    fn one_way_between_shared_units_and_imports_of_other_units_make_no_cycle() {
        let (tree, declared) = project(
            &[
                ("core/application/x.py", "from steps.domain import a\n"),
                ("core/domain/c.py", ""),
                ("steps/domain/a.py", ""),
                ("app/domain/z.py", "from core.domain import c\n"),
                ("steps/application/y.py", "from app.domain import z\n"),
            ],
            // `app` is not shared: `steps` importing it is forbidden, and is no edge between shared units
            &["core", "steps"],
        );
        let found = problems(&tree, &Readers, &declared).unwrap();
        assert!(found.problems.is_empty(), "{:?}", found.problems);
        assert_eq!(found.edges.len(), 1);
        let forbidden: Vec<&str> = found.forbidden.iter().map(|f| f.detail.as_str()).collect();
        assert_eq!(
            forbidden,
            [
                "steps/application/y.py:1: imports app.domain.z, in domain of unit app; the shared unit steps \
                 imports only shared units, and app is not shared"
            ]
        );
    }

    /// The units an import goes from and to, once each, in order.
    fn links(direction: &Direction) -> Vec<(&str, &str)> {
        let mut links: Vec<(&str, &str)> = direction
            .links
            .iter()
            .map(|link| (link.from.as_str(), link.to.as_str()))
            .collect();
        links.sort();
        links.dedup();
        links
    }

    #[test]
    fn every_import_of_one_unit_by_another_is_a_link_whether_or_not_the_rules_allow_it() {
        let (tree, declared) = project(
            &[
                ("core/domain/c.py", ""),
                (
                    "core/domain/d.py",
                    "from core.domain import c
import os
",
                ),
                (
                    "app/domain/z.py",
                    "from core.domain import c
",
                ),
                (
                    "app/application/a.py",
                    "from app.domain import z
",
                ),
                // Forbidden: `steps` is shared and imports `app`, which is not
                (
                    "steps/application/y.py",
                    "from app.domain import z
",
                ),
                // The code of a unit outside its layers is forbidden as well, and an import of it is a link
                (
                    "steps/domain/w.py",
                    "import core
",
                ),
            ],
            &["core", "steps"],
        );
        let found = problems(&tree, &Readers, &declared).unwrap();
        assert_eq!(found.forbidden.len(), 2, "{:?}", found.forbidden);
        assert_eq!(
            links(&found),
            [("app", "core"), ("steps", "app"), ("steps", "core")]
        );
    }

    #[test]
    fn a_project_of_one_unit_has_no_edge() {
        let declared = Declared::new(
            parse_declaration("stack = \"python\"\nareas = []\nui = \"none\"\n").unwrap(),
        )
        .unwrap();
        let tree = Fake::new(&[
            ("application/x.py", "from domain import a\n"),
            ("domain/a.py", ""),
        ]);
        let found = problems(&tree, &Readers, &declared).unwrap();
        assert!(found.problems.is_empty() && found.forbidden.is_empty() && found.edges.is_empty());
        assert!(found.links.is_empty());
    }

    /// What the direction check makes of the code that is moving in a project of one unit declared as `declaration`
    /// says, over `files`, and the problems it found.
    fn moving(declaration: &str, files: &[(&str, &str)]) -> (Option<Order>, Vec<String>) {
        let declared = Declared::new(parse_declaration(declaration).unwrap()).unwrap();
        let found = problems(&Fake::new(files), &Readers, &declared).unwrap();
        assert!(found.forbidden.is_empty(), "{:?}", found.forbidden);
        (found.moving, found.problems)
    }

    fn next(order: &Order) -> Vec<Vec<&str>> {
        order
            .next
            .iter()
            .map(|group| group.iter().map(|next| next.path.as_str()).collect())
            .collect()
    }

    const PYTHON_MOVING: &str =
        "stack = \"python\"\nareas = []\nui = \"none\"\nmoving = [\"legacy\", \"old.py\"]\n";

    #[test]
    fn python_files_that_are_moving_are_ordered_by_what_they_import_of_each_other() {
        let (order, problems) = moving(
            PYTHON_MOVING,
            &[
                ("domain/a.py", ""),
                // A leaf: what it imports is the standard library, a package of the project's and a layer
                (
                    "legacy/money.py",
                    "import os\nimport requests\nfrom domain import a\n",
                ),
                // A module, a name taken from one, a package and a relative import: each a file that is moving
                (
                    "legacy/order.py",
                    "from legacy import money\nfrom legacy.money import total\nfrom . import tax\nimport legacy.pkg\n",
                ),
                ("legacy/tax.py", "from .order import x\nimport legacy.tax\n"),
                ("legacy/pkg/__init__.py", ""),
                ("legacy/pkg/inner.py", "from ..pkg import inner\n"),
                // Outside the directory listed, and a file listed alone
                ("old.py", "import legacy.money\n"),
                ("elsewhere.py", "import legacy.order\n"),
            ],
        );
        let order = order.expect("the declaration lists paths");
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(order.total, 6);
        assert!(order.ordered);
        // `order` and `tax` import each other and wait for `money` and the package; `old.py` waits for `money`;
        // `inner` imports only itself; `elsewhere.py` is not listed
        assert_eq!(
            next(&order),
            [
                vec!["legacy/money.py"],
                vec!["legacy/pkg/__init__.py"],
                vec!["legacy/pkg/inner.py"]
            ]
        );
        assert_eq!(order.more, 0);
    }

    #[test]
    fn python_files_that_import_each_other_are_one_group_and_wait_for_the_files_they_import() {
        let (order, _) = moving(
            PYTHON_MOVING,
            &[
                ("legacy/a.py", "from legacy import b\n"),
                (
                    "legacy/b.py",
                    "from legacy.a import x\nfrom legacy import leaf\n",
                ),
                ("legacy/c.py", "from legacy import a\n"),
                ("legacy/leaf.py", ""),
            ],
        );
        // a and b are one group, which waits for the leaf; c waits for them
        assert_eq!(next(&order.unwrap()), [vec!["legacy/leaf.py"]]);
    }

    #[test]
    fn a_file_with_a_syntax_error_is_counted_and_says_nothing() {
        let (order, problems) = moving(
            "stack = \"python\"\nareas = []\nui = \"none\"\nmoving = [\"legacy\"]\n",
            &[("legacy/a.py", "def broken(:\n"), ("legacy/b.py", "")],
        );
        assert!(problems.is_empty(), "{problems:?}");
        let order = order.unwrap();
        assert_eq!(order.total, 2);
        assert_eq!(next(&order), [vec!["legacy/a.py"], vec!["legacy/b.py"]]);
    }

    #[test]
    fn typescript_files_that_are_moving_are_ordered_by_what_they_import_of_each_other() {
        let (order, problems) = moving(
            "stack = \"typescript\"\nareas = []\nui = \"none\"\nmoving = [\"src/legacy\"]\n",
            &[
                ("src/domain/index.ts", ""),
                (
                    "src/legacy/money.ts",
                    "import { d } from \"../domain\";\nimport x from \"react\";\n",
                ),
                (
                    "src/legacy/order.ts",
                    "import { m } from \"./money\";\nexport * from \"./tax.js\";\n",
                ),
                ("src/legacy/tax.ts", "import \"./order\";\n"),
                ("src/legacy/ui/index.tsx", "import { o } from \"..\";\n"),
                ("src/legacy/index.ts", "import \"./ui\";\n"),
                ("src/legacy/logo.svg", "<svg/>"),
            ],
        );
        let order = order.unwrap();
        assert!(problems.is_empty(), "{problems:?}");
        // The `.svg` is no source. `order` and `tax` import each other and wait for `money`; `index` and `ui/index`
        // import each other and nothing else
        assert_eq!(order.total, 5);
        assert_eq!(
            next(&order),
            [
                vec!["src/legacy/index.ts", "src/legacy/ui/index.tsx"],
                vec!["src/legacy/money.ts"]
            ]
        );
    }

    #[test]
    fn rust_files_that_are_moving_are_counted_and_put_in_no_order() {
        let (order, problems) = moving(
            "stack = \"rust\"\nareas = []\nui = \"none\"\nmoving = [\"crates/legacy\"]\n",
            &[
                ("crates/legacy/Cargo.toml", "[package]\nname = \"legacy\"\n"),
                ("crates/legacy/src/lib.rs", "pub mod a;\n"),
                ("crates/legacy/src/a.rs", ""),
            ],
        );
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(order, Some(Order::unordered(2)));
    }

    #[test]
    fn nothing_is_said_of_a_project_that_lists_no_path_as_moving() {
        let (order, _) = moving(
            "stack = \"python\"\nareas = []\nui = \"none\"\n",
            &[("legacy/a.py", "")],
        );
        assert_eq!(order, None);
    }

    /// The layers of a Python project with no screens, each an empty package: what a file that is moving can be put in.
    const LAYERS: &[(&str, &str)] = &[
        ("composition/__init__.py", ""),
        ("infrastructure/__init__.py", ""),
        ("infrastructure/inbound/__init__.py", ""),
        ("infrastructure/outbound/__init__.py", ""),
        ("application/__init__.py", ""),
        ("protocol/__init__.py", ""),
        ("protocol/inbound/__init__.py", ""),
        ("protocol/outbound/__init__.py", ""),
        ("domain/__init__.py", ""),
    ];

    /// The layers each file named by the order of a Python project that lists `legacy` could go in, by path. `None`
    /// for every layer there is, and the names, innermost first, otherwise.
    fn candidates(files: &[(&str, &str)]) -> Vec<(String, Option<Vec<String>>)> {
        let mut all = LAYERS.to_vec();
        all.extend_from_slice(files);
        let (order, _) = moving(
            "stack = \"python\"\nareas = []\nui = \"none\"\nmoving = [\"legacy\"]\n",
            &all,
        );
        order
            .unwrap()
            .next
            .into_iter()
            .flatten()
            .map(|file| {
                let places = match file.candidates {
                    domain::moving::Candidates::Any => None,
                    domain::moving::Candidates::Only(places) => Some(places),
                };
                (file.path, places)
            })
            .collect()
    }

    fn only(path: &str, places: &[&str]) -> (String, Option<Vec<String>>) {
        (
            path.to_string(),
            Some(places.iter().map(|place| place.to_string()).collect()),
        )
    }

    #[test]
    fn the_layers_a_file_could_go_in_are_those_that_pass_for_what_it_imports_of_the_layers() {
        let found = candidates(&[
            ("legacy/leaf.py", "import os\nimport requests\n"),
            (
                "legacy/price.py",
                "from domain import money\nfrom legacy import leaf\n",
            ),
            (
                "legacy/store.py",
                "from infrastructure.outbound import db\n",
            ),
            ("domain/money.py", ""),
            ("infrastructure/outbound/db.py", ""),
        ]);
        // `price` waits for `leaf`, and is not named: the order is the graph's, the layers are the rules'
        assert_eq!(
            found,
            [
                ("legacy/leaf.py".to_string(), None),
                only(
                    "legacy/store.py",
                    &["infrastructure.outbound", "composition"]
                ),
            ]
        );
    }

    #[test]
    fn the_layers_a_file_could_go_in_are_those_that_the_layers_importing_it_may_import() {
        let found = candidates(&[
            ("legacy/price.py", "from domain import money\n"),
            ("legacy/rule.py", ""),
            // `domain` reads `price`, and `application` reads `rule`
            ("domain/money.py", ""),
            ("domain/uses.py", "from legacy import price\n"),
            ("application/use.py", "from legacy import rule\n"),
            // The entry file is no layer, and a place of its own that may import `composition` alone
            ("__main__.py", "from legacy import rule\n"),
        ]);
        assert_eq!(
            found,
            [
                only("legacy/price.py", &["domain"]),
                // Read by `application` and by the entry file: no layer is imported by both
                only("legacy/rule.py", &[]),
            ]
        );
    }

    #[test]
    fn what_a_file_imports_of_code_outside_the_layers_that_is_not_moving_passes_every_layer() {
        let found = candidates(&[
            ("legacy/a.py", "import helper\nimport legacy.b\n"),
            ("legacy/b.py", ""),
            ("helper.py", ""),
        ]);
        assert_eq!(found, [("legacy/b.py".to_string(), None)]);
    }

    #[test]
    fn a_file_in_a_unit_could_go_in_a_layer_of_its_unit_by_the_rules_between_units() {
        let (order, problems) = moving(
            "stack = \"python\"\nareas = []\nui = \"none\"\nmoving = [\"billing/legacy\"]\n\
             [[unit]]\nname = \"core\"\npath = \"core\"\nshared = true\n\
             [[unit]]\nname = \"billing\"\npath = \"billing\"\n",
            &[
                ("core/domain/__init__.py", ""),
                ("core/domain/money.py", ""),
                ("core/application/__init__.py", ""),
                ("core/application/pay.py", ""),
                ("billing/domain/__init__.py", ""),
                ("billing/application/__init__.py", ""),
                ("billing/infrastructure/__init__.py", ""),
                ("billing/infrastructure/inbound/__init__.py", ""),
                ("billing/infrastructure/outbound/__init__.py", ""),
                // A shared unit's `domain` is read as its own is, by the table
                ("billing/legacy/price.py", "from core.domain import money\n"),
                // Another unit's `application` is read by no `application`, and the table lets no other layer in
                (
                    "billing/legacy/use.py",
                    "from core.application import pay\n",
                ),
            ],
        );
        assert!(problems.is_empty(), "{problems:?}");
        let found: Vec<_> = order
            .unwrap()
            .next
            .into_iter()
            .flatten()
            .map(|file| (file.path, file.candidates))
            .collect();
        let names = |places: &[&str]| {
            domain::moving::Candidates::Only(places.iter().map(|place| place.to_string()).collect())
        };
        assert_eq!(
            found,
            [
                (
                    "billing/legacy/price.py".to_string(),
                    names(&[
                        "domain of unit billing",
                        "application of unit billing",
                        "infrastructure.outbound of unit billing",
                    ])
                ),
                ("billing/legacy/use.py".to_string(), names(&[])),
            ]
        );
    }

    #[test]
    fn typescript_files_are_given_the_layers_by_what_they_import_and_by_who_imports_them() {
        let (order, problems) = moving(
            "stack = \"typescript\"\nareas = []\nui = \"none\"\nmoving = [\"src/legacy\"]\n",
            &[
                ("src/composition/index.ts", ""),
                ("src/infrastructure/index.ts", ""),
                ("src/infrastructure/inbound/index.ts", ""),
                ("src/infrastructure/outbound/index.ts", ""),
                ("src/application/index.ts", ""),
                ("src/protocol/index.ts", ""),
                ("src/protocol/inbound/index.ts", ""),
                ("src/protocol/outbound/index.ts", ""),
                ("src/domain/index.ts", ""),
                (
                    "src/legacy/store.ts",
                    "import { db } from \"../infrastructure/outbound\";\n",
                ),
                (
                    "src/legacy/price.ts",
                    "import { money } from \"../domain\";\n",
                ),
                // `domain` reads `price`
                (
                    "src/domain/uses.ts",
                    "import { price } from \"../legacy/price\";\n",
                ),
            ],
        );
        assert!(problems.is_empty(), "{problems:?}");
        let found: Vec<_> = order
            .unwrap()
            .next
            .into_iter()
            .flatten()
            .map(|file| (file.path, file.candidates))
            .collect();
        let names = |places: &[&str]| {
            domain::moving::Candidates::Only(places.iter().map(|place| place.to_string()).collect())
        };
        assert_eq!(
            found,
            [
                ("src/legacy/price.ts".to_string(), names(&["domain"])),
                (
                    "src/legacy/store.ts".to_string(),
                    names(&["infrastructure.outbound", "composition"])
                ),
            ]
        );
    }
}

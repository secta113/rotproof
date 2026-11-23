//! The direction check, read from the tree: each language's code read through the ports, every import resolved to a
//! place, and judged by the rules in `domain`; and, from the imports from a shared unit to another, the cycles among
//! the shared units.

mod moving;
pub(crate) mod python;
pub(crate) mod rust;
mod templates;
mod typescript;

use std::collections::BTreeMap;
use std::io;

use crate::tree::{code_files, exactly, is_file, program_files, read_text};
use domain::declaration::Declared;
use domain::definitions::Definition;
use domain::direction::{Edge, Forbidden, cycles, place_of};
use domain::layers::{Language, Layout, Place, table};
use domain::modules::crate_module;
use domain::moving::Order;
use domain::outside::Outside;
use domain::readers::Read;
use domain::rust::{Source, code, without_tests};
use domain::templates::Premise;
use domain::units::Unit;
use domain::units_doc::Link;
use domain::vocabulary::{Crate, Defined};
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
    /// How the references between templates were read, in a project whose `ui` holds a template: said on every run
    pub templates: Option<Premise>,
    /// Every import from the code of an inner layer into a path of `unchecked` or `moving`, which no check judges: said
    /// on every run, in the order of the file and the line, and failing nothing
    pub outside: Vec<Outside>,
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
    let places = present_places(tree, declared);
    let entries = declared.entries(&table());
    let mut moving = Moving::list(tree, layout, declared)?;
    let mut direction = match layout.language {
        Language::Python => python::python(
            tree,
            parsers,
            declared,
            layout,
            &places,
            &entries,
            &mut moving,
        ),
        Language::TypeScript => typescript::typescript(
            tree,
            parsers,
            declared,
            layout,
            &places,
            &entries,
            &mut moving,
        ),
        Language::Rust => rust::rust(tree, parsers, declared, layout, &places, &entries),
    }?;
    // What each file that is moving does that the check of the I/O of the inner layers fails, which narrows the layers
    // it could go in. Rust files are counted and not read
    if layout.language != Language::Rust {
        moving.does(crate::inner_io::of_files(
            tree,
            parsers,
            declared,
            &moving.files(),
        )?);
    }
    // The imports of the files of a Rust project are its crates' dependencies, which are no files'
    direction.moving = moving.order(
        layout.language != Language::Rust,
        &places,
        &entries,
        &declared.units,
        &declared.allowed,
    );
    // A cycle of shared units is no import of a file but the shape of them all, so it is a problem, which no approval
    // covers, and not a forbidden import
    direction.problems.extend(cycles(&direction.edges));
    direction.outside.sort();
    direction.outside.dedup();
    Ok(direction)
}

/// The code files in the paths `declared` lists in `moving`, in the byte order of their paths: what the direction check
/// reads to put the code in the order to move it, and the dependencies check counts the imports of.
pub(crate) fn moving_files(tree: &dyn Tree, declared: &Declared) -> io::Result<Vec<String>> {
    let Some(layout) = &declared.layout else {
        return Ok(Vec::new());
    };
    Ok(Moving::list(tree, layout, declared)?.files())
}

/// The places of `declared` that are there. An import of a layer declared absent names a module the project does not
/// have, so it is not judged.
pub(crate) fn present_places<'a>(tree: &dyn Tree, declared: &'a Declared) -> Vec<&'a Place> {
    declared
        .places
        .iter()
        .filter(|p| !declared.is_absent(p) && exactly(tree, &p.path).is_ok())
        .collect()
}

/// The code files in the layers, each with the place it sits in. `placed_by` turns a file's path into the path its
/// place is looked up by: the path itself, or for Python the one its module is named by. A file that sits in no place
/// is left out.
pub(crate) fn layer_files<'a>(
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
pub(crate) fn entry_files<'a>(tree: &dyn Tree, entries: &'a [Place]) -> Vec<(String, &'a Place)> {
    entries
        .iter()
        .filter(|entry| is_file(tree, &entry.path))
        .map(|entry| (entry.path.clone(), entry))
        .collect()
}

/// The directories of the project that hold tests and are no layer (`tests/` in Python, at the root of the project and
/// in the directory and the root of each unit), those that are there. An import that lands in one is an import of a
/// test.
pub(crate) fn test_dirs(tree: &dyn Tree, layout: &Layout, units: &[Unit]) -> Vec<String> {
    let mut dirs = layout.not_layers_in(&layout.scope);
    for unit in units {
        dirs.extend(layout.not_layers_in(&unit.path));
        dirs.extend(layout.not_layers_in(&unit.root));
    }
    dirs.sort();
    dirs.dedup();
    dirs.retain(|dir| exactly(tree, dir).is_ok_and(|(_, is_dir)| is_dir));
    dirs
}

/// The outline of the code file at `path`, as the reader of `language` gives it, for the rule that lets a
/// `protocol.inbound` import the vocabularies of its own unit's `domain` by name. `None` when the file cannot be
/// read as text or parsed: the layers' own walk says so for every file it reads, so no finding is made twice, and
/// the import it lands in is judged as before, forbidden. An error is a file that could not be read at all.
pub fn outline_of(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    language: Language,
    path: &str,
) -> io::Result<Option<Vec<Definition>>> {
    let source = match read_text(tree, path) {
        Ok(source) => source,
        Err(e) if e.kind() == io::ErrorKind::InvalidData => return Ok(None),
        Err(e) => return Err(io::Error::new(e.kind(), format!("{path}: {e}"))),
    };
    Ok(parsers.outline(language, &source, path).ok())
}

/// What the files that `protocol.inbound` imports from the `domain` of its own unit define, each file read once for
/// a run: `defined` for a Python or a TypeScript file, read by its path from the root.
pub struct Landings<'p> {
    tree: &'p dyn Tree,
    parsers: &'p dyn Parsers,
    language: Language,
    known: BTreeMap<String, Option<Defined>>,
}

impl<'p> Landings<'p> {
    pub fn new(tree: &'p dyn Tree, parsers: &'p dyn Parsers, language: Language) -> Self {
        Landings {
            tree,
            parsers,
            language,
            known: BTreeMap::new(),
        }
    }

    /// The names the file at `path` defines, for `domain::vocabulary::judged`. `None` when the file cannot be read
    /// ([`outline_of`]): the import is then judged by the table, which forbids it.
    pub fn defined(&mut self, path: &str) -> io::Result<Option<&Defined>> {
        if !self.known.contains_key(path) {
            let defined = outline_of(self.tree, self.parsers, self.language, path)?
                .map(|outline| Defined::new(&outline));
            self.known.insert(path.to_string(), defined);
        }
        Ok(self.known[path].as_ref())
    }
}

/// What the Rust crate of `domain` defines, module by module, for the rule that lets a `protocol.inbound` import the
/// vocabularies of its own unit's `domain` by name (`domain::vocabulary::Crate`): the program files of its `src/`
/// (a test file is none, and the items under `#[cfg(test)]` are blanked first, as the program does not hold them),
/// each read as the module it is, since the rules of an enum are written in any of them. `None` when a file cannot be
/// parsed, since a rule in it could not be seen and the enum must not be let through for it, so every import is judged
/// by the table; a file that is not UTF-8 is a finding of the Rust walk, which reads it too. An error is a file that
/// could not be read at all.
pub fn domain_crate(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    layout: &Layout,
    place: &Place,
) -> io::Result<Option<Crate>> {
    let src = format!("{}/src/", place.path);
    let mut walk = Vec::new();
    let mut programs = Vec::new();
    for (path, source) in program_files(tree, layout, place, "definitions", &mut walk)? {
        // A build script or an example sits beside the crate's modules and is none of them
        let Some(rel) = path.strip_prefix(&src) else {
            continue;
        };
        let program = without_tests(&code(&source));
        match parsers.outline(Language::Rust, &program, &path) {
            Ok(outline) => programs.push((crate_module(rel), program, outline, path)),
            Err(_) => return Ok(None),
        }
    }
    let files: Vec<Source> = programs
        .iter()
        .map(|(module, code, outline, path)| Source {
            path,
            module,
            code,
            outline,
        })
        .collect();
    Ok(Some(Crate::new(&files)))
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
    use protocol::outbound::Writer;

    /// A Python project whose units, made in code, are `core` and `steps` (both shared) and `app`, at the directories
    /// of the same names.
    fn project(files: &[(&str, &str)], shared: &[&str]) -> (Fake, Declared) {
        let mut declared = Declared::new(
            parse_declaration("stack = \"python\"\nareas = []\nui_design = \"none\"\n").unwrap(),
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
            parse_declaration("stack = \"python\"\nareas = []\nui_design = \"none\"\n").unwrap(),
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
        "stack = \"python\"\nareas = []\nui_design = \"none\"\nmoving = [\"legacy\", \"old.py\"]\n";

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
            "stack = \"python\"\nareas = []\nui_design = \"none\"\nmoving = [\"legacy\"]\n",
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
            "stack = \"typescript\"\nareas = []\nui_design = \"none\"\nmoving = [\"src/legacy\"]\n",
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
            "stack = \"rust\"\nareas = []\nui_design = \"none\"\nmoving = [\"crates/legacy\"]\n",
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
            "stack = \"python\"\nareas = []\nui_design = \"none\"\n",
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
        candidates_declaring("", files)
    }

    /// [`candidates`] for a project whose declaration has `rest` after its first lines, the tables of it.
    fn candidates_declaring(
        rest: &str,
        files: &[(&str, &str)],
    ) -> Vec<(String, Option<Vec<String>>)> {
        let mut all = LAYERS.to_vec();
        all.extend_from_slice(files);
        let (order, _) = moving(
            &format!(
                "stack = \"python\"\nareas = []\nui_design = \"none\"\nmoving = [\"legacy\"]\n{rest}"
            ),
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
            ("legacy/leaf.py", "import os\nimport json\n"),
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
            "stack = \"python\"\nareas = []\nui_design = \"none\"\nmoving = [\"billing/legacy\"]\n\
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

    /// The places that judge no I/O, which are all a file that does some can go in.
    const FREE: &[&str] = &[
        "infrastructure.outbound",
        "infrastructure.inbound",
        "composition",
    ];

    #[test]
    fn a_python_file_that_does_io_the_check_of_the_inner_layers_fails_is_offered_no_layer_it_judges()
     {
        let free = |path: &str| only(path, FREE);
        // The first five groups are named, so the kinds of I/O are tried in two runs
        let found = candidates(&[
            // An import of the table
            ("legacy/table.py", "import shutil\n"),
            // A name judged by the allowlists, a group and the terminal
            ("legacy/name.py", "import os\nhere = os.getcwd()\n"),
            ("legacy/clock.py", "import time\nnow = time.time()\n"),
            ("legacy/args.py", "import sys\nargs = sys.argv\n"),
            ("legacy/term.py", "print('x')\n"),
        ]);
        assert_eq!(
            found,
            [
                free("legacy/args.py"),
                free("legacy/clock.py"),
                free("legacy/name.py"),
                free("legacy/table.py"),
                free("legacy/term.py"),
            ]
        );
        let found = candidates(&[
            // A class, a star import of a module with names that are I/O, and what cannot be read
            (
                "legacy/path.py",
                "from pathlib import Path\nhere = Path('a')\n",
            ),
            // The import of a class in a type position alone is free, and the file is offered every layer
            (
                "legacy/typed.py",
                "from pathlib import Path\n\n\ndef f(p: Path) -> None:\n    pass\n",
            ),
            ("legacy/star.py", "from datetime import *\n"),
            ("legacy/dyn.py", "eval('1')\n"),
            // What is pure: the names of an allowlist, and a module that only holds values
            (
                "legacy/pure.py",
                "import os\nimport json\nx = os.fspath('a')\n",
            ),
        ]);
        assert_eq!(
            found,
            [
                free("legacy/dyn.py"),
                free("legacy/path.py"),
                ("legacy/pure.py".to_string(), None),
                free("legacy/star.py"),
                ("legacy/typed.py".to_string(), None),
            ]
        );
    }

    #[test]
    fn a_python_method_the_project_defines_is_left_out_of_the_net_for_a_file_that_is_moving_too() {
        let call = ("legacy/call.py", "def f(p):\n    return p.read_text()\n");
        // The net judges a method of a path by its name: the file may not go in an inner layer
        assert_eq!(candidates(&[call]), [only("legacy/call.py", FREE)]);
        // The project defines one of that name and number of parameters, wherever it is: it is its own, and no path's
        let defined = (
            "tests/helper.py",
            "class T:\n    def read_text(self):\n        pass\n",
        );
        assert_eq!(
            candidates(&[call, defined]),
            [("legacy/call.py".to_string(), None)]
        );
    }

    #[test]
    fn a_python_file_is_offered_the_layers_that_allow_every_package_it_imports() {
        let files = [
            ("legacy/model.py", "import pydantic\n"),
            ("legacy/web.py", "import pydantic\nimport requests\n"),
            // The standard library is no package
            ("legacy/plain.py", "import json\nimport typing\n"),
        ];
        // No list: the packages are allowed nowhere, and a file that imports none is not narrowed
        let found = candidates(&files);
        assert_eq!(
            found,
            [
                only("legacy/model.py", FREE),
                ("legacy/plain.py".to_string(), None),
                only("legacy/web.py", FREE),
            ]
        );
        // `pydantic` allowed for `domain` and for the levels of `protocol`: where `requests` is not, `web.py` is not
        let found = candidates_declaring(
            "[dependencies]\nallowed = { domain = [\"pydantic\"], protocol = [\"pydantic\"] }\n",
            &files,
        );
        let names = |places: &[&str]| places.iter().map(|place| place.to_string()).collect();
        assert_eq!(
            found[0],
            (
                "legacy/model.py".to_string(),
                Some(names(&[
                    "domain",
                    "protocol.outbound",
                    "protocol.inbound",
                    "infrastructure.outbound",
                    "infrastructure.inbound",
                    "composition",
                ]))
            )
        );
        assert_eq!(found[2], only("legacy/web.py", FREE));
    }

    #[test]
    fn what_a_unit_allows_applies_to_the_files_that_are_moving_in_its_directory_only() {
        let (order, problems) = moving(
            "stack = \"python\"\nareas = []\nui_design = \"none\"\nmoving = [\"billing/legacy\", \"shipping/legacy\"]\n\
             [[unit]]\nname = \"billing\"\npath = \"billing\"\nallowed = { domain = [\"pydantic\"] }\n\
             [[unit]]\nname = \"shipping\"\npath = \"shipping\"\n",
            &[
                ("billing/domain/__init__.py", ""),
                ("shipping/domain/__init__.py", ""),
                ("billing/legacy/model.py", "import pydantic\n"),
                ("shipping/legacy/model.py", "import pydantic\n"),
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
        let has = |path: &str, place: &str| match &found.iter().find(|(p, _)| p == path).unwrap().1
        {
            domain::moving::Candidates::Only(places) => places.contains(&place.to_string()),
            domain::moving::Candidates::Any => true,
        };
        assert!(has("billing/legacy/model.py", "domain of unit billing"));
        assert!(!has("shipping/legacy/model.py", "domain of unit shipping"));
    }

    /// The layers a TypeScript project that lists `src/legacy` and has `rest` in its declaration, with the layers each
    /// an empty module, could put each file of `files` in, by path.
    fn typescript_candidates(
        rest: &str,
        files: &[(&str, &str)],
    ) -> Vec<(String, domain::moving::Candidates)> {
        let layers = [
            ("src/composition/index.ts", ""),
            ("src/infrastructure/index.ts", ""),
            ("src/infrastructure/inbound/index.ts", ""),
            ("src/infrastructure/outbound/index.ts", ""),
            ("src/application/index.ts", ""),
            ("src/protocol/index.ts", ""),
            ("src/protocol/inbound/index.ts", ""),
            ("src/protocol/outbound/index.ts", ""),
            ("src/domain/index.ts", ""),
        ];
        let (order, problems) = moving(
            &format!(
                "stack = \"typescript\"\nareas = []\nui_design = \"none\"\nmoving = [\"src/legacy\"]\n{rest}"
            ),
            &[&layers[..], files].concat(),
        );
        assert!(problems.is_empty(), "{problems:?}");
        order
            .unwrap()
            .next
            .into_iter()
            .flatten()
            .map(|file| (file.path, file.candidates))
            .collect()
    }

    #[test]
    fn a_typescript_file_that_does_io_is_offered_no_layer_the_check_judges() {
        let free = || {
            domain::moving::Candidates::Only(FREE.iter().map(|place| place.to_string()).collect())
        };
        let found = typescript_candidates(
            "",
            &[
                (
                    "src/legacy/fs.ts",
                    "import { readFile } from \"node:fs\";\n",
                ),
                ("src/legacy/clock.ts", "export const now = Date.now();\n"),
                (
                    "src/legacy/global.ts",
                    "export const get = () => fetch(\"x\");\n",
                ),
                ("src/legacy/dyn.ts", "export const r = eval(\"1\");\n"),
                ("src/legacy/pure.ts", "export const n = Math.max(1, 2);\n"),
            ],
        );
        assert_eq!(
            found,
            [
                ("src/legacy/clock.ts".to_string(), free()),
                ("src/legacy/dyn.ts".to_string(), free()),
                ("src/legacy/fs.ts".to_string(), free()),
                ("src/legacy/global.ts".to_string(), free()),
                (
                    "src/legacy/pure.ts".to_string(),
                    domain::moving::Candidates::Any
                ),
            ]
        );
    }

    #[test]
    fn a_typescript_file_is_offered_the_layers_that_allow_every_package_it_imports() {
        let found = typescript_candidates(
            "[dependencies]\nallowed = { domain = [\"zod\"] }\n",
            &[
                ("src/legacy/schema.ts", "import { z } from \"zod\";\n"),
                ("src/legacy/other.ts", "import _ from \"lodash\";\n"),
            ],
        );
        let names = |places: &[&str]| {
            domain::moving::Candidates::Only(places.iter().map(|place| place.to_string()).collect())
        };
        assert_eq!(
            found,
            [
                (
                    "src/legacy/other.ts".to_string(),
                    names(&[
                        "infrastructure.outbound",
                        "infrastructure.inbound",
                        "composition"
                    ])
                ),
                (
                    "src/legacy/schema.ts".to_string(),
                    names(&[
                        "domain",
                        "infrastructure.outbound",
                        "infrastructure.inbound",
                        "composition"
                    ])
                ),
            ]
        );
    }

    /// What the inner layers read of the code outside the layers, as it is said, in the project `declaration` declares
    /// with `files`.
    fn outside_in(declaration: &str, files: &[(&str, &str)]) -> Vec<String> {
        let declared = Declared::new(parse_declaration(declaration).unwrap()).unwrap();
        let found = problems(&Fake::new(files), &Readers, &declared).unwrap();
        assert!(found.forbidden.is_empty(), "{:?}", found.forbidden);
        found.outside.iter().map(Outside::said).collect()
    }

    /// [`outside_in`] for a Python project with the layers, which lists `scripts` as unchecked and `legacy` as moving
    /// unless `declaration` is given.
    fn outside_of(declaration: Option<&str>, files: &[(&str, &str)]) -> Vec<String> {
        let all = [LAYERS, files].concat();
        outside_in(declaration.unwrap_or(PYTHON_OUTSIDE), &all)
    }

    const PYTHON_OUTSIDE: &str = "stack = \"python\"\nareas = []\nui_design = \"none\"\nunchecked = [\"scripts\"]\nmoving = [\"legacy\"]\n";

    #[test]
    fn what_an_inner_layer_imports_of_a_path_in_unchecked_or_moving_is_listed_and_fails_nothing() {
        let found = outside_of(
            None,
            &[
                (
                    "domain/a.py",
                    "import os\nfrom legacy import io_helper\nimport scripts.tool\nimport legacy.sub.x\n",
                ),
                ("legacy/io_helper.py", "import shutil\n"),
                ("legacy/sub/x.py", ""),
                ("scripts/tool.py", ""),
                // The layers that do I/O, and wire, may read it
                ("infrastructure/outbound/b.py", "import legacy.io_helper\n"),
                ("composition/c.py", "import scripts.tool\n"),
                // Another path is no path listed
                ("other/d.py", ""),
                ("application/e.py", "import other.d\nimport legacy\n"),
            ],
        );
        assert_eq!(
            found,
            [
                "application/e.py:2 imports legacy, in a path of moving",
                "domain/a.py:2 imports legacy.io_helper, in a path of moving",
                "domain/a.py:3 imports scripts.tool, in a path of unchecked",
                "domain/a.py:4 imports legacy.sub.x, in a path of moving",
            ]
        );
    }

    #[test]
    fn nothing_is_listed_when_no_inner_layer_reads_a_listed_path_or_no_path_is_listed() {
        assert_eq!(
            outside_of(
                None,
                &[
                    ("domain/a.py", "import os\nimport json\n"),
                    ("legacy/io_helper.py", "import shutil\n"),
                ]
            ),
            Vec::<String>::new()
        );
        assert_eq!(
            outside_of(
                Some("stack = \"python\"\nareas = []\nui_design = \"none\"\n"),
                &[("domain/a.py", "from legacy import io_helper\n")]
            ),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_typescript_inner_layer_importing_a_path_in_unchecked_or_moving_is_listed() {
        let found = outside_in(
            "stack = \"typescript\"\nareas = []\nui_design = \"none\"\nunchecked = [\"src/scripts\"]\nmoving = [\"src/legacy\"]\n",
            &[
                ("src/domain/index.ts", ""),
                (
                    "src/domain/a.ts",
                    "import { f } from \"../legacy/helper\";\nimport g from \"../scripts/tool.js\";\nimport h from \"./index\";\n",
                ),
                ("src/legacy/helper.ts", ""),
                ("src/scripts/tool.ts", ""),
                ("src/infrastructure/outbound/index.ts", ""),
                (
                    "src/infrastructure/outbound/b.ts",
                    "import { f } from \"../../legacy/helper\";\n",
                ),
            ],
        );
        assert_eq!(
            found,
            [
                "src/domain/a.ts:1 imports ../legacy/helper, in a path of moving",
                "src/domain/a.ts:2 imports ../scripts/tool.js, in a path of unchecked",
            ]
        );
    }

    #[test]
    fn typescript_files_are_given_the_layers_by_what_they_import_and_by_who_imports_them() {
        let (order, problems) = moving(
            "stack = \"typescript\"\nareas = []\nui_design = \"none\"\nmoving = [\"src/legacy\"]\n",
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

    #[test]
    fn a_python_or_typescript_landing_file_is_read_once_and_gives_what_it_defines() {
        let tree = Fake::new(&[
            (
                "core/domain/song.py",
                "from enum import Enum\n\nclass Dir(Enum):\n    UP = 1\n\nclass Song:\n    pass\n",
            ),
            (
                "web/domain/dir.ts",
                "export const Dir = [\"up\", \"down\"] as const;\nexport type Dir = (typeof Dir)[number];\n\
                 export enum Mode { A }\nexport interface Song { id: number }\n",
            ),
        ]);
        let mut python = Landings::new(&tree, &Readers, Language::Python);
        let defined = python.defined("core/domain/song.py").unwrap().unwrap();
        assert_eq!(defined.vocabularies(), ["Dir"]);
        let mut typescript = Landings::new(&tree, &Readers, Language::TypeScript);
        let defined = typescript.defined("web/domain/dir.ts").unwrap().unwrap();
        assert_eq!(defined.vocabularies(), ["Dir"]);
        // A second ask is answered from what was read, even when the file has changed since
        tree.write("web/domain/dir.ts", "").unwrap();
        let again = typescript.defined("web/domain/dir.ts").unwrap().unwrap();
        assert_eq!(again.vocabularies(), ["Dir"]);
    }

    #[test]
    fn a_landing_file_that_cannot_be_parsed_is_none_and_one_that_is_missing_an_error() {
        let tree = Fake::new(&[("web/domain/broken.ts", "export const = ;;; {\n")]);
        let mut typescript = Landings::new(&tree, &Readers, Language::TypeScript);
        assert!(
            typescript
                .defined("web/domain/broken.ts")
                .unwrap()
                .is_none()
        );
        // A file that is not there is an error, not a file that fails the import silently
        assert!(typescript.defined("web/domain/missing.ts").is_err());
    }

    #[test]
    fn the_crate_of_a_domain_is_read_from_its_program_files_without_what_is_under_cfg_test() {
        let table = table();
        let layout = domain::layers::layout("rust", "none").unwrap();
        let places = layout.places_in(&table, "crates", &domain::layers::UnitTag::default());
        let place = places.iter().find(|p| p.name == "domain").unwrap();
        let song = || vec!["song".to_string()];
        let tree = Fake::new(&[
            ("crates/domain/src/lib.rs", "pub mod song;\n"),
            (
                "crates/domain/src/song.rs",
                "pub enum Dir { Up }\n#[cfg(test)]\nmod tests { impl Dir { fn f(&self) {} } }\n",
            ),
            ("crates/domain/tests/x.rs", "impl Dir { fn g(&self) {} }\n"),
            ("crates/domain/build.rs", "impl Dir { fn h(&self) {} }\n"),
        ]);
        // The impl blocks of the test module, of `tests/` and of a build script are none of the program's
        let krate = domain_crate(&tree, &Readers, &layout, place)
            .unwrap()
            .unwrap();
        assert_eq!(krate.module(&song()).unwrap().vocabularies(), ["Dir"]);
        assert!(krate.module(&[]).unwrap().vocabularies().is_empty());
        // A block in another file of the crate counts, by the path it is written with
        let tree = Fake::new(&[
            ("crates/domain/src/lib.rs", "pub mod song;\npub mod rank;\n"),
            ("crates/domain/src/song.rs", "pub enum Dir { Up }\n"),
            (
                "crates/domain/src/rank/mod.rs",
                "impl crate::song::Dir { pub fn flip(&self) {} }\n",
            ),
        ]);
        let krate = domain_crate(&tree, &Readers, &layout, place)
            .unwrap()
            .unwrap();
        assert!(krate.module(&song()).unwrap().vocabularies().is_empty());
    }
}

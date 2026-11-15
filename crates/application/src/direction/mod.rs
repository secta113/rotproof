//! The direction check, read from the tree: each language's code read through the ports, every import resolved to a
//! place, and judged by the rules in `domain`; and, from the imports from a shared unit to another, the cycles among
//! the shared units.

mod python;
mod rust;
mod typescript;

use std::io;

use crate::tree::{code_files, exactly, is_file};
use domain::declaration::Declared;
use domain::direction::{Edge, Forbidden, cycles, place_of};
use domain::layers::{Language, Layout, Place, table};
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
    let mut direction = match layout.language {
        Language::Python => {
            python::python(tree, parsers, layout, &places, &entries, &declared.units)
        }
        Language::TypeScript => {
            typescript::typescript(tree, parsers, layout, &places, &entries, &declared.units)
        }
        Language::Rust => rust::rust(tree, parsers, layout, &places, &entries, &declared.units),
    }?;
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
    }
}

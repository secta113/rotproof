//! The direction check, read from the tree: each language's code read through the ports, every import resolved to a
//! place, and judged by the rules in `domain`.

mod python;
mod rust;
mod typescript;

use std::io;

use crate::tree::{code_files, exactly, is_file};
use domain::declaration::Declared;
use domain::direction::{Forbidden, place_of};
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
    match layout.language {
        Language::Python => python::python(tree, parsers, layout, &places, &entries),
        Language::TypeScript => typescript::typescript(tree, parsers, layout, &places, &entries),
        Language::Rust => rust::rust(tree, parsers, layout, &places, &entries),
    }
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

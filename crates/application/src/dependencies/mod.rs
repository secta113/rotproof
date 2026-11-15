//! The dependencies check, read from the tree: the manifest of each package (`pyproject.toml`, `package.json`, a
//! crate's `Cargo.toml`) is compared with what the code of the package imports, and judged by the rules in
//! `domain::dependencies`. Each language collects what its code needs in its own module, as the direction check
//! reads imports (the same files, the same readers, the same resolution of where an import lands).
//!
//! The verdict is decided from the tree and the declaration alone. What is installed on a machine is read in one
//! place only, Python's `.venv` (`venv.rs`), and only to word the hint of a message.

mod python;
mod venv;

use std::collections::BTreeSet;
use std::io;

use crate::direction::present_places;
use crate::tree::{is_file, read_code};
use domain::declaration::Declared;
use domain::dependencies::Listing;
use domain::layers::{Language, table};
use protocol::outbound::{Parsers, Tree};

/// The heading the findings are printed under.
pub const HEADING: &str = "the manifests list the dependencies the code imports";

/// What one run read of the manifests of the project, for the check of the declaration that names them.
#[derive(Debug, Default)]
struct Read {
    /// Whether any manifest was read: a project whose manifests are all missing has nothing to compare
    any: bool,
    /// The runtime dependencies every manifest read lists, as the language compares names
    runtime: BTreeSet<String>,
}

/// Every finding of the dependencies check of `declared`, in the order of the manifests. An error is a file that
/// could not be read at all.
pub fn problems(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    declared: &Declared,
) -> io::Result<Vec<String>> {
    let Some(layout) = &declared.layout else {
        // A repository of records only has no code to compare
        return Ok(Vec::new());
    };
    let places = present_places(tree, declared);
    let entries = declared.entries(&table());
    let mut found = Vec::new();
    let read = match layout.language {
        Language::Python => python::python(
            tree, parsers, declared, layout, &places, &entries, &mut found,
        )?,
        Language::TypeScript | Language::Rust => Read::default(),
    };
    if read.any {
        for name in
            domain::dependencies::stale(&without_import(declared, layout.language), &read.runtime)
        {
            found.push(format!(
                "{}: without_import under [dependencies] lists {name}, which no manifest lists as a dependency: take \
                 it out",
                domain::declaration::DECLARATION
            ));
        }
    }
    Ok(found)
}

/// The names `without_import` lists, as the language compares names.
fn without_import(declared: &Declared, language: Language) -> Vec<String> {
    declared
        .declaration
        .dependencies
        .without_import
        .iter()
        .map(|name| normalized(language, name.trim()))
        .collect()
}

/// A name as the language compares names.
fn normalized(language: Language, name: &str) -> String {
    match language {
        Language::Python => domain::dependencies::python::normalize(name),
        Language::TypeScript => name.to_string(),
        Language::Rust => domain::dependencies::rust::normalize(name),
    }
}

/// The manifest at `path`, read: `None` when there is none, which is no finding (a unit with no manifest is not
/// compared with it), or when it cannot be read, which is one in `found`.
fn listing(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    language: Language,
    path: &str,
    found: &mut Vec<String>,
) -> io::Result<Option<Listing>> {
    if !is_file(tree, path) {
        return Ok(None);
    }
    let Some(source) = read_code(tree, path, "dependencies", found)? else {
        return Ok(None);
    };
    match parsers.listing(language, &source) {
        Ok(listing) => Ok(Some(listing)),
        Err((line, why)) => {
            let format = match language {
                Language::TypeScript => "JSON",
                Language::Python | Language::Rust => "TOML",
            };
            found.push(format!(
                "{path}:{line}: cannot be read as {format} ({why}), so its dependencies cannot be compared with the \
                 imports: fix the file"
            ));
            Ok(None)
        }
    }
}

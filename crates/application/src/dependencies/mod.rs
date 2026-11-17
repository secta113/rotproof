//! The dependencies check, read from the tree: the manifest of each package (`pyproject.toml`, `package.json`, a
//! crate's `Cargo.toml`) is compared with what the code of the package imports, and judged by the rules in
//! `domain::dependencies`. Each language collects what its code needs in its own module, as the direction check
//! reads imports (the same files, the same readers, the same resolution of where an import lands).
//!
//! The verdict is decided from the tree and the declaration alone. What is installed on a machine is read in one
//! place only, Python's `.venv` (`venv.rs`), and only to word the hint of a message.

mod python;
mod rust;
mod typescript;
mod venv;

use std::collections::BTreeSet;
use std::io;

use crate::direction::present_places;
use crate::tree::{is_file, read_code};
use domain::declaration::Declared;
use domain::dependencies::Listing;
pub use domain::dependencies::Use;
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
    /// Every dependency every manifest read lists, of any kind, as the language compares names
    listed: BTreeSet<String>,
    /// Every import of a package (not of a unit, the standard library or a module of the project) in the code the
    /// check reads, tests included, whether or not the unit has a manifest: Python and TypeScript, and none in Rust,
    /// whose crates are not told from the paths that name them
    packages: Vec<Use>,
    /// Every import of a package in the files that are moving, which the check reads for nothing else: they are no part
    /// of what the manifests are compared with. Only for the files `read_all` was asked for (`Ask::Moving`)
    moving: Vec<Use>,
}

/// The packages the code of a project imports, and what its manifests list: what the allowlist of the inner layers
/// (`inner_io.rs`) is judged by, read as the dependencies check reads them.
#[derive(Debug, Default)]
pub struct Packages {
    /// Whether any manifest was read
    pub manifests: bool,
    /// What every manifest read lists, of any kind, as the language compares names
    pub listed: BTreeSet<String>,
    /// Each import of a package, in the order of the files and the lines of the code of the layers and the entry
    /// files, then the tests
    pub uses: Vec<Use>,
}

/// What the dependencies check reads of `declared`, with the findings dropped: Python and TypeScript only.
pub fn packages(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    declared: &Declared,
) -> io::Result<Packages> {
    let read = read_all(tree, parsers, declared, Ask::Whole, &mut Vec::new())?;
    Ok(Packages {
        manifests: read.any,
        listed: read.listed,
        uses: read.packages,
    })
}

/// Every import of a package, as the check reads one, in each of the code files `files` that are moving into the
/// layers (`moving` in the declaration): the packages they need as the code of a layer would, by the unit whose
/// directory holds each file. They are compared with no manifest, as the code of no layer is. Python and TypeScript
/// only.
pub fn packages_of(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    declared: &Declared,
    files: &[String],
) -> io::Result<Vec<Use>> {
    Ok(read_all(tree, parsers, declared, Ask::Moving(files), &mut Vec::new())?.moving)
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
    let mut found = Vec::new();
    let read = read_all(tree, parsers, declared, Ask::Whole, &mut found)?;
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

/// What a reading is for.
#[derive(Clone, Copy)]
enum Ask<'a> {
    /// The manifests and every import of the code the check reads, compared
    Whole,
    /// The imports of a package in these files that are moving, and nothing else: the code of the layers is not read
    Moving(&'a [String]),
}

/// The manifests and the code of `declared` read by the reader of its language, the findings of the comparison into
/// `found`, or, when only the files that are moving are asked for, the imports of a package in them.
fn read_all(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    declared: &Declared,
    ask: Ask,
    found: &mut Vec<String>,
) -> io::Result<Read> {
    let Some(layout) = &declared.layout else {
        return Ok(Read::default());
    };
    let places = present_places(tree, declared);
    match layout.language {
        Language::Python => python::python(tree, parsers, declared, layout, &places, ask, found),
        Language::TypeScript => {
            typescript::typescript(tree, parsers, declared, layout, &places, ask, found)
        }
        Language::Rust => {
            let entries = declared.entries(&table());
            rust::rust(tree, parsers, declared, layout, &places, &entries, found)
        }
    }
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

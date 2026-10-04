//! The readers of code: [`Parsers`] with Ruff for Python, oxc for TypeScript and toml_edit for a `Cargo.toml`.

use std::collections::BTreeSet;
use std::io;

use crate::code::{Aliases, Manifest, Parsers, Source};
use crate::tree::Tree;

/// Every reader Rotproof has.
#[derive(Debug, Default, Clone, Copy)]
pub struct Readers;

impl Parsers for Readers {
    fn python(&self, source: &str, path: &str) -> Source<Vec<String>> {
        crate::python::read(source, path)
    }

    fn python_definitions(&self, source: &str) -> BTreeSet<String> {
        crate::python::definitions(source)
    }

    fn typescript(&self, source: &str, path: &str) -> Source<String> {
        crate::typescript::read(source, path)
    }

    fn typescript_aliases(&self, tree: &dyn Tree) -> io::Result<Aliases> {
        crate::typescript::aliases(tree)
    }

    fn manifest(&self, source: &str) -> Result<Manifest, (usize, String)> {
        crate::cargo::read(source)
    }
}

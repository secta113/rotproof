//! The readers of code: [`Parsers`] with Ruff for Python, oxc for TypeScript and toml_edit for a `Cargo.toml`.

use std::collections::BTreeSet;
use std::io;

use domain::code::{Aliases, Manifest, Source};
use protocol::outbound::Parsers;
use protocol::outbound::Tree;

/// Every reader Rotproof has.
#[derive(Debug, Default, Clone, Copy)]
pub struct Readers;

impl Parsers for Readers {
    fn python(&self, source: &str, path: &str) -> Source<Vec<String>> {
        crate::outbound::python::read(source, path)
    }

    fn python_definitions(&self, source: &str) -> BTreeSet<String> {
        crate::outbound::python::definitions(source)
    }

    fn python_definition(&self, source: &str, dotted: &str) -> Option<String> {
        crate::outbound::python::definition(source, dotted)
    }

    fn typescript(&self, source: &str, path: &str) -> Source<String> {
        crate::outbound::typescript::read(source, path)
    }

    fn typescript_aliases(&self, tree: &dyn Tree) -> io::Result<Aliases> {
        crate::outbound::typescript::aliases(tree)
    }

    fn manifest(&self, source: &str) -> Result<Manifest, (usize, String)> {
        crate::outbound::cargo::read(source)
    }
}

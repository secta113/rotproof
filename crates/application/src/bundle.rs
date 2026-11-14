//! `docs/knowledge/` read from the tree: its documents, sorted out by the rules in `domain`.

use std::io;

use crate::tree::read_text;
use domain::bundle::{
    Docs, KnowledgeFolder, Problems, expected, in_docs, is_document, knowledge_with_rules,
};
use protocol::outbound::Tree;

/// The bundle of one repository, `docs/` in its tree, and the areas its knowledge documents are grouped by.
pub struct Bundle<'a> {
    tree: &'a dyn Tree,
    /// In the order the index files list them
    pub areas: Vec<String>,
}

impl<'a> Bundle<'a> {
    pub fn new(tree: &'a dyn Tree, areas: Vec<String>) -> Self {
        Bundle { tree, areas }
    }

    /// The documents directly in `docs/<folder>/`, except reserved names: file name -> text. An error names the path
    /// it could not read.
    pub fn read_folder(&self, folder: &str) -> io::Result<Docs> {
        self.read_docs(folder).map_err(Unreadable::with_path)
    }

    /// The documents directly in `docs/<folder>/`, as [`read_folder`](Self::read_folder) reads them, with the path an
    /// error could not read kept apart from why, for a caller that says it in its own words.
    pub fn read_docs(&self, folder: &str) -> Result<Docs, Unreadable> {
        let dir = in_docs(folder);
        let mut docs = Docs::new();
        for (name, is_dir) in self.tree.entries(&dir).map_err(|error| Unreadable {
            path: dir.clone(),
            error,
        })? {
            if is_document(&name, is_dir) {
                let path = format!("{dir}/{name}");
                let text =
                    read_text(self.tree, &path).map_err(|error| Unreadable { path, error })?;
                docs.insert(name, text);
            }
        }
        Ok(docs)
    }

    /// Every file Rotproof generates in the bundle (the index files and the rules) -> what it should contain now, and
    /// the documents left out of the index files.
    pub fn expected(&self) -> io::Result<(Vec<(String, String)>, Problems)> {
        Ok(self.expected_of(&self.read_folder("knowledge")?))
    }

    /// What [`expected`](Self::expected) gives for `docs`, the documents of `docs/knowledge/` already read.
    pub fn expected_of(&self, docs: &Docs) -> (Vec<(String, String)>, Problems) {
        expected(docs, &self.areas)
    }

    /// The documents of `docs/knowledge/`.
    pub fn read_knowledge(&self) -> io::Result<KnowledgeFolder> {
        Ok(self.knowledge_of(&self.read_folder("knowledge")?))
    }

    /// What [`read_knowledge`](Self::read_knowledge) gives for `docs`, the documents of `docs/knowledge/` already
    /// read.
    pub fn knowledge_of(&self, docs: &Docs) -> KnowledgeFolder {
        knowledge_with_rules(docs, &self.areas)
    }
}

/// A file or directory of the bundle that could not be read: its path, and why.
#[derive(Debug)]
pub struct Unreadable {
    pub path: String,
    pub error: io::Error,
}

impl Unreadable {
    /// The error with the path in front of why, as the commands that write say it.
    pub fn with_path(self) -> io::Error {
        io::Error::new(self.error.kind(), format!("{}: {}", self.path, self.error))
    }
}

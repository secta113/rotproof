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

    /// The documents directly in `docs/<folder>/`, except reserved names: file name -> text.
    pub fn read_folder(&self, folder: &str) -> io::Result<Docs> {
        let dir = in_docs(folder);
        let mut docs = Docs::new();
        for (name, is_dir) in self.tree.entries(&dir).map_err(|e| with_path(e, &dir))? {
            if is_document(&name, is_dir) {
                let path = format!("{dir}/{name}");
                let text = read_text(self.tree, &path).map_err(|e| with_path(e, &path))?;
                docs.insert(name, text);
            }
        }
        Ok(docs)
    }

    /// Every file Rotproof generates in the bundle (the index files and the rules) -> what it should contain now, and
    /// the documents left out of the index files.
    pub fn expected(&self) -> io::Result<(Vec<(String, String)>, Problems)> {
        let knowledge = self.read_folder("knowledge")?;
        Ok(expected(&knowledge, &self.areas))
    }

    /// The documents of `docs/knowledge/`.
    pub fn read_knowledge(&self) -> io::Result<KnowledgeFolder> {
        Ok(knowledge_with_rules(
            &self.read_folder("knowledge")?,
            &self.areas,
        ))
    }
}

fn with_path(e: io::Error, path: &str) -> io::Error {
    io::Error::new(e.kind(), format!("{path}: {e}"))
}

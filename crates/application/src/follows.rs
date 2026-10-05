//! What a knowledge document follows, read from the tree and hashed as it is now, against the hashes the document
//! pinned (`follows.rs` in `domain`).

use std::collections::BTreeMap;

use crate::tree::{exactly, read_text};
use domain::code::Parsers;
use domain::follows::{Followed, directory_hash, drifted, followed};
use domain::records::content_hash;
use domain::schema::{Knowledge, Status};
use domain::tree::Tree;

/// The hash of what `key` names as it is now: `Ok(None)` when it is not there, `Err` when it cannot be read.
pub fn current(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    key: &str,
) -> Result<Option<String>, String> {
    let file_text = |path: &str| -> Result<Option<String>, String> {
        match exactly(tree, path) {
            Ok((found, false)) => read_text(tree, &found)
                .map(Some)
                .map_err(|e| format!("{path} cannot be read: {e}")),
            _ => Ok(None),
        }
    };
    match followed(key)? {
        Followed::File(path) => Ok(file_text(&path)?.map(|text| content_hash(&text))),
        Followed::Definition { file, name } => Ok(file_text(&file)?
            .and_then(|source| parsers.python_definition(&source, &name))
            .map(|text| content_hash(&text.replace("\r\n", "\n")))),
        Followed::Directory(dir) => {
            if !matches!(exactly(tree, &dir), Ok((_, true))) {
                return Ok(None);
            }
            let paths = tree
                .files(&dir)
                .map_err(|e| format!("{dir}/ cannot be read: {e}"))?;
            let mut files = Vec::new();
            for path in paths.into_iter().filter(|p| p.ends_with(".py")) {
                let text =
                    read_text(tree, &path).map_err(|e| format!("{path} cannot be read: {e}"))?;
                files.push((path, text));
            }
            Ok(Some(directory_hash(&files)))
        }
    }
}

/// Every way the knowledge documents that hold (`documents`: file name -> document) drifted from what they follow. A
/// deprecated document no longer holds, so what it followed is not read.
pub fn drift(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    documents: &BTreeMap<String, Knowledge>,
) -> Vec<String> {
    let mut found = Vec::new();
    for (name, doc) in documents {
        if doc.status == Status::Deprecated {
            continue;
        }
        for (key, pinned) in &doc.follows {
            match current(tree, parsers, key) {
                Ok(now) => found.extend(drifted(name, key, pinned, now.as_deref())),
                Err(why) => found.push(format!("knowledge/{name} follows {key}: {why}")),
            }
        }
    }
    found
}

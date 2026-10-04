//! Reading a project's declaration, `.config/rotproof.toml`, from its tree.

use std::io;

use crate::tree::{Lookup, exactly, lookup};
use domain::layers::{DECLARATION, Declaration, MISSING, parse_declaration};
use domain::tree::Tree;

/// The areas a project declares, or why the declaration cannot be read.
pub fn areas(tree: &dyn Tree) -> io::Result<Result<Vec<String>, String>> {
    Ok(match declaration(tree)? {
        None => Err(MISSING.into()),
        Some(Err(why)) => Err(format!("{DECLARATION}: {why}")),
        Some(Ok(declaration)) => Ok(declaration.areas),
    })
}

/// The declaration, or why it cannot be read. `Ok(None)` when the file does not exist.
pub fn declaration(tree: &dyn Tree) -> io::Result<Option<Result<Declaration, String>>> {
    // By the exact name, before anything asks the operating system: `.config/Rotproof.toml` opens as the declaration on
    // Windows, and is not there on Linux, where asking first would say only that the declaration is missing
    let path = match lookup(tree, "", DECLARATION) {
        Lookup::Found { path, .. } => path,
        Lookup::Missing => return Ok(None),
        Lookup::Spelled(_) => {
            return Ok(Some(Err(
                exactly(tree, DECLARATION).expect_err("spelled otherwise")
            )));
        }
    };
    let text = tree
        .read(&path)
        .map_err(|e| io::Error::new(e.kind(), format!("{DECLARATION}: {e}")))?;
    Ok(Some(parse_declaration(&text)))
}

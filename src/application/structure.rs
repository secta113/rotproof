//! The structure check, read from the tree: the declaration, the paths the rules name and the code files of the
//! directories they list, judged by the rules in `domain`.

use std::io;

use crate::application::layers::declaration;
use crate::application::tree::{code_files, exactly};
use crate::domain::layers::{DECLARATION, Declared, MISSING};
use crate::domain::structure::{Seen, listed, named, problems as judged};
use crate::domain::tree::Tree;

/// What the structure check found.
#[derive(Debug, Default)]
pub struct Structure {
    /// Every way the tree differs from its declaration
    pub found: Vec<String>,
    /// What was not checked, and why. Printed on every run, so a skipped check is never silent
    pub skipped: Option<String>,
    /// The declaration with its layout, when it can be read and fits, for the checks that follow
    pub declared: Option<Declared>,
}

/// Every way `tree` differs from its declaration. An error is a file or directory that could not be read.
pub fn problems(tree: &dyn Tree) -> io::Result<Structure> {
    let failed = |found: Vec<String>| {
        Ok(Structure {
            found,
            skipped: None,
            declared: None,
        })
    };
    let declared = match declaration(tree)? {
        None => return failed(vec![MISSING.into()]),
        Some(Err(why)) => return failed(vec![format!("{DECLARATION}: {why}")]),
        Some(Ok(declaration)) => match Declared::new(declaration) {
            Ok(declared) => declared,
            Err(found) => return failed(found),
        },
    };
    let Some(layout) = &declared.layout else {
        return Ok(Structure {
            found: Vec::new(),
            skipped: Some(format!(
                "the layers are not checked: {DECLARATION} declares stack = \"none\" (records only)"
            )),
            declared: Some(declared),
        });
    };
    let mut seen = Seen::default();
    for path in named(&declared) {
        let exact = exactly(tree, &path).map(|_| ());
        seen.named.insert(path, exact);
    }
    for dir in listed(&declared, layout, &seen) {
        let code = code_files(tree, &dir, |name| layout.is_code(name))?;
        seen.code.insert(dir, code);
    }
    Ok(Structure {
        found: judged(&declared, layout, &seen),
        skipped: None,
        declared: Some(declared),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::tree::fake::Fake;

    #[test]
    fn the_tree_is_read_through_the_port_alone() {
        // No file system: a declaration, two layers, one in another case, and code outside the layers
        let tree = Fake::new(&[
            (
                ".config/rotproof.toml",
                "stack = \"python\"\nareas = [\"a\"]\nabsent = [\"ui\", \"handler\", \"application\"]\n",
            ),
            ("domain/__init__.py", ""),
            ("Infrastructure/__init__.py", ""),
            ("utils/__init__.py", ""),
            ("scripts/run.py", ""),
        ]);
        let found = problems(&tree).unwrap().found;
        assert_eq!(
            found,
            [
                "infrastructure is missing: infrastructure (Infrastructure is there: names are compared exactly, as \
                 Linux and GitHub compare them)",
                "code outside the layers: Infrastructure (move it into a layer, or list it in unchecked in \
                 .config/rotproof.toml)",
                "code outside the layers: scripts (move it into a layer, or list it in unchecked in \
                 .config/rotproof.toml)",
            ]
        );
    }
}

//! Rotproof's own layers, checked by Rotproof's library as `rotproof check` checks a project's: every layer of the Rust
//! layout is a crate in `crates/`, each crate depends only on the layers the table allows, and no comment in `crates/`
//! or in `xtask/` holds a marker (xtask is outside the layers, where the Rust layout does not look).
//!
//! Rotproof keeps no `.config/rotproof.toml` and no `docs/`: its records are kept outside the repository. So the
//! declaration is [`DECLARATION_TEXT`] below, put in place of the file on the tree the checks read, and the records
//! are not checked. No option of the command line serves this one repository (the rust-stack spec, decision 4).

use std::collections::BTreeSet;
use std::io;
use std::path::Path;

use application::tree::read_code;
use domain::layers::DECLARATION;
use domain::markers::{MARKERS, in_file};
use domain::tree::Tree;
use infrastructure::disk::Disk;
use infrastructure::readers::Readers;

/// Rotproof's declaration: the Rust layout, every layer present. The records are kept outside, so it names no area
pub const DECLARATION_TEXT: &str = "stack = \"rust\"\nareas = []\n";

/// Where xtask sits, outside the layers: its comments are read for markers too
const XTASK: &str = "xtask";

/// The tree of the repository at `root`, with [`DECLARATION_TEXT`] at [`DECLARATION`]: every other path is the disk's.
struct WithDeclaration {
    disk: Disk,
}

impl WithDeclaration {
    /// The directory that holds the declaration, and its name in it
    fn place() -> (&'static str, &'static str) {
        DECLARATION
            .rsplit_once('/')
            .expect("the declaration sits in a directory")
    }
}

impl Tree for WithDeclaration {
    fn read(&self, path: &str) -> io::Result<String> {
        if path == DECLARATION {
            return Ok(DECLARATION_TEXT.into());
        }
        self.disk.read(path)
    }

    fn entries(&self, dir: &str) -> io::Result<Vec<(String, bool)>> {
        let (folder, name) = Self::place();
        let mut entries = match self.disk.entries(dir) {
            Ok(entries) => entries,
            Err(_) if dir == folder => Vec::new(),
            Err(e) => return Err(e),
        };
        let added = if dir.is_empty() {
            (folder, true)
        } else if dir == folder {
            (name, false)
        } else {
            return Ok(entries);
        };
        if !entries.iter().any(|(entry, _)| entry == added.0) {
            entries.push((added.0.to_string(), added.1));
        }
        Ok(entries)
    }

    fn files(&self, dir: &str) -> io::Result<Vec<String>> {
        self.disk.files(dir)
    }

    fn landed(&self, dir: &str, rel: &str) -> Option<String> {
        self.disk.landed(dir, rel)
    }

    fn found(&self, path: &str) -> Option<bool> {
        if path == DECLARATION {
            Some(false)
        } else if path == Self::place().0 {
            Some(true)
        } else {
            self.disk.found(path)
        }
    }
}

/// Every way Rotproof's own layers break the rules, each named by its check. An error is a file that could not be read.
pub fn problems(root: &Path) -> Result<Vec<String>, String> {
    let tree = WithDeclaration {
        disk: Disk::new(root),
    };
    let structure = application::structure::problems(&tree).map_err(|e| e.to_string())?;
    let mut found: Vec<String> = structure
        .found
        .into_iter()
        .map(|why| format!("structure: {why}"))
        .collect();
    // A skipped check is a failure here: the declaration names a stack with layers, so nothing may be skipped
    found.extend(structure.skipped.map(|why| format!("structure: {why}")));
    let Some(declared) = structure.declared else {
        return Ok(found);
    };
    let direction =
        application::direction::problems(&tree, &Readers, &declared).map_err(|e| e.to_string())?;
    found.extend(direction.into_iter().map(|why| format!("direction: {why}")));
    let mut markers =
        application::markers::problems(&tree, &Readers, &declared).map_err(|e| e.to_string())?;
    let (in_xtask, words) = xtask_markers(&tree)?;
    markers.found.extend(in_xtask);
    let mut all: BTreeSet<usize> = markers
        .words
        .iter()
        .filter_map(|word| MARKERS.iter().position(|m| m == word))
        .collect();
    all.extend(words);
    markers.words = all.into_iter().map(|i| MARKERS[i]).collect();
    if !markers.found.is_empty() {
        // As `rotproof check` prints them: the heading once, and each finding under it with the line it names
        found.push(format!("markers: {}", markers.heading()));
        found.extend(
            markers
                .found
                .iter()
                .map(|why| format!("  {}", why.replace('\n', "\n    "))),
        );
    }
    Ok(found)
}

/// The markers in the comments of the `.rs` files in [`XTASK`], read as the marker check reads a Rust layout's: the
/// findings, and the markers as positions in [`MARKERS`]. xtask sits outside the layers, where the Rust layout does
/// not look, and is Rotproof's code all the same (user, 2026-10-05). The floor: at least one file is read.
fn xtask_markers(tree: &dyn Tree) -> Result<(Vec<String>, BTreeSet<usize>), String> {
    let mut found = Vec::new();
    let mut words = BTreeSet::new();
    let mut read = 0;
    let paths = tree.files(XTASK).map_err(|e| format!("{XTASK}: {e}"))?;
    for path in paths
        .iter()
        .filter(|path| utils::rust::is_source(utils::paths::file_name(path)))
    {
        let Some(source) =
            read_code(tree, path, "comments", &mut found).map_err(|e| e.to_string())?
        else {
            continue;
        };
        read += 1;
        let (here, positions) = in_file(path, &source, &utils::rust::comments(&source));
        found.extend(here);
        words.extend(positions);
    }
    if read == 0 {
        found.push(format!("no .rs file of {XTASK}/ was read"));
    }
    Ok((found, words))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_declaration_is_found_where_rotproof_looks_and_nothing_else_moves() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("crates/domain")).unwrap();
        std::fs::write(root.path().join("README.md"), "# Readme\n").unwrap();
        let tree = WithDeclaration {
            disk: Disk::new(root.path()),
        };
        assert_eq!(tree.read(DECLARATION).unwrap(), DECLARATION_TEXT);
        let mut top = tree.entries("").unwrap();
        top.sort();
        assert_eq!(
            top,
            [
                (".config".to_string(), true),
                ("README.md".to_string(), false),
                ("crates".to_string(), true),
            ]
        );
        assert_eq!(
            tree.entries(".config").unwrap(),
            [("rotproof.toml".to_string(), false)]
        );
        assert_eq!(tree.found(DECLARATION), Some(false));
        assert_eq!(tree.read("README.md").unwrap(), "# Readme\n");
        assert!(tree.entries("nowhere").is_err());
        // Read as Rotproof reads it, by its exact name
        let declaration = application::layers::declaration(&tree).unwrap();
        assert_eq!(declaration.unwrap().unwrap().stack, "rust");
    }
}

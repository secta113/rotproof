//! The code that is moving, read from the tree: the code files in the paths `moving` lists, and which of them import
//! which, and what each does that the check of the I/O of the inner layers fails. No finding comes of it: a file that
//! cannot be read, or whose imports are misread, only leaves out what it would have said, since no check judges the
//! code that is moving.

use std::collections::{BTreeMap, BTreeSet};
use std::io;

use crate::tree::{code_files, exactly};
use domain::code::is_source;
use domain::declaration::Declared;
use domain::direction::Landing;
use domain::inner_io::Allowlist;
use domain::layers::{Language, Layout, Place};
use domain::moving::{Does, File, Order, order};
use domain::paths::{file_name, join, parent};
use domain::rust::is_source as is_rust;
use domain::units::Unit;

/// What is read of one file that is moving.
#[derive(Debug, Default)]
struct Read<'a> {
    /// The files that are moving that it imports
    needs: BTreeSet<String>,
    /// Where its other imports land
    landings: Vec<Landing<'a>>,
    /// The places of the files that import it that are in a layer, or are an entry file, by their unit and name: a
    /// place is named so, not held, as the file that is read is no longer than the loop that reads it
    readers: BTreeSet<(String, String)>,
    /// What it does that the check of the I/O of the inner layers fails
    does: Does,
}

/// The files of the paths `moving` lists, the imports among them, and what each imports of the layers and the layers
/// import of it.
#[derive(Debug, Default)]
pub(super) struct Moving<'a> {
    /// Whether the declaration lists a path at all: nothing is said of a project that lists none
    listed: bool,
    /// Every code file in the paths listed, by path
    files: BTreeMap<String, Read<'a>>,
}

impl<'a> Moving<'a> {
    /// The code files of the paths `declared` lists in `moving` that are there, as the language of `layout` reads them.
    /// A path that is not there, or holds or sits in a layer, is the structure check's to say.
    pub(super) fn list(
        tree: &dyn protocol::outbound::Tree,
        layout: &Layout,
        declared: &Declared,
    ) -> io::Result<Moving<'a>> {
        let mut moving = Moving {
            listed: !declared.declaration.moving.is_empty(),
            ..Moving::default()
        };
        let is_code = |name: &str| match layout.language {
            Language::Python => layout.is_code(name),
            Language::TypeScript => is_source(name),
            Language::Rust => is_rust(name),
        };
        for path in declared
            .moving()
            .filter(|path| declared.overlapped(path).is_none())
        {
            let Ok((path, is_dir)) = exactly(tree, path) else {
                continue;
            };
            // A test is no code to move into a layer, wherever it sits: the paths of `moving` that are one fail as a
            // declaration, and the tests inside a broader path are left out
            if is_dir {
                for file in code_files(tree, &path, is_code)? {
                    if !declared.is_tests(&file) {
                        moving.files.insert(file, Read::default());
                    }
                }
            } else if is_code(file_name(&path)) && !declared.is_tests(&path) {
                moving.files.insert(path, Read::default());
            }
        }
        Ok(moving)
    }

    /// The files, in the byte order of their paths.
    pub(super) fn files(&self) -> Vec<String> {
        self.files.keys().cloned().collect()
    }

    /// The file that is moving among `candidates`, the first that is: the files a module at the place an import lands
    /// can be, the most specific first.
    fn find(&self, candidates: impl IntoIterator<Item = String>) -> Option<String> {
        candidates
            .into_iter()
            .find(|candidate| self.files.contains_key(candidate))
    }

    /// Note an import by the file `from`, which is moving, of what `candidates` name and `landing` is where it lands: a
    /// file that is moving (not itself), or else where it lands, when it lands anywhere.
    pub(super) fn imports(
        &mut self,
        from: &str,
        candidates: Vec<String>,
        landing: Option<Landing<'a>>,
    ) {
        let imported = self.find(candidates);
        let Some(read) = self.files.get_mut(from) else {
            return;
        };
        match (imported, landing) {
            (Some(to), _) if to != from => {
                read.needs.insert(to);
            }
            (Some(_), _) => {}
            (None, Some(landing)) => read.landings.push(landing),
            (None, None) => {}
        }
    }

    /// Note an import by a file of the place `reader`, a layer's or an entry file's, of what `candidates` name, when it
    /// is a file that is moving. `candidates` is asked for only when there is such a file to find.
    pub(super) fn read_by(&mut self, reader: &Place, candidates: impl FnOnce() -> Vec<String>) {
        if self.files.is_empty() {
            return;
        }
        if let Some(to) = self.find(candidates())
            && let Some(read) = self.files.get_mut(&to)
        {
            read.readers
                .insert((reader.unit.name.clone(), reader.name.clone()));
        }
    }

    /// Note what each file does, by its path: a file that is not here is read as one that does nothing.
    pub(super) fn does(&mut self, found: BTreeMap<String, Does>) {
        for (path, does) in found {
            if let Some(read) = self.files.get_mut(&path) {
                read.does = does;
            }
        }
    }

    /// What to say of the files: `None` when the declaration lists no path. `read` is whether their imports were read:
    /// where they were not, they are counted and not put in an order. `places` and `entries` are what the readers of a
    /// file are found among, and what it could be moved into, and `allowed` is what the inner layers may import of the
    /// packages.
    pub(super) fn order(
        &self,
        read: bool,
        places: &[&'a Place],
        entries: &'a [Place],
        units: &[Unit],
        allowed: &Allowlist,
    ) -> Option<Order> {
        if !self.listed {
            return None;
        }
        if !read {
            return Some(Order::unordered(self.files.len()));
        }
        let files: Vec<File> = self
            .files
            .iter()
            .map(|(path, read)| File {
                path: path.clone(),
                needs: read.needs.clone(),
                landings: read.landings.clone(),
                readers: read
                    .readers
                    .iter()
                    .filter_map(|(unit, name)| {
                        places
                            .iter()
                            .copied()
                            .chain(entries)
                            .find(|place| &place.unit.name == unit && &place.name == name)
                    })
                    .collect(),
                does: read.does.clone(),
            })
            .collect();
        Some(order(&files, places, units, allowed))
    }
}

/// The files a Python module at `target` (a path from the root, the module's parts joined with `/`) can be, the most
/// specific first: the module is a file (`legacy/order.py`) or a package (`legacy/order/__init__.py`), and a name
/// imported from it (`legacy.order.total`, which is not a file) is read as the module that holds it.
pub(super) fn python_files(target: &str) -> Vec<String> {
    let mut files = Vec::new();
    let mut at = target;
    while !at.is_empty() {
        files.push(format!("{at}.py"));
        files.push(join(at, "__init__.py"));
        at = parent(at);
    }
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_python_module_is_a_file_or_a_package_and_a_name_in_it_is_read_as_the_module() {
        assert_eq!(
            python_files("legacy/order/total"),
            [
                "legacy/order/total.py",
                "legacy/order/total/__init__.py",
                "legacy/order.py",
                "legacy/order/__init__.py",
                "legacy.py",
                "legacy/__init__.py",
            ]
        );
        assert!(python_files("").is_empty());
    }
}

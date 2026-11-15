//! The code that is moving, read from the tree: the code files in the paths `moving` lists, and which of them import
//! which. No finding comes of it: a file that cannot be read, or whose imports are misread, only leaves out what it
//! would have said, since no check judges the code that is moving.

use std::collections::{BTreeMap, BTreeSet};
use std::io;

use crate::tree::{code_files, exactly};
use domain::code::is_source;
use domain::declaration::Declared;
use domain::layers::{Language, Layout};
use domain::moving::{File, Order, order};
use domain::paths::{file_name, join, parent};
use domain::rust::is_source as is_rust;

/// The files of the paths `moving` lists, and the imports among them.
#[derive(Debug, Default)]
pub(super) struct Moving {
    /// Whether the declaration lists a path at all: nothing is said of a project that lists none
    listed: bool,
    /// What the files import among themselves, by path: every code file in the paths listed
    needs: BTreeMap<String, BTreeSet<String>>,
}

impl Moving {
    /// The code files of the paths `declared` lists in `moving` that are there, as the language of `layout` reads them.
    /// A path that is not there, or holds or sits in a layer, is the structure check's to say.
    pub(super) fn list(
        tree: &dyn protocol::outbound::Tree,
        layout: &Layout,
        declared: &Declared,
    ) -> io::Result<Moving> {
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
            if is_dir {
                for file in code_files(tree, &path, is_code)? {
                    moving.needs.insert(file, BTreeSet::new());
                }
            } else if is_code(file_name(&path)) {
                moving.needs.insert(path, BTreeSet::new());
            }
        }
        Ok(moving)
    }

    /// The files, in the byte order of their paths.
    pub(super) fn files(&self) -> Vec<String> {
        self.needs.keys().cloned().collect()
    }

    /// Note that the file `from` imports a file that is moving, the first of `candidates` that is: the files a module
    /// at the place an import lands can be, the most specific first. An import of itself is none.
    pub(super) fn imports(&mut self, from: &str, candidates: impl IntoIterator<Item = String>) {
        let Some(to) = candidates
            .into_iter()
            .find(|candidate| self.needs.contains_key(candidate))
        else {
            return;
        };
        if to != from
            && let Some(needs) = self.needs.get_mut(from)
        {
            needs.insert(to);
        }
    }

    /// What to say of the files: `None` when the declaration lists no path. `read` is whether their imports were read:
    /// where they were not, they are counted and not put in an order.
    pub(super) fn order(&self, read: bool) -> Option<Order> {
        if !self.listed {
            return None;
        }
        if !read {
            return Some(Order::unordered(self.needs.len()));
        }
        let files: Vec<File> = self
            .needs
            .iter()
            .map(|(path, needs)| File {
                path: path.clone(),
                needs: needs.clone(),
            })
            .collect();
        Some(order(&files))
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

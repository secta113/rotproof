//! What a project's `.venv` provides, read from the metadata of the distributions installed in it, for the hint of a
//! message: which distribution an import of a module is of, so that the line to add can be named.
//!
//! This is a convenience and never evidence. A `.venv` is ignored by git and differs from one machine to another, so
//! the verdict of the check never reads it: the same files give the same answer in CI, which installs nothing. It is
//! read through the tree's directories and files and not its walk, which leaves out a directory that is hidden or
//! ignored.

use std::collections::BTreeMap;

use domain::dependencies::python::{dist_info_name, record_names};
use domain::paths::join;
use protocol::outbound::Tree;

/// The distributions installed in an environment, by the names its modules are imported as.
pub(super) struct Venv {
    /// The environment's directory, from the root
    pub dir: String,
    provides: BTreeMap<String, Vec<String>>,
}

impl Venv {
    /// The environment in the directory `dir`, or `None` when there is none, or it has no `site-packages` (a
    /// Windows environment keeps it in `Lib/`, another in `lib/python3.x/`) holding the metadata of a distribution.
    pub(super) fn read(tree: &dyn Tree, dir: &str) -> Option<Venv> {
        if tree.found(dir) != Some(true) {
            return None;
        }
        let mut sites = vec![join(dir, "Lib/site-packages")];
        for (name, is_dir) in tree.entries(&join(dir, "lib")).unwrap_or_default() {
            if is_dir && name.starts_with("python") {
                sites.push(join(dir, &format!("lib/{name}/site-packages")));
            }
        }
        let mut provides: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut any = false;
        for site in sites {
            for (name, is_dir) in tree.entries(&site).unwrap_or_default() {
                let Some(distribution) = is_dir.then(|| dist_info_name(&name)).flatten() else {
                    continue;
                };
                let Ok(record) = tree.read(&join(&join(&site, &name), "RECORD")) else {
                    continue;
                };
                any = true;
                for module in record_names(&record) {
                    let providers = provides.entry(module).or_default();
                    if !providers.contains(&distribution) {
                        providers.push(distribution.clone());
                    }
                }
            }
        }
        any.then(|| Venv {
            dir: dir.to_string(),
            provides,
        })
    }

    /// The distributions that installed a module of the name `import`.
    pub(super) fn providers(&self, import: &str) -> &[String] {
        self.provides.get(import).map_or(&[][..], Vec::as_slice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::fake::Fake;

    #[test]
    fn the_modules_a_distribution_installed_are_read_from_its_record_in_either_layout() {
        let tree = Fake::new(&[
            (
                ".venv/lib/python3.12/site-packages/pillow-10.0.0.dist-info/RECORD",
                "PIL/__init__.py,sha256=a,1\nPIL/Image.py,sha256=b,2\n",
            ),
            (
                ".venv/Lib/site-packages/six-1.16.0.dist-info/RECORD",
                "six.py,sha256=a,1\n",
            ),
            (".venv/Lib/site-packages/six-1.16.0.dist-info/METADATA", ""),
        ]);
        let venv = Venv::read(&tree, ".venv").unwrap();
        assert_eq!(venv.providers("PIL"), ["pillow"]);
        assert_eq!(venv.providers("six"), ["six"]);
        assert!(venv.providers("requests").is_empty());
    }

    #[test]
    fn a_directory_that_holds_no_installed_distribution_is_no_environment() {
        let tree = Fake::new(&[(".venv/pyvenv.cfg", ""), ("x.py", "")]);
        assert!(Venv::read(&tree, ".venv").is_none());
        assert!(Venv::read(&tree, "nowhere").is_none());
    }
}

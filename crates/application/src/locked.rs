//! The locked-project check of a Python project, read from the tree: each unit's manifest is there, a lock file is
//! committed at its root or above it, and every virtual environment on disk is the `.venv` beside a lock file that no
//! `.gitignore` leaves to be committed (`domain::locked` has the rules).
//!
//! The virtual environments are looked for in the direct children of each directory the walk of `Tree::files` reaches,
//! the ignored and hidden children too, which is where `.venv` and `venv` are: a child is one when it holds a
//! `pyvenv.cfg`, and the search never goes into a child the walk leaves out, so a tree of `node_modules` or a `.tox`
//! is not read, and an environment is found at any depth the walk reaches. This is the one place a check reads what
//! the project's `.gitignore` hides, and what it finds is on the machine it runs on and not in the commit: CI, which
//! has no environment, never fails on one, and a machine that has one fails where it can do harm.

use std::collections::BTreeSet;
use std::io;

use crate::tree::is_file;
use domain::declaration::Declared;
use domain::layers::Language;
use domain::locked::{CONFIG, Environment, VENV, is_lock, no_lock, no_manifest};
use domain::paths::{file_name, join, parent};
use protocol::outbound::Tree;

/// Every finding of the check, in the byte order of the path each is about. Nothing for a project that is not
/// Python.
pub fn problems(tree: &dyn Tree, declared: &Declared) -> io::Result<Vec<String>> {
    let Some(layout) = &declared.layout else {
        return Ok(Vec::new());
    };
    if layout.language != Language::Python {
        return Ok(Vec::new());
    }
    let files = tree.files("")?;
    let locks: BTreeSet<&str> = files
        .iter()
        .filter(|path| is_lock(file_name(path)))
        .map(|path| parent(path))
        .collect();
    // Each finding with the path it is about, which sorts them
    let mut found: Vec<(String, String)> = Vec::new();
    // The manifest of each distinct root of a unit (the project's root when it declares none)
    let roots: BTreeSet<&str> = declared
        .units
        .iter()
        .map(|unit| {
            if unit.declared {
                unit.root.as_str()
            } else {
                ""
            }
        })
        .collect();
    for root in roots {
        let manifest = join(root, &layout.manifest);
        if !is_file(tree, &manifest) {
            found.push((manifest.clone(), no_manifest(root, &layout.manifest)));
        }
        if !locked_at_or_above(&locks, root) {
            found.push((manifest, no_lock(root, &layout.manifest)));
        }
    }
    found.extend(environments(tree, declared, &files, &locks));
    // Stable: the missing manifest of a root comes before its missing lock file
    found.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(found.into_iter().map(|(_, said)| said).collect())
}

/// Whether a lock file is in `dir` or in a directory above it.
fn locked_at_or_above(locks: &BTreeSet<&str>, dir: &str) -> bool {
    let mut at = dir;
    loop {
        if locks.contains(at) {
            return true;
        }
        if at.is_empty() {
            return false;
        }
        at = parent(at);
    }
}

/// The directories the walk reaches: the root, and each directory that holds a file it gives, or holds one that does.
fn walked(files: &[String]) -> BTreeSet<&str> {
    let mut dirs = BTreeSet::from([""]);
    for file in files {
        let mut at = parent(file);
        while !at.is_empty() && dirs.insert(at) {
            at = parent(at);
        }
    }
    dirs
}

/// Every finding of a virtual environment, with the path of the environment: one in each child of a directory the walk
/// reaches that holds `pyvenv.cfg` and is not in a path no check reads (`unchecked`, `moving`).
fn environments(
    tree: &dyn Tree,
    declared: &Declared,
    files: &[String],
    locks: &BTreeSet<&str>,
) -> Vec<(String, String)> {
    let mut found = Vec::new();
    for dir in walked(files) {
        // A directory that cannot be listed holds no environment that can be told of
        let Ok(entries) = tree.entries(dir) else {
            continue;
        };
        let mut children: Vec<&String> = entries
            .iter()
            .filter(|(_, is_dir)| *is_dir)
            .map(|(name, _)| name)
            .collect();
        children.sort();
        for name in children {
            let path = join(dir, name);
            if tree.found(&join(&path, CONFIG)) != Some(false) || declared.is_not_checked(&path) {
                continue;
            }
            let beside_lock = locks.contains(dir);
            let environment = Environment {
                // Asked only of the one environment that may stay: nothing else is told of its ignore
                ignored: name == VENV && beside_lock && tree.ignored(&join(&path, CONFIG)),
                beside_lock,
                path: path.clone(),
            };
            found.extend(domain::locked::environment(&environment).map(|said| (path, said)));
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::fake::Fake;
    use domain::declaration::parse_declaration;

    /// What the check finds in a Python project declared as `declaration` (the fields after `ui`), over `files`, of
    /// which the paths in `ignored` are excluded by a `.gitignore`.
    fn check(declaration: &str, files: &[(&str, &str)], ignored: &[&str]) -> Vec<String> {
        let text = format!(
            "stack = \"python\"\nareas = []\nui = \"none\"\nfiles = \"2.0.0\"\n{declaration}"
        );
        let declared =
            Declared::new(parse_declaration(&text).unwrap()).unwrap_or_else(|e| panic!("{e:#?}"));
        let tree = Fake::new(files);
        for path in ignored {
            tree.ignore(path);
        }
        problems(&tree, &declared).unwrap()
    }

    const PROJECT: [(&str, &str); 2] = [("pyproject.toml", ""), ("uv.lock", "")];

    #[test]
    fn a_project_with_a_manifest_a_lock_file_and_an_ignored_dot_venv_passes() {
        let mut files = PROJECT.to_vec();
        files.push((".venv/pyvenv.cfg", "home = /usr/bin\n"));
        files.push(("domain/a.py", ""));
        assert!(check("", &files, &[".venv"]).is_empty());
        // No environment on this machine: CI, which starts from nothing
        assert!(check("", &PROJECT, &[]).is_empty());
    }

    #[test]
    fn a_project_without_a_manifest_or_a_lock_file_fails_and_a_requirements_file_does_not_count() {
        let found = check("", &[("requirements.txt", "requests\n")], &[]);
        assert_eq!(found.len(), 2, "{found:#?}");
        assert!(found[0].starts_with("pyproject.toml is missing: a Python project lists"));
        assert!(found[1].starts_with("pyproject.toml has no lock file in the root or above it"));
        let found = check("", &[("pyproject.toml", "")], &[]);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert!(found[0].contains("has no lock file"), "{found:#?}");
        // A lock file with no manifest is told of the manifest alone
        let found = check("", &[("poetry.lock", "")], &[]);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert!(found[0].starts_with("pyproject.toml is missing"));
    }

    #[test]
    fn each_tools_lock_file_is_accepted_and_one_a_gitignore_excludes_is_not_committed() {
        for lock in [
            "uv.lock",
            "poetry.lock",
            "pdm.lock",
            "pylock.toml",
            "pylock.dev.toml",
        ] {
            let found = check("", &[("pyproject.toml", ""), (lock, "")], &[]);
            assert!(found.is_empty(), "{lock}: {found:#?}");
        }
        let found = check("", &PROJECT, &["uv.lock"]);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert!(found[0].contains("has no lock file"), "{found:#?}");
    }

    #[test]
    fn a_unit_needs_its_own_manifest_and_a_lock_file_at_its_root_or_above_it() {
        let declaration = "[[unit]]\nname = \"api\"\npath = \"packages/api/src/api\"\nroot = \"packages/api\"\n\
                           absent = [\"composition\", \"infrastructure\", \"application\", \"protocol\"]\n";
        // The lock file of a workspace is at the root, the manifest of each member is its own
        let found = check(
            declaration,
            &[
                ("uv.lock", ""),
                ("packages/api/pyproject.toml", ""),
                ("packages/api/src/api/domain/a.py", ""),
            ],
            &[],
        );
        assert!(found.is_empty(), "{found:#?}");
        // A unit with no manifest of its own is not covered by the root's
        let found = check(declaration, &[("pyproject.toml", ""), ("uv.lock", "")], &[]);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert!(
            found[0].starts_with("packages/api/pyproject.toml is missing"),
            "{found:#?}"
        );
        // And a lock file in another directory is no lock file of this one
        let found = check(
            declaration,
            &[
                ("packages/api/pyproject.toml", ""),
                ("packages/web/uv.lock", ""),
            ],
            &[],
        );
        assert_eq!(found.len(), 1, "{found:#?}");
        assert!(
            found[0].starts_with(
                "packages/api/pyproject.toml has no lock file in packages/api or above it"
            ),
            "{found:#?}"
        );
    }

    #[test]
    fn a_virtual_environment_that_is_not_dot_venv_fails_even_when_the_gitignore_hides_it() {
        let mut files = PROJECT.to_vec();
        files.push(("venv/pyvenv.cfg", ""));
        files.push(("venv/Lib/site-packages/x.py", ""));
        for ignored in [&[][..], &["venv"][..]] {
            let found = check("", &files, ignored);
            assert_eq!(found.len(), 1, "{found:#?}");
            assert!(
                found[0].starts_with("venv/ is a virtual environment (it holds pyvenv.cfg)"),
                "{found:#?}"
            );
        }
        // Told by `pyvenv.cfg`, not by the name: a directory called env that is no environment is nothing
        let mut plain = PROJECT.to_vec();
        plain.push(("env/settings.py", ""));
        plain.push((".env", "KEY=1\n"));
        assert!(check("", &plain, &[]).is_empty());
        // And a renamed one is found by what it holds
        let mut renamed = PROJECT.to_vec();
        renamed.push(("py312/pyvenv.cfg", ""));
        assert_eq!(check("", &renamed, &["py312"]).len(), 1);
    }

    #[test]
    fn a_dot_venv_is_allowed_only_beside_a_lock_file_and_ignored() {
        let declaration = "[[unit]]\nname = \"api\"\npath = \"packages/api/src/api\"\nroot = \"packages/api\"\n\
                           absent = [\"composition\", \"infrastructure\", \"application\", \"protocol\"]\n";
        let layers = [
            ("packages/api/pyproject.toml", ""),
            ("packages/api/src/api/domain/a.py", ""),
            ("packages/api/.venv/pyvenv.cfg", ""),
        ];
        // The lock file is at the root, so the one environment is the root's
        let mut workspace = layers.to_vec();
        workspace.push(("uv.lock", ""));
        let found = check(declaration, &workspace, &["packages/api/.venv"]);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert!(
            found[0].starts_with(
                "packages/api/.venv/ is a virtual environment in a directory with no lock file"
            ),
            "{found:#?}"
        );
        // With a lock file of its own beside it, it is the right one
        let mut own = layers.to_vec();
        own.push(("packages/api/uv.lock", ""));
        assert!(check(declaration, &own, &["packages/api/.venv"]).is_empty());
        // Not ignored: it can be committed
        let found = check(declaration, &own, &[]);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert!(
            found[0].starts_with(
                "packages/api/.venv/ is a virtual environment that no .gitignore excludes"
            ),
            "{found:#?}"
        );
    }

    #[test]
    fn the_search_goes_into_no_directory_the_walk_leaves_out() {
        // `tools/.tox/py312` is below an ignored directory: only the children of a walked directory are looked at
        let mut files = PROJECT.to_vec();
        files.push(("tools/run.py", ""));
        files.push(("tools/.tox/py312/pyvenv.cfg", ""));
        files.push(("node_modules/pkg/venv/pyvenv.cfg", ""));
        assert!(check("", &files, &["tools/.tox", "node_modules"]).is_empty());
        // The ignored child itself is looked at, and so is a hidden one
        files.push(("tools/venv/pyvenv.cfg", ""));
        let found = check("", &files, &["tools/.tox", "node_modules", "tools/venv"]);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert!(
            found[0].starts_with("tools/venv/ is a virtual environment"),
            "{found:#?}"
        );
    }

    #[test]
    fn a_path_in_unchecked_is_left_alone_and_a_project_that_is_not_python_is_not_judged() {
        let mut files = PROJECT.to_vec();
        files.push(("fixtures/venv/pyvenv.cfg", ""));
        let found = check("unchecked = [\"fixtures\"]\n", &files, &[]);
        assert!(found.is_empty(), "{found:#?}");
        let declared = Declared::new(
            parse_declaration("stack = \"rust\"\nareas = []\nui = \"none\"\nfiles = \"2.0.0\"\n")
                .unwrap(),
        )
        .unwrap();
        let tree = Fake::new(&[("venv/pyvenv.cfg", "")]);
        assert!(problems(&tree, &declared).unwrap().is_empty());
    }
}

//! Reading a crate's `Cargo.toml` with toml_edit: the dependencies it declares that can name a path, and what it says
//! about its workspace.
//!
//! - **The dependencies read:** `[dependencies]` and `[build-dependencies]`, and both under `[target.<cfg>]`, in every
//!   form TOML writes a table in (`name = { path = ... }`, `[dependencies.name]`, dotted keys). The forms with `_`
//!   (`build_dependencies`), which Cargo accepts before edition 2024, are read too. `[dev-dependencies]` serve the
//!   tests, which may import every layer, so they are not read.
//! - **Where a dependency comes from:** its `path`, or its workspace's entry of the same name when it says
//!   `workspace = true`. One from a registry or git names no place in the project. A dependency renamed with `package`
//!   is found by its path all the same.
//! - **Its workspace:** `[package] workspace` when set, whether the manifest declares `[workspace]`, and the entries
//!   of `[workspace.dependencies]`.
//!
//! Not read: `[patch]` and `[replace]`, which can point a dependency from a registry to a path, and source files
//! taken from another crate's directory (`#[path]`, `include!`, `[lib] path`).

use toml_edit::{Document, Item, TableLike};

use crate::code::{Dependency, Manifest, Origin};
use crate::source::line_of;

/// The tables of dependencies that are read, at the top or under `[target.<cfg>]`.
const READ: [&str; 3] = ["dependencies", "build-dependencies", "build_dependencies"];

/// Read one manifest, or `Err` with the line of the first error and what the parser says.
pub fn read(source: &str) -> Result<Manifest, (usize, String)> {
    let document = Document::parse(source).map_err(|e| {
        (
            line_of(source, e.span().map_or(0, |span| span.start)),
            e.message().trim().to_string(),
        )
    })?;
    let top = document.as_table();
    fn table(item: Option<&Item>) -> Option<&dyn TableLike> {
        item.and_then(Item::as_table_like)
    }
    let mut tables: Vec<&dyn TableLike> = READ.iter().filter_map(|k| table(top.get(k))).collect();
    if let Some(targets) = table(top.get("target")) {
        for (_, target) in targets.iter() {
            if let Some(target) = target.as_table_like() {
                tables.extend(READ.iter().filter_map(|k| table(target.get(k))));
            }
        }
    }
    let mut dependencies = Vec::new();
    for deps in tables {
        for (name, item) in deps.iter() {
            let Some(origin) = origin(item) else {
                continue;
            };
            let at = deps
                .get_key_value(name)
                .and_then(|(key, _)| key.span())
                .or_else(|| item.span())
                .map_or(0, |span| span.start);
            dependencies.push(Dependency {
                line: line_of(source, at),
                name: name.to_string(),
                origin,
            });
        }
    }
    let workspace = table(top.get("workspace"));
    let workspace_dependencies = workspace
        .and_then(|w| table(w.get("dependencies")))
        .map(|deps| {
            deps.iter()
                .map(|(name, item)| {
                    let path = match origin(item) {
                        Some(Origin::Path(path)) => Some(path),
                        _ => None,
                    };
                    (name.to_string(), path)
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Manifest {
        dependencies,
        workspace: table(top.get("package"))
            .and_then(|p| p.get("workspace"))
            .and_then(Item::as_str)
            .map(String::from),
        is_workspace: workspace.is_some(),
        workspace_dependencies,
    })
}

/// Where one dependency comes from, or `None` for a registry or git.
fn origin(item: &Item) -> Option<Origin> {
    let entry = item.as_table_like()?;
    if let Some(path) = entry.get("path").and_then(Item::as_str) {
        return Some(Origin::Path(path.to_string()));
    }
    (entry.get("workspace").and_then(Item::as_bool) == Some(true)).then_some(Origin::Workspace)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn read_names(source: &str) -> Vec<(usize, String, Origin)> {
        read(source)
            .unwrap()
            .dependencies
            .into_iter()
            .map(|d| (d.line, d.name, d.origin))
            .collect()
    }

    #[test]
    fn every_form_of_a_dependency_is_read_with_its_line() {
        let source = "\
[package]
name = \"application\"

[dependencies]
serde = \"1\"
domain = { path = \"../domain\" }
git = { git = \"https://example.com/x\" }
utils.workspace = true
core = { path = \"../domain\", package = \"domain\" }

[dependencies.infrastructure]
path = \"../infrastructure\"

[build-dependencies]
handler = { path = \"../handler\" }

[build_dependencies]
old = { path = \"../old\" }

[dev-dependencies]
tests_only = { path = \"../handler\" }

[target.'cfg(unix)'.dependencies]
unix = { path = \"../unix\" }

[target.'cfg(unix)'.dev-dependencies]
unix_tests = { path = \"../unix\" }
";
        let path = |p: &str| Origin::Path(p.into());
        assert_eq!(
            read_names(source),
            vec![
                (6, "domain".into(), path("../domain")),
                (8, "utils".into(), Origin::Workspace),
                (9, "core".into(), path("../domain")),
                (11, "infrastructure".into(), path("../infrastructure")),
                (15, "handler".into(), path("../handler")),
                (18, "old".into(), path("../old")),
                (24, "unix".into(), path("../unix")),
            ]
        );
    }

    #[test]
    fn dotted_keys_at_the_top_are_read() {
        assert_eq!(
            read_names("dependencies.domain.path = \"../domain\"\n"),
            vec![(1, "domain".into(), Origin::Path("../domain".into()))]
        );
    }

    #[test]
    fn the_workspace_is_read() {
        let manifest = read(
            "[workspace]\nmembers = [\"crates/*\"]\n\n[workspace.dependencies]\ndomain = { path = \"crates/domain\" }\n\
             serde = \"1\"\n",
        )
        .unwrap();
        assert!(manifest.is_workspace);
        assert_eq!(manifest.workspace, None);
        assert_eq!(
            manifest.workspace_dependencies,
            BTreeMap::from([
                ("domain".into(), Some("crates/domain".into())),
                ("serde".into(), None),
            ])
        );
        let member = read("[package]\nname = \"a\"\nworkspace = \"../..\"\n").unwrap();
        assert!(!member.is_workspace);
        assert_eq!(member.workspace.as_deref(), Some("../.."));
    }

    #[test]
    fn a_manifest_that_is_not_toml_says_where() {
        let (line, why) = read("[package]\nname = \"a\"\n[dependencies\n").unwrap_err();
        assert_eq!(line, 3);
        assert!(!why.is_empty() && !why.contains('\n'), "{why}");
    }
}

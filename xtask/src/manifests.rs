//! Every dependency of the workspace's crates is named once, in `[workspace.dependencies]` of the root `Cargo.toml`,
//! and each crate takes it with `workspace = true`. Split into a crate per layer, the same dependency was written in up
//! to four manifests, each with its version, and nothing but the eye kept them equal.

use toml_edit::{DocumentMut, Item, TableLike};

/// The tables that declare dependencies, as Cargo reads them: the spellings with `_` are older, and still read
const TABLES: &[&str] = &[
    "dependencies",
    "dev-dependencies",
    "dev_dependencies",
    "build-dependencies",
    "build_dependencies",
];

/// The members of the workspace, as the root `Cargo.toml` lists them
pub fn members(root: &str) -> Result<Vec<String>, String> {
    let doc: DocumentMut = root.parse().map_err(|e| format!("Cargo.toml: {e}"))?;
    let members = doc
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(Item::as_array)
        .ok_or("Cargo.toml lists no members under [workspace]")?;
    members
        .iter()
        .map(|m| {
            m.as_str()
                .map(str::to_string)
                .ok_or_else(|| format!("Cargo.toml: a member is not a string: {m}"))
        })
        .collect()
}

/// Where the manifest at `path` names a dependency's version or path itself, instead of taking it from the workspace
pub fn problems(path: &str, manifest: &str) -> Result<Vec<String>, String> {
    let doc: DocumentMut = manifest.parse().map_err(|e| format!("{path}: {e}"))?;
    let mut found = Vec::new();
    let mut check = |at: &str, table: &dyn TableLike| {
        for (name, entry) in table.iter() {
            let taken = entry
                .as_table_like()
                .and_then(|e| e.get("workspace"))
                .and_then(Item::as_bool);
            if taken != Some(true) {
                found.push(format!(
                    "{path}: [{at}] {name} names its own source: write it in [workspace.dependencies] of Cargo.toml, \
                     and take it with {name}.workspace = true"
                ));
            }
        }
    };
    for table in TABLES {
        if let Some(entries) = doc.get(table).and_then(Item::as_table_like) {
            check(table, entries);
        }
    }
    if let Some(targets) = doc.get("target").and_then(Item::as_table_like) {
        for (cfg, target) in targets.iter() {
            for table in TABLES {
                if let Some(entries) = target.get(table).and_then(Item::as_table_like) {
                    check(&format!("target.{cfg}.{table}"), entries);
                }
            }
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_members_are_read_from_the_workspace() {
        let root = "[workspace]\nmembers = [\"xtask\", \"crates/domain\"]\n";
        assert_eq!(members(root).unwrap(), ["xtask", "crates/domain"]);
        assert!(members("[package]\nname = \"a\"\n").is_err());
    }

    #[test]
    fn a_dependency_taken_from_the_workspace_passes_in_every_spelling() {
        let manifest = "[package]\nname = \"a\"\n\n[dependencies]\ndomain.workspace = true\n\
                        clap = { workspace = true, features = [\"env\"] }\n\n[dependencies.serde]\nworkspace = true\n\n\
                        [dev-dependencies]\ntempfile.workspace = true\n\n\
                        [target.'cfg(windows)'.build-dependencies]\ncc.workspace = true\n";
        assert_eq!(
            problems("a/Cargo.toml", manifest).unwrap(),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_dependency_that_names_its_own_source_fails_by_its_table_and_name() {
        let manifest = "[package]\nname = \"a\"\n\n[dependencies]\nregex = \"1\"\n\
                        domain = { path = \"../domain\" }\nutils = { workspace = false, path = \"../utils\" }\n\n\
                        [dev_dependencies]\ntempfile = \"3\"\n\n[build-dependencies]\ncc = { version = \"1\" }\n\n\
                        [target.'cfg(unix)'.dependencies]\nlibc = \"0.2\"\n";
        let found = problems("a/Cargo.toml", manifest).unwrap();
        let named: Vec<&str> = found
            .iter()
            .map(|why| why.split(" names").next().unwrap())
            .collect();
        assert_eq!(
            named,
            [
                "a/Cargo.toml: [dependencies] regex",
                "a/Cargo.toml: [dependencies] domain",
                "a/Cargo.toml: [dependencies] utils",
                "a/Cargo.toml: [dev_dependencies] tempfile",
                "a/Cargo.toml: [build-dependencies] cc",
                "a/Cargo.toml: [target.cfg(unix).dependencies] libc",
            ]
        );
    }

    #[test]
    fn a_manifest_that_cannot_be_read_is_an_error() {
        assert!(problems("a/Cargo.toml", "[dependencies\n").is_err());
    }
}

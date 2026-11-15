//! The dependencies check of a Rust project: the `[dependencies]` of each crate's `Cargo.toml` that its code does not
//! name.
//!
//! Cargo stops a crate that names a crate its manifest does not list, so only the other half is judged here. The
//! crates are those the direction check reads (the crate of each layer, and the package of each unit's entry files),
//! and the code of one is its `.rs` files in `src/`, `tests/`, `benches/` and `examples/`, where Cargo lets a crate use
//! its `[dependencies]` (not `build.rs`, which uses `[build-dependencies]`, which are never judged). A dependency is
//! named by a path that starts with it (`serde::Serialize`, in a `use`, an expression, an attribute or a macro), by a
//! name bound to it with `as`, by `extern crate`, or by a `use` of the crate alone, with `-` and `_` one name; what the
//! manifest calls a dependency with `package = ...` is the name it is used by. A crate used and never named (a `-sys`
//! crate that is only linked) is listed in `without_import`.
//!
//! Not read: a `[lib] path` or `[[bin]] path` that puts code outside those directories, a file taken in with
//! `include!` or `#[path]`, and `[workspace.dependencies]`, which is no crate's dependency until one takes it.

use std::collections::BTreeSet;
use std::io;

use super::{Read, listing, without_import};
use crate::direction::rust::{Manifests, crates};
use crate::tree::{code_files, read_code};
use domain::declaration::Declared;
use domain::dependencies::rust::names;
use domain::dependencies::{Kind, Use, judge};
use domain::layers::{Language, Layout, Place};
use domain::paths::{join, parent};
use domain::rust::{code, is_source};
use domain::text::line_of;
use protocol::outbound::{Parsers, Tree};

/// The directories of a crate whose code may use its `[dependencies]`.
const CODE: [&str; 4] = ["src", "tests", "benches", "examples"];

/// The dependencies check of a Rust project, into `found`.
pub(super) fn rust(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    declared: &Declared,
    layout: &Layout,
    places: &[&Place],
    entries: &[Place],
    found: &mut Vec<String>,
) -> io::Result<Read> {
    // What cannot be read is the direction check's to say: it reads the same files
    let mut ignored = Vec::new();
    let mut manifests = Manifests::new(parsers);
    let crates = crates(tree, layout, places, entries, &mut manifests, &mut ignored)?;
    let without = without_import(declared, Language::Rust);
    let mut read = Read::default();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for (path, _) in crates {
        if !seen.insert(path.clone()) {
            continue;
        }
        let Some(listing) = listing(tree, parsers, Language::Rust, &path, &mut ignored)? else {
            continue;
        };
        read.any = true;
        let runtime: Vec<&str> = listing
            .deps
            .iter()
            .filter(|dep| dep.kind == Kind::Runtime)
            .map(|dep| dep.name.as_str())
            .collect();
        read.runtime
            .extend(runtime.iter().map(|name| name.to_string()));
        let mut uses = Vec::new();
        for directory in CODE {
            let files = code_files(tree, &join(parent(&path), directory), is_source)?;
            for file in files {
                let Some(source) = read_code(tree, &file, "paths", &mut ignored)? else {
                    continue;
                };
                for (at, name) in names(&code(&source), &runtime) {
                    uses.push(Use {
                        line: line_of(&source, at),
                        file: file.clone(),
                        shown: name.clone(),
                        package: name,
                        test: false,
                        unit: None,
                    });
                }
            }
        }
        for problem in judge(Language::Rust, &listing, &uses, &without) {
            found.push(problem.said(Language::Rust, &path, None));
        }
    }
    Ok(read)
}

#[cfg(test)]
mod tests {
    use super::super::problems;
    use crate::tree::fake::Fake;
    use domain::declaration::{Declared, parse_declaration};
    use infrastructure::outbound::readers::Readers;

    /// What the dependencies check finds in a Rust project declared as `declaration` (the fields after `ui`), over
    /// `files`.
    fn check(declaration: &str, files: &[(&str, &str)]) -> Vec<String> {
        let text = format!(
            "stack = \"rust\"\nareas = []\nui = \"none\"\nfiles = \"2.0.0\"\n{declaration}"
        );
        let declared =
            Declared::new(parse_declaration(&text).unwrap()).unwrap_or_else(|e| panic!("{e:#?}"));
        problems(&Fake::new(files), &Readers, &declared).unwrap()
    }

    const DOMAIN: &str = "[package]\nname = \"domain\"\n\n[dependencies]\nserde = \"1\"\nserde_json = \"1\"\n\
                          tokio = { version = \"1\", features = [\"rt\"] }\nlog-it = \"0.4\"\n\
                          openssl-sys = \"0.9\"\nunused = \"1\"\n\n[dev-dependencies]\nproptest = \"1\"\n\n\
                          [build-dependencies]\ncc = \"1\"\n";

    #[test]
    fn a_dependency_no_path_of_the_crate_names_is_unused() {
        let found = check(
            "",
            &[
                ("crates/domain/Cargo.toml", DOMAIN),
                (
                    "crates/domain/src/lib.rs",
                    "use serde::Serialize;\n#[derive(Serialize)]\npub struct A;\npub fn f() { serde_json::json!({}); }\n\
                     // tokio::spawn is a comment\nuse log_it as log;\n",
                ),
                (
                    "crates/domain/tests/t.rs",
                    "#[tokio::test]\nasync fn t() {}\n",
                ),
            ],
        );
        assert_eq!(found.len(), 2, "{found:#?}");
        assert_eq!(
            found[0],
            "crates/domain/Cargo.toml:9: lists the dependency openssl-sys in [dependencies], and no `use` or path in \
             the crate names it: take it out, or, when the code uses it without naming it, list it in without_import \
             under [dependencies] in .config/rotproof.toml"
        );
        assert!(
            found[1].starts_with("crates/domain/Cargo.toml:10: lists the dependency unused"),
            "{found:#?}"
        );
    }

    #[test]
    fn a_crate_used_and_never_named_is_listed_in_without_import_and_that_name_may_be_written_either_way()
     {
        let files = [
            ("crates/domain/Cargo.toml", DOMAIN),
            (
                "crates/domain/src/lib.rs",
                "use serde::Serialize;\npub fn f() { serde_json::json!({}); tokio::spawn(); }\nuse log_it as log;\n",
            ),
        ];
        assert_eq!(check("", &files).len(), 2);
        let found = check(
            "[dependencies]\nwithout_import = [\"openssl_sys\", \"unused\"]\n",
            &files,
        );
        assert!(found.is_empty(), "{found:#?}");
    }

    #[test]
    fn a_dependency_of_the_workspace_is_judged_in_the_crate_that_takes_it_and_a_rename_by_its_name()
    {
        let found = check(
            "",
            &[
                (
                    "crates/domain/Cargo.toml",
                    "[package]\nname = \"domain\"\n\n[dependencies]\nserde.workspace = true\n\
                     json = { package = \"serde_json\", version = \"1\" }\n\n[dependencies.rand]\nversion = \"0.8\"\n",
                ),
                (
                    "crates/domain/src/lib.rs",
                    "use serde::Serialize;\nfn f() { json::to_string(&1); }\n",
                ),
            ],
        );
        assert_eq!(found.len(), 1, "{found:#?}");
        assert!(
            found[0].starts_with("crates/domain/Cargo.toml:8: lists the dependency rand"),
            "{found:#?}"
        );
    }

    #[test]
    fn the_entry_package_is_judged_and_a_manifest_with_no_dependencies_says_nothing() {
        let found = check(
            "",
            &[
                (
                    "Cargo.toml",
                    "[package]\nname = \"app\"\n\n[dependencies]\ncomposition = { path = \"crates/composition\" }\nclap = \"4\"\n",
                ),
                ("src/main.rs", "fn main() { composition::run(); }\n"),
                (
                    "crates/composition/Cargo.toml",
                    "[package]\nname = \"composition\"\n",
                ),
                ("crates/composition/src/lib.rs", ""),
            ],
        );
        assert_eq!(found.len(), 1, "{found:#?}");
        assert!(
            found[0].starts_with("Cargo.toml:6: lists the dependency clap"),
            "{found:#?}"
        );
    }
}

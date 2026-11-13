//! The environment check: only `infrastructure` reads or writes the environment variables.
//!
//! - The environment is input from outside, as unchecked as a request: `infrastructure` reads it into a settings type
//!   it validates, and `composition` passes that on. Every other layer and the entry file fails on a read or a write.
//! - **What counts,** by language: in Python, the names of `os` in [`PYTHON`] (`os.environ`, `os.getenv`); in
//!   TypeScript, `process.env` and Vite's `import.meta.env`; in Rust, the functions of `std::env` in [`RUST`]
//!   (`std::env::var`). The command line (`sys.argv`, `std::env::args`) is no environment variable, and `env!` and
//!   `option_env!` are read when Rust compiles, not from outside when the program runs; so is a crate's `build.rs`,
//!   which Cargo runs before it compiles and tells what it needs in the environment (`OUT_DIR`). Of
//!   `import.meta.env`, the values Vite sets itself in [`VITE_OWN`] (`import.meta.env.DEV`) say how the code was
//!   built, not what came from outside, and pass; a value the project passes in (`VITE_API_URL`) counts.
//! - **How it is seen:** every form written in the code, through an alias the file binds too (`import os as o`,
//!   `from os import getenv as g`, `import { env } from "node:process"`, `const { env } = process`,
//!   `use std::env;` then `env::var`). What is built at run time is not seen (`getattr(os, "environ")`,
//!   `from os import *`, `use std as s;`), as the direction check does not see an import built at run time.
//! - **Where:** the code of every layer but `infrastructure`, and the entry file; not `tests/` or another path that is
//!   no layer. A layer's tests in TypeScript sit in its directory, and are its code.
//!
//! Python and TypeScript are read by the readers of their language, which find the forms above in the syntax tree
//! (`Source::environment`); Rust by Rotproof's own scanner, here.

use crate::layers::Place;
use crate::rust::{code, items};
use crate::text::line_of;

/// The names of Python's `os` that read or write the environment.
pub const PYTHON: [&str; 6] = [
    "environ", "environb", "getenv", "getenvb", "putenv", "unsetenv",
];

/// The functions of Rust's `std::env` that read or write the environment.
pub const RUST: [&str; 6] = ["var", "var_os", "vars", "vars_os", "set_var", "remove_var"];

/// The values of `import.meta.env` Vite sets itself, from how the code is built.
pub const VITE_OWN: [&str; 5] = ["BASE_URL", "DEV", "MODE", "PROD", "SSR"];

/// The file Cargo runs before it compiles a crate, from the crate's root.
pub const BUILD_SCRIPT: &str = "build.rs";

/// The layer that alone may read the environment.
pub const READER: &str = "infrastructure";

/// The heading a read or a write outside `infrastructure` is printed under.
pub const HEADING: &str = "only infrastructure reads the environment (read it there into a settings type it \
                           validates, and let composition pass that on: the environment is input from outside, as \
                           unchecked as a request)";

/// Whether the code of `place` is judged: every place but `infrastructure` and its levels. The entry file is a place
/// of its own, and judged.
pub fn is_judged(place: &Place) -> bool {
    place.name != READER && place.parent.as_deref() != Some(READER)
}

/// One finding: the file at `path`, whose text is `source`, reads or writes the environment through `what` on `line`.
pub fn finding(path: &str, source: &str, line: usize, what: &str) -> String {
    let text = source.lines().nth(line - 1).unwrap_or("").trim();
    format!("{path}:{line}: {what}\n{text}")
}

/// Every read or write of the environment in a Rust source: the line, and the path it names (`std::env::var`).
pub fn in_rust(source: &str) -> Vec<(usize, String)> {
    items(&code(source), &["std", "env"])
        .into_iter()
        .filter(|(_, name)| RUST.contains(&name.as_str()))
        .map(|(at, name)| (line_of(source, at), format!("std::env::{name}")))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(source: &str) -> Vec<(usize, String)> {
        in_rust(source)
    }

    #[test]
    fn a_rust_read_is_found_by_its_path_and_through_use() {
        let source = "\
fn a() -> Option<String> {
    std::env::var(\"A\").ok()
}
use std::env;
fn b() {
    let _ = env::var_os(\"B\");
    env::set_var(\"C\", \"1\");
}
use std::env::vars;
";
        assert_eq!(
            names(source),
            [
                (2, "std::env::var".to_string()),
                (6, "std::env::var_os".into()),
                (7, "std::env::set_var".into()),
                (9, "std::env::vars".into()),
            ]
        );
    }

    #[test]
    fn what_is_no_environment_variable_or_no_std_env_passes() {
        let source = "\
const VERSION: &str = env!(\"CARGO_PKG_VERSION\");
fn a() {
    let _ = option_env!(\"X\");
    let _ = std::env::args();
    let _ = std::env::current_dir();
    // std::env::var in a comment
    let _ = \"std::env::var in a string\";
}
mod env { pub fn var() {} }
fn b() { env::var(); }
";
        // No `use std::env`, so `env::var` names the file's own module
        assert!(names(source).is_empty());
    }

    #[test]
    fn a_group_binds_env_too() {
        assert_eq!(
            names("use std::{env, fs};\nfn a() { env::vars(); }\n"),
            [(2, "std::env::vars".to_string())]
        );
        assert_eq!(
            names("use std::env::{self, var};\nfn a() { env::remove_var(\"X\"); }\n"),
            [
                (1, "std::env::var".to_string()),
                (2, "std::env::remove_var".into())
            ]
        );
    }

    #[test]
    fn only_infrastructure_is_left_unjudged() {
        let place = |name: &str, parent: Option<&str>| Place {
            name: name.into(),
            path: String::new(),
            parent: parent.map(String::from),
            role: String::new(),
            imports: Vec::new(),
            below: Vec::new(),
            files: Vec::new(),
        };
        assert!(!is_judged(&place("infrastructure", None)));
        assert!(!is_judged(&place(
            "infrastructure.inbound",
            Some("infrastructure")
        )));
        assert!(is_judged(&place("composition", None)));
        assert!(is_judged(&place("protocol.inbound", Some("protocol"))));
    }

    #[test]
    fn a_finding_names_the_line_and_what_it_reads() {
        assert_eq!(
            finding(
                "domain/a.py",
                "x = 1\ny = os.environ['A']\n",
                2,
                "os.environ"
            ),
            "domain/a.py:2: os.environ\ny = os.environ['A']"
        );
    }
}

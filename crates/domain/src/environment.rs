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
//!   `use std::env;` then `env::var`, `use std::env as e;` then `e::var`, `use std as s;` then `s::env::var`, or
//!   from `::std::env::var`). A glob of `std::env` or of `std` (`use std::env::*;`) brings in names the scanner cannot
//!   tell, so it fails as a read does. What is built at run time is not seen (`getattr(os, "environ")`, `from os import
//!   *`), as the direction check does not see an import built at run time.
//! - **Where:** the code of every layer but `infrastructure`, and the entry file; not `tests/` or another path that is
//!   no layer, and not a test beside the code, which a TypeScript layout tells by its name (`*.test.*`, `__tests__/`):
//!   a test is no code of a layer.
//!
//! Python and TypeScript are read by the readers of their language, which find the forms above in the syntax tree
//! (`Source::environment`); Rust by Rotproof's own scanner, here.

use crate::layers::{Language, Place};
use crate::rust::{Item, code, items};
use crate::text::line_of;

/// The names of Python's `os` that read or write the environment.
pub const PYTHON: [&str; 6] = [
    "environ", "environb", "getenv", "getenvb", "putenv", "unsetenv",
];

/// The functions of Rust's `std::env` that read or write the environment.
pub const RUST: [&str; 6] = ["var", "var_os", "vars", "vars_os", "set_var", "remove_var"];

/// The values of `import.meta.env` Vite sets itself, from how the code is built.
pub const VITE_OWN: [&str; 5] = ["BASE_URL", "DEV", "MODE", "PROD", "SSR"];

/// What a finding says of a glob of `std::env` or of `std`: the names it brings in cannot be read.
const GLOB: &str =
    " (a glob: the names it brings in cannot be read, so name them: `use std::env::var;`)";

/// The file Cargo runs before it compiles a crate, from the crate's root.
pub const BUILD_SCRIPT: &str = "build.rs";

/// The layer that alone may read the environment.
pub const READER: &str = "infrastructure";

/// The heading a read or a write outside `infrastructure` is printed under.
pub const HEADING: &str = "only infrastructure reads the environment (read it there into a settings type it \
                           validates, and let composition pass that on: the environment is input from outside, as \
                           unchecked as a request)";

const GUIDE_TEXT: &str = include_str!("../../../project/rotproof/environment.md.in");

/// What the guide says of this rule in the section on the layers, for a project in `language`: what counts there.
pub fn guide(language: Language) -> String {
    let reads = match language {
        Language::Python => {
            "`os.environ` and `os.getenv` (and the other names of `os` that touch it)"
        }
        Language::TypeScript => {
            "`process.env` and `import.meta.env` (except what Vite sets itself, such as `import.meta.env.DEV`)"
        }
        Language::Rust => {
            "`std::env::var` and the other functions of `std::env` that read or set a variable (and on a glob of \
             `std::env` or of `std`, whose names cannot be read: name what you use)"
        }
    };
    GUIDE_TEXT.replace("{reads}", reads)
}

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

/// Every read or write of the environment in a Rust source: the line, and the path it names (`std::env::var`), or the
/// glob of `std::env`, or of `std`, that brings in names the scanner cannot tell, which fails as a read does.
pub fn in_rust(source: &str) -> Vec<(usize, String)> {
    items(&code(source), &["std", "env"])
        .into_iter()
        .filter_map(|(at, item)| {
            let what = match item {
                Item::Named(name) if RUST.contains(&name.as_str()) => format!("std::env::{name}"),
                Item::Named(_) => return None,
                Item::Glob(path) => format!("{}::*{GLOB}", path.join("::")),
            };
            Some((line_of(source, at), what))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layers::UnitTag;

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
    fn a_path_from_a_leading_colons_is_the_same_path() {
        let source =
            "fn a() { ::std::env::var(\"A\"); }\nuse ::std::env;\nfn b() { env::vars(); }\n";
        assert_eq!(
            names(source),
            [
                (1, "std::env::var".to_string()),
                (3, "std::env::vars".into())
            ]
        );
    }

    #[test]
    fn a_rename_of_std_env_or_of_a_module_above_it_is_followed() {
        let source = "\
use std::env as e;
fn a() { let _ = e::var(\"A\"); }
use std as s;
fn b() { let _ = s::env::var_os(\"B\"); s::env::set_var(\"C\", \"1\"); }
use std::{self as t, env as u};
fn c() { t::env::vars(); u::remove_var(\"D\"); }
use std::env::{self as w};
fn d() { w::vars_os(); }
use s::env as x;
fn e() { x::var(\"E\"); }
";
        assert_eq!(
            names(source),
            [
                (2, "std::env::var".to_string()),
                (4, "std::env::var_os".into()),
                (4, "std::env::set_var".into()),
                (6, "std::env::vars".into()),
                (6, "std::env::remove_var".into()),
                (8, "std::env::vars_os".into()),
                (10, "std::env::var".into()),
            ]
        );
    }

    #[test]
    fn a_path_that_goes_on_after_the_function_is_still_a_read() {
        let source = "\
fn a() { let _ = std::env::var(\"A\").unwrap_or_default().len(); }
fn b() { let _ = std::env::vars().count(); }
";
        assert_eq!(
            names(source),
            [
                (1, "std::env::var".to_string()),
                (2, "std::env::vars".into())
            ]
        );
    }

    #[test]
    fn a_glob_of_std_env_or_of_std_fails_as_one_whose_names_cannot_be_read() {
        let found =
            names("use std::env::*;\nfn a() { var(\"A\"); }\nuse std::*;\nuse std::{env::*};\n");
        assert_eq!(
            found.iter().map(|(line, _)| *line).collect::<Vec<_>>(),
            [1, 3, 4]
        );
        assert!(found[0].1.starts_with("std::env::*"));
        assert!(found[1].1.starts_with("std::*"));
        assert!(found[0].1.contains("cannot be read") && found[0].1.contains("name them"));
        // Through a rename, and the globs that bring in no `env`
        assert_eq!(names("use std as s;\nuse s::env::*;\n").len(), 1);
        assert_eq!(names("use std::env as e;\nuse e::*;\n").len(), 1);
        assert!(names("use std::io::*;\nuse std::collections::*;\nuse super::*;\nuse std::prelude::v1::*;\n").is_empty());
    }

    #[test]
    fn the_rest_of_std_env_and_what_is_renamed_to_it_passes() {
        let source = "\
use std::env as e;
const V: &str = env!(\"CARGO_PKG_VERSION\");
fn a() {
    let _ = option_env!(\"X\");
    let _ = e::args();
    let _ = e::current_dir();
    let _: Option<e::VarError> = None;
    let _ = std::env::args().count();
}
";
        assert!(names(source).is_empty());
    }

    #[test]
    fn only_infrastructure_is_left_unjudged() {
        let place = |name: &str, parent: Option<&str>| Place {
            name: name.into(),
            path: String::new(),
            parent: parent.map(String::from),
            role: String::new(),
            imports: Vec::new(),
            lacks: Vec::new(),
            below: Vec::new(),
            files: Vec::new(),
            unit: UnitTag::default(),
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

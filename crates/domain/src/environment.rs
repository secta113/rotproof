//! The environment check: only `infrastructure` reads or writes the environment variables.
//!
//! - The environment is input from outside, as unchecked as a request: `infrastructure` reads it into a settings type
//!   it validates, and `composition` passes that on. Every other layer and the entry file fails on a read or a write.
//! - **What counts,** by language: in Python, the names of `os` in [`PYTHON`] (`os.environ`, `os.getenv`), the two
//!   functions of the path modules that expand a variable or the home directory ([`PYTHON_PATH_FUNCTIONS`] of
//!   [`PYTHON_PATH_MODULES`]: `os.path.expandvars`, `os.path.expanduser`) and the constructors of `pathlib` that give
//!   the home directory ([`PYTHON_HOME`]: `pathlib.Path.home`); in TypeScript, `process.env` and Vite's
//!   `import.meta.env`; in Rust, the functions of `std::env` in [`RUST`] (`std::env::var`, and `home_dir` and
//!   `temp_dir`, which read `HOME` and `TMPDIR`). The command line (`sys.argv`, `std::env::args`) is no environment
//!   variable, and `env!` and `option_env!` are read when Rust compiles, not from outside when the program runs; so is
//!   a crate's `build.rs`, which Cargo runs before it compiles and tells what it needs in the environment
//!   (`OUT_DIR`). Of `import.meta.env`, the values Vite sets itself in [`VITE_OWN`] (`import.meta.env.DEV`) say how
//!   the code was built, not what came from outside, and pass; a value the project passes in (`VITE_API_URL`) counts.
//! - **How it is seen:** every form written in the code, through an alias the file binds too (`import os as o`,
//!   `from os import getenv as g`, `import { env } from "node:process"`, `const { env } = process`,
//!   `use std::env;` then `env::var`, `use std::env as e;` then `e::var`, `use std as s;` then `s::env::var`, or
//!   from `::std::env::var`). A glob of `std::env` or of `std` (`use std::env::*;`) brings in names the scanner cannot
//!   tell, so it fails as a read does; so does a star import in Python of a module whose names count (`from os import
//!   *`). In Python the name is the one the lexical scopes of the file give it ([`crate::code::Use`]): a parameter or
//!   a variable called `os` is not the module. What is built at run time is not seen (`getattr(os, "environ")`), as
//!   the direction check does not see an import built at run time.
//! - **Where:** the code of every layer but `infrastructure`, and the entry file; not `tests/` or another path that is
//!   no layer, and not a test beside the code, which a TypeScript layout tells by its name (`*.test.*`, `__tests__/`):
//!   a test is no code of a layer.
//!
//! TypeScript is read by its reader, which finds the forms above in the syntax tree (`Source::environment`); Python
//! by its reader of uses ([`in_python`] judges them); Rust by Rotproof's own scanner, here.

use crate::code::{How, Use};
use crate::layers::{Language, Place};
use crate::rust::{Item, code, items};
use crate::text::line_of;

/// The names of Python's `os` that read or write the environment.
pub const PYTHON: [&str; 6] = [
    "environ", "environb", "getenv", "getenvb", "putenv", "unsetenv",
];

/// The modules of Python's path functions: `os.path` is `posixpath` or `ntpath`, which share `genericpath`.
pub const PYTHON_PATH_MODULES: [&str; 4] = ["os.path", "posixpath", "ntpath", "genericpath"];

/// The functions of those modules that read the environment: a variable in a path, and the home directory.
pub const PYTHON_PATH_FUNCTIONS: [&str; 2] = ["expandvars", "expanduser"];

/// The classes of `pathlib` whose `home` reads the home directory from the environment.
pub const PYTHON_HOME: [&str; 3] = ["pathlib.Path", "pathlib.PosixPath", "pathlib.WindowsPath"];

/// The functions of Rust's `std::env` that read or write the environment.
pub const RUST: [&str; 8] = [
    "var",
    "var_os",
    "vars",
    "vars_os",
    "set_var",
    "remove_var",
    "home_dir",
    "temp_dir",
];

/// The values of `import.meta.env` Vite sets itself, from how the code is built.
pub const VITE_OWN: [&str; 5] = ["BASE_URL", "DEV", "MODE", "PROD", "SSR"];

/// What a finding says of a glob of `std::env` or of `std`: the names it brings in cannot be read.
const GLOB: &str =
    " (a glob: the names it brings in cannot be read, so name them: `use std::env::var;`)";

/// What a finding says of a star import of a Python module whose names count.
const STAR: &str =
    " (a star import: the names it brings in cannot be read, so name them: `from os import path`)";

/// The modules whose star import fails: each has a name that counts among those it brings in. `pathlib` brings in
/// `Path`, whose `home` reads the environment.
const PYTHON_STAR_MODULES: [&str; 6] = [
    "os",
    "os.path",
    "posixpath",
    "ntpath",
    "genericpath",
    "pathlib",
];

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
            "`os.environ` and `os.getenv` (and the other names of `os` that touch it), `os.path.expandvars` and \
             `os.path.expanduser`, `Path.home` and a star import of `os`, `os.path` or `pathlib`, whose names cannot \
             be read: name what you use"
        }
        Language::TypeScript => {
            "`process.env` and `import.meta.env` (except what Vite sets itself, such as `import.meta.env.DEV`)"
        }
        Language::Rust => {
            "`std::env::var` and the other functions of `std::env` that read or set a variable, `home_dir` and \
             `temp_dir` among them (and on a glob of `std::env` or of `std`, whose names cannot be read: name what \
             you use)"
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

/// Every read or write of the environment in a Python file, from its uses: the line, and the name that counts, whole
/// when it is one of the path functions or `Path.home`, `os.<name>` for a name of `os` (`os.environ` for
/// `os.environ.get`, which goes on after the name), and the star import of a module whose names count. A use in a type
/// position is judged as the rest: an annotation that names `os.environ` still names it.
pub fn in_python(uses: &[Use]) -> Vec<(usize, String)> {
    let mut found: Vec<(usize, String)> = uses
        .iter()
        .filter_map(|used| {
            let what = match used.how {
                How::Star if PYTHON_STAR_MODULES.contains(&used.name.as_str()) => {
                    format!("{}.*{STAR}", used.name)
                }
                How::Star => return None,
                _ => counted(&used.name)?,
            };
            Some((used.line, what))
        })
        .collect();
    found.sort();
    found
}

/// The part of a dotted name that reads the environment, when it starts with one: `os.environ` of `os.environ.get`.
fn counted(name: &str) -> Option<String> {
    let parts: Vec<&str> = name.split('.').collect();
    if parts.len() >= 2 && parts[0] == "os" && PYTHON.contains(&parts[1]) {
        return Some(format!("os.{}", parts[1]));
    }
    // A function of a path module: `os.path.expanduser` is three parts, `posixpath.expanduser` two
    for modules in [1, 2] {
        if parts.len() > modules
            && PYTHON_PATH_MODULES.contains(&parts[..modules].join(".").as_str())
            && PYTHON_PATH_FUNCTIONS.contains(&parts[modules])
        {
            return Some(parts[..=modules].join("."));
        }
    }
    // `home` of a class of `pathlib`
    if parts.len() >= 3
        && PYTHON_HOME.contains(&parts[..2].join(".").as_str())
        && parts[2] == "home"
    {
        return Some(parts[..3].join("."));
    }
    None
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
    fn the_functions_that_read_the_home_directory_or_the_temporary_one_are_reads() {
        let source = "\
fn a() { let _ = std::env::home_dir(); }
use std::env as e;
fn b() { let _ = e::temp_dir().join(\"x\"); }
use std::env::{home_dir, temp_dir};
";
        assert_eq!(
            names(source),
            [
                (1, "std::env::home_dir".to_string()),
                (3, "std::env::temp_dir".into()),
                (4, "std::env::home_dir".into()),
                (4, "std::env::temp_dir".into()),
            ]
        );
        // The directory of the program itself is no variable
        assert!(
            names("fn a() { let _ = std::env::current_dir(); let _ = std::env::current_exe(); }")
                .is_empty()
        );
    }

    fn used(line: usize, name: &str, how: How) -> Use {
        Use {
            line,
            name: name.into(),
            how,
            in_type: false,
        }
    }

    fn called(line: usize, name: &str) -> Use {
        used(
            line,
            name,
            How::Called {
                positional: 1,
                keywords: Vec::new(),
                unpacked: false,
            },
        )
    }

    #[test]
    fn a_python_use_counts_by_the_name_it_starts_with() {
        let uses = [
            used(1, "os.environ", How::Imported),
            called(2, "os.environ.get"),
            used(3, "os.environ", How::Value),
            called(4, "os.getenv"),
            called(5, "os.path.expanduser"),
            called(6, "os.path.expandvars"),
            called(7, "posixpath.expanduser"),
            called(8, "ntpath.expandvars"),
            called(9, "genericpath.expanduser"),
            called(10, "pathlib.Path.home"),
            called(11, "pathlib.PosixPath.home"),
            called(12, "pathlib.WindowsPath.home"),
            used(13, "pathlib.Path.home", How::Value),
        ];
        let found: Vec<(usize, String)> = in_python(&uses);
        assert_eq!(
            found,
            [
                (1, "os.environ".to_string()),
                (2, "os.environ".into()),
                (3, "os.environ".into()),
                (4, "os.getenv".into()),
                (5, "os.path.expanduser".into()),
                (6, "os.path.expandvars".into()),
                (7, "posixpath.expanduser".into()),
                (8, "ntpath.expandvars".into()),
                (9, "genericpath.expanduser".into()),
                (10, "pathlib.Path.home".into()),
                (11, "pathlib.PosixPath.home".into()),
                (12, "pathlib.WindowsPath.home".into()),
                (13, "pathlib.Path.home".into()),
            ]
        );
    }

    #[test]
    fn what_is_no_environment_variable_passes_in_python() {
        let uses = [
            called(1, "os.path.join"),
            called(2, "os.path.exists"),
            called(3, "os.getcwd"),
            used(4, "os.path", How::Imported),
            called(5, "pathlib.Path"),
            called(6, "pathlib.Path.cwd"),
            called(7, "sys.argv"),
            called(8, "environ"),
            called(9, "mylib.os.environ"),
            called(10, "os"),
            called(11, "pathlib.PurePath.home"),
            called(12, "os.path.expanduser.cache"),
        ];
        // Only the last is a read: the name goes on after the function, as `os.environ.get` after `os.environ`
        assert_eq!(in_python(&uses), [(12, "os.path.expanduser".to_string())]);
    }

    #[test]
    fn a_star_import_of_a_module_whose_names_count_fails_and_another_does_not() {
        let found = in_python(&[
            used(1, "os", How::Star),
            used(2, "os.path", How::Star),
            used(3, "pathlib", How::Star),
            used(4, "posixpath", How::Star),
            used(5, "typing", How::Star),
            used(6, "domain.song", How::Star),
        ]);
        assert_eq!(
            found.iter().map(|(line, _)| *line).collect::<Vec<_>>(),
            [1, 2, 3, 4]
        );
        assert!(found[0].1.starts_with("os.*"));
        assert!(found[1].1.starts_with("os.path.*"));
        assert!(found[0].1.contains("cannot be read") && found[0].1.contains("name them"));
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

//! The check that the inner layers do no I/O, read from the tree: the code of `domain`, `protocol`, `application` and
//! `presentation`, read by the reader of its language, and judged by the rules in `domain::inner_io`. What it finds is
//! a [`Forbidden`] of its own rule, kept as an import is: `rotproof approve` approves it, and an approval that matches
//! nothing fails.

use std::collections::BTreeSet;
use std::io;

use crate::tree::{code_files, read_code};
use domain::declaration::Declared;
use domain::direction::Forbidden;
use domain::inner_io::{findings, in_python, in_rust, in_typescript, is_judged};
use domain::layers::{Language, Layout, Place};
use domain::paths::{file_name, join, parent};
use protocol::outbound::{Parsers, Tree};

/// What the check found: the I/O of the inner layers, and every file that could not be read.
#[derive(Debug, Default)]
pub struct Judged {
    pub forbidden: Vec<Forbidden>,
    /// A file that is not UTF-8, which cannot be read
    pub problems: Vec<String>,
}

/// Every use of I/O in the inner layers of the project of `declared`. An error is a directory that could not be walked.
pub fn problems(tree: &dyn Tree, parsers: &dyn Parsers, declared: &Declared) -> io::Result<Judged> {
    let mut judged = Judged::default();
    let Some(layout) = &declared.layout else {
        // A repository of records only has no code to read
        return Ok(judged);
    };
    for layer in declared
        .places
        .iter()
        .filter(|p| p.is_layer() && !declared.is_absent(p) && is_judged(p))
    {
        // The files of the layer, with their text, in the order of their paths
        let mut files = Vec::new();
        for path in files_of(tree, layout, layer)? {
            if let Some(source) = read_code(tree, &path, "I/O", &mut judged.problems)? {
                files.push((path, source));
            }
        }
        let tests = test_paths(layout, &files);
        for (path, source) in &files {
            if tests.iter().any(|test| is_under(path, test)) {
                continue;
            }
            let found = match layout.language {
                Language::Python => {
                    let read = parsers.python(source, path);
                    in_python(&read.imports, &read.uses)
                }
                Language::TypeScript => {
                    let read = parsers.typescript(source, path);
                    in_typescript(&read.imports, &read.uses)
                }
                Language::Rust => in_rust(source),
            };
            judged
                .forbidden
                .extend(findings(path, source, &layer.name, &found));
        }
    }
    // In the byte order of the path, as every check lists what it finds file by file (a file's lines keep their order)
    judged.forbidden.sort_by(|a, b| a.file.cmp(&b.file));
    judged.problems.sort();
    Ok(judged)
}

/// The files of `layer` that are code, but for a test by its name: the whole of the layer's directory in Python and
/// TypeScript, and in Rust the crate's `src/` alone, since Cargo keeps a crate's tests, benches, examples and build
/// script beside it.
fn files_of(tree: &dyn Tree, layout: &Layout, layer: &Place) -> io::Result<Vec<String>> {
    let dir = match layout.language {
        Language::Rust => join(&layer.path, "src"),
        Language::Python | Language::TypeScript => layer.path.clone(),
    };
    Ok(code_files(tree, &dir, |name| layout.has_comments(name))?
        .into_iter()
        .filter(|path| !layout.is_test_file(path))
        .collect())
}

/// The paths of the test modules of a Rust crate, from the files of `files` (path, text): a module declared
/// `#[cfg(test)] mod x;` in a file is `x.rs` or `x/mod.rs`, in the directory of the file for `lib.rs`, `main.rs` and
/// `mod.rs` and in the directory named as the file for any other. A file under such a path is a test. Each is a path
/// of the file, or of the directory, that is a test as a whole. None in a stack that tells no module apart.
fn test_paths(layout: &Layout, files: &[(String, String)]) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    if layout.language != Language::Rust {
        return found;
    }
    for (path, source) in files {
        let names = domain::rust::test_modules(&domain::rust::code(source));
        if names.is_empty() {
            continue;
        }
        let stem = file_name(path).trim_end_matches(".rs");
        let dir = match stem {
            "lib" | "main" | "mod" => parent(path).to_string(),
            _ => join(parent(path), stem),
        };
        for name in names {
            found.insert(join(&dir, &name));
        }
    }
    found
}

/// Whether `path` is the file `base.rs` or sits in the directory `base/`.
fn is_under(path: &str, base: &str) -> bool {
    path == format!("{base}.rs") || path.starts_with(&format!("{base}/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::fake::Fake;
    use domain::direction::Rule;
    use infrastructure::outbound::readers::Readers;

    fn declared(stack: &str, ui: &str) -> Declared {
        Declared::new(
            domain::declaration::parse_declaration(&format!(
                "stack = \"{stack}\"\nareas = []\nui = \"{ui}\"\n"
            ))
            .unwrap(),
        )
        .unwrap()
    }

    /// The findings of a tree, as `file: entry (names)`, one for each line
    fn judged(files: &[(&str, &str)], stack: &str, ui: &str) -> Vec<String> {
        let tree = Fake::new(files);
        let found = problems(&tree, &Readers, &declared(stack, ui)).unwrap();
        assert!(found.problems.is_empty(), "{:?}", found.problems);
        found
            .forbidden
            .iter()
            .map(|f| {
                let names = match &f.rule {
                    Rule::InnerIo { names } if !names.is_empty() => {
                        format!(" ({})", names.join(", "))
                    }
                    _ => String::new(),
                };
                format!(
                    "{}:{}{names}",
                    f.file,
                    f.detail.split(':').nth(1).unwrap_or_default()
                ) + &format!(" {}", f.import)
            })
            .collect()
    }

    #[test]
    fn a_python_project_is_judged_in_its_four_layers_and_not_in_the_others() {
        let io = "import shutil\nshutil.rmtree('x')\n";
        let found = judged(
            &[
                ("domain/a.py", io),
                ("protocol/inbound/b.py", "from socket import socket\n"),
                ("protocol/outbound/c.py", "import sqlite3\n"),
                ("application/d.py", "import subprocess\n"),
                ("infrastructure/inbound/e.py", io),
                ("infrastructure/outbound/f.py", io),
                ("composition/g.py", io),
                ("__main__.py", io),
                ("tests/test_a.py", io),
            ],
            "python",
            "none",
        );
        assert_eq!(
            found,
            [
                "application/d.py:1 subprocess",
                "domain/a.py:1 shutil",
                "domain/a.py:2 (rmtree) shutil",
                "protocol/inbound/b.py:1 (socket) socket",
                "protocol/outbound/c.py:1 sqlite3",
            ]
        );
    }

    #[test]
    fn the_screens_of_a_project_with_a_ui_are_judged_in_presentation_and_not_in_ui() {
        let found = problems(
            &Fake::new(&[
                ("ui/pages/home.py", "import webbrowser\n"),
                ("presentation/models/m.py", "import shutil\n"),
                ("presentation/display/d.py", "import tempfile\n"),
            ]),
            &Readers,
            &declared("python", "atomic"),
        )
        .unwrap();
        let said: Vec<(&str, bool)> = found
            .forbidden
            .iter()
            .map(|f| {
                (
                    f.file.as_str(),
                    f.detail.contains("a screen's model does no I/O"),
                )
            })
            .collect();
        assert_eq!(
            said,
            [
                ("presentation/display/d.py", true),
                ("presentation/models/m.py", true)
            ]
        );
    }

    #[test]
    fn a_typescript_test_beside_the_code_is_free_and_the_code_is_not() {
        let found = judged(
            &[
                ("src/domain/index.ts", ""),
                ("src/domain/a.ts", "import { readFile } from 'node:fs';\n"),
                (
                    "src/domain/a.test.ts",
                    "import { readFile } from 'node:fs';\n",
                ),
                ("src/domain/__tests__/helper.ts", "import fs from 'fs';\n"),
                ("src/domain/b.ts", "import sqlite from 'bun:sqlite';\n"),
                ("src/infrastructure/outbound/c.ts", "import fs from 'fs';\n"),
                ("src/main.ts", "import fs from 'fs';\n"),
            ],
            "typescript",
            "none",
        );
        assert_eq!(
            found,
            ["src/domain/a.ts:1 (readFile) fs", "src/domain/b.ts:1 bun:"]
        );
    }

    #[test]
    fn a_package_that_the_manifest_lists_does_not_make_fs_another_module() {
        let tree = Fake::new(&[
            (
                "package.json",
                "{\"name\":\"x\",\"dependencies\":{\"fs\":\"1.0.0\"}}\n",
            ),
            ("src/domain/a.ts", "import fs from 'fs';\n"),
        ]);
        let found = problems(&tree, &Readers, &declared("typescript", "none")).unwrap();
        assert_eq!(found.forbidden.len(), 1);
        assert_eq!(found.forbidden[0].import, "fs");
    }

    #[test]
    fn a_rust_crate_is_judged_in_its_src_and_not_in_its_tests_or_what_cfg_test_marks() {
        let found = judged(
            &[
                ("crates/domain/Cargo.toml", "[package]\nname = \"domain\"\n"),
                (
                    "crates/domain/src/lib.rs",
                    "pub mod a;\n#[cfg(test)]\nmod tests;\n#[cfg(test)]\nmod helpers {\n    pub fn f() { std::fs::read(\"x\"); }\n}\npub fn g() { std::fs::read(\"y\"); }\n",
                ),
                (
                    "crates/domain/src/a.rs",
                    "pub fn a() { println!(\"a\"); }\n#[cfg(test)]\nmod tests;\n",
                ),
                ("crates/domain/src/tests.rs", "use std::process::Command;\n"),
                (
                    "crates/domain/src/a/tests.rs",
                    "use std::process::Command;\n",
                ),
                (
                    "crates/domain/src/a/tests/more.rs",
                    "use std::process::Command;\n",
                ),
                ("crates/domain/src/other.rs", "use std::process::Command;\n"),
                ("crates/domain/tests/it.rs", "use std::fs;\n"),
                ("crates/domain/benches/b.rs", "use std::fs;\n"),
                (
                    "crates/domain/build.rs",
                    "fn main() { std::fs::write(\"a\", \"b\"); }\n",
                ),
                (
                    "crates/infrastructure/Cargo.toml",
                    "[package]\nname = \"infrastructure\"\n",
                ),
                (
                    "crates/infrastructure/src/lib.rs",
                    "pub fn i() { std::fs::read(\"x\"); }\n",
                ),
                ("src/main.rs", "fn main() { println!(\"x\"); }\n"),
            ],
            "rust",
            "none",
        );
        assert_eq!(
            found,
            [
                "crates/domain/src/a.rs:1 (println!) std::io",
                "crates/domain/src/lib.rs:8 (read) std::fs",
                "crates/domain/src/other.rs:1 (Command) std::process",
            ]
        );
    }

    #[test]
    fn a_layer_declared_absent_and_a_repository_of_records_are_not_read() {
        let tree = Fake::new(&[("domain/a.py", "import shutil\n")]);
        let declared = Declared::new(
            domain::declaration::parse_declaration(
                "stack = \"python\"\nareas = []\nui = \"none\"\nabsent = [\"domain\"]\n",
            )
            .unwrap(),
        )
        .unwrap();
        assert!(
            problems(&tree, &Readers, &declared)
                .unwrap()
                .forbidden
                .is_empty()
        );
        let records = Declared::new(
            domain::declaration::parse_declaration("stack = \"none\"\nareas = [\"a\"]\n").unwrap(),
        )
        .unwrap();
        assert!(
            problems(&tree, &Readers, &records)
                .unwrap()
                .forbidden
                .is_empty()
        );
    }
}
